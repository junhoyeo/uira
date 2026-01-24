mod config;
mod hooks;
mod linter;
mod typos;

use astrape_agents::get_agent_definitions;
use astrape_claude::{
    extract_prompt, install_hooks as install_claude_hooks, list_installed_hooks, read_input,
    write_output,
};
use astrape_core::HookEvent;
use astrape_features::builtin_skills::{create_builtin_skills, get_builtin_skill};
use astrape_hook::KeywordDetector;
use astrape_sdk::{create_astrape_session, SessionOptions};
use clap::{Parser, Subcommand};
use colored::Colorize;
use config::Config;
use hooks::HookExecutor;
use linter::Linter;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process;
use typos::TyposChecker;

#[derive(Parser)]
#[command(name = "astrape")]
#[command(version, about = "⚡ Lightning-fast Rust-native git hooks manager & AI harness", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize astrape configuration
    Init {
        #[arg(short, long, default_value = "astrape.yml")]
        config: String,
    },
    /// Install git hooks to .git/hooks/
    Install,
    /// Run a specific git hook
    Run { hook: String },
    /// Lint JS/TS files with native oxc
    Lint {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        files: Vec<String>,
    },
    /// Check for typos
    Typos {
        #[arg(long, help = "Use AI to decide whether to apply fixes or ignore")]
        ai: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        files: Vec<String>,
    },
    /// Manage AI harness hooks (Claude Code)
    Hook {
        #[command(subcommand)]
        action: HookCommands,
    },
    /// Manage agents
    Agent {
        #[command(subcommand)]
        action: AgentCommands,
    },
    /// Manage SDK sessions
    Session {
        #[command(subcommand)]
        action: SessionCommands,
    },
    /// Manage skills
    Skill {
        #[command(subcommand)]
        action: SkillCommands,
    },
}

#[derive(Subcommand)]
enum AgentCommands {
    /// List all available agents
    List,
    /// Show details of a specific agent
    Info { name: String },
    /// Delegate a task to an agent
    Delegate {
        #[arg(short, long)]
        agent: String,
        #[arg(short, long)]
        prompt: String,
        #[arg(short, long)]
        model: Option<String>,
    },
}

#[derive(Subcommand)]
enum SessionCommands {
    /// Start a new session
    Start {
        #[arg(short, long)]
        config: Option<String>,
    },
    /// Show session status
    Status,
}

#[derive(Subcommand)]
enum SkillCommands {
    /// List available skills
    List,
    /// Show skill template
    Show { name: String },
}

