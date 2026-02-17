use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

pub fn dialog_agent(
    agents: &[&str],
    on_select: impl FnMut(String) + 'static,
) -> DialogSelect<String> {
    let items = agents
        .iter()
        .map(|agent| {
            let mut item = DialogSelectItem::new(*agent, (*agent).to_string());
            item.category = Some("Agents".to_string());
            item
        })
        .collect();

    DialogSelect::new("Agent", items)
        .with_placeholder("Search agents")
        .on_select(on_select)
}
