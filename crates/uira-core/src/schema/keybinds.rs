use serde::{Deserialize, Deserializer, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct KeybindsConfig {
    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub scroll_up: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub scroll_down: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub page_up: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub page_down: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub command_palette: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub toggle_sidebar: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub toggle_todos: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub collapse_tools: Option<Vec<String>>,

    #[serde(default, deserialize_with = "deserialize_binding_list")]
    pub expand_tools: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum BindingList {
    Single(String),
    Multi(Vec<String>),
}

fn deserialize_binding_list<'de, D>(deserializer: D) -> Result<Option<Vec<String>>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<BindingList>::deserialize(deserializer)?;
    Ok(value.map(|parsed| match parsed {
        BindingList::Single(single) => vec![single],
        BindingList::Multi(multi) => multi,
    }))
}
