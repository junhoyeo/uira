use std::time::{Duration, SystemTime};

use crate::widgets::dialog::{DialogSelect, DialogSelectItem};

#[derive(Clone, Debug)]
pub struct SessionEntry {
    pub id: String,
    pub title: String,
    pub updated_at: SystemTime,
}

pub fn dialog_session_list(
    sessions: Vec<SessionEntry>,
    on_select: impl FnMut(String) + 'static,
) -> DialogSelect<String> {
    let now = SystemTime::now();
    let mut items = Vec::new();
    for session in sessions {
        let mut item = DialogSelectItem::new(session.title, session.id);
        item.category = Some(session_category(now, session.updated_at));
        items.push(item);
    }
    DialogSelect::new("Sessions", items)
        .with_placeholder("Search sessions")
        .on_select(on_select)
}

fn session_category(now: SystemTime, timestamp: SystemTime) -> String {
    let elapsed = now.duration_since(timestamp).unwrap_or(Duration::ZERO);
    if elapsed < Duration::from_secs(24 * 60 * 60) {
        "Today".to_string()
    } else if elapsed < Duration::from_secs(7 * 24 * 60 * 60) {
        "This week".to_string()
    } else if elapsed < Duration::from_secs(30 * 24 * 60 * 60) {
        "This month".to_string()
    } else {
        "Older".to_string()
    }
}
