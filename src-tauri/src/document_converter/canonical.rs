use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Component, Path, PathBuf};

use uuid::Uuid;

use super::errors::{ConverterError, ConverterResult};
use super::types::{
    CanonicalBlock, CanonicalDocument, CanonicalPage, EngineConversion, ProcessingSummary,
    SourceMetadata, OUTPUT_SCHEMA_VERSION,
};

pub fn sanitize_stem(name: &str) -> String {
    let stem = Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("document");
    let mut result = String::with_capacity(stem.len().min(96));
    let mut previous_separator = false;
    for character in stem.chars().take(96) {
        let safe = character.is_alphanumeric() || matches!(character, '-' | '_' | ' ');
        if safe {
            result.push(character);
            previous_separator = false;
        } else if !previous_separator {
            result.push('-');
            previous_separator = true;
        }
    }
    let result = result.trim_matches([' ', '-', '.', '_']).trim().to_string();
    if result.is_empty() {
        "document".to_string()
    } else {
        result
    }
}

pub fn safe_relative_path(value: &str) -> ConverterResult<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(ConverterError::new(
            "INVALID_PATH",
            "Asset path is outside the conversion package",
        ));
    }
    Ok(path.to_path_buf())
}

pub fn next_available_output(root: &Path, stem: &str) -> ConverterResult<PathBuf> {
    if !root.is_dir() || root.is_symlink() {
        return Err(ConverterError::new(
            "OUTPUT_WRITE_FAILED",
            "Output destination is not a writable directory",
        ));
    }
    for suffix in 1..=10_000u32 {
        let candidate_name = if suffix == 1 {
            stem.to_string()
        } else {
            format!("{stem}-{suffix}")
        };
        let candidate = root.join(candidate_name);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(ConverterError::new(
        "OUTPUT_WRITE_FAILED",
        "Could not allocate a non-conflicting output directory",
    ))
}

pub fn preflight_destination(root: &Path) -> ConverterResult<()> {
    let metadata = fs::symlink_metadata(root).map_err(|_| {
        ConverterError::new("OUTPUT_WRITE_FAILED", "Output destination does not exist")
    })?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err(ConverterError::new(
            "OUTPUT_WRITE_FAILED",
            "Output destination must be a real directory",
        ));
    }
    let probe = root.join(format!(".agentic-os-write-test-{}", Uuid::new_v4()));
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
        .map_err(|_| {
            ConverterError::new("OUTPUT_WRITE_FAILED", "Output destination is not writable")
        })?;
    fs::remove_file(probe)?;
    Ok(())
}

pub fn build_document(
    source: SourceMetadata,
    fingerprint: String,
    engine: &str,
    engine_version: &str,
    model_version: &str,
    requested_mode: &str,
    conversion: EngineConversion,
) -> CanonicalDocument {
    let mut pages = Vec::with_capacity(conversion.pages.len());
    for raw_page in conversion.pages {
        let mut blocks = parse_blocks(raw_page.page_number, &raw_page.text, raw_page.confidence);
        if let Some(asset_ref) = raw_page.rendered_asset_path {
            let order = blocks.len() as u32;
            blocks.push(CanonicalBlock {
                id: format!("page-{:03}-asset-001", raw_page.page_number),
                block_type: "image".to_string(),
                page: raw_page.page_number,
                order,
                text: Some(format!("Page {} source image", raw_page.page_number)),
                level: None,
                bbox: None,
                confidence: None,
                asset_ref: Some(asset_ref),
                metadata: BTreeMap::new(),
            });
        }
        pages.push(CanonicalPage {
            number: raw_page.page_number,
            source_mode: raw_page.source_mode,
            rotation_correction: raw_page.rotation_correction,
            blocks,
        });
    }

    CanonicalDocument {
        schema_version: OUTPUT_SCHEMA_VERSION,
        agentic_os_version: env!("CARGO_PKG_VERSION").to_string(),
        conversion_timestamp: chrono::Utc::now().to_rfc3339(),
        conversion_fingerprint: fingerprint,
        source,
        engine: engine.to_string(),
        engine_version: engine_version.to_string(),
        model_version: model_version.to_string(),
        processing: ProcessingSummary {
            requested_mode: requested_mode.to_string(),
            actual_mode: conversion.processing_mode,
            pages_total: conversion.total_pages,
            pages_digital: conversion.pages_digital,
            pages_ocr: conversion.pages_ocr,
        },
        pages,
        warnings: conversion.warnings,
        metadata: BTreeMap::new(),
    }
}

