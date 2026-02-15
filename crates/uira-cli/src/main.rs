//! Uira - Native AI Coding Agent

use clap::{CommandFactory, Parser};
use clap_complete::{generate, Shell};
use colored::Colorize;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;
use uira_agent::{
    init_subscriber, init_tui_subscriber, Agent, AgentConfig, ExecutorConfig,
    RecursiveAgentExecutor, TelemetryConfig,
};
use uira_orchestration::{get_agent_definitions, ModelRegistry};
use uira_types::ExecutionResult;
use uira_providers::{
    AnthropicClient, GeminiClient, ModelClient, OllamaClient, OpenAIClient, OpenCodeClient,
    ProviderConfig,
};
use uira_sandbox::SandboxPolicy;

mod commands;
mod config;
mod rpc;
mod session;

use commands::{
    AuthCommands, Cli, CliMode, Commands, ConfigCommands, GatewayCommands, GoalsCommands,
    SessionsCommands, SkillsCommands, TasksCommands,
};
use config::CliConfig;
use session::{
    display_sessions_list, display_sessions_tree, list_rollout_sessions, SessionStorage,
};

#[tokio::main]
async fn main() {
    let telemetry_config = TelemetryConfig::default();

    let cli = Cli::parse();
    let config = CliConfig::load();

    let result = if cli.mode == CliMode::Rpc {
        init_subscriber(&telemetry_config);
        run_rpc(&cli, &config).await
    } else {
        match &cli.command {
            Some(Commands::Exec { prompt, json }) => {
                init_subscriber(&telemetry_config);
                run_exec(&cli, &config, prompt, *json).await
            }
            Some(Commands::Resume {
                session_id,
                fork,
                fork_at,
            }) => {
                init_subscriber(&telemetry_config);
                run_resume(session_id.as_deref(), *fork, *fork_at).await
            }
            Some(Commands::Sessions { command }) => {
                init_subscriber(&telemetry_config);
                run_sessions(command).await
            }
            Some(Commands::Auth { command }) => {
                init_subscriber(&telemetry_config);
                run_auth(command, &config).await
            }
            Some(Commands::Config { command }) => {
                init_subscriber(&telemetry_config);
                run_config(command, &config).await
            }
            Some(Commands::Goals { command }) => {
                init_subscriber(&telemetry_config);
                run_goals(command).await
            }
            Some(Commands::Tasks { command }) => {
                init_subscriber(&telemetry_config);
                run_tasks(command).await
            }
            Some(Commands::Completion { shell }) => {
                init_subscriber(&telemetry_config);
                generate_completions(*shell);
                Ok(())
            }
            Some(Commands::Gateway { command }) => {
                init_subscriber(&telemetry_config);
                run_gateway(command).await
            }
            Some(Commands::Skills { command }) => {
                init_subscriber(&telemetry_config);
                run_skills(command).await
            }
            None => {
                if let Some(prompt) = cli.get_prompt() {
                    init_subscriber(&telemetry_config);
                    run_exec(&cli, &config, &prompt, false).await
                } else {
                    let tracing_rx = init_tui_subscriber(&telemetry_config);
                    run_interactive(&cli, &config, Some(tracing_rx)).await
                }
            }
        }
    };

    if let Err(e) = result {
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(1);
    }
}

async fn run_rpc(cli: &Cli, config: &CliConfig) -> Result<(), Box<dyn std::error::Error>> {
    if cli.command.is_some() || cli.get_prompt().is_some() {
        return Err("RPC mode does not support subcommands or prompt arguments".into());
    }

    let uira_config = uira_core::loader::load_config(None).ok();
    let agent_model_overrides = build_agent_model_overrides(uira_config.as_ref());
    let agent_defs = get_agent_definitions(None);
    let registry = ModelRegistry::new();
    let (client, _provider_config) =
        create_client(cli, config, &agent_defs, &registry, &agent_model_overrides)?;

    let (external_mcp_servers, external_mcp_specs) =
        prepare_external_mcp(uira_config.as_ref()).await?;
    let agent_config = create_agent_config(
        cli,
        config,
        &agent_defs,
        uira_config.as_ref(),
        external_mcp_servers,
        external_mcp_specs,
    );

    rpc::run_rpc_mode(agent_config, client).await
}

async fn run_exec(
    cli: &Cli,
    config: &CliConfig,
    prompt: &str,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use futures::StreamExt;
    use uira_types::{Item, ThreadEvent};

    let uira_config = uira_core::loader::load_config(None).ok();
    let agent_model_overrides = build_agent_model_overrides(uira_config.as_ref());
    let agent_defs = get_agent_definitions(None);
    let registry = ModelRegistry::new();
    let (client, provider_config) =
        create_client(cli, config, &agent_defs, &registry, &agent_model_overrides)?;
    let (external_mcp_servers, external_mcp_specs) =
        prepare_external_mcp(uira_config.as_ref()).await?;
    let agent_config = create_agent_config(
        cli,
        config,
        &agent_defs,
        uira_config.as_ref(),
        external_mcp_servers,
        external_mcp_specs,
    );

    let executor_config = ExecutorConfig::new(provider_config, agent_config.clone());
    let executor = Arc::new(RecursiveAgentExecutor::new(executor_config));
    let agent = Agent::new_with_executor(agent_config, client, Some(executor)).with_rollout()?;

    if !json_output {
        println!("{} {}", "Running:".cyan().bold(), prompt.dimmed());
        println!();
    }

    if cli.verbose {
        let (mut agent, mut event_stream) = agent.with_event_stream();

        let event_printer = tokio::spawn(async move {
            while let Some(event) = event_stream.next().await {
                match &event {
                    ThreadEvent::TurnStarted { turn_number } => {
                        println!("{}", format!("── Turn {} ──", turn_number).cyan());
                    }
                    ThreadEvent::ContentDelta { delta } => {
                        print!("{}", delta);
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                    ThreadEvent::ThinkingDelta { thinking } => {
                        print!("{} {}", "thinking:".magenta(), thinking.dimmed());
                        use std::io::Write;
                        let _ = std::io::stdout().flush();
                    }
                    ThreadEvent::ItemStarted {
                        item: Item::ToolCall { name, .. },
                    } => {
                        println!("\n{} {}", "→".yellow(), name.yellow().bold());
                    }
                    ThreadEvent::ItemStarted { .. } => {}
                    ThreadEvent::ItemCompleted {
                        item: Item::ToolResult { output, .. },
                    } => {
                        println!("{}", output.dimmed());
                    }
                    ThreadEvent::ItemCompleted { .. } => {}
                    ThreadEvent::TurnCompleted { turn_number, usage } => {
                        println!(
                            "\n{}\n",
                            format!(
                                "── Turn {} complete ({}in/{}out) ──",
                                turn_number, usage.input_tokens, usage.output_tokens
                            )
                            .dimmed()
                        );
                    }
                    ThreadEvent::Error { message, .. } => {
                        println!("{}: {}", "Error".red().bold(), message);
                    }
                    ThreadEvent::ThreadCompleted { usage } => {
                        println!(
                            "{}",
                            format!(
                                "✓ completed (total: {}in/{}out)",
                                usage.input_tokens, usage.output_tokens
                            )
                            .green()
                        );
                        break;
                    }
                    ThreadEvent::ThreadCancelled => {
                        println!("{}", "Thread cancelled".yellow());
                        break;
                    }
                    _ => {}
                }
            }
        });

        let result = agent.run(prompt).await?;
        let _ = event_printer.await;

        println!();
        println!("{}", "─".repeat(40).dimmed());
        if json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            print_result(&result);
        }
    } else {
        let mut agent = agent;
        let result = agent.run(prompt).await?;

        if json_output {
            println!("{}", serde_json::to_string_pretty(&result)?);
        } else {
            print_result(&result);
        }
    }

    Ok(())
}

