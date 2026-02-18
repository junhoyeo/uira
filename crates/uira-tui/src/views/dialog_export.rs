use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

#[derive(Clone, Debug)]
pub enum ExportFormat {
    Markdown,
    Json,
    PlainText,
}

pub fn dialog_export(on_select: impl FnMut(ExportFormat) + 'static) -> DialogSelect<ExportFormat> {
    let mut markdown = DialogSelectItem::new("Markdown", ExportFormat::Markdown);
    markdown.category = Some("Export format".to_string());
    markdown.footer = Some(".md".to_string());

    let mut json = DialogSelectItem::new("JSON", ExportFormat::Json);
    json.category = Some("Export format".to_string());
    json.footer = Some(".json".to_string());

    let mut plain = DialogSelectItem::new("Plain text", ExportFormat::PlainText);
    plain.category = Some("Export format".to_string());
    plain.footer = Some(".txt".to_string());

    DialogSelect::new("Export", vec![markdown, json, plain])
        .with_placeholder("Choose export type")
        .on_select(on_select)
}
