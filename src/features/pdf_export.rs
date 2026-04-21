use chrono::Local;
use tracing::info;

/// PDF report export: combine screenshots + annotations + OCR text into a PDF.
///
/// Uses a simple SVG-to-PDF approach (generate SVG, convert via browser or tool).
/// For a lightweight approach, we generate an HTML report and use browser print-to-PDF.
pub struct PdfReport {
    title: String,
    entries: Vec<ReportEntry>,
}

struct ReportEntry {
    screenshot_path: String,
    caption: String,
    ocr_text: Option<String>,
}

impl PdfReport {
    pub fn new(title: &str) -> Self {
        Self { title: title.to_string(), entries: Vec::new() }
    }

    pub fn add_screenshot(&mut self, path: &str, caption: &str, ocr_text: Option<&str>) {
        self.entries.push(ReportEntry {
            screenshot_path: path.to_string(),
            caption: caption.to_string(),
            ocr_text: ocr_text.map(|s| s.to_string()),
        });
    }

    /// Generate HTML report (can be printed to PDF from browser).
    pub fn generate_html(&self) -> String {
        let mut html = format!(r#"<!DOCTYPE html>
<html><head><meta charset="utf-8"><title>{}</title>
<style>
body {{ font-family: sans-serif; max-width: 800px; margin: 0 auto; padding: 20px; }}
h1 {{ color: #333; border-bottom: 2px solid #007AFF; padding-bottom: 10px; }}
.entry {{ margin: 20px 0; page-break-inside: avoid; }}
.entry img {{ max-width: 100%; border: 1px solid #ddd; border-radius: 8px; }}
.caption {{ font-weight: bold; margin: 8px 0; }}
.ocr {{ background: #f5f5f5; padding: 12px; border-radius: 6px; font-family: monospace; white-space: pre-wrap; }}
.timestamp {{ color: #888; font-size: 12px; }}
</style></head><body>
<h1>{}</h1>
<p class="timestamp">Generated: {}</p>
"#, self.title, self.title, Local::now().format("%Y-%m-%d %H:%M:%S"));

        for (i, entry) in self.entries.iter().enumerate() {
            html.push_str(&format!(r#"
<div class="entry">
<h3>#{} — {}</h3>
<img src="{}" alt="Screenshot">
"#, i + 1, entry.caption, entry.screenshot_path));

            if let Some(ref text) = entry.ocr_text {
                html.push_str(&format!(r#"<div class="ocr">{}</div>"#, text));
            }
            html.push_str("</div>\n");
        }

        html.push_str("</body></html>");
        html
    }

    /// Save HTML report to file.
    pub fn save(&self, path: &str) -> Result<String, String> {
        let html = self.generate_html();
        std::fs::write(path, &html).map_err(|e| e.to_string())?;
        info!(path = %path, entries = self.entries.len(), "PDF report (HTML) saved");
        Ok(path.to_string())
    }
}
