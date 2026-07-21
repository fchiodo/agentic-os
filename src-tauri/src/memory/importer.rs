use std::collections::{HashMap, HashSet};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use reqwest::header::{CONTENT_LENGTH, CONTENT_TYPE, LOCATION};
use reqwest::{redirect::Policy, Url};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use uuid::Uuid;

use crate::db::Db;
use crate::error::{AppError, AppResult};

use super::{
    DocumentImportRecord, DocumentImportRequest, DocumentImportResult, DocumentSourceReadResult,
    ExtractedMemoryCandidate, ManualSaveRequest, MemoryIngestFailure,
};

const MAX_DOCUMENT_BYTES: usize = 2 * 1024 * 1024;
const MAX_CANDIDATES: usize = 10;
const MAX_HISTORY_ROWS: i64 = 100;

struct AcquiredDocument {
    raw: String,
    extraction_text: String,
    source_ref: String,
    warnings: Vec<String>,
}

#[derive(Serialize)]
struct SourceFrontmatter<'a> {
    id: &'a str,
    kind: &'static str,
    domain: &'a str,
    title: &'a str,
    input_kind: &'a str,
    source_ref: &'a str,
    captured_at: &'a str,
    content_hash: &'a str,
    trust: &'static str,
}

#[derive(Debug)]
struct RankedClaim {
    heading: String,
    text: String,
    score: i32,
    explicit_decision: bool,
}

pub async fn import_document(
    db: &Db,
    request: &DocumentImportRequest,
) -> AppResult<DocumentImportResult> {
    super::index::ensure_tables(db)?;
    validate_request(request)?;

    let mut acquired = acquire(request).await?;
    if acquired.raw.as_bytes().len() > MAX_DOCUMENT_BYTES {
        return Err(io_error("document exceeds the 2 MiB import limit"));
    }
    if acquired.raw.trim().is_empty() {
        return Err(io_error("document is empty"));
    }
    if acquired.raw.contains('\0') {
        return Err(io_error("document contains unsupported NUL bytes"));
    }
    if super::pipeline::contains_secrets(&acquired.raw) {
        audit_import_reject(db, &request.title, "secrets detected");
        return Err(io_error(
            "document import rejected: a credential or secret was detected; redact it before importing",
        ));
    }
    if super::pipeline::contains_prompt_injection(&acquired.raw) {
        acquired.warnings.push(
            "Possible prompt-injection language detected. The original source was preserved as untrusted and extracted facts still require approval."
                .to_string(),
        );
    }

    let import_id = Uuid::new_v4().to_string();
    let created_at = chrono::Utc::now().to_rfc3339();
    let source_path = format!(
        "_sources/{}/{}-{}-{}.md",
        request.domain,
        chrono::Utc::now().format("%Y-%m-%d"),
        slugify(&request.title),
        &import_id[..8]
    );
    let content_hash = crate::audit::compute_content_hash(&acquired.raw);
    let snapshot = serialize_snapshot(
        &import_id,
        request,
        &acquired.source_ref,
        &created_at,
        &content_hash,
        &acquired.raw,
    )?;

    persist_source(
        db,
        &import_id,
        request,
        &acquired.source_ref,
        &source_path,
        &content_hash,
        acquired.raw.as_bytes().len() as i64,
        &created_at,
        &snapshot,
    )?;

    let candidates = extract_candidates(
        &request.title,
        &acquired.extraction_text,
        &source_path,
        &acquired.source_ref,
    );
    if candidates.is_empty() {
        acquired.warnings.push(
            "No sufficiently self-contained facts were found. The complete source is still available in the import history."
                .to_string(),
        );
    }

    let provenance = format!("document:{import_id}");
    let mut proposals = Vec::new();
    let mut rejected = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let save = ManualSaveRequest {
            domain: request.domain.clone(),
            mem_type: candidate.mem_type.clone(),
            title: candidate.title.clone(),
            body: candidate.body.clone(),
            tags: candidate.tags.clone(),
            sensitivity: candidate.sensitivity.clone(),
            source: Some(provenance.clone()),
            confidence: candidate.confidence,
            valid_from: candidate.valid_from.clone(),
            valid_until: candidate.valid_until.clone(),
            stale_after_days: candidate.stale_after_days,
            expires: candidate.expires.clone(),
            supersedes_id: candidate.supersedes_id.clone(),
        };
        match super::pipeline::process_import_candidate(db, &save, &provenance, &import_id) {
            Ok(proposal) => proposals.push(proposal),
            Err(error) => rejected.push(MemoryIngestFailure {
                index,
                title: candidate.title.clone(),
                error: error.to_string(),
            }),
        }
    }

    if !rejected.is_empty() {
        acquired.warnings.push(format!(
            "{} extracted candidate(s) did not pass the deterministic memory gate.",
            rejected.len()
        ));
    }
    update_import_after_extraction(db, &import_id, proposals.len() as i64, &acquired.warnings)?;

    let record = get(db, &import_id)?
        .ok_or_else(|| io_error("document import record vanished after creation"))?;
    Ok(DocumentImportResult {
        import: record,
        proposals,
        rejected,
        warnings: acquired.warnings,
    })
}