pub fn render_markdown(document: &CanonicalDocument) -> String {
    let mut output = String::new();
    for page in &document.pages {
        for block in &page.blocks {
            let text = block.text.as_deref().unwrap_or("").trim();
            match block.block_type.as_str() {
                "heading" => {
                    let level = block.level.unwrap_or(2).clamp(1, 6);
                    output.push_str(&"#".repeat(level as usize));
                    output.push(' ');
                    output.push_str(text);
                    output.push_str("\n\n");
                }
                "list-item" => {
                    output.push_str("- ");
                    output.push_str(text.trim_start_matches(['•', '-', '*', ' ']));
                    output.push('\n');
                }
                "numbered-item" => {
                    output.push_str(text);
                    output.push('\n');
                }
                "table" | "code" | "formula" => {
                    output.push_str(text);
                    output.push_str("\n\n");
                }
                "image" => {
                    if let Some(asset_ref) = &block.asset_ref {
                        output.push_str(&format!("![{}]({})\n\n", text, asset_ref));
                    }
                }
                _ => {
                    if !text.is_empty() {
                        output.push_str(text);
                        output.push_str("\n\n");
                    }
                }
            }
        }
    }
    output.trim_end().to_string() + "\n"
}

fn parse_blocks(page: u32, text: &str, confidence: Option<f64>) -> Vec<CanonicalBlock> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut index = 0usize;
    let mut order = 0u32;
    let mut paragraph = Vec::new();

    let flush_paragraph =
        |blocks: &mut Vec<CanonicalBlock>, paragraph: &mut Vec<&str>, order: &mut u32| {
            if paragraph.is_empty() {
                return;
            }
            let value = paragraph.join("\n").trim().to_string();
            paragraph.clear();
            if value.is_empty() {
                return;
            }
            let kind = if *order == 0 && value.len() <= 120 && !value.ends_with(['.', '!', '?']) {
                "heading"
            } else {
                "paragraph"
            };
            blocks.push(block(
                page,
                *order,
                kind,
                value,
                confidence,
                (kind == "heading").then_some(1),
            ));
            *order += 1;
        };

    while index < lines.len() {
        let line = lines[index].trim_end();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            flush_paragraph(&mut blocks, &mut paragraph, &mut order);
            index += 1;
            continue;
        }
        if trimmed.starts_with("```") {
            flush_paragraph(&mut blocks, &mut paragraph, &mut order);
            let mut value = vec![line];
            index += 1;
            while index < lines.len() {
                value.push(lines[index]);
                let done = lines[index].trim().starts_with("```");
                index += 1;
                if done {
                    break;
                }
            }
            blocks.push(block(
                page,
                order,
                "code",
                value.join("\n"),
                confidence,
                None,
            ));
            order += 1;
            continue;
        }
        if trimmed.starts_with("$$") || (trimmed.starts_with("\\[") && trimmed.ends_with("\\]")) {
            flush_paragraph(&mut blocks, &mut paragraph, &mut order);
            let mut value = vec![line];
            if !trimmed.ends_with("$$") || trimmed.len() == 2 {
                index += 1;
                while index < lines.len() {
                    value.push(lines[index]);
                    let done = lines[index].trim().ends_with("$$");
                    index += 1;
                    if done {
                        break;
                    }
                }
            } else {
                index += 1;
            }
            blocks.push(block(
                page,
                order,
                "formula",
                value.join("\n"),
                confidence,
                None,
            ));
            order += 1;
            continue;
        }
        if trimmed.starts_with('#') {
            flush_paragraph(&mut blocks, &mut paragraph, &mut order);
            let level = trimmed
                .chars()
                .take_while(|character| *character == '#')
                .count()
                .clamp(1, 6) as u8;
            let value = trimmed[level as usize..].trim().to_string();
            blocks.push(block(
                page,
                order,
                "heading",
                value,
                confidence,
                Some(level),
            ));
            order += 1;
            index += 1;
            continue;
        }
        if trimmed.starts_with(['-', '*', '•']) {
            flush_paragraph(&mut blocks, &mut paragraph, &mut order);
            blocks.push(block(
                page,
                order,
                "list-item",
                trimmed.to_string(),
                confidence,
                None,
            ));
            order += 1;
            index += 1;
            continue;
        }
        if is_numbered_item(trimmed) {
            flush_paragraph(&mut blocks, &mut paragraph, &mut order);
            blocks.push(block(
                page,
                order,
                "numbered-item",
                trimmed.to_string(),
                confidence,
                None,
            ));
            order += 1;
            index += 1;
            continue;
        }
        if trimmed.contains('|') && index + 1 < lines.len() && lines[index + 1].contains('|') {
            flush_paragraph(&mut blocks, &mut paragraph, &mut order);
            let mut table_lines = Vec::new();
            while index < lines.len()
                && lines[index].contains('|')
                && !lines[index].trim().is_empty()
            {
                table_lines.push(lines[index].trim());
                index += 1;
            }
            blocks.push(block(
                page,
                order,
                "table",
                table_lines.join("\n"),
                confidence,
                None,
            ));
            order += 1;
            continue;
        }
        paragraph.push(line);
        index += 1;
    }
    flush_paragraph(&mut blocks, &mut paragraph, &mut order);
    blocks
}

