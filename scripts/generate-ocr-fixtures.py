#!/usr/bin/env python3
"""Generate small, non-sensitive OCR fixtures and their benchmark manifest."""

from __future__ import annotations

import json
from pathlib import Path

from PIL import Image, ImageDraw, ImageFont
from reportlab.lib import colors
from reportlab.lib.pagesizes import A4
from reportlab.lib.styles import getSampleStyleSheet
from reportlab.lib.units import mm
from reportlab.platypus import PageBreak, Paragraph, SimpleDocTemplate, Spacer, Table, TableStyle


ROOT = Path(__file__).resolve().parents[1]
OUTPUT = ROOT / "tests" / "fixtures" / "ocr"
WIDTH, HEIGHT = A4


def footer(canvas, document) -> None:
    canvas.saveState()
    canvas.setFont("Helvetica", 8)
    canvas.setFillColor(colors.HexColor("#666666"))
    canvas.drawRightString(WIDTH - 18 * mm, 12 * mm, f"Synthetic fixture - page {document.page}")
    canvas.restoreState()


def digital_pdf(path: Path, pages: int = 10) -> None:
    styles = getSampleStyleSheet()
    story = []
    for page in range(1, pages + 1):
        story.extend(
            [
                Paragraph(f"Synthetic Annual Report - Section {page}", styles["Title"]),
                Spacer(1, 8 * mm),
                Paragraph(
                    "This page contains machine-readable English and Italian text. "
                    "Revenue increased by eight percent. I documenti restano sul Mac e non vengono caricati online.",
                    styles["BodyText"],
                ),
                Spacer(1, 4 * mm),
                Paragraph("Key points", styles["Heading2"]),
                Paragraph("- Reproducible local processing<br/>- Deterministic output<br/>- Verified model files", styles["BodyText"]),
            ]
        )
        if page < pages:
            story.append(PageBreak())
    SimpleDocTemplate(str(path), pagesize=A4, leftMargin=18 * mm, rightMargin=18 * mm).build(
        story, onFirstPage=footer, onLaterPages=footer
    )


def scan_image(path: Path, page: int, rotation: int = 0) -> None:
    image = Image.new("RGB", (1240, 1754), "white")
    draw = ImageDraw.Draw(image)
    font = ImageFont.load_default(size=34)
    small = ImageFont.load_default(size=25)
    draw.rectangle((80, 80, 1160, 1670), outline="#c7c1b5", width=3)
    draw.text((120, 130), f"SCANNED INVOICE {page:02d}", fill="#171715", font=font)
    draw.text((120, 230), "Local OCR fixture / documento sintetico", fill="#464641", font=small)
    draw.text((120, 330), "Description                 Qty        Amount", fill="#171715", font=small)
    draw.line((120, 375, 1080, 375), fill="#77776f", width=2)
    draw.text((120, 420), "Document processing          1         EUR 42.00", fill="#171715", font=small)
    draw.text((120, 500), "Model integrity check        1         EUR 18.00", fill="#171715", font=small)
    draw.text((120, 650), "TOTAL                                  EUR 60.00", fill="#171715", font=font)
    draw.text((120, 820), "Formula: E = mc^2", fill="#171715", font=small)
    draw.text((120, 930), "Notes:", fill="#171715", font=small)
    draw.text((150, 990), "- English recognition", fill="#171715", font=small)
    draw.text((150, 1040), "- Riconoscimento italiano", fill="#171715", font=small)
    if rotation:
        image = image.rotate(rotation, expand=True, fillcolor="white")
    image.save(path, "PNG", optimize=True)


def scanned_pdf(path: Path, pages: int = 10) -> None:
    images = []
    for page in range(1, pages + 1):
        image_path = OUTPUT / f".scan-{page:02d}.png"
        scan_image(image_path, page)
        images.append(Image.open(image_path).convert("RGB"))
    images[0].save(path, "PDF", save_all=True, append_images=images[1:], resolution=150)
    for image in images:
        image.close()
    for source in OUTPUT.glob(".scan-*.png"):
        source.unlink()