#[derive(Subcommand)]
enum HookCommands {
    /// Install AI harness hooks for Claude Code
    Install,
    /// List installed AI harness hooks
    List,
    /// Detect keywords from stdin (used by shell scripts)
    DetectKeywords {
        /// Optional agent name for context-aware messages
        #[arg(long)]
        agent: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { config } => init_command(&config),
        Commands::Install => install_command(),
        Commands::Run { hook } => run_command(&hook),
        Commands::Lint { files } => lint_command(&files),
        Commands::Typos { ai, files } => typos_command(ai, &files),
        Commands::Hook { action } => hook_command(action),
        Commands::Agent { action } => agent_command(action),
        Commands::Session { action } => session_command(action),
        Commands::Skill { action } => skill_command(action),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn init_command(config_path: &str) -> anyhow::Result<()> {
    println!("⚡ Initializing astrape...\n");

    let mut created_files = Vec::new();

    if !Path::new(config_path).exists() {
        let config = Config::default_config();
        let yaml = config.to_yaml()?;
        fs::write(config_path, yaml)?;
        created_files.push(config_path.to_string());
    }

    if created_files.is_empty() {
        println!("ℹ️  Config already exists");
    } else {
        println!("✅ Created:");
        for file in &created_files {
            println!("   • {}", file);
        }
    }

    println!("\n📦 Next steps:");
    println!("   1. Run: astrape install");
    println!("   2. Commit normally - hooks will run automatically");

    Ok(())
}

fn install_command() -> anyhow::Result<()> {
    println!("📦 Installing git hooks...\n");

    let git_dir = find_git_dir()?;
    let hooks_dir = git_dir.join("hooks");

    if !hooks_dir.exists() {
        fs::create_dir_all(&hooks_dir)?;
    }

    let config_path = "astrape.yml";
    if !Path::new(config_path).exists() {
        anyhow::bail!(
            "Config file not found: {}. Run 'astrape init' first.",
            config_path
        );
    }

    let config = Config::from_file(config_path)?;
    let mut installed_hooks = Vec::new();

    for hook_name in config.hooks.keys() {
        let hook_path = hooks_dir.join(hook_name);
        let hook_script = generate_hook_script(hook_name);

        if hook_path.exists() {
            let existing = fs::read_to_string(&hook_path)?;
            if !existing.contains("# astrape managed hook") {
                let backup_path = hooks_dir.join(format!("{}.backup", hook_name));
                fs::rename(&hook_path, &backup_path)?;
                println!(
                    "   ⚠️  Backed up existing {} to {}.backup",
                    hook_name, hook_name
                );
            }
        }

        fs::write(&hook_path, hook_script)?;

        let mut perms = fs::metadata(&hook_path)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&hook_path, perms)?;

        installed_hooks.push(hook_name.clone());
    }

    if installed_hooks.is_empty() {
        println!("ℹ️  No hooks defined in config");
    } else {
        println!("✅ Installed hooks:");
        for hook in &installed_hooks {
            println!("   • {}", hook);
        }
    }

    println!("\n🎉 Done! Git hooks are now active.");
    Ok(())
}

fn find_git_dir() -> anyhow::Result<std::path::PathBuf> {
    let current = std::env::current_dir()?;
    let mut path = current.as_path();

    loop {
        let git_dir = path.join(".git");
        if git_dir.is_dir() {
            return Ok(git_dir);
        }

        match path.parent() {
            Some(parent) => path = parent,
            None => anyhow::bail!("Not a git repository (or any parent up to mount point)"),
        }
    }
}

fn generate_hook_script(hook_name: &str) -> String {
    format!(
        r#"#!/bin/sh
# astrape managed hook - do not edit
# This hook was generated by astrape. To modify, edit astrape.yml

exec astrape run {}
"#,
        hook_name
    )
}

fn run_command(hook_name: &str) -> anyhow::Result<()> {
    let config_path = "astrape.yml";

    if !Path::new(config_path).exists() {
        anyhow::bail!(
            "Config file not found: {}. Run 'astrape init' first.",
            config_path
        );
    }

    let config = Config::from_file(config_path)?;

    let hook_config = config
        .hooks
        .get(hook_name)
        .ok_or_else(|| anyhow::anyhow!("Hook '{}' not found in config", hook_name))?;

    let executor = HookExecutor::new(hook_name.to_string());
    executor.execute(hook_config)?;

    println!("\n✅ Hook '{}' completed successfully", hook_name);
    Ok(())
}

fn lint_command(files: &[String]) -> anyhow::Result<()> {
    let linter = Linter::default();

    let files_to_lint = if files.is_empty() {
        collect_files_from_cwd()?
    } else {
        files.to_vec()
    };

    let success = linter.run(&files_to_lint)?;

    if !success {
        process::exit(1);
    }

    Ok(())
}

fn typos_command(ai: bool, files: &[String]) -> anyhow::Result<()> {
    if ai {
        println!("🔍 Checking for typos with AI assistance...\n");

        let config = Config::from_file("astrape.yml").ok();
        let ai_config = config.as_ref().and_then(|c| c.ai.clone());
        let ai_hooks = config.and_then(|c| c.ai_hooks);

        let mut checker = TyposChecker::with_hooks(ai_config, ai_hooks);
        let success = checker.run(files)?;
        if !success {
            process::exit(1);
        }
    } else {
        println!("🔍 Checking for typos...\n");
        let mut cmd = std::process::Command::new("typos");
        if files.is_empty() {
            cmd.arg(".");
        } else {
            cmd.args(files);
        }
        let status = cmd.status().map_err(|_| {
            anyhow::anyhow!("Failed to run typos. Is it installed? Run: cargo install typos-cli")
        })?;
        if !status.success() {
            process::exit(1);
        }
        println!("✓ No typos found");
    }

    Ok(())
}

fn collect_files_from_cwd() -> anyhow::Result<Vec<String>> {
    let mut files = Vec::new();
    collect_files_recursive(Path::new("."), &mut files)?;
    Ok(files)
}

fn collect_files_recursive(dir: &Path, files: &mut Vec<String>) -> anyhow::Result<()> {
    if !dir.is_dir() {
        return Ok(());
    }

    let dir_name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("");
    if dir_name == "node_modules" || dir_name == ".git" || dir_name == "dist" || dir_name == "build"
    {
        return Ok(());
    }

    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            collect_files_recursive(&path, files)?;
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            if matches!(
                ext,
                "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
            ) {
                files.push(path.to_string_lossy().to_string());
            }
        }
    }

    Ok(())
}

