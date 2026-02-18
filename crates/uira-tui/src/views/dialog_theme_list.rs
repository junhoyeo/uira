use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

pub fn dialog_theme_list(
    themes: &[&str],
    current_theme: Option<&str>,
    on_select: impl FnMut(String) + 'static,
) -> DialogSelect<String> {
    let mut items = Vec::new();
    for theme in themes {
        let mut item = DialogSelectItem::new(*theme, (*theme).to_string());
        item.category = Some("Themes".to_string());
        if current_theme.is_some_and(|current| current == *theme) {
            item.footer = Some("current".to_string());
        }
        items.push(item);
    }

    DialogSelect::new("Theme", items)
        .with_placeholder("Filter themes")
        .with_footer_hint("Up/Down preview | Enter apply | Esc close")
        .on_select(on_select)
}
