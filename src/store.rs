use crate::mime::parse_email;
use crate::model::{Email, EmailSummary};
use anyhow::Context;
use chrono::{DateTime, Duration, NaiveDateTime, Utc};
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
    max_age_hours: u64,
    max_emails: usize,
    max_size_mb: usize,
    total_size_bytes: usize,
}

pub type SharedStore = Arc<RwLock<EmailStore>>;

#[derive(Debug)]
pub enum SaveError {
    TooBig,
    Io(anyhow::Error),
}

impl From<anyhow::Error> for SaveError {
    fn from(e: anyhow::Error) -> Self {
        SaveError::Io(e)
    }
}

/// Persisted alongside devmail.mbox — tracks which emails have been read.
#[derive(Serialize, Deserialize, Default)]
struct MailState {
    #[serde(default)]
    read_ids: HashSet<Uuid>,
}

impl EmailStore {
    pub fn new_memory(max_age_hours: u64, max_emails: usize, max_size_mb: usize) -> SharedStore {
        Arc::new(RwLock::new(EmailStore {
            emails: IndexMap::new(),
            disk: None,
            max_age_hours,
            max_emails,
            max_size_mb,
            total_size_bytes: 0,
        }))
    }

    /// Creates a disk-backed store and immediately reloads email metadata from the
    /// existing mbox file (if any), honouring saved read state.
    /// Bodies, raw messages, and attachments are NOT kept in memory — only shells.
    pub fn new_disk(
        dir: PathBuf,
        max_age_hours: u64,
        max_emails: usize,
        max_size_mb: usize,
    ) -> anyhow::Result<SharedStore> {
        let state = load_state(&dir);
        let mut emails = IndexMap::new();
        let mut total_size_bytes: usize = 0;

        for (raw, received_at, id) in load_mbox_messages(&dir)? {
            match parse_email(&raw, "", vec![], received_at) {
                Ok(mut email) => {
                    email.id = id;
                    email.read = state.read_ids.contains(&id);
                    total_size_bytes += email.size_bytes;
                    // Disk mode: discard bodies/raw/attachment data; keep metadata (incl. attachment count).
                    email.raw = String::new();
                    email.text_body = None;
                    email.html_body = None;
                    for att in &mut email.attachments { att.data = vec![]; }
                    emails.insert(id, email);
                }
                Err(e) => {
                    tracing::warn!("Skipping unreadable mbox entry ({id}): {e}");
                }
            }
        }

        tracing::info!("Loaded {} email(s) from disk", emails.len());

        let mut store = EmailStore {
            emails,
            disk: Some(dir),
            max_age_hours,
            max_emails,
            max_size_mb,
            total_size_bytes,
        };
        store.enforce_limits();
        Ok(Arc::new(RwLock::new(store)))
    }

    pub fn len(&self) -> usize {
        self.emails.len()
    }

    pub fn save(&mut self, email: Email) -> Result<(), SaveError> {
        let max_bytes = self.max_size_mb.saturating_mul(1024 * 1024);

        if max_bytes > 0 {
            // Reject emails that are larger than the entire inbox limit.
            if email.size_bytes > max_bytes {
                return Err(SaveError::TooBig);
            }

            // Evict oldest emails to make room for the incoming one.
            let mut evicted = false;
            while self.total_size_bytes + email.size_bytes > max_bytes {
                if let Some((_, removed)) = self.emails.shift_remove_index(0) {
                    self.total_size_bytes =
                        self.total_size_bytes.saturating_sub(removed.size_bytes);
                    evicted = true;
                } else {
                    break;
                }
            }
            if evicted {
                if let Some(ref dir) = self.disk {
                    if let Err(e) = rewrite_mbox_filtered(dir, &self.emails) {
                        return Err(SaveError::Io(e));
                    }
                    if let Err(e) = persist_state(dir, &self.emails) {
                        tracing::warn!("Failed to persist state after size eviction: {e}");
                    }
                }
            }
        }

        if let Some(ref dir) = self.disk {
            append_mbox(dir, &email).map_err(SaveError::Io)?;
            // Disk mode: store shell only (metadata, no body/raw/attachment data).
            let mut shell = email;
            let size_bytes = shell.size_bytes;
            shell.raw = String::new();
            shell.text_body = None;
            shell.html_body = None;
            for att in &mut shell.attachments { att.data = vec![]; }
            self.total_size_bytes += size_bytes;
            self.emails.insert(shell.id, shell);
        } else {
            self.total_size_bytes += email.size_bytes;
            self.emails.insert(email.id, email);
        }

        self.enforce_limits();
        Ok(())
    }

