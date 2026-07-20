use serde_yaml::Value;

use super::{MemoryFrontmatter, MemoryType, Provenance, Sensitivity};

/// Parse YAML frontmatter from a markdown file. Expects content starting
/// with `---\n`. Returns (frontmatter, body_without_frontmatter).
pub fn parse(content: &str) -> Option<(MemoryFrontmatter, String)> {
    let content = content.strip_prefix("---\n").or_else(|| content.strip_prefix("---\r\n"))?;
    let end = content.find("\n---\n").or_else(|| content.find("\n---\r\n"))?;
    let yaml_str = &content[..end];
    let body = &content[end + 5..];

    let yaml: Value = serde_yaml::from_str(yaml_str).ok()?;
    let fm = yaml_to_frontmatter(&yaml)?;
    Some((fm, body.trim().to_string()))
}

/// Serialize a frontmatter block + body into a complete markdown file.
pub fn serialize(fm: &MemoryFrontmatter, body: &str) -> String {
    let mut out = String::from("---\n");
    // Manual serialization for precise control over the YAML output
    out.push_str(&format!("id: {}\n", fm.id));
    out.push_str(&format!("type: {}\n", fm.mem_type.as_str()));
    out.push_str(&format!("domain: {}\n", fm.domain));
    out.push_str(&format!("title: \"{}\"\n", escape_yaml(&fm.title)));
    out.push_str(&format!("created: {}\n", fm.created));
    out.push_str(&format!("updated: {}\n", fm.updated));
    out.push_str("provenance:\n");
    out.push_str(&format!("  source: {}\n", fm.provenance.source));
    out.push_str(&format!("  ts: {}\n", fm.provenance.ts));
    out.push_str(&format!("confidence: {}\n", fm.confidence));
    out.push_str(&format!("sensitivity: {}\n", fm.sensitivity.as_str()));
    if let Some(ref v) = fm.valid_from {
        out.push_str(&format!("valid_from: {}\n", v));
    }
    if let Some(ref v) = fm.valid_until {
        out.push_str(&format!("valid_until: {}\n", v));
    }
    if let Some(v) = fm.stale_after_days {
        out.push_str(&format!("stale_after_days: {}\n", v));
    }
    if let Some(ref v) = fm.last_confirmed {
        out.push_str(&format!("last_confirmed: {}\n", v));
    }
    if let Some(v) = fm.confirmations {
        out.push_str(&format!("confirmations: {}\n", v));
    }
    if let Some(ref v) = fm.expires {
        out.push_str(&format!("expires: {}\n", v));
    }
    if !fm.tags.is_empty() {
        out.push_str("tags:\n");
        for tag in &fm.tags {
            out.push_str(&format!("  - {}\n", tag));
        }
    }
    out.push_str("---\n\n");
    out.push_str(body);
    out
}

fn escape_yaml(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn yaml_to_frontmatter(yaml: &Value) -> Option<MemoryFrontmatter> {
    let obj = yaml.as_mapping()?;
    let id = obj.get("id")?.as_str()?.to_string();
    let mem_type = MemoryType::parse(obj.get("type")?.as_str()?)?;
    let domain = obj.get("domain")?.as_str()?.to_string();
    let title = obj.get("title")?.as_str()?.to_string();
    let created = obj.get("created")?.as_str()?.to_string();
    let updated = obj.get("updated")?.as_str()?.to_string();

    let prov = obj.get("provenance")?.as_mapping()?;
    let provenance = Provenance {
        source: prov.get("source")?.as_str()?.to_string(),
        ts: prov.get("ts")?.as_str()?.to_string(),
    };

    let confidence = obj
        .get("confidence")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.7);
    let sensitivity = Sensitivity::parse(
        obj.get("sensitivity")
            .and_then(|v| v.as_str())
            .unwrap_or("normal"),
    );

    let valid_from = obj
        .get("valid_from")
        .and_then(|v| v.as_str())
        .map(String::from);
    let valid_until = obj
        .get("valid_until")
        .and_then(|v| v.as_str())
        .map(String::from);
    let stale_after_days = obj
        .get("stale_after_days")
        .and_then(|v| v.as_i64());
    let last_confirmed = obj
        .get("last_confirmed")
        .and_then(|v| v.as_str())
        .map(String::from);
    let confirmations = obj
        .get("confirmations")
        .and_then(|v| v.as_i64());
    let expires = obj
        .get("expires")
        .and_then(|v| v.as_str())
        .map(String::from);

    let tags = obj
        .get("tags")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    Some(MemoryFrontmatter {
        id,
        mem_type,
        domain,
        title,
        created,
        updated,
        provenance,
        confidence,
        sensitivity,
        valid_from,
        valid_until,
        stale_after_days,
        last_confirmed,
        confirmations,
        expires,
        tags,
    })
}