def table_pdf(path: Path) -> None:
    styles = getSampleStyleSheet()
    data = [["Region", "Revenue", "Growth"], ["EMEA", "EUR 2.4B", "8%"], ["Americas", "USD 3.1B", "6%"], ["APAC", "USD 1.7B", "11%"]]
    table = Table(data, colWidths=[70 * mm, 45 * mm, 35 * mm], repeatRows=1)
    table.setStyle(TableStyle([
        ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#ded8ca")),
        ("GRID", (0, 0), (-1, -1), 0.5, colors.HexColor("#444440")),
        ("ALIGN", (1, 1), (-1, -1), "RIGHT"),
        ("FONTNAME", (0, 0), (-1, 0), "Helvetica-Bold"),
        ("PADDING", (0, 0), (-1, -1), 8),
    ]))
    story = [Paragraph("Regional performance", styles["Title"]), Spacer(1, 8 * mm), table, PageBreak(), Paragraph("Continued table notes", styles["Heading1"]), Paragraph("EMEA values are reported in euros. Narrow columns and numeric alignment are intentional.", styles["BodyText"])]
    SimpleDocTemplate(str(path), pagesize=A4, leftMargin=18 * mm, rightMargin=18 * mm).build(story, onFirstPage=footer, onLaterPages=footer)


def columns_pdf(path: Path) -> None:
    from reportlab.pdfgen import canvas

    pdf = canvas.Canvas(str(path), pagesize=A4)
    for page in range(1, 3):
        pdf.setFont("Helvetica-Bold", 18)
        pdf.drawString(18 * mm, HEIGHT - 22 * mm, f"Two-column research note {page}")
        pdf.setFont("Helvetica", 10)
        left = "Left column. Reading order starts here. Local processing protects private documents. " * 9
        right = "Right column. Reading order continues here. Canonical Markdown preserves content. " * 9
        for column, text in enumerate((left, right)):
            frame_x = 18 * mm + column * 90 * mm
            text_object = pdf.beginText(frame_x, HEIGHT - 36 * mm)
            text_object.setLeading(14)
            words = text.split()
            line = ""
            for word in words:
                if len(line) + len(word) > 46:
                    text_object.textLine(line)
                    line = word
                else:
                    line = f"{line} {word}".strip()
            if line:
                text_object.textLine(line)
            pdf.drawText(text_object)
        pdf.setFont("Helvetica", 8)
        pdf.drawRightString(WIDTH - 18 * mm, 12 * mm, f"Synthetic fixture - page {page}")
        pdf.showPage()
    pdf.save()


def mixed_pdf(path: Path) -> None:
    digital = OUTPUT / ".mixed-digital.pdf"
    scanned = OUTPUT / ".mixed-scan.pdf"
    digital_pdf(digital, 1)
    scanned_pdf(scanned, 1)
    from pypdf import PdfReader, PdfWriter

    writer = PdfWriter()
    for source in (digital, scanned):
        for page in PdfReader(source).pages:
            writer.add_page(page)
    with path.open("wb") as stream:
        writer.write(stream)
    digital.unlink()
    scanned.unlink()


def main() -> None:
    OUTPUT.mkdir(parents=True, exist_ok=True)
    digital_pdf(OUTPUT / "digital-10-pages.pdf")
    scanned_pdf(OUTPUT / "scanned-10-pages.pdf")
    table_pdf(OUTPUT / "table.pdf")
    columns_pdf(OUTPUT / "columns.pdf")
    mixed_pdf(OUTPUT / "mixed.pdf")
    scan_image(OUTPUT / "image.png", 1, rotation=0)
    manifest = {
        "schemaVersion": 1,
        "synthetic": True,
        "fixtures": [
            {"id": "A", "path": "digital-10-pages.pdf", "expectedMode": "digital", "pages": 10},
            {"id": "B", "path": "scanned-10-pages.pdf", "expectedMode": "ocr", "pages": 10},
            {"id": "C", "path": "table.pdf", "expectedMode": "digital", "pages": 2},
            {"id": "D", "path": "columns.pdf", "expectedMode": "digital", "pages": 2},
            {"id": "E", "path": "image.png", "expectedMode": "ocr", "pages": 1},
            {"id": "mixed", "path": "mixed.pdf", "expectedMode": "hybrid", "pages": 2},
        ],
    }
    (OUTPUT / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