async fn run_resume(
    session_id: Option<&str>,
    fork: bool,
    fork_at: Option<usize>,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = SessionStorage::new()?;

    match session_id {
        Some(id) => {
            let action = if fork { "Forking from" } else { "Resuming" };
            println!(
                "{} {}",
                format!("{} session:", action).cyan().bold(),
                id.yellow()
            );

            if fork {
                println!(
                    "{}",
                    format!(
                        "Creating fork{}",
                        fork_at
                            .map(|n| format!(" at message {}", n))
                            .unwrap_or_default()
                    )
                    .dimmed()
                );
            }

            let session = storage.load(id)?;

            println!("{}", "─".repeat(50).dimmed());
            println!("{}: {}", "Provider".cyan(), session.meta.provider.yellow());
            println!("{}: {}", "Model".cyan(), session.meta.model.yellow());
            println!(
                "{}: {}",
                "Turns".cyan(),
                session.meta.turns.to_string().yellow()
            );
            println!("{}: {}", "Summary".cyan(), session.meta.summary.dimmed());
            println!("{}", "─".repeat(50).dimmed());
            println!();

            let messages_to_show = if let Some(count) = fork_at {
                &session.messages[..count.min(session.messages.len())]
            } else {
                &session.messages
            };

            for msg in messages_to_show {
                let role = match msg.role {
                    uira_types::Role::User => "User".green(),
                    uira_types::Role::Assistant => "Assistant".blue(),
                    uira_types::Role::System => "System".yellow(),
                    uira_types::Role::Tool => "Tool".magenta(),
                };
                let content = get_message_text(&msg.content);
                println!("{}: {}", role.bold(), content);
                println!();
            }

            if fork {
                println!(
                    "{}",
                    "Fork created. Start a new conversation from this point.".green()
                );
            } else {
                println!(
                    "{}",
                    "To continue this session, use the messages above as context.".dimmed()
                );
            }
        }
        _ => {
            println!("{}", "Recent sessions:".cyan().bold());
            println!("{}", "─".repeat(80).dimmed());

            let sessions = storage.list_recent(10)?;

            if sessions.is_empty() {
                println!("{}", "No sessions found.".dimmed());
            } else {
                for session in sessions {
                    let age = chrono::Utc::now()
                        .signed_duration_since(session.updated_at)
                        .num_minutes();
                    let age_str = if age < 60 {
                        format!("{}m ago", age)
                    } else if age < 1440 {
                        format!("{}h ago", age / 60)
                    } else {
                        format!("{}d ago", age / 1440)
                    };

                    println!(
                        "{} {} {} ({}, {} turns)",
                        session.id.get(..8).unwrap_or(&session.id).yellow(),
                        age_str.dimmed(),
                        session.summary,
                        session.model.cyan(),
                        session.turns
                    );
                }
            }

            println!("{}", "─".repeat(80).dimmed());
            println!(
                "{}",
                "Use 'uira resume <session_id>' to resume a session".dimmed()
            );
            println!(
                "{}",
                "Use 'uira resume <session_id> --fork' to create a fork".dimmed()
            );
        }
    }
    Ok(())
}

async fn run_sessions(command: &SessionsCommands) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        SessionsCommands::List { limit, tree } => {
            println!("{}", "Sessions (from rollout files):".cyan().bold());
            println!("{}", "─".repeat(82).dimmed());

            let entries = list_rollout_sessions(*limit)?;

            if *tree {
                display_sessions_tree(&entries);
            } else {
                display_sessions_list(&entries);
            }

            println!("{}", "─".repeat(82).dimmed());
            println!(
                "{}",
                "Use 'uira sessions list --tree' to show fork relationships".dimmed()
            );
        }
        SessionsCommands::Info { session_id } => {
            println!("{} {}", "Session info:".cyan().bold(), session_id.yellow());

            let entries = list_rollout_sessions(1000)?;
            let entry = entries
                .iter()
                .find(|e| e.thread_id == *session_id || e.thread_id.starts_with(session_id))
                .ok_or_else(|| format!("Session not found: {}", session_id))?;

            println!("{}", "─".repeat(50).dimmed());
            println!("{}: {}", "Session ID".cyan(), entry.thread_id.yellow());
            println!("{}: {}", "Timestamp".cyan(), entry.timestamp);
            println!("{}: {}", "Provider".cyan(), entry.provider.yellow());
            println!("{}: {}", "Model".cyan(), entry.model.yellow());
            println!("{}: {}", "Turns".cyan(), entry.turns);
            println!("{}: {}", "Fork count".cyan(), entry.fork_count);
            if let Some(ref parent) = entry.parent_id {
                println!("{}: {}", "Parent session".cyan(), parent.yellow());
            }
            println!("{}: {}", "Path".cyan(), entry.path.display());
            println!("{}", "─".repeat(50).dimmed());
        }
        SessionsCommands::Delete { session_id } => {
            println!(
                "{} {}",
                "Deleting session:".red().bold(),
                session_id.yellow()
            );

            let entries = list_rollout_sessions(1000)?;
            let entry = entries
                .iter()
                .find(|e| e.thread_id == *session_id || e.thread_id.starts_with(session_id))
                .ok_or_else(|| format!("Session not found: {}", session_id))?;

            std::fs::remove_file(&entry.path)?;
            println!("{} Session deleted", "✓".green().bold());
        }
    }
    Ok(())
}

