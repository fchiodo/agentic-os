use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use lopdf::Document;
use sha2::{Digest, Sha256};

use super::errors::{ConverterError, ConverterResult};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Classification {
    Image,
    DigitalPdf,
    OcrPdf,
    HybridCandidate,
}

#[derive(Debug, Clone)]
pub struct Inspection {
    pub classification: Classification,
    pub page_count: u32,
    pub extracted_text: Option<String>,
    pub warnings: Vec<String>,
}

pub fn hash_file(path: &Path) -> ConverterResult<String> {
    let file = File::open(path)
        .map_err(|_| ConverterError::new("UNSUPPORTED_FILE", "Source file cannot be read"))?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 1024 * 1024];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn inspect(path: &Path) -> ConverterResult<Inspection> {
    match path
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
    {
        Some(extension) if matches!(extension.as_str(), "png" | "jpg" | "jpeg") => Ok(Inspection {
            classification: Classification::Image,
            page_count: 1,
            extracted_text: None,
            warnings: Vec::new(),
        }),
        Some(extension) if extension == "pdf" => inspect_pdf(path),
        _ => Err(ConverterError::new(
            "UNSUPPORTED_FILE",
            "Supported formats are PDF, PNG, JPG, and JPEG",
        )),
    }
}

fn inspect_pdf(path: &Path) -> ConverterResult<Inspection> {
    let document = Document::load(path).map_err(|error| {
        let message = error.to_string().to_ascii_lowercase();
        if message.contains("encrypt") || message.contains("password") {
            ConverterError::new(
                "ENCRYPTED_PDF",
                "This PDF is password protected and cannot currently be converted",
            )
        } else {
            ConverterError::new("INVALID_PDF", "This PDF is invalid or corrupted")
        }
    })?;
    if document.is_encrypted() {
        return Err(ConverterError::new(
            "ENCRYPTED_PDF",
            "This PDF is password protected and cannot currently be converted",
        ));
    }
    let page_count = document.get_pages().len() as u32;
    if page_count == 0 {
        return Err(ConverterError::new("INVALID_PDF", "PDF contains no pages"));
    }
    if page_count > 2_000 {
        return Err(ConverterError::new(
            "RESOURCE_LIMIT_EXCEEDED",
            "PDF exceeds the 2,000 page safety limit",
        ));
    }

    let mut warnings = Vec::new();
    if page_count >= 300 {
        warnings.push(format!(
            "Large document: {page_count} pages. Conversion may take several minutes."
        ));
    }
    let mut page_texts = pdf_extract::extract_text_by_pages(path).unwrap_or_default();
    page_texts.resize(page_count as usize, String::new());
    let page_metrics = page_texts
        .iter()
        .map(|text| text_quality(text))
        .collect::<Vec<_>>();
    let good_pages = page_metrics
        .iter()
        .filter(|metrics| metrics.is_machine_readable())
        .count();
    let poor_pages = page_metrics
        .iter()
        .filter(|metrics| metrics.needs_ocr())
        .count();
    let classification = if good_pages == page_count as usize {
        Classification::DigitalPdf
    } else if poor_pages == page_count as usize {
        Classification::OcrPdf
    } else {
        Classification::HybridCandidate
    };
    if poor_pages > 0 && good_pages > 0 {
        warnings.push(format!(
            "Mixed document: {good_pages} page(s) have usable embedded text and {poor_pages} page(s) need OCR."
        ));
    }
    let extracted_text = page_texts.join("\u{c}");

    Ok(Inspection {
        classification,
        page_count,
        extracted_text: (!extracted_text.trim().is_empty()).then_some(extracted_text),
        warnings,
    })
}

#[derive(Debug, Clone, Copy)]
struct TextQuality {
    characters: usize,
    alphanumeric_ratio: f64,
    replacement_ratio: f64,
}

impl TextQuality {
    fn is_machine_readable(self) -> bool {
        self.characters >= 80
            && self.alphanumeric_ratio >= 0.45
            && self.replacement_ratio < 0.01
    }

    fn needs_ocr(self) -> bool {
        self.characters < 15
            || self.alphanumeric_ratio < 0.30
            || self.replacement_ratio >= 0.01
    }
}

fn text_quality(text: &str) -> TextQuality {
    let characters = text.chars().count();
    let total = characters.max(1) as f64;
    let alphanumeric = text
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count() as f64;
    let replacements = text.chars().filter(|character| *character == '�').count() as f64;
    TextQuality {
        characters,
        alphanumeric_ratio: alphanumeric / total,
        replacement_ratio: replacements / total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_requires_more_than_nonempty_text() {
        let poor = text_quality("x");
        assert!(poor.needs_ocr());
        let corrupted = text_quality(&"�".repeat(200));
        assert!(corrupted.replacement_ratio > 0.9);
        let good = text_quality(&"Annual revenue increased by eight percent. ".repeat(20));
        assert!(good.is_machine_readable());
    }

    #[test]
    fn classifies_synthetic_digital_scanned_and_mixed_pdfs() {
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures/ocr");
        assert_eq!(
            inspect(&fixtures.join("digital-10-pages.pdf"))
                .unwrap()
                .classification,
            Classification::DigitalPdf
        );
        assert_eq!(
            inspect(&fixtures.join("scanned-10-pages.pdf"))
                .unwrap()
                .classification,
            Classification::OcrPdf
        );
        assert_eq!(
            inspect(&fixtures.join("mixed.pdf"))
                .unwrap()
                .classification,
            Classification::HybridCandidate
        );
    }

    #[test]
    fn streams_source_hash() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("source.png");
        std::fs::write(&path, b"abc").unwrap();
        assert_eq!(
            hash_file(&path).unwrap(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