fn block(
    page: u32,
    order: u32,
    kind: &str,
    text: String,
    confidence: Option<f64>,
    level: Option<u8>,
) -> CanonicalBlock {
    CanonicalBlock {
        id: format!("page-{page:03}-block-{order:04}"),
        block_type: kind.to_string(),
        page,
        order,
        text: Some(text),
        level,
        bbox: None,
        confidence,
        asset_ref: None,
        metadata: BTreeMap::new(),
    }
}

fn is_numbered_item(line: &str) -> bool {
    let prefix = line.split_whitespace().next().unwrap_or("");
    prefix.ends_with('.')
        && prefix[..prefix.len().saturating_sub(1)]
            .chars()
            .all(|value| value.is_ascii_digit())
}

pub fn write_document_files(
    package_root: &Path,
    stem: &str,
    document: &CanonicalDocument,
) -> ConverterResult<(PathBuf, PathBuf)> {
    fs::create_dir_all(package_root)?;
    let markdown_path = package_root.join(format!("{stem}.md"));
    let json_path = package_root.join("document.json");
    write_atomic_file(&markdown_path, render_markdown(document).as_bytes())?;
    let json = serde_json::to_vec_pretty(document)?;
    write_atomic_file(&json_path, &json)?;
    Ok((markdown_path, json_path))
}

fn write_atomic_file(path: &Path, bytes: &[u8]) -> ConverterResult<()> {
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let file = File::create(&temporary)?;
    let mut writer = BufWriter::new(file);
    writer.write_all(bytes)?;
    writer.flush()?;
    writer.get_ref().sync_all()?;
    drop(writer);
    fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_hostile_filenames() {
        assert_eq!(sanitize_stem("../../Annual:Report?.pdf"), "Annual-Report");
        assert_eq!(sanitize_stem("..."), "document");
    }

    #[test]
    fn rejects_non_relative_asset_paths() {
        assert!(safe_relative_path("../secret").is_err());
        assert!(safe_relative_path("/tmp/secret").is_err());
        assert!(safe_relative_path("assets/page-001.png").is_ok());
    }

    #[test]
    fn collision_handling_is_non_destructive() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join("Report")).unwrap();
        fs::create_dir(temp.path().join("Report-2")).unwrap();
        assert_eq!(
            next_available_output(temp.path(), "Report").unwrap(),
            temp.path().join("Report-3")
        );
    }

    #[test]
    fn parses_table_and_formula_blocks() {
        let blocks = parse_blocks(
            1,
            "# Report\n\n| A | B |\n| --- | --- |\n| 1 | 2 |\n\n$$E=mc^2$$",
            None,
        );
        assert_eq!(blocks[0].block_type, "heading");
        assert_eq!(blocks[1].block_type, "table");
        assert_eq!(blocks[2].block_type, "formula");
    }
}
