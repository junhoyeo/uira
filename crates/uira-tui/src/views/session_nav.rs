use std::collections::HashMap;

use ratatui::{
    prelude::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::widgets::chat::ChatMessage;

#[derive(Debug, Clone)]
pub struct SessionView {
    pub session_id: String,
    pub agent_name: Option<String>,
    pub model: Option<String>,
    pub completed: Option<bool>,
    pub messages: Vec<ChatMessage>,
    pub parent_session_id: Option<String>,
    pub child_session_ids: Vec<String>,
}

impl SessionView {
    pub fn root() -> Self {
        Self {
            session_id: "root".to_string(),
            agent_name: None,
            model: None,
            completed: None,
            messages: Vec::new(),
            parent_session_id: None,
            child_session_ids: Vec::new(),
        }
    }

    pub fn child(
        session_id: String,
        agent_name: String,
        model: Option<String>,
        parent_id: String,
    ) -> Self {
        Self {
            session_id,
            agent_name: Some(agent_name),
            model,
            completed: None,
            messages: Vec::new(),
            parent_session_id: Some(parent_id),
            child_session_ids: Vec::new(),
        }
    }
}

#[derive(Debug)]
pub struct SessionStack {
    sessions: HashMap<String, SessionView>,
    stack: Vec<String>,
}

impl SessionStack {
    pub fn new() -> Self {
        let root = SessionView::root();
        let root_id = root.session_id.clone();
        let mut sessions = HashMap::new();
        sessions.insert(root_id.clone(), root);
        Self {
            sessions,
            stack: vec![root_id],
        }
    }

    pub fn register_child(&mut self, session: SessionView) {
        let child_id = session.session_id.clone();
        if let Some(parent_id) = &session.parent_session_id {
            if let Some(parent) = self.sessions.get_mut(parent_id) {
                if !parent.child_session_ids.contains(&child_id) {
                    parent.child_session_ids.push(child_id.clone());
                }
            }
        }
        self.sessions.insert(child_id, session);
    }

    pub fn push_session(&mut self, session_id: &str) -> bool {
        if self.sessions.contains_key(session_id) {
            self.stack.push(session_id.to_string());
            true
        } else {
            false
        }
    }

    pub fn pop_session(&mut self) -> bool {
        if self.stack.len() > 1 {
            self.stack.pop();
            true
        } else {
            false
        }
    }

    pub fn current(&self) -> Option<&SessionView> {
        self.stack.last().and_then(|id| self.sessions.get(id))
    }

    pub fn current_mut(&mut self) -> Option<&mut SessionView> {
        let id = self.stack.last()?.clone();
        self.sessions.get_mut(&id)
    }

    pub fn current_id(&self) -> &str {
        self.stack.last().map(String::as_str).unwrap_or("root")
    }

    pub fn is_in_child(&self) -> bool {
        self.stack.len() > 1
    }

    pub fn depth(&self) -> usize {
        self.stack.len().saturating_sub(1)
    }

    pub fn sibling_ids(&self) -> Vec<String> {
        if let Some(current) = self.current() {
            if let Some(parent_id) = &current.parent_session_id {
                if let Some(parent) = self.sessions.get(parent_id) {
                    return parent.child_session_ids.clone();
                }
            }
        }
        Vec::new()
    }

    pub fn next_sibling(&mut self) -> bool {
        let siblings = self.sibling_ids();
        if siblings.len() <= 1 {
            return false;
        }
        let current_id = self.current_id().to_string();
        if let Some(idx) = siblings.iter().position(|s| s == &current_id) {
            let next_idx = (idx + 1) % siblings.len();
            if let Some(last) = self.stack.last_mut() {
                *last = siblings[next_idx].clone();
                return true;
            }
        }
        false
    }

    pub fn prev_sibling(&mut self) -> bool {
        let siblings = self.sibling_ids();
        if siblings.len() <= 1 {
            return false;
        }
        let current_id = self.current_id().to_string();
        if let Some(idx) = siblings.iter().position(|s| s == &current_id) {
            let prev_idx = if idx == 0 {
                siblings.len() - 1
            } else {
                idx - 1
            };
            if let Some(last) = self.stack.last_mut() {
                *last = siblings[prev_idx].clone();
                return true;
            }
        }
        false
    }

    pub fn get_session(&self, session_id: &str) -> Option<&SessionView> {
        self.sessions.get(session_id)
    }

    pub fn get_session_mut(&mut self, session_id: &str) -> Option<&mut SessionView> {
        self.sessions.get_mut(session_id)
    }

    pub fn mark_completed(&mut self, session_id: &str, success: bool) -> bool {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.completed = Some(success);
            true
        } else {
            false
        }
    }

    pub fn push_message_to(&mut self, session_id: &str, msg: ChatMessage) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.messages.push(msg);
        }
    }
}

pub fn render_session_header(stack: &SessionStack, theme_accent: Color) -> Option<Line<'static>> {
    if !stack.is_in_child() {
        return None;
    }

    let current = stack.current()?;
    let agent = current.agent_name.as_deref().unwrap_or("unknown");
    let model = current.model.as_deref().unwrap_or("");
    let depth = stack.depth();

    let mut spans = vec![
        Span::styled(
            "← Parent",
            Style::default()
                .fg(theme_accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(" (Esc)  ", Style::default().fg(Color::DarkGray)),
    ];

    let siblings = stack.sibling_ids();
    if siblings.len() > 1 {
        spans.push(Span::styled("‹ Prev", Style::default().fg(theme_accent)));
        spans.push(Span::styled(" ([)  ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled("Next ›", Style::default().fg(theme_accent)));
        spans.push(Span::styled(" (])  ", Style::default().fg(Color::DarkGray)));
    }

    let mut info = agent.to_string();
    if !model.is_empty() {
        info.push_str(" · ");
        info.push_str(model);
    }
    info.push_str(&format!(" · depth:{}", depth));
    spans.push(Span::styled(
        info,
        Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD),
    ));

    Some(Line::from(spans))
}

impl Default for SessionStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_always_present() {
        let stack = SessionStack::new();
        assert!(!stack.is_in_child());
        assert_eq!(stack.depth(), 0);
        assert!(stack.current().is_some());
    }

    #[test]
    fn test_root_cannot_be_popped() {
        let mut stack = SessionStack::new();
        assert!(!stack.pop_session());
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_push_pop_navigation() {
        let mut stack = SessionStack::new();
        let child = SessionView::child("child1".into(), "explore".into(), None, "root".into());
        stack.register_child(child);

        assert!(stack.push_session("child1"));
        assert!(stack.is_in_child());
        assert_eq!(stack.depth(), 1);

        assert!(stack.pop_session());
        assert!(!stack.is_in_child());
        assert_eq!(stack.depth(), 0);
    }

    #[test]
    fn test_sibling_navigation() {
        let mut stack = SessionStack::new();
        stack.register_child(SessionView::child(
            "c1".into(),
            "explore".into(),
            None,
            "root".into(),
        ));
        stack.register_child(SessionView::child(
            "c2".into(),
            "architect".into(),
            None,
            "root".into(),
        ));

        stack.push_session("c1");
        assert!(stack.next_sibling());
        assert_eq!(stack.current_id(), "c2");

        assert!(stack.prev_sibling());
        assert_eq!(stack.current_id(), "c1");
    }
}