async fn run_auth(
    command: &AuthCommands,
    _config: &CliConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use uira_providers::providers::{AnthropicAuth, GoogleAuth, OpenAIAuth};
    use uira_providers::{AuthProvider, CredentialStore, OAuthCallbackServer, StoredCredential};

    const DEFAULT_OAUTH_PORT: u16 = 8765;

    match command {
        AuthCommands::Login { provider } => {
            let provider = match provider {
                Some(p) => p.clone(),
                _ => {
                    println!("{}", "Available providers:".cyan().bold());
                    println!("{}", "─".repeat(50).dimmed());
                    println!("  {} anthropic  - Anthropic (Claude)", "•".cyan());
                    println!("  {} openai     - OpenAI (GPT models)", "•".cyan());
                    println!("  {} google     - Google (Gemini models)", "•".cyan());
                    println!("{}", "─".repeat(50).dimmed());
                    println!();
                    println!("{}", "Usage: uira auth login <provider>".dimmed());
                    println!("{}", "Example: uira auth login anthropic".dimmed());
                    return Ok(());
                }
            };

            println!("{} {}", "Logging in to:".cyan().bold(), provider.yellow());

            let provider_lower = provider.to_lowercase();
            let is_anthropic = provider_lower == "anthropic";

            let (auth_provider, oauth_port): (Box<dyn AuthProvider>, u16) =
                match provider_lower.as_str() {
                    "anthropic" => (Box::new(AnthropicAuth::new()), DEFAULT_OAUTH_PORT),
                    "openai" => (Box::new(OpenAIAuth::new()), OpenAIAuth::oauth_port()),
                    "google" | "gemini" => (Box::new(GoogleAuth::new()), DEFAULT_OAUTH_PORT),
                    _ => {
                        return Err(format!(
                        "Unknown provider: {}. Run 'uira auth login' to see available providers.",
                        provider
                    )
                        .into());
                    }
                };

            println!("{}", "Starting OAuth flow...".dimmed());
            let challenge = auth_provider.start_oauth(0).await?;

            // Anthropic uses code-copy flow (user pastes code manually)
            // Other providers use localhost callback
            let auth_code = if is_anthropic {
                // Open browser
                println!("{}", "Opening browser for authorization...".cyan());
                println!(
                    "{}",
                    format!("If browser doesn't open, visit:\n{}", challenge.url).dimmed()
                );

                if let Err(e) = webbrowser::open(&challenge.url) {
                    eprintln!("{}: {}", "Warning: Failed to open browser".yellow(), e);
                    println!("{}", "Please open the URL manually.".yellow());
                }

                println!();
                println!(
                    "{}",
                    "After authorizing, copy the code from the browser and paste it here.".cyan()
                );
                println!(
                    "{}",
                    "The code will be displayed on Anthropic's page after you authorize.".dimmed()
                );
                println!();

                print!("{}", "Enter authorization code: ".green().bold());
                use std::io::Write;
                std::io::stdout().flush()?;

                let mut code_input = String::new();
                std::io::stdin().read_line(&mut code_input)?;
                let code = code_input.trim().to_string();

                if code.is_empty() {
                    return Err("Authorization code cannot be empty. Please try again.".into());
                }

                code
            } else {
                // Use localhost callback server for other providers
                let server = Arc::new(OAuthCallbackServer::new(oauth_port));

                let server_clone = server.clone();
                std::thread::spawn(move || {
                    let rt = tokio::runtime::Runtime::new().unwrap();
                    rt.block_on(async {
                        let _ = server_clone.start().await;
                    });
                });

                tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

                println!("{}", "Opening browser for authorization...".cyan());
                println!(
                    "{}",
                    format!("If browser doesn't open, visit: {}", challenge.url).dimmed()
                );

                if let Err(e) = webbrowser::open(&challenge.url) {
                    eprintln!("{}: {}", "Warning: Failed to open browser".yellow(), e);
                    println!("{}", "Please open the URL manually.".yellow());
                }

                println!("{}", "Waiting for authorization...".dimmed());
                let callback = server.wait_for_callback(&challenge.state).await?;
                callback.code
            };

            // Exchange code for tokens
            println!("{}", "Exchanging authorization code for tokens...".dimmed());
            let tokens = auth_provider
                .exchange_code(&auth_code, &challenge.verifier)
                .await?;

            // Save credentials
            let mut store = CredentialStore::load()?;

            // Convert tokens to StoredCredential via JSON to handle secrecy version mismatch
            let credential_json = serde_json::json!({
                "type": "oauth",
                "access_token": tokens.access_token,
                "refresh_token": tokens.refresh_token,
                "expires_at": tokens.expires_at,
            });
            let credential: StoredCredential = serde_json::from_value(credential_json)?;

            store.insert(provider.to_lowercase(), credential);
            store.save()?;

            println!(
                "{} Successfully authenticated with {}",
                "✓".green().bold(),
                provider.yellow()
            );
        }
        AuthCommands::Logout { provider } => {
            let mut store = CredentialStore::load()?;

            if store.remove(&provider.to_lowercase()).is_some() {
                store.save()?;
                println!(
                    "{} Logged out from {}",
                    "✓".green().bold(),
                    provider.yellow()
                );
            } else {
                println!(
                    "{} No credentials found for {}",
                    "✗".red(),
                    provider.yellow()
                );
            }
        }
        AuthCommands::Status => {
            println!("{}", "Authentication status:".cyan().bold());
            println!("{}", "─".repeat(50).dimmed());

            let store = CredentialStore::load()?;

            if store.is_empty() {
                println!("{}", "No providers configured".dimmed());
                println!();
                println!("{}", "To authenticate, run:".dimmed());
                println!("  {} uira auth login <provider>", "→".cyan());
                println!();
                println!("{}", "Supported providers:".dimmed());
                println!("  • anthropic");
                println!("  • openai");
                println!("  • google");
            } else {
                println!("{}", "Configured providers:".green());
                for provider in store.providers() {
                    println!("  {} {}", "✓".green(), provider.yellow());
                }
            }

            println!("{}", "─".repeat(50).dimmed());
        }
    }
    Ok(())
}

