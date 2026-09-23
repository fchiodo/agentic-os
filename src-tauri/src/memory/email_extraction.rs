use mail_parser::MimeHeaders as _;

use crate::error::{AppError, AppResult};

/// Emails enter the vault exactly like documents: the original file is
/// preserved byte-for-byte in `_sources/`, and this module produces the
/// deterministic markdown snapshot (headers + text body) that feeds search,
/// candidate extraction, and the security scanners. Parsing is fully local
/// and deterministic — no sidecar, no model.
const MAIL_PARSER_VERSION: &str = "0.11";
const MSG_PARSER_VERSION: &str = "0.3";
const MAX_BODY_CHARS: usize = 400_000;
const MAX_IMAGES: usize = 5;
const MAX_IMAGE_BYTES: usize = 4 * 1024 * 1024;

/// An image embedded in or attached to the email. Bytes are carried in
/// memory: the importer persists them beside the original in `_sources/`
/// and hands them to the vision enrichment turn.
#[derive(Debug)]
pub struct EmailImage {
    pub file_name: String,
    /// Content-ID for inline images, used to rewrite `[cid:...]` markers.
    pub content_id: Option<String>,
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct EmailExtraction {
    /// Markdown snapshot: metadata header block followed by the body text.
    pub snapshot: String,
    pub images: Vec<EmailImage>,
    pub engine: &'static str,
    pub version: &'static str,
    pub warnings: Vec<String>,
}

/// Parse an RFC 5322 `.eml` file (any charset / transfer encoding).
pub fn extract_eml(bytes: &[u8]) -> AppResult<EmailExtraction> {
    let message = mail_parser::MessageParser::default()
        .parse(bytes)
        .ok_or_else(|| parse_error("the .eml file could not be parsed as an email message"))?;

    let subject = message.subject().unwrap_or("(no subject)").to_string();
    let from = format_addresses(message.from());
    let to = format_addresses(message.to());
    let cc = format_addresses(message.cc());
    let date = message
        .date()
        .map(|value| value.to_rfc3339())
        .unwrap_or_default();

    let mut warnings = Vec::new();
    // mail-parser transparently converts an HTML-only body to text in
    // body_text(); the reliable signal is the MIME type of the part it
    // selected as the text body.
    let html_only = message
        .text_body
        .first()
        .and_then(|part_id| message.part(*part_id))
        .is_some_and(|part| matches!(part.body, mail_parser::PartType::Html(_)));
    if html_only {
        warnings.push(
            "The email has no plain-text part; the HTML body was converted to text.".to_string(),
        );
    }
    let body = message
        .body_text(0)
        .map(|text| text.to_string())
        .or_else(|| {
            message
                .body_html(0)
                .map(|html| super::importer::html_to_text(&html))
        })
        .unwrap_or_default();

    let mut images = Vec::new();
    let mut skipped_attachments = 0usize;
    for part in message.attachments() {
        let is_image = part
            .content_type()
            .is_some_and(|ctype| ctype.c_type.eq_ignore_ascii_case("image"));
        if !is_image {
            skipped_attachments += 1;
            continue;
        }
        if images.len() == MAX_IMAGES {
            skipped_attachments += 1;
            warnings.push(format!(
                "Only the first {MAX_IMAGES} embedded images were kept."
            ));
            continue;
        }
        let bytes = part.contents().to_vec();
        if bytes.is_empty() || bytes.len() > MAX_IMAGE_BYTES {
            skipped_attachments += 1;
            warnings.push(
                "An embedded image was skipped because it is empty or exceeds the 4 MiB limit."
                    .to_string(),
            );
            continue;
        }
        let subtype = part
            .content_type()
            .and_then(|ctype| ctype.subtype())
            .unwrap_or("png")
            .to_ascii_lowercase();
        let fallback_name = format!("image-{}.{subtype}", images.len() + 1);
        let file_name = sanitize_image_name(
            part.attachment_name().unwrap_or(&fallback_name),
            &fallback_name,
        );
        images.push(EmailImage {
            file_name,
            content_id: part.content_id().map(str::to_string),
            bytes,
        });
    }
    if skipped_attachments > 0 {
        warnings.push(format!(
            "{skipped_attachments} non-image attachment(s) were not imported — import them individually if they matter."
        ));
    }

    build_extraction(
        &subject,
        &from,
        &to,
        &cc,
        &date,
        &body,
        images,
        "mail-parser",
        MAIL_PARSER_VERSION,
        warnings,
    )
}

/// Parse an Outlook `.msg` (OLE compound file, the Windows Outlook format).
pub fn extract_msg(bytes: &[u8]) -> AppResult<EmailExtraction> {
    let outlook = msg_parser::Outlook::from_slice(bytes)
        .map_err(|error| parse_error(format!("the .msg file could not be parsed: {error}")))?;

    let subject = if outlook.subject.trim().is_empty() {
        "(no subject)".to_string()
    } else {
        outlook.subject.clone()
    };
    let from = format_person(&outlook.sender.name, &outlook.sender.email);
    let to = outlook
        .to
        .iter()
        .map(|person| format_person(&person.name, &person.email))
        .collect::<Vec<_>>()
        .join(", ");
    let cc = outlook
        .cc
        .iter()
        .map(|person| format_person(&person.name, &person.email))
        .collect::<Vec<_>>()
        .join(", ");
    let date = outlook.headers.date.clone();

    let mut warnings = Vec::new();
    if !outlook.attachments.is_empty() {
        // The .msg parser does not expose reliable attachment payloads;
        // embedded images are only recovered from .eml files.
        warnings.push(format!(
            "{} attachment(s) were not imported — forward the mail as .eml to recover embedded images.",
            outlook.attachments.len()
        ));
    }

    build_extraction(
        &subject,
        &from,
        &to,
        &cc,
        &date,
        &outlook.body,
        Vec::new(),
        "msg-parser",
        MSG_PARSER_VERSION,
        warnings,
    )
}

#[allow(clippy::too_many_arguments)]
fn build_extraction(
    subject: &str,
    from: &str,
    to: &str,
    cc: &str,
    date: &str,
    body: &str,
    images: Vec<EmailImage>,
    engine: &'static str,
    version: &'static str,
    warnings: Vec<String>,
) -> AppResult<EmailExtraction> {
    let body = normalize_body(body);
    if body.is_empty() && subject == "(no subject)" {
        return Err(parse_error(
            "the email contains no readable subject or body text",
        ));
    }

    let mut snapshot = format!("# {}\n\n", subject.trim());
    if !from.is_empty() {
        snapshot.push_str(&format!("- **From:** {from}\n"));
    }
    if !to.is_empty() {
        snapshot.push_str(&format!("- **To:** {to}\n"));
    }
    if !cc.is_empty() {
        snapshot.push_str(&format!("- **Cc:** {cc}\n"));
    }
    if !date.is_empty() {
        snapshot.push_str(&format!("- **Date:** {}\n", date.trim()));
    }
    snapshot.push_str("\n---\n\n");
    snapshot.push_str(&body);

    Ok(EmailExtraction {
        snapshot,
        images,
        engine,
        version,
        warnings,
    })
}

fn sanitize_image_name(raw: &str, fallback: &str) -> String {
    let base = raw
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(fallback)
        .trim()
        .to_ascii_lowercase();
    let cleaned: String = base
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '-'
            }
        })
        .collect();
    let cleaned = cleaned.trim_matches(['-', '.']).to_string();
    if cleaned.is_empty() || !cleaned.contains('.') {
        fallback.to_string()
    } else {
        cleaned.chars().take(60).collect()
    }
}

