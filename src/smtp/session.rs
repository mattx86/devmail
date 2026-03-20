use crate::mime;
use crate::store::{SaveError, SharedStore};
use super::parser::{parse_command, SmtpCommand};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

enum SmtpState {
    Connected,
    Greeted {
        client_id: String,
    },
    InTransaction {
        client_id: String,
        mail_from: String,
        rcpt_to: Vec<String>,
    },
}

pub struct SmtpSession {
    state: SmtpState,
    store: SharedStore,
    peer_addr: std::net::SocketAddr,
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
    /// Maximum accepted message size in bytes (0 = unlimited).
    max_size_bytes: usize,
}

impl SmtpSession {
    pub fn new(
        stream: TcpStream,
        store: SharedStore,
        peer_addr: std::net::SocketAddr,
        max_size_bytes: usize,
    ) -> Self {
        let (read_half, write_half) = stream.into_split();
        SmtpSession {
            state: SmtpState::Connected,
            store,
            peer_addr,
            reader: BufReader::new(read_half),
            writer: write_half,
            max_size_bytes,
        }
    }

    async fn write_response(&mut self, s: &str) -> anyhow::Result<()> {
        self.writer.write_all(s.as_bytes()).await?;
        Ok(())
    }

    async fn read_line(&mut self) -> anyhow::Result<String> {
        let mut line = String::new();
        let n = self.reader.read_line(&mut line).await?;
        if n == 0 {
            anyhow::bail!("connection closed");
        }
        Ok(line)
    }

    pub async fn run(mut self) -> anyhow::Result<()> {
        self.write_response("220 devmail ESMTP ready\r\n").await?;
        loop {
            let line = match self.read_line().await {
                Ok(l) => l,
                Err(_) => break,
            };
            let cmd = parse_command(&line);
            match self.handle(cmd).await {
                Ok(true) => break,  // QUIT
                Ok(false) => {}
                Err(e) => {
                    tracing::debug!("SMTP session error: {e}");
                    break;
                }
            }
        }
        Ok(())
    }

    /// Returns Ok(true) when the session should end (QUIT received).
    async fn handle(&mut self, cmd: SmtpCommand) -> anyhow::Result<bool> {
        match cmd {
            SmtpCommand::Ehlo(id) | SmtpCommand::Helo(id) => {
                self.state = SmtpState::Greeted { client_id: id };
                let mut resp = String::from("250-devmail\r\n");
                if self.max_size_bytes > 0 {
                    resp.push_str(&format!("250-SIZE {}\r\n", self.max_size_bytes));
                }
                resp.push_str("250-PIPELINING\r\n");
                resp.push_str("250-SMTPUTF8\r\n");
                resp.push_str("250 8BITMIME\r\n");
                self.write_response(&resp).await?;
            }

            SmtpCommand::Rset => {
                let client_id = match &self.state {
                    SmtpState::Greeted { client_id } => client_id.clone(),
                    SmtpState::InTransaction { client_id, .. } => client_id.clone(),
                    SmtpState::Connected => String::new(),
                };
                self.state = SmtpState::Greeted { client_id };
                self.write_response("250 OK\r\n").await?;
            }

            SmtpCommand::MailFrom { addr, declared_size } => {
                let client_id = match &self.state {
                    SmtpState::Greeted { client_id } => client_id.clone(),
                    SmtpState::InTransaction { client_id, .. } => client_id.clone(),
                    SmtpState::Connected => {
                        self.write_response("503 Bad sequence of commands\r\n").await?;
                        return Ok(false);
                    }
                };
                // RFC 1870: reject immediately if the declared size exceeds our limit.
                if let Some(sz) = declared_size {
                    if self.max_size_bytes > 0 && sz > self.max_size_bytes {
                        self.write_response("552 Message exceeds maximum permitted size\r\n").await?;
                        return Ok(false);
                    }
                }
                self.state = SmtpState::InTransaction {
                    client_id,
                    mail_from: addr,
                    rcpt_to: vec![],
                };
                self.write_response("250 OK\r\n").await?;
            }

            SmtpCommand::RcptTo(addr) => {
                if let SmtpState::InTransaction { ref mut rcpt_to, .. } = self.state {
                    rcpt_to.push(addr);
                    self.write_response("250 OK\r\n").await?;
                } else {
                    self.write_response("503 Bad sequence of commands\r\n").await?;
                }
            }

            SmtpCommand::Data => {
                // Extract what we need before any awaits to satisfy the borrow checker.
                let (mail_from, rcpt_to, client_id) = match &self.state {
                    SmtpState::InTransaction {
                        mail_from,
                        rcpt_to,
                        client_id,
                    } => {
                        if rcpt_to.is_empty() {
                            self.write_response("503 Need RCPT TO first\r\n").await?;
                            return Ok(false);
                        }
                        (mail_from.clone(), rcpt_to.clone(), client_id.clone())
                    }
                    _ => {
                        self.write_response("503 Bad sequence of commands\r\n").await?;
                        return Ok(false);
                    }
                };

                self.write_response("354 End data with <CR><LF>.<CR><LF>\r\n")
                    .await?;
                let body = self.read_data().await?;
                let received_at = chrono::Utc::now();

                // Prepend a Received trace header (RFC 5321 §4.4).
                let received_header = format!(
                    "Received: from {} ([{}])\r\n\tby devmail with ESMTP; {}\r\n",
                    client_id,
                    self.peer_addr.ip(),
                    received_at.format("%a, %d %b %Y %H:%M:%S +0000"),
                );
                let raw = format!("{received_header}{body}");

                let email = match mime::parse_email(&raw, &mail_from, rcpt_to, received_at) {
                    Ok(e) => e,
                    Err(e) => {
                        tracing::warn!("MIME parse warning: {e}");
                        mime::make_raw_email(raw, mail_from, received_at)
                    }
                };

                let save_result = self.store.write().await.save(email);
                let response = match save_result {
                    Ok(()) => "250 OK: message accepted\r\n",
                    Err(SaveError::TooBig) => "552 Message exceeds maximum permitted size\r\n",
                    Err(SaveError::Io(e)) => {
                        tracing::error!("Failed to save email: {e}");
                        "451 Requested action aborted: error in processing\r\n"
                    }
                };
                self.write_response(response).await?;
                self.state = SmtpState::Greeted { client_id };
            }

            SmtpCommand::Noop => {
                self.write_response("250 OK\r\n").await?;
            }

            SmtpCommand::Quit => {
                self.write_response("221 Bye\r\n").await?;
                return Ok(true);
            }

            SmtpCommand::Unknown => {
                self.write_response("500 Command unrecognized\r\n").await?;
            }
        }
        Ok(false)
    }

    /// Reads DATA lines until a line consisting solely of "." is received.
    /// Performs RFC 5321 dot-unstuffing (leading ".." → ".").
    async fn read_data(&mut self) -> anyhow::Result<String> {
        let mut lines: Vec<String> = Vec::new();
        loop {
            let line = self.read_line().await?;
            let trimmed = line.trim_end_matches(['\r', '\n']);
            if trimmed == "." {
                break;
            }
            // Dot-unstuffing: a leading ".." means the sender escaped a real leading dot.
            let content = if trimmed.starts_with("..") {
                &trimmed[1..]
            } else {
                trimmed
            };
            lines.push(content.to_string());
        }
        Ok(lines.join("\r\n") + "\r\n")
    }
}