async fn run_config(
    command: &ConfigCommands,
    config: &CliConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        ConfigCommands::Show => {
            println!("{}", "Current configuration:".cyan().bold());
            println!("{}", serde_json::to_string_pretty(config)?);
        }
        ConfigCommands::Get { key } => {
            let value = match key.as_str() {
                "default_provider" => config.default_provider.clone(),
                "default_model" => config.default_model.clone(),
                "colors" => Some(config.colors.to_string()),
                "verbose" => Some(config.verbose.to_string()),
                _ => None,
            };
            match value {
                Some(v) => println!("{}", v),
                _ => println!("{}: {}", "Unknown key".red(), key),
            }
        }
        ConfigCommands::Set { key, value } => {
            let mut new_config = config.clone();

            match key.as_str() {
                "default_provider" => new_config.default_provider = Some(value.clone()),
                "default_model" => new_config.default_model = Some(value.clone()),
                "colors" => new_config.colors = value.parse().unwrap_or(true),
                "verbose" => new_config.verbose = value.parse().unwrap_or(false),
                _ => {
                    println!("{}: {}", "Unknown key".red(), key);
                    return Ok(());
                }
            }

            new_config.save()?;

            println!(
                "{} {} = {}",
                "Saved:".green().bold(),
                key.yellow(),
                value.green()
            );
        }
        ConfigCommands::Reset => {
            println!("{}", "Resetting configuration to defaults...".cyan().bold());
            let default_config = CliConfig::default();
            default_config.save()?;
            println!("{}", "Configuration reset to defaults.".green());
        }
    }
    Ok(())
}

async fn run_goals(command: &GoalsCommands) -> Result<(), Box<dyn std::error::Error>> {
    use uira_core::loader::load_config;
                    use uira_hooks::GoalRunner;

    match command {
        GoalsCommands::Check => {
            println!("{}", "Running goal verification...".cyan().bold());
            println!("{}", "─".repeat(50).dimmed());

            let config = load_config(None)?;
            let goals_config = &config.goals;

            if goals_config.goals.is_empty() {
                println!("{}", "No goals configured.".yellow());
                println!("Add goals to your uira.jsonc configuration file.");
                return Ok(());
            }

            let runner = GoalRunner::new(std::env::current_dir()?);
            let result = runner.check_all(&goals_config.goals).await;

            println!();
            for goal_result in &result.results {
                let status = if goal_result.passed {
                    "✓".green()
                } else {
                    "✗".red()
                };

                println!(
                    "{} {} {:.1}/{:.1} ({}ms)",
                    status,
                    goal_result.name.bold(),
                    goal_result.score.to_string().cyan(),
                    goal_result.target.to_string().dimmed(),
                    goal_result.duration_ms
                );

                if let Some(ref error) = goal_result.error {
                    println!("  {} {}", "Error:".red(), error.dimmed());
                }
            }

            println!("{}", "─".repeat(50).dimmed());

            let passed = result.results.iter().filter(|r| r.passed).count();
            let total = result.results.len();

            if result.all_passed {
                println!(
                    "{} All goals passed ({}/{})",
                    "✓".green().bold(),
                    passed,
                    total
                );
            } else {
                println!(
                    "{} Some goals failed ({}/{})",
                    "✗".red().bold(),
                    passed,
                    total
                );
                std::process::exit(1);
            }
        }
        GoalsCommands::List => {
            println!("{}", "Configured goals:".cyan().bold());
            println!("{}", "─".repeat(80).dimmed());

            let config = load_config(None)?;
            let goals_config = &config.goals;

            if goals_config.goals.is_empty() {
                println!("{}", "No goals configured.".yellow());
                println!("Add goals to your uira.jsonc configuration file.");
                return Ok(());
            }

            for goal in &goals_config.goals {
                let status = if goal.enabled {
                    "enabled".green()
                } else {
                    "disabled".dimmed()
                };

                println!("{} ({})", goal.name.bold(), status);
                if let Some(desc) = &goal.description {
                    println!("  {}", desc.dimmed());
                }
                println!("  Command: {}", goal.command.cyan());
                println!("  Target:  {:.1}", goal.target);
                println!("  Timeout: {}s", goal.timeout_secs);
                if let Some(workspace) = &goal.workspace {
                    println!("  Workspace: {}", workspace.yellow());
                }
                println!();
            }
        }
        GoalsCommands::Status => {
            println!("{}", "Goal verification status:".cyan().bold());
            println!("{}", "─".repeat(50).dimmed());

            let config = load_config(None)?;
            let goals_config = &config.goals;

            if goals_config.goals.is_empty() {
                println!("{}", "No goals configured.".yellow());
                return Ok(());
            }

            let enabled = goals_config.goals.iter().filter(|g| g.enabled).count();
            let total = goals_config.goals.len();

            println!("Total goals:   {}", total);
            println!("Enabled goals: {}", enabled.to_string().green());
            println!("Disabled goals: {}", (total - enabled).to_string().dimmed());

            if enabled > 0 {
                println!();
                println!("Run 'uira goals check' to verify all goals.");
            }
        }
    }

    Ok(())
}

