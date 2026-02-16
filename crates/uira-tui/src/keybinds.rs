use crossterm::event::{KeyCode, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub modifiers: KeyModifiers,
}

impl KeyBinding {
    pub fn new(code: KeyCode, modifiers: KeyModifiers) -> Self {
        Self { code, modifiers }
    }

    pub fn matches(&self, code: KeyCode, modifiers: KeyModifiers) -> bool {
        self.code == code && self.modifiers == modifiers
    }
}

pub fn parse_keybind(input: &str) -> Option<KeyBinding> {
    let input = input.trim();
    if input.is_empty() {
        return None;
    }

    let parts: Vec<&str> = input.split('+').map(|s| s.trim()).collect();
    let mut modifiers = KeyModifiers::empty();
    let mut key_part = None;

    for part in parts {
        let lower = part.to_lowercase();
        match lower.as_str() {
            "ctrl" | "control" => modifiers |= KeyModifiers::CONTROL,
            "alt" => modifiers |= KeyModifiers::ALT,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            _ => {
                key_part = Some(part);
            }
        }
    }

    let key_part = key_part?;
    let code = parse_key_code(key_part)?;

    Some(KeyBinding::new(code, modifiers))
}

fn parse_key_code(s: &str) -> Option<KeyCode> {
    match s.to_lowercase().as_str() {
        "up" => Some(KeyCode::Up),
        "down" => Some(KeyCode::Down),
        "left" => Some(KeyCode::Left),
        "right" => Some(KeyCode::Right),
        "pageup" | "page_up" => Some(KeyCode::PageUp),
        "pagedown" | "page_down" => Some(KeyCode::PageDown),
        "home" => Some(KeyCode::Home),
        "end" => Some(KeyCode::End),
        "enter" | "return" => Some(KeyCode::Enter),
        "esc" | "escape" => Some(KeyCode::Esc),
        "tab" => Some(KeyCode::Tab),
        "backspace" => Some(KeyCode::Backspace),
        "delete" | "del" => Some(KeyCode::Delete),
        s if s.len() == 1 => {
            let ch = s.chars().next()?;
            Some(KeyCode::Char(ch))
        }
        _ => None,
    }
}

#[derive(Debug, Clone)]
pub struct KeybindConfig {
    pub scroll_up: Vec<KeyBinding>,
    pub scroll_down: Vec<KeyBinding>,
    pub page_up: Vec<KeyBinding>,
    pub page_down: Vec<KeyBinding>,
    pub command_palette: Vec<KeyBinding>,
    pub toggle_sidebar: Vec<KeyBinding>,
    pub collapse_tools: Vec<KeyBinding>,
    pub expand_tools: Vec<KeyBinding>,
}

impl Default for KeybindConfig {
    fn default() -> Self {
        Self {
            scroll_up: vec![KeyBinding::new(KeyCode::Char('k'), KeyModifiers::empty())],
            scroll_down: vec![KeyBinding::new(KeyCode::Char('j'), KeyModifiers::empty())],
            page_up: vec![KeyBinding::new(KeyCode::PageUp, KeyModifiers::empty())],
            page_down: vec![KeyBinding::new(KeyCode::PageDown, KeyModifiers::empty())],
            command_palette: vec![
                KeyBinding::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                KeyBinding::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            ],
            toggle_sidebar: vec![KeyBinding::new(KeyCode::Char('t'), KeyModifiers::empty())],
            collapse_tools: vec![KeyBinding::new(KeyCode::Char('o'), KeyModifiers::CONTROL)],
            expand_tools: vec![KeyBinding::new(
                KeyCode::Char('O'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            )],
        }
    }
}

impl KeybindConfig {
    pub fn from_config(config: &uira_core::config::schema::KeybindsConfig) -> Self {
        let mut keybinds = Self::default();

        if let Some(s) = &config.scroll_up {
            if let Some(kb) = parse_keybind(s) {
                keybinds.scroll_up = vec![kb];
            }
        }

        if let Some(s) = &config.scroll_down {
            if let Some(kb) = parse_keybind(s) {
                keybinds.scroll_down = vec![kb];
            }
        }

        if let Some(s) = &config.page_up {
            if let Some(kb) = parse_keybind(s) {
                keybinds.page_up = vec![kb];
            }
        }

        if let Some(s) = &config.page_down {
            if let Some(kb) = parse_keybind(s) {
                keybinds.page_down = vec![kb];
            }
        }

        if let Some(s) = &config.command_palette {
            if let Some(kb) = parse_keybind(s) {
                keybinds.command_palette = vec![kb];
            }
        }

        if let Some(s) = &config.toggle_sidebar {
            if let Some(kb) = parse_keybind(s) {
                keybinds.toggle_sidebar = vec![kb];
            }
        }

        if let Some(s) = &config.collapse_tools {
            if let Some(kb) = parse_keybind(s) {
                keybinds.collapse_tools = vec![kb];
            }
        }

        if let Some(s) = &config.expand_tools {
            if let Some(kb) = parse_keybind(s) {
                keybinds.expand_tools = vec![kb];
            }
        }

        keybinds
    }

    pub fn matches_any(bindings: &[KeyBinding], code: KeyCode, modifiers: KeyModifiers) -> bool {
        bindings.iter().any(|kb| kb.matches(code, modifiers))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_key() {
        let kb = parse_keybind("k").unwrap();
        assert_eq!(kb.code, KeyCode::Char('k'));
        assert_eq!(kb.modifiers, KeyModifiers::empty());
    }

    #[test]
    fn test_parse_ctrl_key() {
        let kb = parse_keybind("ctrl+p").unwrap();
        assert_eq!(kb.code, KeyCode::Char('p'));
        assert_eq!(kb.modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn test_parse_ctrl_shift_key() {
        let kb = parse_keybind("ctrl+shift+o").unwrap();
        assert_eq!(kb.code, KeyCode::Char('o'));
        assert_eq!(kb.modifiers, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    }

    #[test]
    fn test_parse_arrow_key() {
        let kb = parse_keybind("up").unwrap();
        assert_eq!(kb.code, KeyCode::Up);
        assert_eq!(kb.modifiers, KeyModifiers::empty());
    }

    #[test]
    fn test_parse_page_up() {
        let kb = parse_keybind("pageup").unwrap();
        assert_eq!(kb.code, KeyCode::PageUp);
        assert_eq!(kb.modifiers, KeyModifiers::empty());
    }

    #[test]
    fn test_parse_ctrl_u() {
        let kb = parse_keybind("ctrl+u").unwrap();
        assert_eq!(kb.code, KeyCode::Char('u'));
        assert_eq!(kb.modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn test_matches() {
        let kb = KeyBinding::new(KeyCode::Char('k'), KeyModifiers::empty());
        assert!(kb.matches(KeyCode::Char('k'), KeyModifiers::empty()));
        assert!(!kb.matches(KeyCode::Char('j'), KeyModifiers::empty()));
        assert!(!kb.matches(KeyCode::Char('k'), KeyModifiers::CONTROL));
    }
}