pub fn list(db: &Db, domain: Option<&str>) -> AppResult<Vec<DocumentImportRecord>> {
    super::index::ensure_tables(db)?;
    if let Some(domain) = domain {
        validate_domain(domain)?;
    }
    db.with_conn(|conn| {
        let mut records = Vec::new();
        if let Some(domain) = domain {
            let mut stmt = conn.prepare(
                "SELECT id, domain, title, input_kind, source_ref, source_path,
                        content_hash, byte_count, candidate_count, warning_count,
                        warnings_json, status, created_at, updated_at
                 FROM document_imports WHERE domain = ?1
                 ORDER BY created_at DESC LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![domain, MAX_HISTORY_ROWS], row_to_import)?;
            for row in rows {
                records.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, domain, title, input_kind, source_ref, source_path,
                        content_hash, byte_count, candidate_count, warning_count,
                        warnings_json, status, created_at, updated_at
                 FROM document_imports ORDER BY created_at DESC LIMIT ?1",
            )?;
            let rows = stmt.query_map(params![MAX_HISTORY_ROWS], row_to_import)?;
            for row in rows {
                records.push(row?);
            }
        }
        Ok(records)
    })
}

pub fn read_source(db: &Db, id: &str) -> AppResult<DocumentSourceReadResult> {
    let record = get(db, id)?.ok_or_else(|| io_error("document import not found"))?;
    let (snapshot, _) = super::vault::read_file(&record.source_path)?;
    let content = snapshot
        .split_once("\n---\n\n")
        .map(|(_, body)| body.to_string())
        .ok_or_else(|| io_error("document source snapshot is malformed"))?;
    Ok(DocumentSourceReadResult {
        git_last_commit: super::vault::git_last_commit(&record.source_path),
        import: record,
        content,
    })
}

pub(crate) fn refresh_status(db: &Db, import_id: &str) -> AppResult<()> {
    let (total, pending): (i64, i64) = db.with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*), SUM(CASE WHEN status = 'pending' THEN 1 ELSE 0 END)
             FROM memory_proposals WHERE import_id = ?1",
            params![import_id],
            |row| Ok((row.get(0)?, row.get::<_, Option<i64>>(1)?.unwrap_or(0))),
        )
        .map_err(Into::into)
    })?;
    let status = if total == 0 {
        "no_candidates"
    } else if pending == total {
        "pending"
    } else if pending > 0 {
        "partial"
    } else {
        "completed"
    };
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE document_imports SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status, chrono::Utc::now().to_rfc3339(), import_id],
        )?;
        Ok(())
    })
}