async fn run_tasks(command: &TasksCommands) -> Result<(), Box<dyn std::error::Error>> {
    use uira_orchestration::background_agent::{BackgroundManager, BackgroundTaskConfig};

    let config = BackgroundTaskConfig::default();
    let manager = BackgroundManager::new(config);

    match command {
        TasksCommands::List => {
            println!("{}", "Background tasks:".cyan().bold());
            println!("{}", "─".repeat(80).dimmed());

            let tasks = manager.get_all_tasks();

            if tasks.is_empty() {
                println!("{}", "No background tasks.".dimmed());
                return Ok(());
            }

            for task in tasks {
                let status_str = match task.status {
                    uira_orchestration::background_agent::BackgroundTaskStatus::Queued => {
                        "queued".yellow()
                    }
                    uira_orchestration::background_agent::BackgroundTaskStatus::Pending => {
                        "pending".yellow()
                    }
                    uira_orchestration::background_agent::BackgroundTaskStatus::Running => {
                        "running".cyan()
                    }
                    uira_orchestration::background_agent::BackgroundTaskStatus::Completed => {
                        "completed".green()
                    }
                    uira_orchestration::background_agent::BackgroundTaskStatus::Error => {
                        "error".red()
                    }
                    uira_orchestration::background_agent::BackgroundTaskStatus::Cancelled => {
                        "cancelled".dimmed()
                    }
                };

                println!(
                    "{} {} {}",
                    task.id.get(..12).unwrap_or(&task.id).yellow(),
                    status_str,
                    task.description
                );
                println!("  Agent: {}", task.agent.cyan());

                if let Some(ref progress) = task.progress {
                    println!("  Progress: {} tool calls", progress.tool_calls);
                    if let Some(ref last_tool) = progress.last_tool {
                        println!("  Last tool: {}", last_tool.dimmed());
                    }
                }

                let elapsed = if let Some(completed) = task.completed_at {
                    completed.signed_duration_since(task.started_at)
                } else {
                    chrono::Utc::now().signed_duration_since(task.started_at)
                };

                let elapsed_secs = elapsed.num_seconds();
                if elapsed_secs > 0 {
                    println!("  Elapsed: {}s", elapsed_secs);
                }

                println!();
            }
        }
        TasksCommands::Status { task_id } => {
            let task = manager
                .get_task(task_id)
                .ok_or_else(|| format!("Task not found: {}", task_id))?;

            println!("{}", "Task details:".cyan().bold());
            println!("{}", "─".repeat(50).dimmed());
            println!("{}: {}", "ID".cyan(), task.id.yellow());
            println!("{}: {}", "Description".cyan(), task.description);
            println!("{}: {}", "Agent".cyan(), task.agent.cyan());

            let status_str = match task.status {
                uira_orchestration::background_agent::BackgroundTaskStatus::Queued => {
                    "queued".yellow()
                }
                uira_orchestration::background_agent::BackgroundTaskStatus::Pending => {
                    "pending".yellow()
                }
                uira_orchestration::background_agent::BackgroundTaskStatus::Running => {
                    "running".cyan()
                }
                uira_orchestration::background_agent::BackgroundTaskStatus::Completed => {
                    "completed".green()
                }
                uira_orchestration::background_agent::BackgroundTaskStatus::Error => {
                    "error".red()
                }
                uira_orchestration::background_agent::BackgroundTaskStatus::Cancelled => {
                    "cancelled".dimmed()
                }
            };

            println!("{}: {}", "Status".cyan(), status_str);

            if let Some(ref progress) = task.progress {
                println!("{}: {}", "Tool calls".cyan(), progress.tool_calls);
                if let Some(ref last_tool) = progress.last_tool {
                    println!("{}: {}", "Last tool".cyan(), last_tool);
                }
                if let Some(ref last_message) = progress.last_message {
                    println!("{}: {}", "Last message".cyan(), last_message);
                }
            }

            if let Some(ref result) = task.result {
                println!("{}: {}", "Result".cyan(), result.green());
            }

            if let Some(ref error) = task.error {
                println!("{}: {}", "Error".red().bold(), error.red());
            }

            println!("{}: {}", "Started".cyan(), task.started_at);
            if let Some(completed) = task.completed_at {
                println!("{}: {}", "Completed".cyan(), completed);
            }
        }
        TasksCommands::Cancel { task_id } => {
            println!("{} {}", "Cancelling task:".cyan().bold(), task_id.yellow());

            let _task = manager
                .cancel_task(task_id)
                .ok_or_else(|| format!("Task not found: {}", task_id))?;

            println!("{} Task cancelled", "✓".green().bold());
            println!("Status: {}", "cancelled".dimmed());
        }
    }

    Ok(())
}

