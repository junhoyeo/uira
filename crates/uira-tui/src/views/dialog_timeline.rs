use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

#[derive(Clone, Debug)]
pub struct TimelineEntry {
    pub index: usize,
    pub summary: String,
}

pub fn dialog_timeline(
    entries: Vec<TimelineEntry>,
    on_select: impl FnMut(usize) + 'static,
) -> DialogSelect<usize> {
    let items = entries
        .into_iter()
        .map(|entry| {
            let mut item = DialogSelectItem::new(entry.summary, entry.index);
            item.category = Some("Timeline".to_string());
            item.footer = Some(format!("#{}", entry.index));
            item
        })
        .collect();

    DialogSelect::new("Jump to Message", items)
        .with_placeholder("Search timeline")
        .on_select(on_select)
}
