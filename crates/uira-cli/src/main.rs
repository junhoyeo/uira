//! Uira - Native AI Coding Agent

use clap::Parser;
use colored::Colorize;
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use uira_agent::{Agent, AgentConfig, ExecutorConfig, RecursiveAgentExecutor};
use uira_agents::{get_agent_definitions, ModelRegistry};
use uira_protocol::ExecutionResult;
use uira_providers::{
    AnthropicClient, GeminiClient, ModelClient, OllamaClient, OpenAIClient, OpenCodeClient,
    ProviderConfig,
};
use uira_sandbox::SandboxPolicy;

mod commands;
mod config;
mod session;

use commands::{AuthCommands, Cli, Commands, ConfigCommands, GoalsCommands, TasksCommands};
use config::CliConfig;
use session::SessionStorage;

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();
    let config = CliConfig::load();

    let result = match &cli.command {
        Some(Commands::Exec { prompt, json }) => run_exec(&cli, &config, prompt, *json).await,
        Some(Commands::Resume { session_id }) => run_resume(session_id.as_deref()).await,
        Some(Commands::Auth { command }) => run_auth(command, &config).await,
        Some(Commands::Config { command }) => run_config(command, &config).await,
        Some(Commands::Goals { command }) => run_goals(command).await,
        Some(Commands::Tasks { command }) => run_tasks(command).await,
        None => {
            if let Some(prompt) = cli.get_prompt() {
                run_exec(&cli, &config, &prompt, false).await
            } else {
                run_interactive(&cli, &config).await
            }
        }
    };

    if let Err(e) = result {
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(1);
    }
}

async fn run_exec(
    cli: &Cli,
    config: &CliConfig,
    prompt: &str,
    json_output: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use futures::StreamExt;
    use uira_protocol::{Item, ThreadEvent};

    let uira_config = uira_config::loader::load_config(None).ok();
    let agent_model_overrides = build_agent_model_overrides(uira_config.as_ref());
    let agent_defs = get_agent_definitions(None);
    let registry = ModelRegistry::new();
    let (client, provider_config) =
        create_client(cli, config, &agent_defs, &registry, &agent_model_overrides)?;
    let agent_config = create_agent_config(cli, config, &agent_defs);

    let executor_config = ExecutorConfig::new(provider_config, agent_config.clone());
    let executor = Arc::new(RecursiveAgentExecutor::new(executor_config));
    let agent = Agent::new_with_executor(agent_config, client, Some(executor));

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

async fn run_resume(session_id: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let storage = SessionStorage::new()?;

    match session_id {
        Some(id) => {
            println!("{} {}", "Resuming session:".cyan().bold(), id.yellow());

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

            // Show conversation history
            for msg in &session.messages {
                let role = match msg.role {
                    uira_protocol::Role::User => "User".green(),
                    uira_protocol::Role::Assistant => "Assistant".blue(),
                    uira_protocol::Role::System => "System".yellow(),
                    uira_protocol::Role::Tool => "Tool".magenta(),
                };
                let content = get_message_text(&msg.content);
                println!("{}: {}", role.bold(), content);
                println!();
            }

            println!(
                "{}",
                "To continue this session, use the messages above as context.".dimmed()
            );
        }
        None => {
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
        }
    }
    Ok(())
}

async fn run_auth(
    command: &AuthCommands,
    _config: &CliConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    use uira_auth::providers::{AnthropicAuth, GoogleAuth, OpenAIAuth};
    use uira_auth::{AuthProvider, CredentialStore, OAuthCallbackServer, StoredCredential};

    const DEFAULT_OAUTH_PORT: u16 = 8765;

    match command {
        AuthCommands::Login { provider } => {
            let provider = match provider {
                Some(p) => p.clone(),
                None => {
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
                None => println!("{}: {}", "Unknown key".red(), key),
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
    use uira_config::loader::load_config;
    use uira_goals::GoalRunner;

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
    use uira_features::background_agent::{BackgroundManager, BackgroundTaskConfig};

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
                    uira_features::background_agent::BackgroundTaskStatus::Queued => {
                        "queued".yellow()
                    }
                    uira_features::background_agent::BackgroundTaskStatus::Pending => {
                        "pending".yellow()
                    }
                    uira_features::background_agent::BackgroundTaskStatus::Running => {
                        "running".cyan()
                    }
                    uira_features::background_agent::BackgroundTaskStatus::Completed => {
                        "completed".green()
                    }
                    uira_features::background_agent::BackgroundTaskStatus::Error => "error".red(),
                    uira_features::background_agent::BackgroundTaskStatus::Cancelled => {
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
                uira_features::background_agent::BackgroundTaskStatus::Queued => "queued".yellow(),
                uira_features::background_agent::BackgroundTaskStatus::Pending => {
                    "pending".yellow()
                }
                uira_features::background_agent::BackgroundTaskStatus::Running => "running".cyan(),
                uira_features::background_agent::BackgroundTaskStatus::Completed => {
                    "completed".green()
                }
                uira_features::background_agent::BackgroundTaskStatus::Error => "error".red(),
                uira_features::background_agent::BackgroundTaskStatus::Cancelled => {
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

async fn run_interactive(cli: &Cli, config: &CliConfig) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::io::stdout;

    let uira_config = uira_config::loader::load_config(None).ok();
    let agent_model_overrides = build_agent_model_overrides(uira_config.as_ref());
    let agent_defs = get_agent_definitions(None);
    let registry = ModelRegistry::new();
    let (client, _provider_config) =
        create_client(cli, config, &agent_defs, &registry, &agent_model_overrides)?;
    let agent_config = create_agent_config(cli, config, &agent_defs);

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run TUI
    let mut app = uira_tui::App::new();
    let result = app
        .run_with_agent(&mut terminal, agent_config, client)
        .await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result.map_err(|e| e.into())
}

fn build_agent_model_overrides(
    uira_config: Option<&uira_config::schema::UiraConfig>,
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

fn create_client(
    cli: &Cli,
    config: &CliConfig,
    agent_defs: &std::collections::HashMap<String, uira_agents::types::AgentConfig>,
    registry: &ModelRegistry,
    agent_model_overrides: &std::collections::HashMap<String, String>,
) -> Result<(Arc<dyn ModelClient>, ProviderConfig), Box<dyn std::error::Error>> {
    use secrecy::SecretString;
    use uira_protocol::Provider;

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
    agent_defs: &std::collections::HashMap<String, uira_agents::types::AgentConfig>,
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

    config
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

/// Extract text content from a message content
fn get_message_text(content: &uira_protocol::MessageContent) -> String {
    match content {
        uira_protocol::MessageContent::Text(t) => t.clone(),
        uira_protocol::MessageContent::Blocks(blocks) => blocks
            .iter()
            .filter_map(|b| match b {
                uira_protocol::ContentBlock::Text { text } => Some(text.as_str()),
                uira_protocol::ContentBlock::ToolResult { content, .. } => Some(content.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
        uira_protocol::MessageContent::ToolCalls(calls) => calls
            .iter()
            .map(|c| format!("Tool: {} ({})", c.name, c.id))
            .collect::<Vec<_>>()
            .join("\n"),
    }
}
