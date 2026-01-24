pub mod builder;
pub mod sections;
pub mod types;

pub use builder::{
    build_agent_section, build_delegation_table_section, build_triggers_section,
    generate_orchestrator_prompt,
};
pub use sections::{
    build_agent_registry, build_completion_checklist, build_critical_rules,
    build_delegation_matrix, build_header, build_orchestration_principles,
    build_tool_selection_section, build_trigger_table, build_workflow,
};
pub use types::{
    AgentCategory, AgentConfig, AgentPromptMetadata, AgentTrigger, GeneratorOptions, ModelType,
    PromptSection,
};
