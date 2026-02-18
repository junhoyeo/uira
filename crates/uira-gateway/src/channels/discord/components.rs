use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

const COMPONENT_CUSTOM_ID_KEY: &str = "ucomp";
const MODAL_CUSTOM_ID_KEY: &str = "umodal";
const DEFAULT_TTL: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ButtonStyle {
    #[default]
    Primary,
    Secondary,
    Success,
    Danger,
    Link,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectType {
    #[default]
    String,
    User,
    Role,
    Mentionable,
    Channel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmojiSpec {
    pub name: String,
    pub id: Option<String>,
    pub animated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ButtonSpec {
    pub label: String,
    #[serde(default)]
    pub style: Option<ButtonStyle>,
    pub url: Option<String>,
    pub emoji: Option<EmojiSpec>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default)]
    pub allowed_users: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub label: String,
    pub value: String,
    pub description: Option<String>,
    pub emoji: Option<EmojiSpec>,
    #[serde(default)]
    pub default: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectSpec {
    #[serde(default, rename = "type")]
    pub select_type: Option<SelectType>,
    pub placeholder: Option<String>,
    pub min_values: Option<u32>,
    pub max_values: Option<u32>,
    pub options: Option<Vec<SelectOption>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ComponentBlock {
    Text {
        text: String,
    },
    Section {
        text: Option<String>,
        texts: Option<Vec<String>>,
    },
    Separator {
        spacing: Option<String>,
        divider: Option<bool>,
    },
    Actions {
        buttons: Option<Vec<ButtonSpec>>,
        select: Option<SelectSpec>,
    },
    MediaGallery {
        items: Vec<MediaGalleryItem>,
    },
    File {
        file: String,
        spoiler: Option<bool>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaGalleryItem {
    pub url: String,
    pub description: Option<String>,
    #[serde(default)]
    pub spoiler: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModalFieldSpec {
    #[serde(rename = "type")]
    pub field_type: String,
    pub name: Option<String>,
    pub label: String,
    pub description: Option<String>,
    pub placeholder: Option<String>,
    #[serde(default)]
    pub required: bool,
    pub options: Option<Vec<SelectOption>>,
    pub min_values: Option<u32>,
    pub max_values: Option<u32>,
    pub min_length: Option<u32>,
    pub max_length: Option<u32>,
    pub style: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModalSpec {
    pub title: String,
    pub trigger_label: Option<String>,
    pub trigger_style: Option<ButtonStyle>,
    pub fields: Vec<ModalFieldSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentMessageSpec {
    pub text: Option<String>,
    #[serde(default)]
    pub reusable: bool,
    pub blocks: Option<Vec<ComponentBlock>>,
    pub modal: Option<ModalSpec>,
}

#[derive(Debug, Clone)]
pub struct ComponentEntry {
    pub id: String,
    pub kind: ComponentKind,
    pub label: String,
    pub select_type: Option<SelectType>,
    pub options: Option<Vec<SelectOption>>,
    pub modal_id: Option<String>,
    pub session_key: Option<String>,
    pub agent_id: Option<String>,
    pub account_id: Option<String>,
    pub reusable: bool,
    pub allowed_users: Vec<String>,
    pub message_id: Option<String>,
    pub created_at: Instant,
    pub expires_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    Button,
    Select,
    ModalTrigger,
}

#[derive(Debug, Clone)]
pub struct ModalEntry {
    pub id: String,
    pub title: String,
    pub fields: Vec<ModalFieldDefinition>,
    pub session_key: Option<String>,
    pub agent_id: Option<String>,
    pub account_id: Option<String>,
    pub reusable: bool,
    pub message_id: Option<String>,
    pub created_at: Instant,
    pub expires_at: Instant,
}

#[derive(Debug, Clone)]
pub struct ModalFieldDefinition {
    pub id: String,
    pub name: String,
    pub label: String,
    pub field_type: String,
    pub description: Option<String>,
    pub placeholder: Option<String>,
    pub required: bool,
    pub options: Option<Vec<SelectOption>>,
    pub min_length: Option<u32>,
    pub max_length: Option<u32>,
    pub style: Option<String>,
}

pub fn build_component_custom_id(component_id: &str, modal_id: Option<&str>) -> String {
    let base = format!("{COMPONENT_CUSTOM_ID_KEY}:cid={component_id}");
    match modal_id {
        Some(mid) => format!("{base};mid={mid}"),
        None => base,
    }
}

pub fn build_modal_custom_id(modal_id: &str) -> String {
    format!("{MODAL_CUSTOM_ID_KEY}:mid={modal_id}")
}

pub fn parse_component_custom_id(id: &str) -> Option<(String, Option<String>)> {
    let rest = id.strip_prefix(&format!("{COMPONENT_CUSTOM_ID_KEY}:"))?;
    let mut component_id = None;
    let mut modal_id = None;
    for part in rest.split(';') {
        if let Some(cid) = part.strip_prefix("cid=") {
            if !cid.trim().is_empty() {
                component_id = Some(cid.to_string());
            }
        } else if let Some(mid) = part.strip_prefix("mid=") {
            if !mid.trim().is_empty() {
                modal_id = Some(mid.to_string());
            }
        }
    }
    Some((component_id?, modal_id))
}

pub fn parse_modal_custom_id(id: &str) -> Option<String> {
    let rest = id.strip_prefix(&format!("{MODAL_CUSTOM_ID_KEY}:"))?;
    for part in rest.split(';') {
        if let Some(mid) = part.strip_prefix("mid=") {
            let trimmed = mid.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

pub fn format_component_event_text(kind: ComponentKind, label: &str, values: &[String]) -> String {
    match kind {
        ComponentKind::Button | ComponentKind::ModalTrigger => {
            format!("Clicked \"{label}\".")
        }
        ComponentKind::Select => {
            if values.is_empty() {
                format!("Updated \"{label}\".")
            } else {
                format!("Selected {} from \"{label}\".", values.join(", "))
            }
        }
    }
}

fn generate_short_id(prefix: &str) -> String {
    use std::time::SystemTime;
    let nanos = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{prefix}{:x}", nanos & 0xFFFF_FFFF_FFFF)
}

pub struct ComponentRegistry {
    components: Mutex<HashMap<String, ComponentEntry>>,
    modals: Mutex<HashMap<String, ModalEntry>>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        Self {
            components: Mutex::new(HashMap::new()),
            modals: Mutex::new(HashMap::new()),
        }
    }

    pub fn register(
        &self,
        entries: Vec<ComponentEntry>,
        modals: Vec<ModalEntry>,
        message_id: Option<&str>,
    ) {
        let mut comp_map = self.components.lock().unwrap();
        for mut entry in entries {
            if let Some(mid) = message_id {
                entry.message_id = Some(mid.to_string());
            }
            comp_map.insert(entry.id.clone(), entry);
        }

        let mut modal_map = self.modals.lock().unwrap();
        for mut entry in modals {
            if let Some(mid) = message_id {
                entry.message_id = Some(mid.to_string());
            }
            modal_map.insert(entry.id.clone(), entry);
        }
    }

    pub fn resolve_component(&self, id: &str, consume: bool) -> Option<ComponentEntry> {
        let mut map = self.components.lock().unwrap();
        if consume {
            let entry = map.remove(id)?;
            if entry.expires_at < Instant::now() {
                return None;
            }
            if entry.reusable {
                map.insert(id.to_string(), entry.clone());
            }
            Some(entry)
        } else {
            let entry = map.get(id)?;
            if entry.expires_at < Instant::now() {
                return None;
            }
            Some(entry.clone())
        }
    }

    pub fn resolve_modal(&self, id: &str, consume: bool) -> Option<ModalEntry> {
        let mut map = self.modals.lock().unwrap();
        if consume {
            let entry = map.remove(id)?;
            if entry.expires_at < Instant::now() {
                return None;
            }
            if entry.reusable {
                map.insert(id.to_string(), entry.clone());
            }
            Some(entry)
        } else {
            let entry = map.get(id)?;
            if entry.expires_at < Instant::now() {
                return None;
            }
            Some(entry.clone())
        }
    }

    pub fn cleanup_expired(&self) {
        let now = Instant::now();
        self.components
            .lock()
            .unwrap()
            .retain(|_, e| e.expires_at > now);
        self.modals
            .lock()
            .unwrap()
            .retain(|_, e| e.expires_at > now);
    }

    pub fn clear(&self) {
        self.components.lock().unwrap().clear();
        self.modals.lock().unwrap().clear();
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn build_component_entries(
    spec: &ComponentMessageSpec,
    session_key: Option<&str>,
    agent_id: Option<&str>,
    account_id: Option<&str>,
    ttl: Option<Duration>,
) -> (Vec<ComponentEntry>, Vec<ModalEntry>) {
    let ttl = ttl.unwrap_or(DEFAULT_TTL);
    let now = Instant::now();
    let expires = now + ttl;
    let mut entries = Vec::new();
    let mut modals = Vec::new();

    if let Some(blocks) = &spec.blocks {
        for block in blocks {
            match block {
                ComponentBlock::Actions {
                    buttons: Some(btns),
                    ..
                } => {
                    for btn in btns {
                        if btn.url.is_some() {
                            continue;
                        }
                        let id = generate_short_id("btn_");
                        entries.push(ComponentEntry {
                            id,
                            kind: ComponentKind::Button,
                            label: btn.label.clone(),
                            select_type: None,
                            options: None,
                            modal_id: None,
                            session_key: session_key.map(|s| s.to_string()),
                            agent_id: agent_id.map(|s| s.to_string()),
                            account_id: account_id.map(|s| s.to_string()),
                            reusable: spec.reusable,
                            allowed_users: btn.allowed_users.clone(),
                            message_id: None,
                            created_at: now,
                            expires_at: expires,
                        });
                    }
                }
                ComponentBlock::Actions {
                    select: Some(sel), ..
                } => {
                    let id = generate_short_id("sel_");
                    entries.push(ComponentEntry {
                        id,
                        kind: ComponentKind::Select,
                        label: sel
                            .placeholder
                            .clone()
                            .unwrap_or_else(|| "select".to_string()),
                        select_type: sel.select_type,
                        options: sel.options.clone(),
                        modal_id: None,
                        session_key: session_key.map(|s| s.to_string()),
                        agent_id: agent_id.map(|s| s.to_string()),
                        account_id: account_id.map(|s| s.to_string()),
                        reusable: spec.reusable,
                        allowed_users: Vec::new(),
                        message_id: None,
                        created_at: now,
                        expires_at: expires,
                    });
                }
                _ => {}
            }
        }
    }

    if let Some(modal_spec) = &spec.modal {
        let modal_id = generate_short_id("mdl_");
        let fields: Vec<ModalFieldDefinition> = modal_spec
            .fields
            .iter()
            .enumerate()
            .map(|(i, f)| ModalFieldDefinition {
                id: generate_short_id("fld_"),
                name: f.name.clone().unwrap_or_else(|| format!("field_{}", i + 1)),
                label: f.label.clone(),
                field_type: f.field_type.clone(),
                description: f.description.clone(),
                placeholder: f.placeholder.clone(),
                required: f.required,
                options: f.options.clone(),
                min_length: f.min_length,
                max_length: f.max_length,
                style: f.style.clone(),
            })
            .collect();

        modals.push(ModalEntry {
            id: modal_id.clone(),
            title: modal_spec.title.clone(),
            fields,
            session_key: session_key.map(|s| s.to_string()),
            agent_id: agent_id.map(|s| s.to_string()),
            account_id: account_id.map(|s| s.to_string()),
            reusable: spec.reusable,
            message_id: None,
            created_at: now,
            expires_at: expires,
        });

        let trigger_label = modal_spec
            .trigger_label
            .clone()
            .unwrap_or_else(|| "Open form".to_string());
        let trigger_id = generate_short_id("btn_");
        entries.push(ComponentEntry {
            id: trigger_id,
            kind: ComponentKind::ModalTrigger,
            label: trigger_label,
            select_type: None,
            options: None,
            modal_id: Some(modal_id),
            session_key: session_key.map(|s| s.to_string()),
            agent_id: agent_id.map(|s| s.to_string()),
            account_id: account_id.map(|s| s.to_string()),
            reusable: spec.reusable,
            allowed_users: Vec::new(),
            message_id: None,
            created_at: now,
            expires_at: expires,
        });
    }

    (entries, modals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_component_custom_id() {
        let id = build_component_custom_id("btn_abc", None);
        assert_eq!(id, "ucomp:cid=btn_abc");

        let id = build_component_custom_id("btn_abc", Some("mdl_xyz"));
        assert_eq!(id, "ucomp:cid=btn_abc;mid=mdl_xyz");
    }

    #[test]
    fn test_parse_component_custom_id() {
        let (cid, mid) = parse_component_custom_id("ucomp:cid=btn_abc").unwrap();
        assert_eq!(cid, "btn_abc");
        assert!(mid.is_none());

        let (cid, mid) = parse_component_custom_id("ucomp:cid=btn_abc;mid=mdl_xyz").unwrap();
        assert_eq!(cid, "btn_abc");
        assert_eq!(mid.unwrap(), "mdl_xyz");
    }

    #[test]
    fn test_parse_modal_custom_id() {
        let mid = parse_modal_custom_id("umodal:mid=mdl_xyz").unwrap();
        assert_eq!(mid, "mdl_xyz");
    }

    #[test]
    fn test_parse_invalid_prefix() {
        assert!(parse_component_custom_id("other:cid=abc").is_none());
        assert!(parse_modal_custom_id("other:mid=abc").is_none());
    }

    #[test]
    fn test_format_button_event() {
        let text = format_component_event_text(ComponentKind::Button, "Submit", &[]);
        assert_eq!(text, "Clicked \"Submit\".");
    }

    #[test]
    fn test_format_select_event() {
        let text = format_component_event_text(
            ComponentKind::Select,
            "Color",
            &["red".to_string(), "blue".to_string()],
        );
        assert_eq!(text, "Selected red, blue from \"Color\".");
    }

    #[test]
    fn test_registry_lifecycle() {
        let registry = ComponentRegistry::new();
        let now = Instant::now();
        let entry = ComponentEntry {
            id: "test_btn".to_string(),
            kind: ComponentKind::Button,
            label: "Test".to_string(),
            select_type: None,
            options: None,
            modal_id: None,
            session_key: None,
            agent_id: None,
            account_id: None,
            reusable: false,
            allowed_users: Vec::new(),
            message_id: None,
            created_at: now,
            expires_at: now + Duration::from_secs(60),
        };

        registry.register(vec![entry], Vec::new(), Some("msg_1"));
        let resolved = registry.resolve_component("test_btn", false).unwrap();
        assert_eq!(resolved.label, "Test");
        assert_eq!(resolved.message_id, Some("msg_1".to_string()));

        let consumed = registry.resolve_component("test_btn", true).unwrap();
        assert_eq!(consumed.label, "Test");
        assert!(registry.resolve_component("test_btn", false).is_none());
    }

    #[test]
    fn test_registry_reusable_not_consumed() {
        let registry = ComponentRegistry::new();
        let now = Instant::now();
        let entry = ComponentEntry {
            id: "reuse_btn".to_string(),
            kind: ComponentKind::Button,
            label: "Reuse".to_string(),
            select_type: None,
            options: None,
            modal_id: None,
            session_key: None,
            agent_id: None,
            account_id: None,
            reusable: true,
            allowed_users: Vec::new(),
            message_id: None,
            created_at: now,
            expires_at: now + Duration::from_secs(60),
        };

        registry.register(vec![entry], Vec::new(), None);
        registry.resolve_component("reuse_btn", true).unwrap();
        assert!(registry.resolve_component("reuse_btn", false).is_some());
    }
}