async fn run_skills(command: &SkillsCommands) -> Result<(), Box<dyn std::error::Error>> {
    use uira_core::loader::load_config;
    use uira_gateway::skills::discover_skills;

    let config = load_config(None)?;
    let skills_settings = &config.skills;

    match command {
        SkillsCommands::List => {
            println!("{}", "Discovered skills:".cyan().bold());
            println!("{}", "─".repeat(80).dimmed());

            let paths: Vec<&str> = skills_settings.paths.iter().map(|s| s.as_str()).collect();
            let skills = discover_skills(&paths)?;

            if skills.is_empty() {
                println!("{}", "No skills found.".yellow());
                println!(
                    "{}",
                    format!(
                        "Add SKILL.md files to: {}",
                        skills_settings.paths.join(", ")
                    )
                    .dimmed()
                );
                return Ok(());
            }

            for skill in &skills {
                let emoji = skill
                    .metadata
                    .metadata
                    .as_ref()
                    .and_then(|m| m.emoji.as_deref())
                    .unwrap_or("📦");

                let active_marker = if skills_settings.active.contains(&skill.name) {
                    "✓".green()
                } else {
                    " ".normal()
                };

                println!(
                    "{} {} {} {}",
                    active_marker,
                    emoji,
                    skill.name.bold(),
                    format!("- {}", skill.metadata.description).dimmed()
                );
            }

            println!();
            println!("{}", "─".repeat(80).dimmed());
            println!(
                "Total: {} skills ({} active)",
                skills.len(),
                skills_settings.active.len()
            );
        }
        SkillsCommands::Show { name } => {
            let paths: Vec<&str> = skills_settings.paths.iter().map(|s| s.as_str()).collect();
            let skills = discover_skills(&paths)?;

            let skill = skills
                .iter()
                .find(|s| s.name == *name)
                .ok_or_else(|| format!("Skill not found: {}", name))?;

            println!(
                "{} {}",
                "Skill:".cyan().bold(),
                skill.name.yellow().bold()
            );
            println!("{}", "─".repeat(80).dimmed());

            let content = std::fs::read_to_string(&skill.path)?;
            println!("{}", content);
        }
        SkillsCommands::Install { path } => {
            use std::path::Path;
            use uira_gateway::skills::SkillLoader;

            let source_path = Path::new(path);

            if !source_path.exists() {
                return Err(format!("Path does not exist: {}", path).into());
            }

            if !source_path.is_dir() {
                return Err(format!("Path is not a directory: {}", path).into());
            }

            let skill_md = source_path.join("SKILL.md");
            if !skill_md.exists() {
                return Err(format!("SKILL.md not found in: {}", path).into());
            }

            let loader = SkillLoader::new(&[path])?;
            if loader.discovered.is_empty() {
                return Err(format!("No valid skill found in: {}", path).into());
            }

            let skill_info = &loader.discovered[0];
            let metadata = &skill_info.metadata;

            let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
            let skills_dir = home_dir.join(".uira/skills");
            std::fs::create_dir_all(&skills_dir)?;

            let dest_path = skills_dir.join(&metadata.name);

            if dest_path.exists() {
                return Err(format!(
                    "Skill already exists: {}. Remove it first to reinstall.",
                    dest_path.display()
                )
                .into());
            }

            fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
                std::fs::create_dir_all(dst)?;
                for entry in std::fs::read_dir(src)? {
                    let entry = entry?;
                    let ty = entry.file_type()?;
                    let src_path = entry.path();
                    let dst_path = dst.join(entry.file_name());

                    if ty.is_dir() {
                        copy_dir_recursive(&src_path, &dst_path)?;
                    } else {
                        std::fs::copy(&src_path, &dst_path)?;
                    }
                }
                Ok(())
            }

            copy_dir_recursive(source_path, &dest_path)?;

            println!(
                "{} Installed skill: {}",
                "✓".green().bold(),
                metadata.name.yellow()
            );
            println!("Location: {}", dest_path.display().to_string().dimmed());
            println!();
            println!(
                "{}",
                format!(
                    "To activate, add '{}' to the 'skills.active' list in your config.",
                    metadata.name
                )
                .dimmed()
            );
        }
    }

    Ok(())
}

async fn run_gateway(command: &GatewayCommands) -> Result<(), Box<dyn std::error::Error>> {
    use std::collections::HashMap;
    use uira_gateway::channel_bridge::ChannelSkillConfig;
    use uira_gateway::{
        ChannelBridge, GatewayServer, SkillLoader, SlackChannel, TelegramChannel,
    };

    match command {
        GatewayCommands::Start {
            host,
            port,
            auth_token,
        } => {
            let config = uira_core::loader::load_config(None).ok();
            let mut gateway_settings = config
                .as_ref()
                .map(|c| &c.gateway)
                .cloned()
                .unwrap_or_default();

            let bind_host = host
                .clone()
                .unwrap_or_else(|| gateway_settings.host.clone());
            let bind_port = port.unwrap_or(gateway_settings.port);

            if auth_token.is_some() {
                gateway_settings.auth_token = auth_token.clone();
            }

            let server = GatewayServer::new_with_settings(gateway_settings);
            let session_manager = server.session_manager();

            let channel_settings = config
                .as_ref()
                .map(|c| c.channels.clone())
                .unwrap_or_default();

            let mut telegram_configs = Vec::new();
            if let Some(tg) = channel_settings.telegram {
                telegram_configs.push(tg);
            }
            telegram_configs.extend(channel_settings.telegram_accounts);

            let mut slack_configs = Vec::new();
            if let Some(sl) = channel_settings.slack {
                slack_configs.push(sl);
            }
            slack_configs.extend(channel_settings.slack_accounts);

            let has_channels = !telegram_configs.is_empty() || !slack_configs.is_empty();

            let mut bridge = if has_channels {
                let mut channel_active_skills: HashMap<String, Vec<String>> = HashMap::new();

                for cfg in &telegram_configs {
                    if !cfg.active_skills.is_empty() {
                        channel_active_skills
                            .entry("telegram".to_string())
                            .or_default()
                            .extend(cfg.active_skills.iter().cloned());
                    }
                }
                for cfg in &slack_configs {
                    if !cfg.active_skills.is_empty() {
                        channel_active_skills
                            .entry("slack".to_string())
                            .or_default()
                            .extend(cfg.active_skills.iter().cloned());
                    }
                }

                for skills in channel_active_skills.values_mut() {
                    skills.sort();
                    skills.dedup();
                }

                let skill_config = if !channel_active_skills.is_empty() {
                    let skill_paths = config
                        .as_ref()
                        .map(|c| c.skills.paths.clone())
                        .unwrap_or_default();

                    SkillLoader::new(&skill_paths)
                        .and_then(|loader| {
                            ChannelSkillConfig::from_active_skills(
                                Some(&loader),
                                channel_active_skills,
                            )
                        })
                        .unwrap_or_else(|e| {
                            tracing::warn!("Failed to load channel skills: {e}");
                            ChannelSkillConfig::new()
                        })
                } else {
                    ChannelSkillConfig::new()
                };

                Some(ChannelBridge::with_skill_config(
                    session_manager,
                    skill_config,
                ))
            } else {
                None
            };

            let mut channel_count = 0usize;

            if let Some(ref mut bridge) = bridge {
                for tg_config in telegram_configs {
                    let account_id = tg_config.account_id.clone();
                    let channel = TelegramChannel::new(tg_config);
                    match bridge
                        .register_channel(Box::new(channel), account_id.clone())
                        .await
                    {
                        Ok(()) => {
                            tracing::info!(account_id = %account_id, "Telegram channel registered");
                            channel_count += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                account_id = %account_id,
                                error = %e,
                                "Failed to register Telegram channel"
                            );
                        }
                    }
                }

                for sl_config in slack_configs {
                    let account_id = sl_config.account_id.clone();
                    let channel = SlackChannel::new(sl_config);
                    match bridge
                        .register_channel(Box::new(channel), account_id.clone())
                        .await
                    {
                        Ok(()) => {
                            tracing::info!(account_id = %account_id, "Slack channel registered");
                            channel_count += 1;
                        }
                        Err(e) => {
                            tracing::error!(
                                account_id = %account_id,
                                error = %e,
                                "Failed to register Slack channel"
                            );
                        }
                    }
                }
            }

            if channel_count > 0 {
                println!(
                    "{}",
                    format!(
                        "Gateway started on ws://{}:{} ({channel_count} channel(s) active)",
                        bind_host, bind_port,
                    )
                    .green()
                    .bold()
                );
            } else {
                println!(
                    "{}",
                    format!("Gateway started on ws://{}:{}", bind_host, bind_port)
                        .green()
                        .bold()
                );
            }

            server.start(&bind_host, bind_port).await?;

            if let Some(mut bridge) = bridge {
                bridge.stop().await;
                tracing::info!("Channel bridge stopped");
            }

            Ok(())
        }
    }
}

