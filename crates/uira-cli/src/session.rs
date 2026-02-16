use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::path::PathBuf;
use uira_agent::session::{extract_messages, SessionItem};
use uira_agent::SessionRecorder;
use uira_core::Message;

pub struct SessionEntry {
    pub thread_id: String,
    pub timestamp: DateTime<Utc>,
    pub model: String,
    pub provider: String,
    pub turns: usize,
    pub parent_id: Option<String>,
    pub fork_count: u32,
    pub path: PathBuf,
}

pub fn list_sessions(limit: usize) -> std::io::Result<Vec<SessionEntry>> {
    let sessions = SessionRecorder::list_recent(limit)?;
    let entries = sessions
        .into_iter()
        .map(|(path, meta)| SessionEntry {
            thread_id: meta.thread_id,
            timestamp: meta.timestamp,
            model: meta.model,
            provider: meta.provider,
            turns: meta.turns,
            parent_id: meta.parent_id.map(|id| id.to_string()),
            fork_count: meta.fork_count,
            path,
        })
        .collect();
    Ok(entries)
}

pub fn load_session_messages(session_id: &str) -> std::io::Result<(SessionEntry, Vec<Message>)> {
    let entry = list_sessions(1000)?
        .into_iter()
        .find(|e| e.thread_id == session_id || e.thread_id.starts_with(session_id))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Session not found: {}", session_id),
            )
        })?;

    let items = SessionRecorder::load(&entry.path)?;
    let messages = extract_messages(&items);
    Ok((entry, messages))
}

pub fn summarize_session(items: &[SessionItem]) -> String {
    extract_messages(items)
        .into_iter()
        .find_map(|msg| {
            if msg.role == uira_core::Role::User {
                msg.content.as_text().map(str::to_string)
            } else {
                None
            }
        })
        .unwrap_or_else(|| "(no user message)".to_string())
}

pub fn display_sessions_list(entries: &[SessionEntry]) {
    if entries.is_empty() {
        println!("No sessions found.");
        return;
    }

    println!(
        "{:<24} {:<20} {:<24} {:>5} {:>5}",
        "SESSION ID", "TIMESTAMP", "MODEL", "TURNS", "FORKS"
    );
    println!("{}", "-".repeat(82));

    for entry in entries {
        let timestamp = entry.timestamp.format("%Y-%m-%d %H:%M:%S");
        let model_display = if entry.model.len() > 20 {
            format!("{}...", &entry.model[..17])
        } else {
            entry.model.clone()
        };

        let id_prefix = if entry.parent_id.is_some() {
            "└─"
        } else {
            ""
        };

        println!(
            "{}{:<24} {:<20} {:<24} {:>5} {:>5}",
            id_prefix,
            truncate(&entry.thread_id, 24 - id_prefix.len()),
            timestamp,
            model_display,
            entry.turns,
            entry.fork_count
        );
    }
}

pub fn display_sessions_tree(entries: &[SessionEntry]) {
    if entries.is_empty() {
        println!("No sessions found.");
        return;
    }

    let mut by_parent: HashMap<Option<String>, Vec<&SessionEntry>> = HashMap::new();
    for entry in entries {
        by_parent
            .entry(entry.parent_id.clone())
            .or_default()
            .push(entry);
    }

    let roots: Vec<_> = entries.iter().filter(|e| e.parent_id.is_none()).collect();

    println!("Session Fork Tree:");
    println!("{}", "=".repeat(60));

    for root in roots {
        print_tree_node(root, &by_parent, "", true);
    }
}

fn print_tree_node(
    entry: &SessionEntry,
    by_parent: &HashMap<Option<String>, Vec<&SessionEntry>>,
    prefix: &str,
    is_last: bool,
) {
    let connector = if prefix.is_empty() {
        ""
    } else if is_last {
        "└── "
    } else {
        "├── "
    };

    let timestamp = entry.timestamp.format("%m-%d %H:%M");
    println!(
        "{}{}{} ({}, {} turns)",
        prefix, connector, entry.thread_id, timestamp, entry.turns
    );

    if let Some(children) = by_parent.get(&Some(entry.thread_id.clone())) {
        let child_prefix = if prefix.is_empty() {
            "    ".to_string()
        } else if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        for (i, child) in children.iter().enumerate() {
            let is_last_child = i == children.len() - 1;
            print_tree_node(child, by_parent, &child_prefix, is_last_child);
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
