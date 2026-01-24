mod biome;
mod config;
mod executor;

use biome::BiomeRunner;
use clap::{Parser, Subcommand};
use config::Config;
use executor::HookExecutor;
use std::fs;
use std::process;

#[derive(Parser)]
#[command(name = "astrape")]
#[command(version, about = "⚡ Lightning-fast Rust-native git hooks manager", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Init {
        #[arg(short, long, default_value = "astrape.yml")]
        config: String,
    },
    Install,
    Run {
        hook: String,
    },
    Check {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        files: Vec<String>,
    },
    Fix {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        files: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    let result = match cli.command {
        Commands::Init { config } => init_command(&config),
        Commands::Install => install_command(),
        Commands::Run { hook } => run_command(&hook),
        Commands::Check { files } => check_command(&files),
        Commands::Fix { files } => fix_command(&files),
    };

    if let Err(e) = result {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn init_command(config_path: &str) -> anyhow::Result<()> {
    println!("⚡ Initializing astrape...");

    if std::path::Path::new(config_path).exists() {
        anyhow::bail!("Config file already exists: {}", config_path);
    }

    let config = Config::default_config();
    let yaml = config.to_yaml()?;

    fs::write(config_path, yaml)?;

    println!("✅ Created {}", config_path);
    println!("\nNext steps:");
    println!("  1. Review and customize {}", config_path);
    println!("  2. Run: astrape install");

    Ok(())
}

fn install_command() -> anyhow::Result<()> {
    println!("📦 Installing git hooks...");
    Ok(())
}

fn run_command(hook_name: &str) -> anyhow::Result<()> {
    let config_path = "astrape.yml";

    if !std::path::Path::new(config_path).exists() {
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

fn check_command(files: &[String]) -> anyhow::Result<()> {
    let runner = BiomeRunner::new();
    runner.check(files)
}

fn fix_command(files: &[String]) -> anyhow::Result<()> {
    let runner = BiomeRunner::new();
    runner.fix(files)
}