async fn run_interactive(
    cli: &Cli,
    config: &CliConfig,
    tracing_rx: Option<UnboundedReceiver<String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::io::stdout;

    let uira_config = uira_core::loader::load_config(None).ok();
    let agent_model_overrides = build_agent_model_overrides(uira_config.as_ref());
    let agent_defs = get_agent_definitions(None);
    let registry = ModelRegistry::new();
    let (client, provider_config) =
        create_client(cli, config, &agent_defs, &registry, &agent_model_overrides)?;
    let (external_mcp_servers, external_mcp_specs) =
        prepare_external_mcp(uira_config.as_ref()).await?;
    let agent_config = create_agent_config(
        cli,
        config,
        &agent_defs,
        uira_config.as_ref(),
        external_mcp_servers,
        external_mcp_specs,
    );

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run TUI
    let active_model_id = if provider_config.model.contains('/') {
        provider_config.model.clone()
    } else {
        format!("{}/{}", provider_config.provider, provider_config.model)
    };
    let mut app = uira_tui::App::new().with_model(&active_model_id);
    let theme_name = uira_config
        .as_ref()
        .map(|cfg| cfg.theme.as_str())
        .unwrap_or("default");
    let theme_overrides = build_theme_overrides(uira_config.as_ref());

    if let Err(err) = app.configure_theme(theme_name, theme_overrides) {
        tracing::warn!("Failed to apply theme '{}': {}", theme_name, err);
        let _ = app.configure_theme("default", uira_tui::ThemeOverrides::default());
    }

    let result = app
        .run_with_agent(&mut terminal, agent_config, client, tracing_rx)
        .await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result.map_err(|e| e.into())
}

fn build_agent_model_overrides(
    uira_config: Option<&uira_core::schema::UiraConfig>,
) -> std::collections::HashMap<String, String> {
    let mut overrides = std::collections::HashMap::new();
    if let Some(config) = uira_config {
        for (agent_name, agent_config) in &config.agents.agents {
            if let Some(model) = &agent_config.model {
                overrides.insert(agent_name.clone(), model.clone());
            }
        }
    }
    overrides
}

fn build_theme_overrides(
    uira_config: Option<&uira_core::schema::UiraConfig>,
) -> uira_tui::ThemeOverrides {
    uira_tui::ThemeOverrides {
        bg: uira_config.and_then(|cfg| cfg.theme_colors.bg.clone()),
        fg: uira_config.and_then(|cfg| cfg.theme_colors.fg.clone()),
        accent: uira_config.and_then(|cfg| cfg.theme_colors.accent.clone()),
        error: uira_config.and_then(|cfg| cfg.theme_colors.error.clone()),
        warning: uira_config.and_then(|cfg| cfg.theme_colors.warning.clone()),
        success: uira_config.and_then(|cfg| cfg.theme_colors.success.clone()),
        borders: uira_config.and_then(|cfg| cfg.theme_colors.borders.clone()),
    }
}

fn create_client(
    cli: &Cli,
    config: &CliConfig,
    agent_defs: &std::collections::HashMap<String, uira_orchestration::AgentConfig>,
    registry: &ModelRegistry,
    agent_model_overrides: &std::collections::HashMap<String, String>,
) -> Result<(Arc<dyn ModelClient>, ProviderConfig), Box<dyn std::error::Error>> {
    use secrecy::SecretString;
    use uira_types::Provider;

    let provider = cli
        .provider
        .as_deref()
        .or(config.default_provider.as_deref())
        .unwrap_or("anthropic");

    let model_from_cli = cli.model.clone();
    let model_from_agent = cli.agent.as_ref().and_then(|name| {
        agent_model_overrides.get(name).cloned().or_else(|| {
            agent_defs.get(name).and_then(|def| {
                def.default_model
                    .map(|tier| registry.resolve(tier, Some(provider)))
            })
        })
    });
    let model_from_config = config.default_model.clone();
    let model = model_from_cli.or(model_from_agent).or(model_from_config);

    match provider {
        "anthropic" => {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .ok()
                .map(SecretString::from);

            let provider_config = ProviderConfig {
                provider: Provider::Anthropic,
                api_key,
                model: model.unwrap_or_else(|| "claude-sonnet-4-20250514".to_string()),
                ..Default::default()
            };

            let client = AnthropicClient::new(provider_config.clone())?;
            Ok((Arc::new(client), provider_config))
        }
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY").ok().map(SecretString::from);

            let provider_config = ProviderConfig {
                provider: Provider::OpenAI,
                api_key,
                model: model.unwrap_or_else(|| "gpt-4o".to_string()),
                ..Default::default()
            };

            let client = OpenAIClient::new(provider_config.clone())?;
            Ok((Arc::new(client), provider_config))
        }
        "gemini" | "google" => {
            let api_key = std::env::var("GEMINI_API_KEY")
                .or_else(|_| std::env::var("GOOGLE_API_KEY"))
                .map_err(|_| "GEMINI_API_KEY or GOOGLE_API_KEY not set")?;

            let provider_config = ProviderConfig {
                provider: Provider::Google,
                api_key: Some(SecretString::from(api_key)),
                model: model.unwrap_or_else(|| "gemini-1.5-pro".to_string()),
                ..Default::default()
            };

            let client = GeminiClient::new(provider_config.clone())?;
            Ok((Arc::new(client), provider_config))
        }
        "ollama" => {
            let provider_config = ProviderConfig {
                provider: Provider::Ollama,
                api_key: None,
                model: model.unwrap_or_else(|| "llama3.1".to_string()),
                base_url: Some(
                    std::env::var("OLLAMA_HOST")
                        .unwrap_or_else(|_| "http://localhost:11434".to_string()),
                ),
                ..Default::default()
            };

            let client = OllamaClient::new(provider_config.clone())?;
            Ok((Arc::new(client), provider_config))
        }
        "opencode" => {
            let api_key = std::env::var("OPENCODE_API_KEY")
                .ok()
                .map(SecretString::from);

            let provider_config = ProviderConfig {
                provider: Provider::OpenCode,
                api_key,
                model: model.unwrap_or_else(|| "gpt-5-nano".to_string()),
                ..Default::default()
            };

            let client = OpenCodeClient::new(provider_config.clone())?;
            Ok((Arc::new(client), provider_config))
        }
        _ => Err(format!("Unknown provider: {}", provider).into()),
    }
}