async fn acquire(request: &DocumentImportRequest) -> AppResult<AcquiredDocument> {
    match request.input_kind.as_str() {
        "text" => {
            let raw = request
                .content
                .clone()
                .ok_or_else(|| io_error("content is required for a text import"))?;
            Ok(AcquiredDocument {
                extraction_text: raw.clone(),
                raw,
                source_ref: "manual:pasted-text".to_string(),
                warnings: Vec::new(),
            })
        }
        "file" => {
            let raw = request
                .content
                .clone()
                .ok_or_else(|| io_error("content is required for a file import"))?;
            let file_name = request
                .file_name
                .as_deref()
                .map(clean_reference)
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| "uploaded-document".to_string());
            let is_html = file_name.ends_with(".html") || file_name.ends_with(".htm");
            let extraction_text = if is_html {
                html_to_text(&raw)
            } else {
                raw.clone()
            };
            Ok(AcquiredDocument {
                extraction_text,
                raw,
                source_ref: format!("file:{file_name}"),
                warnings: if is_html {
                    vec!["HTML markup was normalized only for extraction; the source snapshot preserves the original document.".to_string()]
                } else {
                    Vec::new()
                },
            })
        }
        "url" => {
            fetch_url(
                request
                    .source_url
                    .as_deref()
                    .ok_or_else(|| io_error("sourceUrl is required for a URL import"))?,
            )
            .await
        }
        _ => Err(io_error("inputKind must be text, file, or url")),
    }
}