fn normalize_body(value: &str) -> String {
    let normalized = value
        .replace("\r\n", "\n")
        .replace('\0', "")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string();
    normalized.chars().take(MAX_BODY_CHARS).collect()
}

fn format_addresses(header: Option<&mail_parser::Address<'_>>) -> String {
    header
        .map(|address| {
            address
                .iter()
                .map(|entry| {
                    format_person(
                        entry.name().unwrap_or_default(),
                        entry.address().unwrap_or_default(),
                    )
                })
                .filter(|value| !value.is_empty())
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default()
}

fn format_person(name: &str, email: &str) -> String {
    let name = name.trim();
    let email = email.trim();
    match (name.is_empty(), email.is_empty()) {
        (false, false) => format!("{name} <{email}>"),
        (false, true) => name.to_string(),
        (true, false) => email.to_string(),
        (true, true) => String::new(),
    }
}

fn parse_error(message: impl Into<String>) -> AppError {
    AppError::Io(std::io::Error::other(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_EML: &str = "From: Laura Bianchi <laura.bianchi@vendor.example>\r\n\
To: Fabio Chiodo <fabio@example.com>\r\n\
Subject: PowerReviews feed decision\r\n\
Date: Mon, 20 Jul 2026 09:12:00 +0000\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
Confirmed: we switch to the nightly delta feed from August.\r\n\
Full loads stay available as manual fallback only.\r\n";

    #[test]
    fn eml_extraction_builds_snapshot_with_headers_and_body() {
        let extraction = extract_eml(SAMPLE_EML.as_bytes()).unwrap();
        assert!(extraction.snapshot.starts_with("# PowerReviews feed decision"));
        assert!(extraction.snapshot.contains("Laura Bianchi <laura.bianchi@vendor.example>"));
        assert!(extraction.snapshot.contains("nightly delta feed from August"));
        assert_eq!(extraction.engine, "mail-parser");
        assert!(extraction.warnings.is_empty());
    }

    #[test]
    fn eml_with_html_only_body_converts_to_text_with_warning() {
        let eml = "From: a@example.com\r\nTo: b@example.com\r\nSubject: HTML mail\r\n\
Content-Type: text/html; charset=utf-8\r\n\r\n\
<html><body><p>Decision: keep the <b>legacy widget</b> until Q4.</p></body></html>\r\n";
        let extraction = extract_eml(eml.as_bytes()).unwrap();
        assert!(extraction.snapshot.contains("legacy widget"));
        assert!(!extraction.snapshot.contains("<b>"));
        assert!(extraction
            .warnings
            .iter()
            .any(|warning| warning.contains("HTML body")));
    }

    #[test]
    fn invalid_bytes_are_rejected() {
        assert!(extract_msg(b"not an ole file").is_err());
    }

    pub(crate) const SAMPLE_EML_WITH_IMAGE: &str = "From: a@example.com\r\n\
To: b@example.com\r\n\
Subject: Org chart\r\n\
Content-Type: multipart/related; boundary=\"BOUND\"\r\n\
\r\n\
--BOUND\r\n\
Content-Type: text/plain; charset=utf-8\r\n\
\r\n\
The new support model structure is below.\r\n\
[cid:image002.png@01DCD62A.1FEB7F50]\r\n\
--BOUND\r\n\
Content-Type: image/png; name=\"image002.png\"\r\n\
Content-Transfer-Encoding: base64\r\n\
Content-ID: <image002.png@01DCD62A.1FEB7F50>\r\n\
Content-Disposition: inline; filename=\"image002.png\"\r\n\
\r\n\
iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==\r\n\
--BOUND--\r\n";

    #[test]
    fn embedded_inline_image_is_extracted_with_content_id() {
        let extraction = extract_eml(SAMPLE_EML_WITH_IMAGE.as_bytes()).unwrap();
        assert_eq!(extraction.images.len(), 1);
        let image = &extraction.images[0];
        assert_eq!(image.file_name, "image002.png");
        assert_eq!(
            image.content_id.as_deref(),
            Some("image002.png@01DCD62A.1FEB7F50")
        );
        assert!(image.bytes.starts_with(&[0x89, b'P', b'N', b'G']));
        assert!(extraction.snapshot.contains("[cid:image002.png@01DCD62A.1FEB7F50]"));
    }

}

/// Shared fixture for the importer integration test in `mod.rs`.
#[cfg(test)]
pub(crate) fn sample_eml_with_image() -> &'static str {
    tests::SAMPLE_EML_WITH_IMAGE
}