fn create_agent_config(
    cli: &Cli,
    _config: &CliConfig,
    agent_defs: &std::collections::HashMap<String, uira_orchestration::AgentConfig>,
    uira_config: Option<&uira_core::schema::UiraConfig>,
    external_mcp_servers: Vec<uira_core::schema::NamedMcpServerConfig>,
    external_mcp_specs: Vec<uira_types::ToolSpec>,
) -> AgentConfig {
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let sandbox_policy = match cli.sandbox.as_str() {
        "read-only" => SandboxPolicy::read_only(),
        "full-access" => SandboxPolicy::full_access(),
        _ => SandboxPolicy::workspace_write(cwd.clone()),
    };

    let mut config = AgentConfig::new().with_working_directory(cwd);

    config.sandbox_policy = sandbox_policy;

    if cli.full_auto {
        config = config.full_auto();
    }

    if cli.ralph {
        config = config.with_ralph_mode(true);
    }

    if let Some(ref model) = cli.model {
        config = config.with_model(model);
    }

    if let Some(ref agent_name) = cli.agent {
        if let Some(agent_def) = agent_defs.get(agent_name) {
            config = config.with_system_prompt(&agent_def.prompt);
        }
    }

    if let Some(uira_cfg) = uira_config {
        config = config.with_compaction_settings(&uira_cfg.compaction);

        if !uira_cfg.permissions.rules.is_empty() {
            config = config.with_permission_rules(uira_cfg.permissions.rules.clone());
        }
    }

    if !external_mcp_servers.is_empty() && !external_mcp_specs.is_empty() {
        config = config.with_external_mcp(external_mcp_servers, external_mcp_specs);
    }

    config
}

async fn prepare_external_mcp(
    uira_config: Option<&uira_core::schema::UiraConfig>,
) -> Result<
    (
        Vec<uira_core::schema::NamedMcpServerConfig>,
        Vec<uira_types::ToolSpec>,
    ),
    Box<dyn std::error::Error>,
> {
    let Some(uira_cfg) = uira_config else {
        return Ok((Vec::new(), Vec::new()));
    };

    if uira_cfg.mcp.servers.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }

    let cwd = std::env::current_dir()?;
    let parsed_servers = uira_cfg
        .mcp
        .servers
        .iter()
        .map(|server| {
            uira_mcp_client::McpServerConfig::from_command(
                server.name.clone(),
                server.config.command.clone(),
                server.config.args.clone(),
                server.config.env.clone(),
            )
            .map_err(|e| format!("invalid MCP config for '{}': {}", server.name, e))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let discovered =
        uira_mcp_client::discover_tools(&parsed_servers, &cwd, Duration::from_secs(20))
            .await
            .map_err(|e| format!("failed to discover MCP tools: {e}"))?;

    let specs = discovered
        .into_iter()
        .map(|tool| {
            let schema = serde_json::from_value::<uira_types::JsonSchema>(tool.input_schema)
                .unwrap_or_else(|_| uira_types::JsonSchema::object());
            uira_types::ToolSpec::new(tool.namespaced_name, tool.description, schema)
        })
        .collect::<Vec<_>>();

    Ok((uira_cfg.mcp.servers.clone(), specs))
}

fn print_result(result: &ExecutionResult) {
    println!();
    if result.success {
        println!("{}", "─".repeat(40).green());
        println!("{}", result.output);
        println!("{}", "─".repeat(40).green());
        println!(
            "{} turns: {}, tokens: {}",
            "✓".green().bold(),
            result.turns.to_string().cyan(),
            result.usage.total().to_string().cyan()
        );
    } else {
        println!("{}", "─".repeat(40).red());
        if let Some(ref error) = result.error {
            println!("{}: {}", "Error".red().bold(), error);
        }
        println!("{}", "─".repeat(40).red());
    }
}

fn generate_completions(shell: Shell) {
    let mut cmd = Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(shell, &mut cmd, bin_name, &mut std::io::stdout());
}

/// Extract text content from a message content
fn get_message_text(content: &uira_types::MessageContent) -> String {
    match content {
        uira_types::MessageContent::Text(t) => t.clone(),
        uira_types::MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                uira_types::ContentBlock::Text { text } => Some(text.as_str()),
                uira_types::ContentBlock::ToolResult { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        uira_types::MessageContent::ToolCalls(calls) => calls
            .iter()
            .map(|c| format!("Tool: {} ({})", c.name, c.id))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