async fn fetch_url(value: &str) -> AppResult<AcquiredDocument> {
    if value.trim().chars().count() > 2_048 {
        return Err(io_error("source URL exceeds 2048 characters"));
    }
    let url = Url::parse(value.trim()).map_err(|_| io_error("source URL is invalid"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(io_error("source URL must use http or https"));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(io_error("source URL must not contain credentials"));
    }
    let host = url
        .host_str()
        .ok_or_else(|| io_error("source URL has no host"))?
        .to_string();
    if host.eq_ignore_ascii_case("localhost") || host.ends_with(".localhost") {
        return Err(io_error("local and private URL targets are not allowed"));
    }
    let port = url
        .port_or_known_default()
        .ok_or_else(|| io_error("source URL has no valid port"))?;
    let addresses: Vec<SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|_| io_error("source URL host could not be resolved"))?
        .collect();
    let address = addresses
        .iter()
        .copied()
        .find(|address| is_public_ip(address.ip()))
        .ok_or_else(|| io_error("local and private URL targets are not allowed"))?;
    if addresses.iter().any(|address| !is_public_ip(address.ip())) {
        return Err(io_error(
            "source URL resolves to a private or reserved network address",
        ));
    }

    let mut builder = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(8))
        .timeout(std::time::Duration::from_secs(20))
        .redirect(Policy::none())
        .no_proxy()
        .user_agent("AgenticOS-DocumentImporter/1.0");
    if host.parse::<IpAddr>().is_err() {
        builder = builder.resolve(&host, address);
    }
    let client = builder
        .build()
        .map_err(|error| io_error(format!("could not create URL client: {error}")))?;
    let mut response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| io_error(format!("could not fetch source URL: {error}")))?;
    if response.status().is_redirection() {
        let location = response
            .headers()
            .get(LOCATION)
            .and_then(|value| value.to_str().ok())
            .unwrap_or("another URL");
        return Err(io_error(format!(
            "source URL redirects to {location}; import the final URL explicitly so its network target can be validated"
        )));
    }
    if !response.status().is_success() {
        return Err(io_error(format!(
            "source URL returned HTTP {}",
            response.status()
        )));
    }
    if response
        .headers()
        .get(CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .is_some_and(|length| length > MAX_DOCUMENT_BYTES)
    {
        return Err(io_error("remote document exceeds the 2 MiB import limit"));
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.is_empty() && !supported_content_type(&content_type) {
        return Err(io_error(format!(
            "unsupported remote content type: {content_type}"
        )));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| io_error(format!("failed reading source URL: {error}")))?
    {
        if bytes.len() + chunk.len() > MAX_DOCUMENT_BYTES {
            return Err(io_error("remote document exceeds the 2 MiB import limit"));
        }
        bytes.extend_from_slice(&chunk);
    }
    let raw = String::from_utf8(bytes)
        .map_err(|_| io_error("remote document is not valid UTF-8 text"))?;
    let is_html = content_type.contains("text/html");
    let mut warnings = Vec::new();
    if url.scheme() == "http" {
        warnings.push("The source was fetched over unencrypted HTTP.".to_string());
    }
    if content_type.is_empty() {
        warnings.push("The remote server did not provide a Content-Type header.".to_string());
    }
    if is_html {
        warnings.push("HTML markup was normalized only for extraction; the source snapshot preserves the original response.".to_string());
    }
    Ok(AcquiredDocument {
        extraction_text: if is_html {
            html_to_text(&raw)
        } else {
            raw.clone()
        },
        raw,
        source_ref: url.to_string(),
        warnings,
    })
}

fn validate_request(request: &DocumentImportRequest) -> AppResult<()> {
    validate_domain(&request.domain)?;
    let title = request.title.trim();
    if title.is_empty() || title.chars().count() > 200 {
        return Err(io_error("title must be between 1 and 200 characters"));
    }
    if !matches!(request.input_kind.as_str(), "text" | "file" | "url") {
        return Err(io_error("inputKind must be text, file, or url"));
    }
    if matches!(request.input_kind.as_str(), "text" | "file")
        && request
            .content
            .as_deref()
            .map_or(true, |content| content.trim().is_empty())
    {
        return Err(io_error("content is required for text and file imports"));
    }
    if request.input_kind == "url"
        && request
            .source_url
            .as_deref()
            .map_or(true, |url| url.trim().is_empty())
    {
        return Err(io_error("sourceUrl is required for a URL import"));
    }
    Ok(())
}

fn validate_domain(domain: &str) -> AppResult<()> {
    if matches!(
        domain,
        "work" | "planphysique" | "personal" | "family" | "finance" | "research"
    ) {
        Ok(())
    } else {
        Err(io_error("invalid memory domain"))
    }
}

#[allow(clippy::too_many_arguments)]
fn persist_source(
    db: &Db,
    id: &str,
    request: &DocumentImportRequest,
    source_ref: &str,
    source_path: &str,
    content_hash: &str,
    byte_count: i64,
    created_at: &str,
    snapshot: &str,
) -> AppResult<()> {
    let _guard = super::vault::lock_writes();
    super::vault::ensure_vault()?;
    if super::vault::file_exists(source_path)? {
        return Err(io_error("document source path already exists"));
    }
    super::vault::write_file_atomic(source_path, snapshot)?;
    if let Err(error) =
        super::vault::git_commit(&format!("mem({}): import source {}", request.domain, id))
    {
        let _ = super::vault::remove_file(source_path);
        return Err(error);
    }

    let db_result = (|| -> AppResult<()> {
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO document_imports (
                    id, domain, title, input_kind, source_ref, source_path,
                    content_hash, byte_count, candidate_count, warning_count,
                    warnings_json, status, created_at, updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,0,'[]','pending',?9,?9)",
                params![
                    id,
                    request.domain,
                    request.title.trim(),
                    request.input_kind,
                    source_ref,
                    source_path,
                    content_hash,
                    byte_count,
                    created_at,
                ],
            )?;
            Ok(())
        })?;
        crate::audit::append_row(
            db,
            &format!("document-import:{id}"),
            id,
            "document_import",
            "Document source imported",
            &serde_json::json!({
                "importId": id,
                "domain": request.domain,
                "inputKind": request.input_kind,
                "sourceRef": source_ref,
                "sourcePath": source_path,
                "contentHash": content_hash,
                "byteCount": byte_count,
            }),
            None,
            None,
        )
    })();
    if let Err(error) = db_result {
        let _ = db.with_conn(|conn| {
            conn.execute("DELETE FROM document_imports WHERE id = ?1", params![id])?;
            Ok(())
        });
        let _ = super::vault::remove_file(source_path);
        let _ = super::vault::git_commit(&format!("mem({}): rollback import {id}", request.domain));
        return Err(error);
    }
    Ok(())
}

