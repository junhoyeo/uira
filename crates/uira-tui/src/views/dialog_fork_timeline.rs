use crate::widgets::dialog::DialogConfirm;

pub fn dialog_fork_timeline(
    target_index: usize,
    on_submit: impl FnMut(bool) + 'static,
) -> DialogConfirm {
    DialogConfirm::new(
        "Fork Session",
        format!("Create a new branch from message #{}?", target_index),
    )
    .on_submit(on_submit)
}
