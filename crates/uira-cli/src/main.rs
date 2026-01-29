//! Uira - Native AI Coding Agent

use clap::Parser;
use colored::Colorize;
use std::sync::Arc;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};
use uira_agent::{Agent, AgentConfig};
use uira_protocol::ExecutionResult;
use uira_providers::{
    AnthropicClient, GeminiClient, ModelClient, OllamaClient, OpenAIClient, ProviderConfig,
};
use uira_sandbox::SandboxPolicy;

mod commands;
mod config;
mod session;

use commands::{AuthCommands, Cli, Commands, ConfigCommands};
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
    let client = create_client(cli, config)?;
    let agent_config = create_agent_config(cli, config);

    let mut agent = Agent::new(agent_config, client);

    if !json_output {
        println!("{} {}", "Running:".cyan().bold(), prompt.dimmed());
    }

    let result = agent.run(prompt).await?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        print_result(&result);
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
                        session.id[..8].yellow(),
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
    match command {
        AuthCommands::Login { provider } => {
            println!("{} {}", "Logging in to:".cyan().bold(), provider.yellow());

            match provider.to_lowercase().as_str() {
                "anthropic" => {
                    println!("Set the ANTHROPIC_API_KEY environment variable");
                    println!("  export ANTHROPIC_API_KEY=your-api-key");
                }
                "openai" => {
                    println!("Set the OPENAI_API_KEY environment variable");
                    println!("  export OPENAI_API_KEY=your-api-key");
                }
                _ => {
                    println!("Unknown provider: {}", provider);
                }
            }
        }
        AuthCommands::Logout { provider } => {
            println!(
                "{} {}",
                "Logging out from:".cyan().bold(),
                provider.yellow()
            );
            println!("Remove the API key from your environment");
        }
        AuthCommands::Status => {
            println!("{}", "Authentication status:".cyan().bold());

            if std::env::var("ANTHROPIC_API_KEY").is_ok() {
                println!("  {} Anthropic API key set", "✓".green());
            } else {
                println!("  {} Anthropic API key not set", "✗".red());
            }

            if std::env::var("OPENAI_API_KEY").is_ok() {
                println!("  {} OpenAI API key set", "✓".green());
            } else {
                println!("  {} OpenAI API key not set", "✗".red());
            }
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

async fn run_interactive(cli: &Cli, config: &CliConfig) -> Result<(), Box<dyn std::error::Error>> {
    use crossterm::{
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    };
    use ratatui::backend::CrosstermBackend;
    use ratatui::Terminal;
    use std::io::stdout;

    // Create model client
    let client = create_client(cli, config)?;
    let agent_config = create_agent_config(cli, config);

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

fn create_client(
    cli: &Cli,
    config: &CliConfig,
) -> Result<Arc<dyn ModelClient>, Box<dyn std::error::Error>> {
    use secrecy::SecretString;
    use uira_protocol::Provider;

    let provider = cli
        .provider
        .as_deref()
        .or(config.default_provider.as_deref())
        .unwrap_or("anthropic");

    let model = cli.model.clone().or(config.default_model.clone());

    match provider {
        "anthropic" => {
            let api_key =
                std::env::var("ANTHROPIC_API_KEY").map_err(|_| "ANTHROPIC_API_KEY not set")?;

            let provider_config = ProviderConfig {
                provider: Provider::Anthropic,
                api_key: Some(SecretString::from(api_key)),
                model: model.unwrap_or_else(|| "claude-sonnet-4-20250514".to_string()),
                ..Default::default()
            };

            let client = AnthropicClient::new(provider_config)?;
            Ok(Arc::new(client))
        }
        "openai" => {
            let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| "OPENAI_API_KEY not set")?;

            let provider_config = ProviderConfig {
                provider: Provider::OpenAI,
                api_key: Some(SecretString::from(api_key)),
                model: model.unwrap_or_else(|| "gpt-4o".to_string()),
                ..Default::default()
            };

            let client = OpenAIClient::new(provider_config)?;
            Ok(Arc::new(client))
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

            let client = GeminiClient::new(provider_config)?;
            Ok(Arc::new(client))
        }
        "ollama" => {
            let provider_config = ProviderConfig {
                provider: Provider::Ollama,
                api_key: None, // Ollama doesn't need API key
                model: model.unwrap_or_else(|| "llama3.1".to_string()),
                base_url: Some(
                    std::env::var("OLLAMA_HOST")
                        .unwrap_or_else(|_| "http://localhost:11434".to_string()),
                ),
                ..Default::default()
            };

            let client = OllamaClient::new(provider_config)?;
            Ok(Arc::new(client))
        }
        _ => Err(format!("Unknown provider: {}", provider).into()),
    }
}

fn create_agent_config(cli: &Cli, _config: &CliConfig) -> AgentConfig {
    let sandbox_policy = match cli.sandbox.as_str() {
        "read-only" => SandboxPolicy::read_only(),
        "full-access" => SandboxPolicy::full_access(),
        _ => SandboxPolicy::workspace_write(std::env::current_dir().unwrap_or_default()),
    };

    let mut config =
        AgentConfig::new().with_working_directory(std::env::current_dir().unwrap_or_default());

    config.sandbox_policy = sandbox_policy;

    if cli.full_auto {
        config = config.full_auto();
    }

    if let Some(ref model) = cli.model {
        config = config.with_model(model);
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