fn update_import_after_extraction(
    db: &Db,
    import_id: &str,
    candidate_count: i64,
    warnings: &[String],
) -> AppResult<()> {
    let status = if candidate_count == 0 {
        "no_candidates"
    } else {
        "pending"
    };
    let warnings_json = serde_json::to_string(warnings)?;
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE document_imports
             SET candidate_count = ?1, warning_count = ?2, warnings_json = ?3,
                 status = ?4, updated_at = ?5
             WHERE id = ?6",
            params![
                candidate_count,
                warnings.len() as i64,
                warnings_json,
                status,
                chrono::Utc::now().to_rfc3339(),
                import_id,
            ],
        )?;
        Ok(())
    })
}

fn get(db: &Db, id: &str) -> AppResult<Option<DocumentImportRecord>> {
    super::index::ensure_tables(db)?;
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT id, domain, title, input_kind, source_ref, source_path,
                    content_hash, byte_count, candidate_count, warning_count,
                    warnings_json, status, created_at, updated_at
             FROM document_imports WHERE id = ?1",
            params![id],
            row_to_import,
        )
        .optional()
        .map_err(Into::into)
    })
}

fn row_to_import(row: &rusqlite::Row) -> rusqlite::Result<DocumentImportRecord> {
    let warnings_json: String = row.get(10)?;
    Ok(DocumentImportRecord {
        id: row.get(0)?,
        domain: row.get(1)?,
        title: row.get(2)?,
        input_kind: row.get(3)?,
        source_ref: row.get(4)?,
        source_path: row.get(5)?,
        content_hash: row.get(6)?,
        byte_count: row.get(7)?,
        candidate_count: row.get(8)?,
        warning_count: row.get(9)?,
        warnings: serde_json::from_str(&warnings_json).unwrap_or_default(),
        status: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn serialize_snapshot(
    id: &str,
    request: &DocumentImportRequest,
    source_ref: &str,
    captured_at: &str,
    content_hash: &str,
    body: &str,
) -> AppResult<String> {
    let yaml = serde_yaml::to_string(&SourceFrontmatter {
        id,
        kind: "document-source",
        domain: &request.domain,
        title: request.title.trim(),
        input_kind: &request.input_kind,
        source_ref,
        captured_at,
        content_hash,
        trust: "untrusted",
    })
    .map_err(|error| io_error(format!("could not serialize source metadata: {error}")))?;
    Ok(format!("---\n{yaml}---\n\n{body}"))
}

fn extract_candidates(
    document_title: &str,
    input: &str,
    source_path: &str,
    source_ref: &str,
) -> Vec<ExtractedMemoryCandidate> {
    let claims = ranked_claims(input);
    let mut selected = Vec::new();
    let mut heading_counts: HashMap<String, usize> = HashMap::new();
    let mut fingerprints: Vec<HashSet<String>> = Vec::new();
    let mut titles = HashSet::new();
    for claim in claims {
        let heading_key = claim.heading.to_ascii_lowercase();
        if heading_counts.get(&heading_key).copied().unwrap_or(0) >= 2 {
            continue;
        }
        let fingerprint = word_set(&claim.text);
        if fingerprints
            .iter()
            .any(|existing| jaccard(existing, &fingerprint) >= 0.72)
        {
            continue;
        }
        let mut memory_title = candidate_title(document_title, &claim.heading, &claim.text);
        if !titles.insert(memory_title.to_ascii_lowercase()) {
            memory_title = format!(
                "{} ({})",
                memory_title.chars().take(190).collect::<String>(),
                selected.len() + 1
            );
            titles.insert(memory_title.to_ascii_lowercase());
        }
        let link = source_path.trim_end_matches(".md");
        let compact_source_ref = source_ref.chars().take(300).collect::<String>();
        let body = format!(
            "{}\n\nSource: [[{}|source document]] ({}).",
            claim.text.trim(),
            link,
            compact_source_ref
        );
        if body.chars().count() > 1200 {
            continue;
        }
        let tags = candidate_tags(document_title);
        selected.push(ExtractedMemoryCandidate {
            mem_type: if claim.explicit_decision {
                "decision".to_string()
            } else {
                "fact".to_string()
            },
            title: memory_title,
            body,
            tags,
            sensitivity: Some("normal".to_string()),
            confidence: Some(if claim.score >= 55 { 0.84 } else { 0.76 }),
            valid_from: None,
            valid_until: None,
            stale_after_days: Some(180),
            expires: None,
            supersedes_id: None,
        });
        *heading_counts.entry(heading_key).or_insert(0) += 1;
        fingerprints.push(fingerprint);
        if selected.len() == MAX_CANDIDATES {
            break;
        }
    }
    selected
}

fn ranked_claims(input: &str) -> Vec<RankedClaim> {
    let mut claims = Vec::new();
    let mut heading = "Overview".to_string();
    let mut paragraph = String::new();
    let mut in_fence = false;
    let mut in_code_component = false;

    let flush = |paragraph: &mut String, heading: &str, claims: &mut Vec<RankedClaim>| {
        let clean = clean_markdown(paragraph);
        for sentence in split_sentences(&clean) {
            if let Some(claim) = score_claim(heading, &sentence) {
                claims.push(claim);
            }
        }
        paragraph.clear();
    };

    for raw_line in input.replace("\r\n", "\n").lines() {
        let line = raw_line.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            flush(&mut paragraph, &heading, &mut claims);
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if line.starts_with("<CodeBlock") {
            flush(&mut paragraph, &heading, &mut claims);
            in_code_component = true;
            continue;
        }
        if in_code_component {
            if line == "/>" || line.ends_with("/>") {
                in_code_component = false;
            }
            continue;
        }
        if let Some(value) = line.strip_prefix('#') {
            flush(&mut paragraph, &heading, &mut claims);
            heading = value.trim_start_matches('#').trim().to_string();
            continue;
        }
        if line.is_empty() {
            flush(&mut paragraph, &heading, &mut claims);
            continue;
        }
        if line.starts_with("import ")
            || line.starts_with("export ")
            || line.starts_with("<APIResponseCodes")
            || line == "---"
        {
            continue;
        }
        if line.starts_with("- ") || line.starts_with("* ") || numbered_list_line(line) {
            flush(&mut paragraph, &heading, &mut claims);
            let item = line
                .trim_start_matches(['-', '*', ' '])
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')')
                .trim();
            let clean = clean_markdown(item);
            if let Some(claim) = score_claim(&heading, &clean) {
                claims.push(claim);
            }
            continue;
        }
        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(line);
    }
    flush(&mut paragraph, &heading, &mut claims);
    claims.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.text.cmp(&b.text)));
    claims
}

