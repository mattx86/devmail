use crate::mime::parse_email;
use crate::model::{Attachment, Email, EmailSummary};
use anyhow::Context;
use chrono::{DateTime, NaiveDateTime, Utc};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

pub struct EmailStore {
    emails: IndexMap<Uuid, Email>,
    disk: Option<PathBuf>,
}

pub type SharedStore = Arc<RwLock<EmailStore>>;

/// Persisted alongside devmail.mbox — tracks which emails have been read.
#[derive(Serialize, Deserialize, Default)]
struct MailState {
    #[serde(default)]
    read_ids: HashSet<Uuid>,
}

impl EmailStore {
    pub fn new_memory() -> SharedStore {
        Arc::new(RwLock::new(EmailStore {
            emails: IndexMap::new(),
            disk: None,
        }))
    }

    /// Creates a disk-backed store and immediately reloads emails from the
    /// existing mbox file (if any), honouring saved read state.
    pub fn new_disk(dir: PathBuf) -> anyhow::Result<SharedStore> {
        let state = load_state(&dir);
        let mut emails = IndexMap::new();

        for (raw, received_at, id) in load_mbox_messages(&dir)? {
            match parse_email(&raw, "", vec![], received_at) {
                Ok(mut email) => {
                    email.id = id;
                    email.read = state.read_ids.contains(&id);
                    emails.insert(id, email);
                }
                Err(e) => {
                    tracing::warn!("Skipping unreadable mbox entry ({id}): {e}");
                }
            }
        }

        tracing::info!("Loaded {} email(s) from disk", emails.len());

        Ok(Arc::new(RwLock::new(EmailStore {
            emails,
            disk: Some(dir),
        })))
    }

    pub fn len(&self) -> usize {
        self.emails.len()
    }

    pub fn save(&mut self, email: Email) -> anyhow::Result<()> {
        if let Some(ref dir) = self.disk {
            append_mbox(dir, &email)?;
        }
        self.emails.insert(email.id, email);
        Ok(())
    }

    /// Returns all emails newest-first.
    pub fn list(&self) -> Vec<EmailSummary> {
        self.emails.values().rev().map(EmailSummary::from).collect()
    }

    pub fn get(&self, id: Uuid) -> Option<&Email> {
        self.emails.get(&id)
    }

    /// Returns true if the email was found and marked read.
    pub fn mark_read(&mut self, id: Uuid) -> bool {
        let found = if let Some(email) = self.emails.get_mut(&id) {
            email.read = true;
            true
        } else {
            false
        };
        if found {
            if let Some(ref dir) = self.disk {
                if let Err(e) = persist_state(dir, &self.emails) {
                    tracing::warn!("Failed to persist read state: {e}");
                }
            }
        }
        found
    }

    /// Removes the email from memory and rewrites the mbox without it.
    /// Returns true if the email was found and removed.
    pub fn delete(&mut self, id: Uuid) -> bool {
        if self.emails.shift_remove(&id).is_some() {
            if let Some(ref dir) = self.disk {
                if let Err(e) = rewrite_mbox(dir, &self.emails) {
                    tracing::warn!("Failed to rewrite mbox after delete: {e}");
                }
                if let Err(e) = persist_state(dir, &self.emails) {
                    tracing::warn!("Failed to persist state after delete: {e}");
                }
            }
            true
        } else {
            false
        }
    }

    pub fn get_attachment(&self, email_id: Uuid, filename: &str) -> Option<&Attachment> {
        self.emails
            .get(&email_id)?
            .attachments
            .iter()
            .find(|a| a.filename == filename)
    }
}

// ── State persistence ─────────────────────────────────────────────────────────

fn state_path(dir: &PathBuf) -> PathBuf {
    dir.join("devmail_state.json")
}

