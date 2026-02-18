use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

pub fn dialog_tag(
    files: &[String],
    on_select: impl FnMut(String) + 'static,
) -> DialogSelect<String> {
    let items = files
        .iter()
        .map(|file| {
            let mut item = DialogSelectItem::new(file.clone(), file.clone());
            item.category = Some("Files".to_string());
            item
        })
        .collect();

    DialogSelect::new("Tag File", items)
        .with_placeholder("Search files")
        .on_select(on_select)
}