fn score_claim(heading: &str, text: &str) -> Option<RankedClaim> {
    let trimmed = text.trim();
    let char_count = trimmed.chars().count();
    if !(45..=1000).contains(&char_count) || trimmed.starts_with('{') {
        return None;
    }
    let lower = format!("{} {}", heading, trimmed).to_ascii_lowercase();
    let mut score = 10;
    for (needle, points) in [
        ("authentication", 30),
        ("oauth", 28),
        ("jwt", 28),
        ("compatibility date", 24),
        ("required", 18),
        ("requires", 18),
        ("must", 16),
        ("supports", 14),
        ("recommended", 14),
        ("deprecated", 18),
        ("only", 10),
        ("enable", 8),
        ("disable", 8),
        ("endpoint", 8),
        ("header", 8),
        ("identity", 10),
        ("conversation", 6),
        ("timeout", 8),
        ("idempotency", 16),
    ] {
        if lower.contains(needle) {
            score += points;
        }
    }
    if regex::Regex::new(r"\b20\d{2}-\d{2}-\d{2}\b")
        .expect("static date regex")
        .is_match(trimmed)
    {
        score += 12;
    }
    if trimmed.contains('`') || trimmed.contains("POST /") || trimmed.contains("GET /") {
        score += 8;
    }
    if lower.contains("example") || lower.contains("for example") {
        score -= 7;
    }
    if score < 16 {
        return None;
    }
    let decision_text = format!("{} {}", heading, trimmed).to_ascii_lowercase();
    let explicit_decision = heading.to_ascii_lowercase().contains("decision")
        || [
            "we decided",
            "it was decided",
            "approved decision",
            "we agreed",
        ]
        .iter()
        .any(|phrase| decision_text.contains(phrase));
    Some(RankedClaim {
        heading: heading.to_string(),
        text: trimmed.to_string(),
        score,
        explicit_decision,
    })
}