fn hook_command(action: HookCommands) -> anyhow::Result<()> {
    match action {
        HookCommands::Install => {
            println!("⚡ Installing Claude Code hooks...\n");
            let events = vec![HookEvent::UserPromptSubmit, HookEvent::Stop];
            let installed = install_claude_hooks(&events)?;
            if installed.is_empty() {
                println!("ℹ️  No hooks installed");
            } else {
                println!("✅ Installed Claude Code hooks:");
                for hook in &installed {
                    println!("   • {}", hook);
                }
            }
            println!("\n🎉 Done! Claude Code hooks are now active.");
            println!("\nFor OpenCode: Use @astrape/native npm package instead.");
            Ok(())
        }
        HookCommands::List => {
            println!("⚡ Installed AI harness hooks:\n");

            let claude_hooks = list_installed_hooks()?;
            if claude_hooks.is_empty() {
                println!("Claude Code: (none)");
            } else {
                println!("Claude Code:");
                for hook in &claude_hooks {
                    println!("   • {}", hook);
                }
            }

            Ok(())
        }
        HookCommands::DetectKeywords { agent } => {
            let input = read_input()?;
            let prompt = extract_prompt(&input);

            if let Some(prompt_text) = prompt {
                let detector = KeywordDetector::new();
                if let Some(output) = detector.detect(&prompt_text, agent.as_deref()) {
                    write_output(&output)?;
                } else {
                    write_output(&astrape_core::HookOutput::default())?;
                }
            } else {
                write_output(&astrape_core::HookOutput::default())?;
            }

            Ok(())
        }
    }
}

fn agent_command(action: AgentCommands) -> anyhow::Result<()> {
    match action {
        AgentCommands::List => {
            println!("{}", "⚡ Available Agents".bold());
            println!();

            let agents = get_agent_definitions(None);
            let mut sorted_agents: Vec<_> = agents.iter().collect();
            sorted_agents.sort_by_key(|(name, _)| name.as_str());

            // Group agents by base name
            let mut groups: std::collections::HashMap<String, Vec<(&String, &astrape_sdk::AgentConfig)>> =
                std::collections::HashMap::new();

            for (name, config) in &sorted_agents {
                let base_name = name
                    .split('-')
                    .next()
                    .unwrap_or(name)
                    .to_string();
                groups
                    .entry(base_name)
                    .or_default()
                    .push((name, config));
            }

            let mut group_names: Vec<_> = groups.keys().cloned().collect();
            group_names.sort();

            for group in group_names {
                let members = groups.get(&group).unwrap();
                println!("  {}", group.cyan().bold());
                for (name, config) in members {
                    let model = config
                        .model
                        .map(|m| format!(" ({})", m))
                        .unwrap_or_default();
                    println!(
                        "    {} {}{}",
                        "•".dimmed(),
                        name,
                        model.dimmed()
                    );
                }
                println!();
            }

            println!(
                "{} {} agents available",
                "Total:".bold(),
                sorted_agents.len()
            );
            Ok(())
        }
        AgentCommands::Info { name } => {
            let agents = get_agent_definitions(None);
            if let Some(agent) = agents.get(&name) {
                println!("{}", format!("⚡ Agent: {}", name).bold());
                println!();
                println!("{} {}", "Description:".cyan(), agent.description);
                println!(
                    "{} {}",
                    "Model:".cyan(),
                    agent
                        .model
                        .map(|m| m.to_string())
                        .unwrap_or_else(|| "default (sonnet)".to_string())
                );
                println!("{} {}", "Tools:".cyan(), agent.tools.join(", "));
                println!();
                println!("{}", "Prompt:".cyan());
                println!("{}", "-".repeat(60).dimmed());

                // Show first 500 chars of prompt or full if shorter
                let prompt_preview = if agent.prompt.len() > 500 {
                    format!("{}...\n\n[truncated, {} total chars]", &agent.prompt[..500], agent.prompt.len())
                } else {
                    agent.prompt.clone()
                };
                println!("{}", prompt_preview);
                Ok(())
            } else {
                anyhow::bail!("Agent '{}' not found. Use 'astrape agent list' to see available agents.", name);
            }
        }
        AgentCommands::Delegate {
            agent,
            prompt,
            model,
        } => {
            let agents = get_agent_definitions(None);
            if !agents.contains_key(&agent) {
                anyhow::bail!(
                    "Agent '{}' not found. Use 'astrape agent list' to see available agents.",
                    agent
                );
            }

            println!("{}", format!("⚡ Delegating to agent: {}", agent).bold());
            println!("{} {}", "Prompt:".cyan(), prompt);
            if let Some(m) = &model {
                println!("{} {}", "Model override:".cyan(), m);
            }
            println!();

            // For now, just show what would be delegated
            // Full delegation requires SDK bridge implementation
            println!("{}", "Note: Full delegation requires active SDK session.".yellow());
            println!("To delegate, use the Task tool in an active Claude session:");
            println!();
            println!(
                "  Task(subagent_type=\"astrape:{}\", prompt=\"{}\"{})",
                agent,
                prompt,
                model.map(|m| format!(", model=\"{}\"", m)).unwrap_or_default()
            );

            Ok(())
        }
    }
}

