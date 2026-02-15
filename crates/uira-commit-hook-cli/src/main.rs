mod agent_workflow;
mod comments;
mod config;
mod diagnostics;
mod hooks;
mod linter;
mod runtime;
mod typos;

use agent_workflow::detectors::{typos::TyposDetector, Scope};
use agent_workflow::{AgentWorkflow, TaskOptions, WorkflowConfig, WorkflowResult, WorkflowTask};
use clap::{Parser, Subcommand};
use colored::Colorize;
use config::Config;
use hooks::HookExecutor;
use linter::Linter;
use runtime::block_on;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process;
use uira_agent::{init_subscriber, TelemetryConfig};
use uira_orchestration::features::builtin_skills::{create_builtin_skills, get_builtin_skill};
use uira_orchestration::features::uira_state::has_uira_state;
use uira_orchestration::{create_uira_session, get_agent_definitions, AgentConfig, SessionOptions};

#[derive(Parser)]
#[command(name = "uira-commit-hook-cli")]
#[command(version, about = "⚡ Lightning-fast Rust-native git hooks manager & AI harness", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize uira configuration
    Init {
        #[arg(short, long, default_value = "uira.yml")]
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
        #[arg(long, help = "Only check staged files")]
        staged: bool,
        #[arg(long, help = "Automatically stage modified files after fixing")]
        stage: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        files: Vec<String>,
    },
    /// Format code (Rust via rustfmt/cargo fmt, JS/TS via oxfmt)
    Format {
        #[arg(long, help = "Check formatting without applying changes")]
        check: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        files: Vec<String>,
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
    /// Score-based verification goals
    Goals {
        #[command(subcommand)]
        action: GoalsCommands,
    },
    /// Run diagnostics (lsp_diagnostics) on files
    Diagnostics {
        #[arg(long, help = "Use AI to decide and apply fixes")]
        ai: bool,
        #[arg(long, help = "Only check staged files")]
        staged: bool,
        #[arg(long, help = "Automatically stage modified files after fixing")]
        stage: bool,
        #[arg(
            long,
            value_parser = clap::builder::PossibleValuesParser::new(["error", "warning", "all"]),
            help = "Severity filter: error, warning, all (default from config or 'error')"
        )]
        severity: Option<String>,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        files: Vec<String>,
    },
    /// Check and manage comments with AI assistance
    Comments {
        #[arg(long, help = "Use AI to decide whether to remove or keep comments")]
        ai: bool,
        #[arg(long, help = "Only check staged files")]
        staged: bool,
        #[arg(long, help = "Automatically stage modified files after fixing")]
        stage: bool,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        files: Vec<String>,
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
enum GoalsCommands {
    /// Check all goals or a specific goal
    Check {
        /// Optional goal name to check (checks all if not specified)
        name: Option<String>,
    },
    /// Watch goals continuously until all pass
    Watch {
        /// Check interval in seconds
        #[arg(short, long, default_value = "30")]
        interval: u64,
        /// Maximum iterations before giving up
        #[arg(short, long, default_value = "100")]
        max_iterations: u32,
    },
    /// List configured goals
    List,
}

fn main() {
    let telemetry_config = TelemetryConfig::default();
    init_subscriber(&telemetry_config);

    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { config } => init_command(&config),
        Commands::Install => install_command(),
        Commands::Run { hook } => run_command(&hook),
        Commands::Lint { files } => lint_command(&files),
        Commands::Typos {
            ai,
            staged,
            stage,
            files,
        } => typos_command(ai, staged, stage, &files),
        Commands::Format { check, files } => format_command(check, &files),
        Commands::Agent { action } => agent_command(action),
        Commands::Session { action } => session_command(action),
        Commands::Skill { action } => skill_command(action),
        Commands::Goals { action } => goals_command(action),
        Commands::Diagnostics {
            ai,
            staged,
            stage,
            severity,
            files,
        } => diagnostics_command(ai, staged, stage, severity.as_deref(), &files),
        Commands::Comments {
            ai,
            staged,
            stage,
            files,
        } => comments_command(ai, staged, stage, &files),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn init_command(config_path: &str) -> anyhow::Result<()> {
    println!("⚡ Initializing uira...\n");

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
    println!("   1. Run: uira-commit-hook-cli install");
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

    let config_path = "uira.yml";
    if !Path::new(config_path).exists() {
        anyhow::bail!(
            "Config file not found: {}. Run 'uira-commit-hook-cli init' first.",
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
            if !existing.contains("# uira-commit-hook-cli managed hook") {
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
# uira-commit-hook-cli managed hook - do not edit
# This hook was generated by uira-commit-hook-cli. To modify, edit uira.yml

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT" || exit 1

if [ -x "./target/release/uira-commit-hook-cli" ]; then
    exec ./target/release/uira-commit-hook-cli run {}
fi

if [ -x "./target/debug/uira-commit-hook-cli" ]; then
    exec ./target/debug/uira-commit-hook-cli run {}
fi

exec cargo run -q -p uira-commit-hook-cli -- run {}
"#,
        hook_name, hook_name, hook_name
    )
}

fn run_command(hook_name: &str) -> anyhow::Result<()> {
    let config_path = "uira.yml";

    if !Path::new(config_path).exists() {
        anyhow::bail!(
            "Config file not found: {}. Run 'uira-commit-hook-cli init' first.",
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

fn typos_command(ai: bool, staged: bool, stage: bool, files: &[String]) -> anyhow::Result<()> {
    if ai {
        println!("🔍 Starting AI-assisted typos workflow...\n");

        let config = WorkflowConfig {
            auto_stage: stage,
            staged_only: staged,
            files: files.to_vec(),
            ..Default::default()
        };

        block_on(async {
            let working_dir = std::env::current_dir()?;
            let detector = TyposDetector::new(&working_dir);
            let scope = if !files.is_empty() {
                Scope::from_files(working_dir.clone(), files.to_vec())
            } else if staged {
                Scope::from_staged(&working_dir)?
            } else {
                Scope::from_repo(&working_dir)?
            };

            let mut workflow = AgentWorkflow::new(
                WorkflowTask::Typos,
                config,
                Some(Box::new(detector)),
                Some(scope),
            )
            .await?;
            match workflow.run().await? {
                WorkflowResult::Complete {
                    iterations,
                    files_modified,
                    summary,
                } => {
                    println!("\n✅ Typos workflow complete!");
                    println!("   Iterations: {}", iterations);
                    println!("   Files modified: {}", files_modified.len());
                    if let Some(s) = summary {
                        println!("   Summary: {}", s);
                    }
                    Ok(())
                }
                WorkflowResult::MaxIterationsReached {
                    iterations,
                    files_modified,
                } => {
                    println!("\n⚠️  Max iterations ({}) reached", iterations);
                    println!("   Files modified: {}", files_modified.len());
                    std::process::exit(1);
                }
                WorkflowResult::VerificationFailed {
                    remaining_issues,
                    details,
                    ..
                } => {
                    println!(
                        "\n❌ Verification failed: {} issues remain",
                        remaining_issues
                    );
                    println!("   Details: {}", details);
                    std::process::exit(1);
                }
                WorkflowResult::Cancelled => {
                    println!("\n⚠️  Workflow cancelled");
                    std::process::exit(1);
                }
                WorkflowResult::Failed { error } => {
                    eprintln!("\n❌ Workflow failed: {}", error);
                    std::process::exit(1);
                }
            }
        })
    } else {
        println!("🔍 Checking for typos...\n");
        let mut cmd = std::process::Command::new("typos");

        if !files.is_empty() {
            cmd.args(files);
        } else if staged {
            let output = std::process::Command::new("git")
                .args(["diff", "--cached", "--name-only", "--diff-filter=ACM"])
                .output()
                .map_err(|_| anyhow::anyhow!("Failed to get staged files"))?;
            let staged_files: Vec<String> = String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect();
            if staged_files.is_empty() {
                println!("✓ No staged files to check");
                return Ok(());
            }
            cmd.args(&staged_files);
        } else {
            cmd.arg(".");
        }

        let status = cmd.status().map_err(|_| {
            anyhow::anyhow!("Failed to run typos. Is it installed? Run: cargo install typos-cli")
        })?;
        if !status.success() {
            process::exit(1);
        }
        println!("✓ No typos found");
        Ok(())
    }
}

fn format_command(check: bool, files: &[String]) -> anyhow::Result<()> {
    if check {
        println!("🔍 Checking formatting...\n");
    } else {
        println!("🎨 Formatting code...\n");
    }

    let mut ran_any = false;

    let (rust_files, non_rust): (Vec<&String>, Vec<&String>) =
        files.iter().partition(|p| p.ends_with(".rs"));

    if files.is_empty() || !rust_files.is_empty() {
        let mut cmd = if rust_files.is_empty() {
            let mut cmd = std::process::Command::new("cargo");
            cmd.arg("fmt");
            cmd
        } else {
            let mut cmd = std::process::Command::new("rustfmt");
            cmd.args(rust_files);
            cmd
        };

        if check {
            cmd.arg("--check");
        }

        let status = cmd.status().map_err(|_| {
            anyhow::anyhow!("Failed to run rustfmt. Install: rustup component add rustfmt")
        })?;

        ran_any = true;

        if !status.success() {
            if check {
                eprintln!("\n❌ Rust files are not formatted correctly");
                eprintln!("   Run 'uira-commit-hook-cli format' to fix formatting");
            }
            process::exit(1);
        }
    }

    // Filter non_rust to only include JS/TS files or directories for oxfmt
    let js_ts_extensions = [".js", ".jsx", ".ts", ".tsx", ".mjs", ".cjs", ".mts", ".cts"];
    let js_ts_files: Vec<&String> = non_rust
        .into_iter()
        .filter(|p| {
            // Allow directories (oxfmt can handle them)
            std::path::Path::new(p).is_dir() || js_ts_extensions.iter().any(|ext| p.ends_with(ext))
        })
        .collect();

    if files.is_empty() || !js_ts_files.is_empty() {
        let mut cmd = std::process::Command::new("oxfmt");
        if check {
            cmd.arg("--check");
        }

        if files.is_empty() {
            cmd.arg(".");
        } else {
            cmd.args(js_ts_files);
        }

        let status = cmd.status().map_err(|_| {
            anyhow::anyhow!("Failed to run oxfmt. Install: npm add -D oxfmt (or pnpm/yarn/bun)")
        })?;

        ran_any = true;

        if !status.success() {
            if check {
                eprintln!("\n❌ JS/TS files are not formatted correctly");
                eprintln!("   Run 'uira-commit-hook-cli format' to fix formatting");
            }
            process::exit(1);
        }
    }

    if !ran_any {
        println!("ℹ️  No files to format");
        return Ok(());
    }

    if check {
        println!("✓ All files are properly formatted");
    } else {
        println!("✓ Formatting complete");
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

fn agent_command(action: AgentCommands) -> anyhow::Result<()> {
    match action {
        AgentCommands::List => {
            println!("{}", "⚡ Available Agents".bold());
            println!();

            let agents = get_agent_definitions(None);
            let mut sorted_agents: Vec<_> = agents.iter().collect();
            sorted_agents.sort_by_key(|(name, _)| name.as_str());

            // Group agents by base name
            let mut groups: std::collections::HashMap<String, Vec<(&String, &AgentConfig)>> =
                std::collections::HashMap::new();

            for (name, config) in &sorted_agents {
                let base_name = name.split('-').next().unwrap_or(name).to_string();
                groups.entry(base_name).or_default().push((name, config));
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
                    println!("    {} {}{}", "•".dimmed(), name, model.dimmed());
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
                    format!(
                        "{}...\n\n[truncated, {} total chars]",
                        &agent.prompt[..500],
                        agent.prompt.len()
                    )
                } else {
                    agent.prompt.clone()
                };
                println!("{}", prompt_preview);
                Ok(())
            } else {
                anyhow::bail!(
                    "Agent '{}' not found. Use 'uira-commit-hook-cli agent list' to see available agents.",
                    name
                );
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
                    "Agent '{}' not found. Use 'uira-commit-hook-cli agent list' to see available agents.",
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
            println!(
                "{}",
                "Note: Full delegation requires active SDK session.".yellow()
            );
            println!("To delegate, use the delegate_task MCP tool:");
            println!();
            println!(
                "  delegate_task(agent=\"{}\", prompt=\"{}\"{})",
                agent,
                prompt,
                model
                    .map(|m| format!(", model=\"{}\"", m))
                    .unwrap_or_default()
            );

            Ok(())
        }
    }
}

fn session_command(action: SessionCommands) -> anyhow::Result<()> {
    match action {
        SessionCommands::Start { config: _ } => {
            println!("{}", "⚡ Starting Uira Session".bold());
            println!();

            let options = SessionOptions {
                working_directory: Some(std::env::current_dir()?.to_string_lossy().to_string()),
                ..Default::default()
            };

            let session = create_uira_session(Some(options));

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
                "{} Session ready. Use with Claude Agent SDK or uira-commit-hook-cli agent delegate.",
                "✓".green()
            );

            Ok(())
        }
        SessionCommands::Status => {
            println!("{}", "⚡ Session Status".bold());
            println!();

            // Check for state directory (session state)
            let state_dir = Path::new(".uira/state");
            let has_session_state = state_dir.exists();

            // Check for plan state file
            let has_plan_state = has_uira_state(".");

            if has_session_state || has_plan_state {
                println!("{} Active state found:", "✓".green());
                if has_session_state {
                    println!("  {} .uira/state/", "•".dimmed());
                }
                if has_plan_state {
                    println!("  {} .uira/boulder.json", "•".dimmed());
                }
            } else {
                println!("{} No active session state found", "•".dimmed());
            }

            // Check for config files
            let config_candidates = ["uira.yaml", "uira.yml", "uira.json", ".uira.yaml"];
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
                    "Skill '{}' not found. Use 'uira-commit-hook-cli skill list' to see available skills.",
                    name
                );
            }
        }
    }
}

fn goals_command(action: GoalsCommands) -> anyhow::Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async { goals_command_async(action).await })
}

fn diagnostics_command(
    ai: bool,
    staged: bool,
    stage: bool,
    severity: Option<&str>,
    files: &[String],
) -> anyhow::Result<()> {
    use anyhow::Context;
    use colored::Colorize;
    use std::process::Command;
    use uira_oxc::{LintRule, Linter, Severity};

    println!("🔍 Running diagnostics...\n");

    let files_to_check = if staged {
        let output = Command::new("git")
            .args(["diff", "--cached", "--name-only", "--diff-filter=ACMR"])
            .output()
            .context("Failed to get staged files")?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(String::from)
            .collect::<Vec<_>>()
    } else if files.is_empty() {
        collect_files_from_cwd()?
    } else {
        files.to_vec()
    };

    if files_to_check.is_empty() {
        println!("{} No files to check", "✓".green());
        return Ok(());
    }

    if ai {
        let workflow_config = WorkflowConfig {
            auto_stage: stage,
            staged_only: staged,
            files: files_to_check.clone(),
            task_options: TaskOptions {
                severity: severity.map(String::from),
                ..Default::default()
            },
            ..Default::default()
        };

        return block_on(async {
            let mut workflow =
                AgentWorkflow::new(WorkflowTask::Diagnostics, workflow_config, None, None).await?;
            match workflow.run().await? {
                WorkflowResult::Complete {
                    iterations,
                    files_modified,
                    summary,
                } => {
                    println!("\n✅ Diagnostics workflow complete!");
                    println!("   Iterations: {}", iterations);
                    println!("   Files modified: {}", files_modified.len());
                    if let Some(s) = summary {
                        println!("   Summary: {}", s);
                    }
                    Ok(())
                }
                WorkflowResult::MaxIterationsReached { .. } => {
                    println!("\n⚠️  Max iterations reached");
                    std::process::exit(1);
                }
                WorkflowResult::VerificationFailed {
                    remaining_issues, ..
                } => {
                    println!("\n❌ {} issues remain", remaining_issues);
                    std::process::exit(1);
                }
                WorkflowResult::Cancelled => {
                    println!("\n⚠️  Workflow cancelled");
                    std::process::exit(1);
                }
                WorkflowResult::Failed { error } => {
                    eprintln!("\n❌ Workflow failed: {}", error);
                    std::process::exit(1);
                }
            }
        });
    }

    let severity = severity.unwrap_or("error");

    let (js_ts_files, other_files): (Vec<_>, Vec<_>) = files_to_check.iter().partition(|f| {
        let ext = std::path::Path::new(f)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");
        matches!(
            ext,
            "js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
        )
    });

    let rust_files: Vec<_> = other_files
        .iter()
        .filter(|f| f.ends_with(".rs"))
        .cloned()
        .collect();

    let mut error_count = 0;
    let mut warning_count = 0;

    if !js_ts_files.is_empty() {
        let linter = Linter::new(LintRule::recommended());
        let js_files_owned: Vec<String> = js_ts_files.into_iter().cloned().collect();
        let diagnostics = linter.lint_files(&js_files_owned);

        let filtered: Vec<_> = diagnostics
            .iter()
            .filter(|d| match severity {
                "error" => matches!(d.severity, Severity::Error),
                "warning" => matches!(d.severity, Severity::Error | Severity::Warning),
                _ => true,
            })
            .collect();

        for d in &filtered {
            match d.severity {
                Severity::Error => error_count += 1,
                Severity::Warning => warning_count += 1,
                Severity::Info => {}
            }

            let severity_str = match d.severity {
                Severity::Error => "error".red().bold(),
                Severity::Warning => "warning".yellow().bold(),
                Severity::Info => "info".blue(),
            };

            println!(
                "{}:{}:{}: {} [{}]",
                d.file.dimmed(),
                d.line,
                d.column,
                severity_str,
                d.rule.cyan()
            );
            println!("  {}", d.message);
            if let Some(suggestion) = &d.suggestion {
                println!("  {}: {}", "suggestion".dimmed(), suggestion);
            }
            println!();
        }
    }

    if !rust_files.is_empty() {
        let output = Command::new("cargo")
            .arg("check")
            .arg("--message-format=short")
            .output()
            .context("Failed to run cargo check")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            for line in stderr.lines() {
                if line.contains("error")
                    && (severity == "error" || severity == "warning" || severity == "all")
                {
                    error_count += 1;
                    println!("{}", line.red());
                } else if line.contains("warning") && (severity == "warning" || severity == "all") {
                    warning_count += 1;
                    println!("{}", line.yellow());
                }
            }
        }
    }

    println!();
    if error_count > 0 {
        println!(
            "{} {} error(s), {} warning(s)",
            "✗".red().bold(),
            error_count,
            warning_count
        );
        process::exit(1);
    } else if warning_count > 0 {
        println!("{} {} warning(s)", "⚠".yellow().bold(), warning_count);
    } else {
        println!("{} No diagnostics found", "✓".green().bold());
    }

    Ok(())
}

fn comments_command(ai: bool, staged: bool, stage: bool, files: &[String]) -> anyhow::Result<()> {
    use anyhow::Context;
    use colored::Colorize;
    use std::process::Command;
    use uira_comment_checker::{CommentDetector, FilterChain};

    println!("💬 Checking comments...\n");

    let files_to_check = if staged {
        let output = Command::new("git")
            .args(["diff", "--cached", "--name-only", "--diff-filter=ACMR"])
            .output()
            .context("Failed to get staged files")?;
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(String::from)
            .collect::<Vec<_>>()
    } else if files.is_empty() {
        collect_files_from_cwd()?
    } else {
        files.to_vec()
    };

    if files_to_check.is_empty() {
        println!("{} No files to check", "✓".green());
        return Ok(());
    }

    if ai {
        let workflow_config = WorkflowConfig {
            auto_stage: stage,
            staged_only: staged,
            files: files_to_check.clone(),
            ..Default::default()
        };

        return block_on(async {
            let mut workflow =
                AgentWorkflow::new(WorkflowTask::Comments, workflow_config, None, None).await?;
            match workflow.run().await? {
                WorkflowResult::Complete {
                    iterations,
                    files_modified,
                    summary,
                } => {
                    println!("\n✅ Comments workflow complete!");
                    println!("   Iterations: {}", iterations);
                    println!("   Files modified: {}", files_modified.len());
                    if let Some(s) = summary {
                        println!("   Summary: {}", s);
                    }
                    Ok(())
                }
                WorkflowResult::MaxIterationsReached { .. } => {
                    println!("\n⚠️  Max iterations reached");
                    std::process::exit(1);
                }
                WorkflowResult::VerificationFailed {
                    remaining_issues, ..
                } => {
                    println!("\n❌ {} issues remain", remaining_issues);
                    std::process::exit(1);
                }
                WorkflowResult::Cancelled => {
                    println!("\n⚠️  Workflow cancelled");
                    std::process::exit(1);
                }
                WorkflowResult::Failed { error } => {
                    eprintln!("\n❌ Workflow failed: {}", error);
                    std::process::exit(1);
                }
            }
        });
    }

    let detector = CommentDetector::new();
    let filter_chain = FilterChain::new();
    let mut comment_count = 0;

    for file in &files_to_check {
        let content = match std::fs::read_to_string(file) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let comments = detector.detect(&content, file, false);

        for comment in comments {
            if filter_chain.should_skip(&comment) {
                continue;
            }
            comment_count += 1;

            println!(
                "{}:{}: {}",
                file.dimmed(),
                comment.line_number,
                comment.text.trim().yellow()
            );
        }
    }

    println!();
    if comment_count > 0 {
        println!(
            "{} Found {} comment(s). Use --ai to analyze with AI.",
            "!".yellow().bold(),
            comment_count
        );
    } else {
        println!("{} No actionable comments found", "✓".green().bold());
    }

    Ok(())
}

async fn goals_command_async(action: GoalsCommands) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let config_path = cwd.join("uira.yml");

    if !config_path.exists() {
        anyhow::bail!(
            "Config file not found: uira.yml. Run 'uira-commit-hook-cli init' first or create a config with goals."
        );
    }

    let config = uira_core::load_config(Some(&config_path))?;
    let goals = &config.goals.goals;

    if goals.is_empty() {
        println!("{}", "No goals configured in uira.yml".yellow());
        println!();
        println!("Add goals to your config:");
        println!(
            r#"
goals:
  goals:
    - name: pixel-match
      workspace: .uira/goals/pixel-match/
      command: bun run check.ts
      target: 99.9
    - name: test-coverage
      command: "bun run coverage --json | jq '.total'"
      target: 80
"#
        );
        return Ok(());
    }

    match action {
        GoalsCommands::List => {
            println!("{}", "⚡ Configured Goals".bold());
            println!();

            for goal in goals {
                let (status, status_color) = if goal.enabled {
                    ("✓", "green")
                } else {
                    ("○", "dimmed")
                };
                let status_display = if status_color == "green" {
                    status.green()
                } else {
                    status.dimmed()
                };
                println!(
                    "  {} {} (target: {:.1})",
                    status_display,
                    goal.name.bold(),
                    goal.target
                );
                if let Some(desc) = &goal.description {
                    println!("    {}", desc.dimmed());
                }
                println!("    Command: {}", goal.command.dimmed());
                if let Some(ws) = &goal.workspace {
                    println!("    Workspace: {}", ws.dimmed());
                }
                println!();
            }

            println!("{} {} goals configured", "Total:".bold(), goals.len());
            Ok(())
        }
        GoalsCommands::Check { name } => {
            let runner = uira_hooks::GoalRunner::new(&cwd);

            let goals_to_check: Vec<_> = if let Some(ref n) = name {
                goals.iter().filter(|g| g.name == *n).cloned().collect()
            } else {
                goals.clone()
            };

            if goals_to_check.is_empty() {
                if let Some(n) = name {
                    anyhow::bail!("Goal '{}' not found", n);
                }
            }

            println!("{}", "⚡ Checking Goals".bold());
            println!();

            let result = runner.check_all(&goals_to_check).await;

            for r in &result.results {
                let status = if r.passed { "✓".green() } else { "✗".red() };
                println!(
                    "  {} {} {:.1}/{:.1} ({}ms)",
                    status,
                    r.name.bold(),
                    r.score,
                    r.target,
                    r.duration_ms
                );
                if let Some(err) = &r.error {
                    println!("    Error: {}", err.red());
                }
            }

            println!();
            if result.all_passed {
                println!("{}", "✅ All goals passed!".green().bold());
            } else {
                let passed = result.results.iter().filter(|r| r.passed).count();
                println!(
                    "{}",
                    format!("❌ {}/{} goals passed", passed, result.results.len())
                        .red()
                        .bold()
                );
                process::exit(1);
            }
            Ok(())
        }
        GoalsCommands::Watch {
            interval,
            max_iterations,
        } => {
            let runner = uira_hooks::GoalRunner::new(&cwd);

            println!("{}", "⚡ Watching Goals".bold());
            println!(
                "Checking every {}s (max {} iterations)",
                interval, max_iterations
            );
            println!();

            let options = uira_hooks::VerifyOptions {
                check_interval_secs: interval,
                max_iterations,
                max_duration: None,
                on_progress: Some(Box::new(|result| {
                    println!(
                        "[Iteration {}] {}/{} goals passed",
                        result.iteration,
                        result.results.iter().filter(|r| r.passed).count(),
                        result.results.len()
                    );
                    for r in &result.results {
                        let status = if r.passed { "✓" } else { "✗" };
                        println!("  {} {}: {:.1}/{:.1}", status, r.name, r.score, r.target);
                    }
                    println!();
                })),
            };

            let result = runner.verify_until_complete(goals, options).await;

            if result.all_passed {
                println!("{}", "✅ All goals passed!".green().bold());
            } else {
                println!(
                    "{}",
                    format!(
                        "❌ Goals did not pass after {} iterations",
                        result.iteration
                    )
                    .red()
                    .bold()
                );
                process::exit(1);
            }
            Ok(())
        }
    }
}