fn load_state(dir: &PathBuf) -> MailState {
    std::fs::read_to_string(state_path(dir))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn persist_state(dir: &PathBuf, emails: &IndexMap<Uuid, Email>) -> anyhow::Result<()> {
    let read_ids: HashSet<Uuid> = emails.values().filter(|e| e.read).map(|e| e.id).collect();
    let json = serde_json::to_string_pretty(&MailState { read_ids })?;
    std::fs::write(state_path(dir), json)?;
    Ok(())
}

// ── mbox read ─────────────────────────────────────────────────────────────────

/// Parses devmail.mbox and returns `(raw_message, received_at, uuid)` for each
/// stored email. UUIDs come from the `X-DevMail-ID` header; older entries
/// without that header get a fresh random UUID.
fn load_mbox_messages(dir: &PathBuf) -> anyhow::Result<Vec<(String, DateTime<Utc>, Uuid)>> {
    let path = dir.join("devmail.mbox");
    if !path.exists() {
        return Ok(vec![]);
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("reading mbox at {}", path.display()))?;

    let mut messages: Vec<(String, DateTime<Utc>, Uuid)> = Vec::new();
    let mut body_lines: Vec<&str> = Vec::new();
    let mut current_date: Option<DateTime<Utc>> = None;
    let mut in_message = false;

    for line in content.lines() {
        if is_mbox_separator(line) {
            if in_message {
                if let Some(date) = current_date {
                    messages.push(assemble_message(&body_lines, date));
                }
            }
            body_lines = Vec::new();
            current_date = parse_mbox_date(line);
            in_message = true;
        } else if in_message {
            // Un-quote one level of >From quoting.
            body_lines.push(if line.starts_with(">From ") { &line[1..] } else { line });
        }
    }

    if in_message {
        if let Some(date) = current_date {
            messages.push(assemble_message(&body_lines, date));
        }
    }

    Ok(messages)
}

fn is_mbox_separator(line: &str) -> bool {
    line.starts_with("From ") && !line.starts_with("From: ")
}

fn parse_mbox_date(line: &str) -> Option<DateTime<Utc>> {
    let rest = line.strip_prefix("From ")?;
    let date_str = rest.splitn(2, ' ').nth(1)?.trim();
    NaiveDateTime::parse_from_str(date_str, "%a %b %e %H:%M:%S %Y")
        .ok()
        .map(|ndt| ndt.and_utc())
}

fn assemble_message(lines: &[&str], received_at: DateTime<Utc>) -> (String, DateTime<Utc>, Uuid) {
    let (id, start) = match lines.first() {
        Some(first) => match first.strip_prefix("X-DevMail-ID: ") {
            Some(uuid_str) => {
                let id = Uuid::parse_str(uuid_str.trim()).unwrap_or_else(|_| Uuid::new_v4());
                (id, 1)
            }
            None => (Uuid::new_v4(), 0),
        },
        None => (Uuid::new_v4(), 0),
    };

    // Strip trailing blank lines.
    let end = lines[start..]
        .iter()
        .rposition(|l| !l.is_empty())
        .map(|i| start + i + 1)
        .unwrap_or(start);

    let raw = lines[start..end].join("\n");
    (raw, received_at, id)
}

// ── mbox write ────────────────────────────────────────────────────────────────

/// Writes a single email entry to an already-open file handle.
fn write_mbox_entry(file: &mut impl Write, email: &Email) -> anyhow::Result<()> {
    let date_str = email.received_at.format("%a %b %e %H:%M:%S %Y").to_string();
    let from_addr = if email.from.is_empty() { "MAILER-DAEMON" } else { &email.from };
    writeln!(file, "From {} {}", from_addr, date_str)?;
    writeln!(file, "X-DevMail-ID: {}", email.id)?;
    for line in email.raw.lines() {
        if line.starts_with("From ") {
            write!(file, ">")?;
        }
        writeln!(file, "{}", line)?;
    }
    writeln!(file)?; // blank separator
    Ok(())
}

/// Appends one email to <dir>/devmail.mbox.
fn append_mbox(dir: &PathBuf, email: &Email) -> anyhow::Result<()> {
    let path = dir.join("devmail.mbox");
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .with_context(|| format!("opening mbox at {}", path.display()))?;
    write_mbox_entry(&mut file, email)
}

/// Rewrites <dir>/devmail.mbox from the current in-memory email set,
/// atomically replacing the file via a temp file + rename.
fn rewrite_mbox(dir: &PathBuf, emails: &IndexMap<Uuid, Email>) -> anyhow::Result<()> {
    let path = dir.join("devmail.mbox");
    let tmp = dir.join("devmail.mbox.tmp");

    {
        let mut file = std::fs::File::create(&tmp)
            .with_context(|| format!("creating temp mbox at {}", tmp.display()))?;
        for email in emails.values() {
            write_mbox_entry(&mut file, email)?;
        }
    }

    std::fs::rename(&tmp, &path)
        .with_context(|| format!("replacing mbox at {}", path.display()))?;
    Ok(())
}