fn session_command(action: SessionCommands) -> anyhow::Result<()> {
    match action {
        SessionCommands::Start { config } => {
            println!("{}", "⚡ Starting Astrape Session".bold());
            println!();

            let options = SessionOptions {
                working_directory: Some(std::env::current_dir()?.to_string_lossy().to_string()),
                ..Default::default()
            };

            let session = create_astrape_session(Some(options));

            println!("{} Created session with:", "✓".green());
            println!(
                "  {} {} agents",
                "•".dimmed(),
                session.query_options.agents.len()
            );
            println!(
                "  {} {} tools",
                "•".dimmed(),
                session.query_options.allowed_tools.len()
            );
            println!(
                "  {} {} MCP servers",
                "•".dimmed(),
                session.query_options.mcp_servers.len()
            );
            println!(
                "  {} {} context files",
                "•".dimmed(),
                session.state.context_files.len()
            );

            if !session.state.context_files.is_empty() {
                println!();
                println!("{}", "Context files:".cyan());
                for file in &session.state.context_files {
                    println!("  {} {}", "•".dimmed(), file);
                }
            }

            println!();
            println!(
                "{} Session ready. Use with Claude Agent SDK or astrape agent delegate.",
                "✓".green()
            );

            if config.is_some() {
                println!(
                    "{} Custom config path specified but not yet implemented",
                    "⚠".yellow()
                );
            }

            Ok(())
        }
        SessionCommands::Status => {
            println!("{}", "⚡ Session Status".bold());
            println!();

            // Check for state files
            let state_dir = Path::new(".omc/state");
            let astrape_state = Path::new(".astrape/state");

            let has_omc = state_dir.exists();
            let has_astrape = astrape_state.exists();

            if has_omc || has_astrape {
                println!("{} Active state found:", "✓".green());
                if has_omc {
                    println!("  {} .omc/state/", "•".dimmed());
                }
                if has_astrape {
                    println!("  {} .astrape/state/", "•".dimmed());
                }
            } else {
                println!("{} No active session state found", "•".dimmed());
            }

            // Check for config files
            let config_candidates = ["astrape.yaml", "astrape.yml", "astrape.json", ".astrape.yaml"];
            let found_config: Vec<_> = config_candidates
                .iter()
                .filter(|p| Path::new(p).exists())
                .collect();

            println!();
            if found_config.is_empty() {
                println!("{} No config file found", "•".dimmed());
            } else {
                println!("{} Config files:", "✓".green());
                for cfg in found_config {
                    println!("  {} {}", "•".dimmed(), cfg);
                }
            }

            Ok(())
        }
    }
}

fn skill_command(action: SkillCommands) -> anyhow::Result<()> {
    match action {
        SkillCommands::List => {
            println!("{}", "⚡ Available Skills".bold());
            println!();

            let skills = create_builtin_skills();

            if skills.is_empty() {
                println!("{} No skills found.", "•".dimmed());
                println!();
                println!("Skills are loaded from SKILL.md files in the skills/ directory.");
                return Ok(());
            }

            for skill in &skills {
                println!("  {} {}", "•".cyan(), skill.name.bold());
                if !skill.description.is_empty() {
                    println!("    {}", skill.description.dimmed());
                }
                if let Some(agent) = &skill.agent {
                    println!("    Agent: {}", agent);
                }
                if let Some(model) = &skill.model {
                    println!("    Model: {}", model);
                }
                println!();
            }

            println!("{} {} skills available", "Total:".bold(), skills.len());
            Ok(())
        }
        SkillCommands::Show { name } => {
            if let Some(skill) = get_builtin_skill(&name) {
                println!("{}", format!("⚡ Skill: {}", skill.name).bold());
                println!();

                if !skill.description.is_empty() {
                    println!("{} {}", "Description:".cyan(), skill.description);
                }
                if let Some(agent) = &skill.agent {
                    println!("{} {}", "Agent:".cyan(), agent);
                }
                if let Some(model) = &skill.model {
                    println!("{} {}", "Model:".cyan(), model);
                }
                if let Some(hint) = &skill.argument_hint {
                    println!("{} {}", "Argument hint:".cyan(), hint);
                }

                println!();
                println!("{}", "Template:".cyan());
                println!("{}", "-".repeat(60).dimmed());
                println!("{}", skill.template);
                Ok(())
            } else {
                anyhow::bail!(
                    "Skill '{}' not found. Use 'astrape skill list' to see available skills.",
                    name
                );
            }
        }
    }
}
