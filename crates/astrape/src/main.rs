mod config;
mod hooks;
mod linter;
mod typos;

use astrape_claude::{
    extract_prompt, install_hooks as install_claude_hooks, list_installed_hooks, read_input,
    write_output,
};
use astrape_core::HookEvent;
use astrape_hook::KeywordDetector;
use clap::{Parser, Subcommand};
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
    /// Manage AI harness hooks (Claude Code)
    Hook {
        #[command(subcommand)]
        action: HookCommands,
    },
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
        Commands::Typos { ai, stage, files } => typos_command(ai, stage, &files),
        Commands::Format { check, files } => format_command(check, &files),
        Commands::Hook { action } => hook_command(action),
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

fn typos_command(ai: bool, stage: bool, files: &[String]) -> anyhow::Result<()> {
    if ai {
        println!("🔍 Checking for typos with AI assistance...\n");

        let config = Config::from_file("astrape.yml").ok();
        let ai_config = config.as_ref().and_then(|c| c.ai.clone());
        let ai_hooks = config.and_then(|c| c.ai_hooks);

        let mut checker = TyposChecker::with_hooks(ai_config, ai_hooks).with_auto_stage(stage);
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
                eprintln!("   Run 'astrape format' to fix formatting");
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
                eprintln!("   Run 'astrape format' to fix formatting");
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
