use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

#[derive(Clone, Debug)]
pub enum SubagentAction {
    FollowLogs,
    StopTask,
    OpenSession,
}

pub fn dialog_subagent(
    on_select: impl FnMut(SubagentAction) + 'static,
) -> DialogSelect<SubagentAction> {
    let mut follow = DialogSelectItem::new("Follow logs", SubagentAction::FollowLogs);
    follow.category = Some("Subagent".to_string());

    let mut stop = DialogSelectItem::new("Stop task", SubagentAction::StopTask);
    stop.category = Some("Subagent".to_string());

    let mut open = DialogSelectItem::new("Open session", SubagentAction::OpenSession);
    open.category = Some("Subagent".to_string());

    DialogSelect::new("Subagent", vec![follow, stop, open]).on_select(on_select)
}
