use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,

    #[serde(default = "default_storage_path")]
    pub storage_path: String,

    #[serde(default = "default_embedding_model")]
    pub embedding_model: String,

    #[serde(default = "default_embedding_dimension")]
    pub embedding_dimension: usize,

    #[serde(default = "default_embedding_api_key_env")]
    pub embedding_api_key_env: String,

    #[serde(default = "default_embedding_api_base")]
    pub embedding_api_base: String,

    #[serde(default = "default_auto_recall")]
    pub auto_recall: bool,

    #[serde(default = "default_auto_capture")]
    pub auto_capture: bool,

    #[serde(default = "default_max_recall_results")]
    pub max_recall_results: usize,

    #[serde(default = "default_profile_frequency")]
    pub profile_frequency: usize,

    #[serde(default = "default_capture_mode")]
    pub capture_mode: String,

    #[serde(default = "default_container_tag")]
    pub container_tag: String,

    #[serde(default = "default_vector_weight")]
    pub vector_weight: f32,

    #[serde(default = "default_fts_weight")]
    pub fts_weight: f32,

    #[serde(default = "default_temporal_decay_lambda")]
    pub temporal_decay_lambda: f64,

    #[serde(default = "default_mmr_lambda")]
    pub mmr_lambda: f64,

    #[serde(default = "default_min_capture_length")]
    pub min_capture_length: usize,

    #[serde(default = "default_chunk_size")]
    pub chunk_size: usize,

    #[serde(default = "default_chunk_overlap")]
    pub chunk_overlap: usize,
}

impl Default for MemoryConfig {
    fn default() -> Self {
        Self {
            enabled: default_enabled(),
            storage_path: default_storage_path(),
            embedding_model: default_embedding_model(),
            embedding_dimension: default_embedding_dimension(),
            embedding_api_key_env: default_embedding_api_key_env(),
            embedding_api_base: default_embedding_api_base(),
            auto_recall: default_auto_recall(),
            auto_capture: default_auto_capture(),
            max_recall_results: default_max_recall_results(),
            profile_frequency: default_profile_frequency(),
            capture_mode: default_capture_mode(),
            container_tag: default_container_tag(),
            vector_weight: default_vector_weight(),
            fts_weight: default_fts_weight(),
            temporal_decay_lambda: default_temporal_decay_lambda(),
            mmr_lambda: default_mmr_lambda(),
            min_capture_length: default_min_capture_length(),
            chunk_size: default_chunk_size(),
            chunk_overlap: default_chunk_overlap(),
        }
    }
}

fn default_enabled() -> bool {
    false
}

fn default_storage_path() -> String {
    let home = dirs::home_dir()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|| "~".to_string());
    format!("{home}/.uira/memory.db")
}

fn default_embedding_model() -> String {
    "text-embedding-3-small".to_string()
}

fn default_embedding_dimension() -> usize {
    1536
}

fn default_embedding_api_key_env() -> String {
    "OPENAI_API_KEY".to_string()
}

fn default_embedding_api_base() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_auto_recall() -> bool {
    true
}

fn default_auto_capture() -> bool {
    true
}

fn default_max_recall_results() -> usize {
    5
}

fn default_profile_frequency() -> usize {
    5
}

fn default_capture_mode() -> String {
    "all".to_string()
}

fn default_container_tag() -> String {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown".to_string());
    format!("uira_{hostname}")
}

fn default_vector_weight() -> f32 {
    0.7
}

fn default_fts_weight() -> f32 {
    0.3
}

fn default_temporal_decay_lambda() -> f64 {
    0.001
}

fn default_mmr_lambda() -> f64 {
    0.7
}

fn default_min_capture_length() -> usize {
    20
}

fn default_chunk_size() -> usize {
    512
}

fn default_chunk_overlap() -> usize {
    50
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sensible() {
        let config = MemoryConfig::default();
        assert!(!config.enabled);
        assert!(config.storage_path.contains("memory.db"));
        assert_eq!(config.embedding_model, "text-embedding-3-small");
        assert_eq!(config.embedding_dimension, 1536);
        assert!(config.auto_recall);
        assert!(config.auto_capture);
        assert_eq!(config.max_recall_results, 5);
        assert_eq!(config.profile_frequency, 5);
        assert_eq!(config.vector_weight, 0.7);
        assert_eq!(config.fts_weight, 0.3);
        assert_eq!(config.chunk_size, 512);
        assert_eq!(config.chunk_overlap, 50);
    }

    #[test]
    fn deserialize_with_defaults() {
        let yaml = r#"
enabled: true
storage_path: "/tmp/test.db"
"#;
        let config: MemoryConfig = serde_yaml_ng::from_str(yaml).unwrap();
        assert!(config.enabled);
        assert_eq!(config.storage_path, "/tmp/test.db");
        assert_eq!(config.embedding_model, "text-embedding-3-small");
    }

    #[test]
    fn container_tag_includes_hostname() {
        let config = MemoryConfig::default();
        assert!(config.container_tag.starts_with("uira_"));
    }
}
