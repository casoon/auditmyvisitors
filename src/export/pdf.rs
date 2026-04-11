/// PDF export — placeholder implementation.
/// Phase 4 will flesh this out with printpdf layout.
use anyhow::Context;

pub struct PdfExportOptions {
    pub output_path: String,
    pub title: String,
    pub property_name: String,
    pub date_range: String,
}

/// Generates a minimal PDF at the given path.
/// Phase 4 will add proper layout, tables, and insight sections.
pub fn export_pdf(options: PdfExportOptions, content: &str) -> anyhow::Result<()> {
    use printpdf::*;

    let (doc, page1, layer1) = PdfDocument::new(
        &options.title,
        Mm(210.0),
        Mm(297.0),
        "Layer 1",
    );

    let current_layer = doc.get_page(page1).get_layer(layer1);

    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .context("Cannot load PDF font")?;

    current_layer.use_text(&options.title, 18.0, Mm(20.0), Mm(277.0), &font);
    current_layer.use_text(&options.property_name, 12.0, Mm(20.0), Mm(267.0), &font);
    current_layer.use_text(&options.date_range, 10.0, Mm(20.0), Mm(260.0), &font);

    let mut y = 245.0f32;
    for line in content.lines().take(60) {
        if y < 20.0 {
            break;
        }
        current_layer.use_text(line, 9.0, Mm(20.0), Mm(y), &font);
        y -= 6.0;
    }

    let bytes = doc.save_to_bytes().context("Cannot render PDF")?;
    std::fs::write(&options.output_path, bytes)
        .with_context(|| format!("Cannot write PDF to {}", options.output_path))?;

    Ok(())
}
