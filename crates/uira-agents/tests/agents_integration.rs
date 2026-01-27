use uira_agents::{
    get_agent_definitions, prompt_loader::strip_yaml_frontmatter, AgentRegistry, ModelTier,
    PromptLoader, TierBuilder, ToolRestrictionsRegistry,
};
use uira_sdk::{AgentOverrideConfig, AgentOverrides};

#[test]
fn get_agent_definitions_applies_disable_override() {
    let mut overrides = AgentOverrides::new();
    overrides.insert(
        "explore".to_string(),
        AgentOverrideConfig {
            enabled: Some(false),
            ..Default::default()
        },
    );

    let defs = get_agent_definitions(Some(&overrides));
    assert!(!defs.contains_key("explore"));
    assert!(defs.contains_key("architect"));
}

#[test]
fn prompt_loader_embedded_map_strips_frontmatter() {
    static PROMPTS: &[(&str, &str)] = &[("a", "---\nname: a\n---\n\nHello")];
    let loader = PromptLoader::from_embedded_map(PROMPTS);
    assert_eq!(loader.load("a"), "Hello");
    assert!(loader.load("missing").contains("Prompt file not found"));
}

#[test]
fn tier_builder_prefers_dedicated_prompt_file_when_present() {
    let tmp = tempfile::tempdir().unwrap();

    std::fs::write(
        tmp.path().join("executor-high.md"),
        "---\nname: executor-high\n---\n\nDedicated",
    )
    .unwrap();

    let loader = PromptLoader::from_fs(tmp.path());
    let builder = TierBuilder::new(loader);

    let base = uira_sdk::AgentConfig {
        name: "executor".to_string(),
        description: "Executes".to_string(),
        prompt: "Base".to_string(),
        tools: vec!["Read".to_string()],
        model: None,
        default_model: None,
        metadata: None,
    };

    let v = builder.build_variant(&base, ModelTier::High);
    assert_eq!(v.name, "executor-high");
    assert_eq!(v.prompt, "Dedicated");
}

#[test]
fn tool_restrictions_registry_inherits_for_tier_variants() {
    let reg = ToolRestrictionsRegistry::with_default_allowlists();

    let mut cfg = uira_sdk::AgentConfig {
        name: "executor-high".to_string(),
        description: "".to_string(),
        prompt: "".to_string(),
        tools: vec![
            "Read".to_string(),
            "Edit".to_string(),
            "Bash".to_string(),
            "WebFetch".to_string(),
        ],
        model: None,
        default_model: None,
        metadata: None,
    };

    reg.apply(&mut cfg);
    assert!(cfg.tools.contains(&"Read".to_string()));
    assert!(cfg.tools.contains(&"Edit".to_string()));
    assert!(cfg.tools.contains(&"Bash".to_string()));
    assert!(!cfg.tools.contains(&"WebFetch".to_string()));
}

#[test]
fn registry_factory_produces_cloned_configs() {
    let mut reg = AgentRegistry::new();
    reg.register_config(uira_sdk::AgentConfig {
        name: "a".to_string(),
        description: "d".to_string(),
        prompt: "p".to_string(),
        tools: vec!["Read".to_string()],
        model: None,
        default_model: None,
        metadata: None,
    });

    let a1 = reg.get("a").unwrap();
    let a2 = reg.get("a").unwrap();
    assert_eq!(a1.name, a2.name);
}

#[test]
fn strip_yaml_frontmatter_matches_ts_behavior() {
    let md = "---\na: 1\n---\n\nBody\n";
    assert_eq!(strip_yaml_frontmatter(md), "Body");
}

#[test]
fn apply_overrides_is_noop_without_overrides() {
    let tmp = tempfile::tempdir().unwrap();
    let loader = PromptLoader::from_fs(tmp.path());
    let defs = uira_agents::definitions::get_agent_definitions_with_loader(&loader, None, None);
    assert!(defs.contains_key("executor"));
}
