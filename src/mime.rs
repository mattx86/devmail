use crate::model::{Attachment, Email};
use chrono::{DateTime, Utc};
use mail_parser::{Address, MessageParser, MimeHeaders, PartType};
use uuid::Uuid;

/// Parses a raw RFC 5322 message into a typed Email.
pub fn parse_email(
    raw: &str,
    mail_from: &str,
    rcpt_to: Vec<String>,
    received_at: DateTime<Utc>,
) -> anyhow::Result<Email> {
    let msg = MessageParser::default()
        .parse(raw.as_bytes())
        .ok_or_else(|| anyhow::anyhow!("mail-parser returned None for message"))?;

    let from = msg
        .from()
        .and_then(|a| first_address(a))
        .unwrap_or_else(|| mail_from.to_string());

    let to: Vec<String> = msg
        .to()
        .map(|a| all_addresses(a))
        .unwrap_or_else(|| rcpt_to.clone());

    let cc: Vec<String> = msg
        .cc()
        .map(|a| all_addresses(a))
        .unwrap_or_default();

    let subject = msg
        .subject()
        .unwrap_or("(no subject)")
        .to_string();

    let mut text_body: Option<String> = None;
    let mut html_body: Option<String> = None;
    let mut attachments: Vec<Attachment> = Vec::new();

    // mail-parser provides pre-categorised part indices.
    for &idx in &msg.text_body {
        if let Some(part) = msg.parts.get(idx) {
            if let PartType::Text(text) = &part.body {
                if text_body.is_none() {
                    text_body = Some(text.as_ref().to_string());
                }
            }
        }
    }

    for &idx in &msg.html_body {
        if let Some(part) = msg.parts.get(idx) {
            if let PartType::Html(html) = &part.body {
                if html_body.is_none() {
                    html_body = Some(html.as_ref().to_string());
                }
            }
        }
    }

    for &idx in &msg.attachments {
        if let Some(part) = msg.parts.get(idx) {
            let filename = part
                .attachment_name()
                .map(|s: &str| s.to_string())
                .unwrap_or_else(|| format!("attachment-{idx}"));

            let content_type = part
                .content_type()
                .map(|ct| {
                    if let Some(sub) = &ct.c_subtype {
                        format!("{}/{}", ct.c_type, sub)
                    } else {
                        ct.c_type.to_string()
                    }
                })
                .unwrap_or_else(|| "application/octet-stream".to_string());

            let data: Vec<u8> = match &part.body {
                PartType::Binary(b) | PartType::InlineBinary(b) => b.as_ref().to_vec(),
                PartType::Text(t) => t.as_bytes().to_vec(),
                PartType::Html(h) => h.as_bytes().to_vec(),
                _ => continue,
            };

            attachments.push(Attachment {
                filename,
                content_type,
                data,
            });
        }
    }

    Ok(Email {
        id: Uuid::new_v4(),
        received_at,
        raw: raw.to_string(),
        from,
        to,
        cc,
        subject,
        text_body,
        html_body,
        attachments,
        read: false,
    })
}

/// Creates a minimal Email when MIME parsing fails, preserving the raw content.
pub fn make_raw_email(raw: String, mail_from: String, received_at: DateTime<Utc>) -> Email {
    Email {
        id: Uuid::new_v4(),
        received_at,
        from: mail_from,
        to: vec![],
        cc: vec![],
        subject: "(parse error)".to_string(),
        text_body: Some(raw.clone()),
        html_body: None,
        attachments: vec![],
        raw,
        read: false,
    }
}

fn format_addr(addr: &mail_parser::Addr) -> String {
    match (addr.name.as_deref(), addr.address.as_deref()) {
        (Some(name), Some(email)) => format!("{name} <{email}>"),
        (None, Some(email)) => email.to_string(),
        (Some(name), None) => name.to_string(),
        (None, None) => String::new(),
    }
}

fn first_address(addr: &Address) -> Option<String> {
    let s = match addr {
        Address::List(list) => list.first().map(format_addr)?,
        Address::Group(groups) => groups
            .first()
            .and_then(|g| g.addresses.first())
            .map(format_addr)?,
    };
    if s.is_empty() { None } else { Some(s) }
}

fn all_addresses(addr: &Address) -> Vec<String> {
    let strings: Vec<String> = match addr {
        Address::List(list) => list.iter().map(format_addr).collect(),
        Address::Group(groups) => groups
            .iter()
            .flat_map(|g| g.addresses.iter().map(format_addr))
            .collect(),
    };
    strings.into_iter().filter(|s| !s.is_empty()).collect()
}
