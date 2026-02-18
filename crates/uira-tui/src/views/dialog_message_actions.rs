use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

#[derive(Clone, Debug)]
pub enum MessageAction {
    Revert,
    Copy,
    Fork,
    Edit,
}

pub fn dialog_message_actions(
    on_select: impl FnMut(MessageAction) + 'static,
) -> DialogSelect<MessageAction> {
    let mut revert = DialogSelectItem::new("Revert to this message", MessageAction::Revert);
    revert.category = Some("Message actions".to_string());

    let mut copy = DialogSelectItem::new("Copy message", MessageAction::Copy);
    copy.category = Some("Message actions".to_string());

    let mut fork = DialogSelectItem::new("Fork from this message", MessageAction::Fork);
    fork.category = Some("Message actions".to_string());

    let mut edit = DialogSelectItem::new("Edit and resend", MessageAction::Edit);
    edit.category = Some("Message actions".to_string());

    DialogSelect::new("Message Actions", vec![revert, copy, fork, edit]).on_select(on_select)
}
