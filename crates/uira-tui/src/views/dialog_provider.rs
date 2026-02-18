use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

#[derive(Clone, Debug)]
pub struct ProviderOption {
    pub id: String,
    pub title: String,
    pub configured: bool,
}

pub fn dialog_provider(
    providers: Vec<ProviderOption>,
    on_select: impl FnMut(String) + 'static,
) -> DialogSelect<String> {
    let mut items = Vec::new();
    for provider in providers {
        let mut item = DialogSelectItem::new(provider.title, provider.id);
        item.category = Some("Provider".to_string());
        item.footer = Some(if provider.configured {
            "connected".to_string()
        } else {
            "setup".to_string()
        });
        items.push(item);
    }

    DialogSelect::new("Connect Provider", items)
        .with_placeholder("Search providers")
        .with_footer_hint("Up/Down navigate | Enter connect | Esc close")
        .on_select(on_select)
}