    /// Removes emails that exceed the configured age, count, or size limits.
    /// Called on save, on mbox reload, and periodically (once per hour).
    /// Performs at most one mbox rewrite per call regardless of how many are removed.
    pub(crate) fn enforce_limits(&mut self) {
        let initial_len = self.emails.len();

        if self.max_age_hours > 0 {
            let cutoff = Utc::now() - Duration::hours(self.max_age_hours as i64);
            let to_remove: Vec<Uuid> = self
                .emails
                .values()
                .filter(|e| e.received_at < cutoff)
                .map(|e| e.id)
                .collect();
            for id in to_remove {
                if let Some(removed) = self.emails.shift_remove(&id) {
                    self.total_size_bytes =
                        self.total_size_bytes.saturating_sub(removed.size_bytes);
                }
            }
        }

        if self.max_emails > 0 {
            while self.emails.len() > self.max_emails {
                if let Some((_, removed)) = self.emails.shift_remove_index(0) {
                    self.total_size_bytes =
                        self.total_size_bytes.saturating_sub(removed.size_bytes);
                }
            }
        }

        if self.emails.len() < initial_len {
            if let Some(ref dir) = self.disk {
                if let Err(e) = rewrite_mbox_filtered(dir, &self.emails) {
                    tracing::warn!("Failed to rewrite mbox after limit enforcement: {e}");
                }
                if let Err(e) = persist_state(dir, &self.emails) {
                    tracing::warn!("Failed to persist state after limit enforcement: {e}");
                }
            }
        }
    }

    /// Returns all emails newest-first.
    pub fn list(&self) -> Vec<EmailSummary> {
        self.emails.values().rev().map(EmailSummary::from).collect()
    }

    /// Returns the full email, reading from disk in disk mode.
    /// In memory mode, clones the in-memory Email directly.
    pub fn get_full(&self, id: Uuid) -> anyhow::Result<Option<Email>> {
        match &self.disk {
            None => Ok(self.emails.get(&id).cloned()),
            Some(dir) => {
                let shell = match self.emails.get(&id) {
                    Some(s) => s,
                    None => return Ok(None),
                };
                for (raw, received_at, entry_id) in load_mbox_messages(dir)? {
                    if entry_id == id {
                        let mut email =
                            parse_email(&raw, &shell.from, shell.to.clone(), received_at)?;
                        email.id = id;
                        email.read = shell.read;
                        return Ok(Some(email));
                    }
                }
                Ok(None)
            }
        }
    }

    /// Returns `(current_bytes, max_bytes)` for the capacity bar.
    /// When max_size is set, max_bytes is the configured limit.
    /// Otherwise: available RAM (memory mode) or available disk space (disk mode).
    pub fn capacity(&self) -> (u64, u64) {
        let current = self.total_size_bytes as u64;

        if self.max_size_mb > 0 {
            return (current, self.max_size_mb as u64 * 1024 * 1024);
        }

        // Dynamic limit: ask the OS.
        if let Some(ref dir) = self.disk {
            use sysinfo::Disks;
            let disks = Disks::new_with_refreshed_list();
            // Find the disk whose mount point is the longest prefix of our storage dir.
            let dir_str = dir.to_string_lossy();
            let best = disks
                .iter()
                .filter_map(|d| {
                    let mp = d.mount_point().to_string_lossy();
                    if dir_str.starts_with(mp.as_ref()) {
                        Some((mp.len(), d.available_space()))
                    } else {
                        None
                    }
                })
                .max_by_key(|(len, _)| *len);
            let avail = best.map(|(_, space)| space).unwrap_or(0);
            (current, current + avail)
        } else {
            use sysinfo::{MemoryRefreshKind, RefreshKind, System};
            let sys =
                System::new_with_specifics(RefreshKind::new().with_memory(MemoryRefreshKind::new().with_ram()));
            (current, current + sys.available_memory())
        }
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
        if let Some(removed) = self.emails.shift_remove(&id) {
            self.total_size_bytes = self.total_size_bytes.saturating_sub(removed.size_bytes);
            if let Some(ref dir) = self.disk {
                if let Err(e) = rewrite_mbox_filtered(dir, &self.emails) {
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

/// Rewrites <dir>/devmail.mbox, keeping only entries whose UUIDs are present
/// in `kept`. Reads raw content from the existing mbox rather than from memory,
/// making it safe to call in disk mode (where Email.raw is empty).
fn rewrite_mbox_filtered(dir: &PathBuf, kept: &IndexMap<Uuid, Email>) -> anyhow::Result<()> {
    let path = dir.join("devmail.mbox");
    let tmp = dir.join("devmail.mbox.tmp");

    let entries = load_mbox_messages(dir)?;

    {
        let mut file = std::fs::File::create(&tmp)
            .with_context(|| format!("creating temp mbox at {}", tmp.display()))?;

        for (raw, received_at, id) in &entries {
            if let Some(shell) = kept.get(id) {
                let date_str = received_at.format("%a %b %e %H:%M:%S %Y").to_string();
                let from_addr =
                    if shell.from.is_empty() { "MAILER-DAEMON" } else { &shell.from };
                writeln!(file, "From {} {}", from_addr, date_str)?;
                writeln!(file, "X-DevMail-ID: {}", id)?;
                for line in raw.lines() {
                    if line.starts_with("From ") {
                        write!(file, ">")?;
                    }
                    writeln!(file, "{}", line)?;
                }
                writeln!(file)?;
            }
        }
    }

    std::fs::rename(&tmp, &path)
        .with_context(|| format!("replacing mbox at {}", path.display()))?;
    Ok(())
}
