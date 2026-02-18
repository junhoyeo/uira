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
            "cmd" | "meta" => modifiers |= KeyModifiers::SUPER,
            _ => {
                if key_part.is_some() {
                    return None;
                }
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
    pub toggle_todos: Vec<KeyBinding>,
    pub collapse_tools: Vec<KeyBinding>,
    pub expand_tools: Vec<KeyBinding>,
}

impl Default for KeybindConfig {
    fn default() -> Self {
        Self {
            scroll_up: vec![],
            scroll_down: vec![],
            page_up: vec![KeyBinding::new(KeyCode::PageUp, KeyModifiers::empty())],
            page_down: vec![KeyBinding::new(KeyCode::PageDown, KeyModifiers::empty())],
            command_palette: vec![
                KeyBinding::new(KeyCode::Char('p'), KeyModifiers::CONTROL),
                KeyBinding::new(KeyCode::Char('k'), KeyModifiers::CONTROL),
            ],
            toggle_sidebar: vec![KeyBinding::new(KeyCode::Char('b'), KeyModifiers::CONTROL)],
            toggle_todos: vec![KeyBinding::new(KeyCode::Char('t'), KeyModifiers::CONTROL)],
            collapse_tools: vec![KeyBinding::new(KeyCode::Char('o'), KeyModifiers::CONTROL)],
            expand_tools: vec![KeyBinding::new(
                KeyCode::Char('O'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            )],
        }
    }
}

impl KeybindConfig {
    pub fn from_config_with_warnings(
        config: &uira_core::config::schema::KeybindsConfig,
    ) -> (Self, Vec<String>) {
        let mut keybinds = Self::default();
        let mut warnings = Vec::new();

        keybinds.scroll_up = parse_bindings(
            &config.scroll_up,
            keybinds.scroll_up,
            "scroll_up",
            &mut warnings,
        );
        keybinds.scroll_down = parse_bindings(
            &config.scroll_down,
            keybinds.scroll_down,
            "scroll_down",
            &mut warnings,
        );
        keybinds.page_up =
            parse_bindings(&config.page_up, keybinds.page_up, "page_up", &mut warnings);
        keybinds.page_down = parse_bindings(
            &config.page_down,
            keybinds.page_down,
            "page_down",
            &mut warnings,
        );
        keybinds.command_palette = parse_bindings(
            &config.command_palette,
            keybinds.command_palette,
            "command_palette",
            &mut warnings,
        );
        keybinds.toggle_sidebar = parse_bindings(
            &config.toggle_sidebar,
            keybinds.toggle_sidebar,
            "toggle_sidebar",
            &mut warnings,
        );
        keybinds.toggle_todos = parse_bindings(
            &config.toggle_todos,
            keybinds.toggle_todos,
            "toggle_todos",
            &mut warnings,
        );
        keybinds.collapse_tools = parse_bindings(
            &config.collapse_tools,
            keybinds.collapse_tools,
            "collapse_tools",
            &mut warnings,
        );
        keybinds.expand_tools = parse_bindings(
            &config.expand_tools,
            keybinds.expand_tools,
            "expand_tools",
            &mut warnings,
        );

        (keybinds, warnings)
    }

    pub fn from_config(config: &uira_core::config::schema::KeybindsConfig) -> Self {
        Self::from_config_with_warnings(config).0
    }

    pub fn matches_any(bindings: &[KeyBinding], code: KeyCode, modifiers: KeyModifiers) -> bool {
        bindings.iter().any(|kb| kb.matches(code, modifiers))
    }
}

fn parse_bindings(
    source: &Option<Vec<String>>,
    fallback: Vec<KeyBinding>,
    action_name: &str,
    warnings: &mut Vec<String>,
) -> Vec<KeyBinding> {
    let Some(raw_bindings) = source else {
        return fallback;
    };

    let mut parsed = Vec::new();
    for raw in raw_bindings {
        if let Some(binding) = parse_keybind(raw) {
            parsed.push(binding);
        } else {
            warnings.push(format!(
                "Invalid keybind '{}' for action '{}'; keeping defaults",
                raw, action_name
            ));
        }
    }

    if parsed.is_empty() {
        fallback
    } else {
        parsed
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

    #[test]
    fn test_default_toggle_sidebar_uses_ctrl_t() {
        let keybinds = KeybindConfig::default();

        assert_eq!(keybinds.toggle_sidebar.len(), 1);
        assert_eq!(keybinds.toggle_sidebar[0].code, KeyCode::Char('b'));
        assert_eq!(keybinds.toggle_sidebar[0].modifiers, KeyModifiers::CONTROL);

        assert_eq!(keybinds.toggle_todos.len(), 1);
        assert_eq!(keybinds.toggle_todos[0].code, KeyCode::Char('t'));
        assert_eq!(keybinds.toggle_todos[0].modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn test_default_scroll_bindings_empty() {
        let keybinds = KeybindConfig::default();
        assert!(keybinds.scroll_up.is_empty());
        assert!(keybinds.scroll_down.is_empty());
    }
}
