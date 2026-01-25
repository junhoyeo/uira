use astrape_proxy::{auth, config::ProxyConfig};
use std::io::Write;
use tempfile::NamedTempFile;

#[test]
fn test_proxy_config_defaults() {
    let config = ProxyConfig::default();
    assert_eq!(config.port, 8787);
    // Librarian has a default model configured
    assert_eq!(
        config.get_model_for_agent("librarian"),
        Some("opencode/big-pickle")
    );
}

#[test]
fn test_agent_based_routing() {
    let mut temp_file = NamedTempFile::new().unwrap();
    writeln!(
        temp_file,
        r#"
agents:
  explore:
    model: "opencode/gpt-5-nano"
  architect:
    model: "openai/gpt-4.1"
"#
    )
    .unwrap();

    let config = ProxyConfig::from_yaml_file(temp_file.path()).unwrap();

    assert_eq!(
        config.get_model_for_agent("explore"),
        Some("opencode/gpt-5-nano")
    );
    assert_eq!(
        config.get_model_for_agent("architect"),
        Some("openai/gpt-4.1")
    );
    // Librarian keeps default even when loading from YAML
    assert_eq!(
        config.get_model_for_agent("librarian"),
        Some("opencode/big-pickle")
    );
    assert_eq!(config.get_model_for_agent("nonexistent"), None);

    assert_eq!(
        config.resolve_model_for_agent("explore", "fallback"),
        "opencode/gpt-5-nano"
    );
    assert_eq!(
        config.resolve_model_for_agent("nonexistent", "fallback"),
        "fallback"
    );
}

#[test]
fn test_model_to_provider() {
    assert_eq!(auth::model_to_provider("openai/gpt-4"), "openai");
    assert_eq!(auth::model_to_provider("gemini/gemini-pro"), "google");
    assert_eq!(auth::model_to_provider("anthropic/claude-3"), "anthropic");
    assert_eq!(auth::model_to_provider("opencode/gpt-5-nano"), "opencode");
}
