use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Email {
    pub id: Uuid,
    pub received_at: DateTime<Utc>,
    /// Full raw RFC 5322 message — used for mbox storage and the headers popup.
    pub raw: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub attachments: Vec<Attachment>,
    pub read: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Attachment {
    pub filename: String,
    pub content_type: String,
    /// Raw bytes — not included in JSON API responses; served via download endpoint.
    #[serde(skip)]
    pub data: Vec<u8>,
}

/// Lightweight summary shown in the left-pane email list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailSummary {
    pub id: Uuid,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub received_at: DateTime<Utc>,
    pub read: bool,
}

impl From<&Email> for EmailSummary {
    fn from(e: &Email) -> Self {
        EmailSummary {
            id: e.id,
            from: e.from.clone(),
            to: e.to.clone(),
            cc: e.cc.clone(),
            subject: e.subject.clone(),
            received_at: e.received_at,
            read: e.read,
        }
    }
}

/// Attachment metadata without binary data — used in the HTTP API JSON response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttachmentInfo {
    pub filename: String,
    pub content_type: String,
    pub size: usize,
}

/// Full email detail returned by the HTTP API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailDetail {
    pub id: Uuid,
    pub received_at: DateTime<Utc>,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub subject: String,
    pub text_body: Option<String>,
    pub html_body: Option<String>,
    pub attachments: Vec<AttachmentInfo>,
    pub read: bool,
}

impl From<&Email> for EmailDetail {
    fn from(e: &Email) -> Self {
        EmailDetail {
            id: e.id,
            received_at: e.received_at,
            from: e.from.clone(),
            to: e.to.clone(),
            cc: e.cc.clone(),
            subject: e.subject.clone(),
            text_body: e.text_body.clone(),
            html_body: e.html_body.clone(),
            attachments: e
                .attachments
                .iter()
                .map(|a| AttachmentInfo {
                    filename: a.filename.clone(),
                    content_type: a.content_type.clone(),
                    size: a.data.len(),
                })
                .collect(),
            read: e.read,
        }
    }
}
