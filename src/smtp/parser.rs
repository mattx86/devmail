#[derive(Debug)]
pub enum SmtpCommand {
    Ehlo(String),
    Helo(String),
    /// MAIL FROM address and optional declared SIZE parameter (RFC 1870).
    MailFrom { addr: String, declared_size: Option<usize> },
    RcptTo(String),
    Data,
    Rset,
    Noop,
    Quit,
    Unknown,
}

pub fn parse_command(line: &str) -> SmtpCommand {
    let line = line.trim_end_matches(['\r', '\n']);
    let upper = line.to_ascii_uppercase();

    if upper.starts_with("EHLO ") {
        SmtpCommand::Ehlo(line[5..].trim().to_string())
    } else if upper.starts_with("HELO ") {
        SmtpCommand::Helo(line[5..].trim().to_string())
    } else if upper.starts_with("MAIL FROM:") {
        let rest = &line[10..];
        let addr = extract_address(rest);
        // Look for SIZE=<n> parameter after the address (case-insensitive).
        let declared_size = rest.to_ascii_uppercase()
            .split_whitespace()
            .find_map(|tok| tok.strip_prefix("SIZE=").and_then(|n| n.parse().ok()));
        SmtpCommand::MailFrom { addr, declared_size }
    } else if upper.starts_with("RCPT TO:") {
        SmtpCommand::RcptTo(extract_address(&line[8..]))
    } else if upper == "DATA" {
        SmtpCommand::Data
    } else if upper == "RSET" {
        SmtpCommand::Rset
    } else if upper == "NOOP" {
        SmtpCommand::Noop
    } else if upper == "QUIT" {
        SmtpCommand::Quit
    } else {
        SmtpCommand::Unknown
    }
}

/// Extracts the email address from angle brackets, or returns the trimmed string.
fn extract_address(s: &str) -> String {
    let s = s.trim();
    if let (Some(start), Some(end)) = (s.find('<'), s.rfind('>')) {
        s[start + 1..end].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ehlo() {
        let cmd = parse_command("EHLO client.example.com\r\n");
        assert!(matches!(cmd, SmtpCommand::Ehlo(s) if s == "client.example.com"));
    }

    #[test]
    fn test_parse_mail_from_angle() {
        let cmd = parse_command("MAIL FROM:<sender@example.com>\r\n");
        assert!(matches!(cmd, SmtpCommand::MailFrom(s) if s == "sender@example.com"));
    }

    #[test]
    fn test_parse_rcpt_to() {
        let cmd = parse_command("RCPT TO:<to@example.com>\r\n");
        assert!(matches!(cmd, SmtpCommand::RcptTo(s) if s == "to@example.com"));
    }

    #[test]
    fn test_parse_data() {
        assert!(matches!(parse_command("DATA\r\n"), SmtpCommand::Data));
    }

    #[test]
    fn test_parse_quit() {
        assert!(matches!(parse_command("QUIT\r\n"), SmtpCommand::Quit));
    }
}