fn split_sentences(value: &str) -> Vec<String> {
    let value = value.trim();
    if value.chars().count() <= 320 {
        return vec![value.to_string()];
    }
    let chars: Vec<char> = value.chars().collect();
    let mut result = Vec::new();
    let mut start = 0usize;
    for index in 0..chars.len() {
        if matches!(chars[index], '.' | '!' | '?')
            && index + 1 < chars.len()
            && chars[index + 1].is_whitespace()
            && index.saturating_sub(start) >= 45
        {
            result.push(chars[start..=index].iter().collect::<String>());
            start = index + 1;
        }
    }
    if start < chars.len() {
        result.push(chars[start..].iter().collect::<String>());
    }
    result
        .into_iter()
        .map(|sentence| sentence.trim().to_string())
        .filter(|sentence| sentence.chars().count() >= 45)
        .collect()
}

fn clean_markdown(value: &str) -> String {
    let link = regex::Regex::new(r"\[([^\]]+)\]\([^\)]+\)").expect("static link regex");
    let html = regex::Regex::new(r"<[^>]+>").expect("static html regex");
    let spaces = regex::Regex::new(r"\s+").expect("static whitespace regex");
    let cleaned = link.replace_all(value, "$1");
    let cleaned = html.replace_all(&cleaned, " ");
    spaces
        .replace_all(
            cleaned.trim_matches(|c: char| c == '*' || c == '_' || c == '`'),
            " ",
        )
        .trim()
        .to_string()
}

fn html_to_text(value: &str) -> String {
    let without_scripts = regex::Regex::new(r"(?is)<(script|style)[^>]*>.*?</(script|style)>")
        .expect("static script regex")
        .replace_all(value, " ");
    let with_breaks = regex::Regex::new(r"(?i)</?(p|div|br|li|h[1-6]|section|article)[^>]*>")
        .expect("static block regex")
        .replace_all(&without_scripts, "\n");
    let text = regex::Regex::new(r"(?s)<[^>]+>")
        .expect("static tag regex")
        .replace_all(&with_breaks, " ");
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

fn supported_content_type(value: &str) -> bool {
    value.starts_with("text/")
        || value.contains("application/json")
        || value.contains("application/xml")
        || value.contains("application/yaml")
        || value.contains("application/x-yaml")
        || value.contains("application/markdown")
}

fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_unspecified()
        || ip == Ipv4Addr::BROADCAST
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 0)
        || (octets[0] == 192 && octets[1] == 0 && octets[2] == 2)
        || (octets[0] == 198 && (octets[1] == 18 || octets[1] == 19))
        || (octets[0] == 198 && octets[1] == 51 && octets[2] == 100)
        || (octets[0] == 203 && octets[1] == 0 && octets[2] == 113)
        || octets[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    if let Some(ipv4) = ip.to_ipv4() {
        return is_public_ipv4(ipv4);
    }
    let first = ip.segments()[0];
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || first & 0xfe00 == 0xfc00
        || first & 0xffc0 == 0xfe80
        || first == 0x2001 && ip.segments()[1] == 0x0db8)
}

fn numbered_list_line(value: &str) -> bool {
    let mut chars = value.chars();
    let mut has_digit = false;
    while chars.next().is_some_and(|value| {
        if value.is_ascii_digit() {
            has_digit = true;
            true
        } else {
            value == '.' || value == ')'
        }
    }) {}
    has_digit
        && value
            .find(['.', ')'])
            .is_some_and(|index| value[index + 1..].starts_with(' '))
}

