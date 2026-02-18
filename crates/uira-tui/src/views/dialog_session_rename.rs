use crate::widgets::dialog::DialogPrompt;

pub fn dialog_session_rename(
    current_name: impl Into<String>,
    on_submit: impl FnMut(Option<String>) + 'static,
) -> DialogPrompt {
    DialogPrompt::new("Rename Session")
        .with_placeholder("Session name")
        .with_value(current_name)
        .with_validator(|value| {
            if value.trim().is_empty() {
                return Err("Session name cannot be empty".to_string());
            }
            Ok(())
        })
        .on_submit(on_submit)
}
