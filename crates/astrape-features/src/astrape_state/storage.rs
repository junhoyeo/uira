use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use regex::Regex;

use crate::astrape_state::constants::{
    ASTRAPE_DIR, ASTRAPE_STATE_FILE, PLANNER_PLANS_DIR, PLAN_EXTENSION,
};
use crate::astrape_state::types::{AstrapeState, PlanProgress, PlanSummary};

pub fn get_astrape_file_path(directory: impl AsRef<Path>) -> PathBuf {
    directory
        .as_ref()
        .join(ASTRAPE_DIR)
        .join(ASTRAPE_STATE_FILE)
}

pub fn read_astrape_state(directory: impl AsRef<Path>) -> Option<AstrapeState> {
    let file_path = get_astrape_file_path(directory);

    let content = fs::read_to_string(file_path).ok()?;
    serde_json::from_str(&content).ok()
}

pub fn write_astrape_state(directory: impl AsRef<Path>, state: &AstrapeState) -> bool {
    let file_path = get_astrape_file_path(directory);

    let Some(parent) = file_path.parent() else {
        return false;
    };

    if fs::create_dir_all(parent).is_err() {
        return false;
    }

    let Ok(payload) = serde_json::to_string_pretty(state) else {
        return false;
    };

    fs::write(file_path, payload).is_ok()
}

pub fn append_session_id(
    directory: impl AsRef<Path>,
    session_id: impl Into<String>,
) -> Option<AstrapeState> {
    let session_id = session_id.into();
    let mut state = read_astrape_state(directory.as_ref())?;

    if !state.session_ids.iter().any(|id| id == &session_id) {
        state.session_ids.push(session_id);
        if write_astrape_state(directory, &state) {
            return Some(state);
        }
    }

    Some(state)
}

pub fn clear_astrape_state(directory: impl AsRef<Path>) -> bool {
    let file_path = get_astrape_file_path(directory);

    match fs::remove_file(file_path) {
        Ok(()) => true,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

pub fn find_planner_plans(directory: impl AsRef<Path>) -> Vec<PathBuf> {
    let plans_dir = directory.as_ref().join(PLANNER_PLANS_DIR);
    let Ok(entries) = fs::read_dir(plans_dir) else {
        return vec![];
    };

    let mut plans: Vec<(PathBuf, std::time::SystemTime)> = entries
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some(&PLAN_EXTENSION[1..]) {
                return None;
            }
            let modified = e.metadata().and_then(|m| m.modified()).ok()?;
            Some((path, modified))
        })
        .collect();

    plans.sort_by(|a, b| b.1.cmp(&a.1));
    plans.into_iter().map(|p| p.0).collect()
}

pub fn get_plan_progress(plan_path: impl AsRef<Path>) -> PlanProgress {
    let content = match fs::read_to_string(plan_path) {
        Ok(c) => c,
        Err(_) => {
            return PlanProgress {
                total: 0,
                completed: 0,
                is_complete: true,
            };
        }
    };

    let unchecked_re = Regex::new(r"(?m)^[-*]\s*\[\s*\]").unwrap();
    let checked_re = Regex::new(r"(?m)^[-*]\s*\[[xX]\]").unwrap();

    let unchecked = unchecked_re.find_iter(&content).count();
    let checked = checked_re.find_iter(&content).count();
    let total = unchecked + checked;

    PlanProgress {
        total,
        completed: checked,
        is_complete: total == 0 || checked == total,
    }
}

pub fn get_plan_name(plan_path: impl AsRef<Path>) -> String {
    plan_path
        .as_ref()
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_string()
}

pub fn create_astrape_state(
    plan_path: impl AsRef<Path>,
    session_id: impl Into<String>,
) -> AstrapeState {
    let plan_path = plan_path.as_ref();
    AstrapeState {
        active_plan: plan_path.to_string_lossy().to_string(),
        started_at: Utc::now().to_rfc3339(),
        session_ids: vec![session_id.into()],
        plan_name: get_plan_name(plan_path),
        metadata: None,
    }
}

pub fn get_plan_summaries(directory: impl AsRef<Path>) -> Vec<PlanSummary> {
    find_planner_plans(directory.as_ref())
        .into_iter()
        .filter_map(|plan_path| {
            let modified = fs::metadata(&plan_path).and_then(|m| m.modified()).ok()?;
            let last_modified: DateTime<Utc> = modified.into();
            Some(PlanSummary {
                name: get_plan_name(&plan_path),
                progress: get_plan_progress(&plan_path),
                path: plan_path,
                last_modified,
            })
        })
        .collect()
}

pub fn has_astrape_state(directory: impl AsRef<Path>) -> bool {
    read_astrape_state(directory).is_some()
}

pub fn get_active_plan_path(directory: impl AsRef<Path>) -> Option<String> {
    read_astrape_state(directory).map(|s| s.active_plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn plan_progress_counts_checkboxes() {
        let dir = TempDir::new().unwrap();
        let plan_path = dir.path().join("plan.md");
        fs::write(
            &plan_path,
            "- [ ] one\n- [x] two\n* [X] three\nnot a checkbox\n",
        )
        .unwrap();

        let progress = get_plan_progress(&plan_path);
        assert_eq!(progress.total, 3);
        assert_eq!(progress.completed, 2);
        assert!(!progress.is_complete);
    }

    #[test]
    fn state_round_trip_and_append_session() {
        let dir = TempDir::new().unwrap();
        let plan_path = dir.path().join(".omc").join("plans").join("demo.md");
        fs::create_dir_all(plan_path.parent().unwrap()).unwrap();
        fs::write(&plan_path, "- [ ] task\n").unwrap();

        let state = create_astrape_state(&plan_path, "ses_1");
        assert!(write_astrape_state(dir.path(), &state));

        let loaded = read_astrape_state(dir.path()).unwrap();
        assert_eq!(loaded.plan_name, "demo");
        assert_eq!(loaded.session_ids, vec!["ses_1".to_string()]);

        let updated = append_session_id(dir.path(), "ses_2").unwrap();
        assert_eq!(updated.session_ids.len(), 2);
        assert!(updated.session_ids.contains(&"ses_2".to_string()));

        assert!(clear_astrape_state(dir.path()));
        assert!(read_astrape_state(dir.path()).is_none());
    }

    #[test]
    fn find_planner_plans_filters_md() {
        let dir = TempDir::new().unwrap();
        let plans_dir = dir.path().join(PLANNER_PLANS_DIR);
        fs::create_dir_all(&plans_dir).unwrap();

        fs::write(plans_dir.join("a.md"), "- [ ] a\n").unwrap();
        fs::write(plans_dir.join("b.txt"), "nope").unwrap();

        let plans = find_planner_plans(dir.path());
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].file_name().unwrap(), "a.md");
    }
}