fn word_set(value: &str) -> HashSet<String> {
    value
        .split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.chars().count() >= 3)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn jaccard(left: &HashSet<String>, right: &HashSet<String>) -> f64 {
    if left.is_empty() && right.is_empty() {
        return 1.0;
    }
    let intersection = left.intersection(right).count() as f64;
    let union = left.union(right).count() as f64;
    intersection / union
}

fn candidate_title(document_title: &str, heading: &str, claim: &str) -> String {
    let generic_heading = matches!(
        heading.to_ascii_lowercase().as_str(),
        "overview" | "introduction" | "details" | "notes"
    );
    let prefix = if generic_heading {
        document_title
    } else {
        heading
    };
    let subject = claim
        .split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join(" ")
        .trim_end_matches(['.', ',', ':', ';'])
        .to_string();
    let title = format!("{prefix}: {subject}");
    title.chars().take(200).collect()
}

fn candidate_tags(title: &str) -> Vec<String> {
    let mut tags = vec!["document-import".to_string()];
    for tag in slugify(title)
        .split('-')
        .filter(|tag| tag.len() >= 3)
        .take(4)
    {
        let tag = tag.to_string();
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

fn clean_reference(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(260)
        .collect::<String>()
        .trim()
        .to_string()
}

fn slugify(value: &str) -> String {
    let mut slug = String::new();
    let mut separator = false;
    for character in value.chars().flat_map(char::to_lowercase) {
        if character.is_ascii_alphanumeric() {
            slug.push(character);
            separator = false;
        } else if !separator && !slug.is_empty() {
            slug.push('-');
            separator = true;
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        "document".to_string()
    } else {
        slug.chars().take(80).collect()
    }
}

fn audit_import_reject(db: &Db, title: &str, reason: &str) {
    let _ = crate::audit::append_row(
        db,
        "document-import-gate",
        "document-import",
        "policy_decision",
        &format!("Document import rejected: {reason}"),
        &serde_json::json!({ "title": title, "reason": reason }),
        None,
        None,
    );
}

fn io_error(message: impl Into<String>) -> AppError {
    AppError::Io(std::io::Error::other(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn private_and_reserved_addresses_are_rejected() {
        assert!(!is_public_ip("127.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("10.0.0.1".parse().unwrap()));
        assert!(!is_public_ip("169.254.1.1".parse().unwrap()));
        assert!(!is_public_ip("::1".parse().unwrap()));
        assert!(is_public_ip("1.1.1.1".parse().unwrap()));
        assert!(is_public_ip("2606:4700:4700::1111".parse().unwrap()));
    }

    #[test]
    fn extraction_prioritizes_authentication_and_keeps_source_link() {
        let content = r#"
# Authentication

Headless API endpoints require authentication unless enforcement is disabled. Sierra supports API tokens and OAuth client credentials with short-lived JWT tokens.

Authentication can be tested before organization-wide enforcement by sending the X-Sierra-Force-Headless-API-Authorization header.

# Compatibility

All API requests are required to use Sierra-API-Compatibility-Date. The latest supported compatibility date is 2025-02-01.
"#;
        let candidates = extract_candidates(
            "Sierra Headless API",
            content,
            "_sources/work/sierra.md",
            "file:sierra.md",
        );
        assert!(!candidates.is_empty());
        assert!(candidates.iter().any(|candidate| {
            candidate.body.contains("OAuth client credentials")
                && candidate.body.contains("short-lived JWT tokens")
        }));
        assert!(candidates.iter().all(|candidate| candidate
            .body
            .contains("[[_sources/work/sierra|source document]]")));
        assert!(candidates.len() <= MAX_CANDIDATES);
    }

    #[test]
    fn documentation_placeholders_are_not_secrets() {
        assert!(!super::super::pipeline::contains_secrets(
            "Authorization: Bearer YOUR_API_TOKEN"
        ));
        assert!(super::super::pipeline::contains_secrets(
            "Authorization: Bearer real-token-value-1234567890"
        ));
    }
}
