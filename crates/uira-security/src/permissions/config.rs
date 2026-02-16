use super::{Action, PatternError, PermissionEvaluator, PermissionRule};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigAction {
    Allow,
    Deny,
    Ask,
}

impl From<ConfigAction> for Action {
    fn from(action: ConfigAction) -> Self {
        match action {
            ConfigAction::Allow => Action::Allow,
            ConfigAction::Deny => Action::Deny,
            ConfigAction::Ask => Action::Ask,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigRule {
    pub name: Option<String>,
    pub permission: String,
    pub pattern: String,
    pub action: ConfigAction,
    pub comment: Option<String>,
}

impl From<ConfigRule> for PermissionRule {
    fn from(config: ConfigRule) -> Self {
        let mut rule =
            PermissionRule::new(&config.permission, &config.pattern, config.action.into());
        if let Some(name) = config.name {
            rule = rule.with_name(name);
        }
        if let Some(comment) = config.comment {
            rule = rule.with_comment(comment);
        }
        rule
    }
}

pub fn build_evaluator_from_rules(
    rules: Vec<ConfigRule>,
) -> Result<PermissionEvaluator, PatternError> {
    let permission_rules: Vec<PermissionRule> = rules.into_iter().map(Into::into).collect();
    PermissionEvaluator::with_rules(permission_rules)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_rule_conversion() {
        let config_rule = ConfigRule {
            name: Some("test-rule".to_string()),
            permission: "file:write".to_string(),
            pattern: "src/**".to_string(),
            action: ConfigAction::Allow,
            comment: None,
        };

        let rule: PermissionRule = config_rule.into();
        assert_eq!(rule.permission, "file:write");
        assert_eq!(rule.pattern, "src/**");
        assert_eq!(rule.action, Action::Allow);
    }

    #[test]
    fn test_build_evaluator() {
        let rules = vec![
            ConfigRule {
                name: None,
                permission: "file:read".to_string(),
                pattern: "**".to_string(),
                action: ConfigAction::Allow,
                comment: None,
            },
            ConfigRule {
                name: None,
                permission: "file:write".to_string(),
                pattern: "**".to_string(),
                action: ConfigAction::Ask,
                comment: None,
            },
        ];

        let evaluator = build_evaluator_from_rules(rules).unwrap();
        assert!(evaluator.evaluate("file:read", "any/path").is_allowed());
        assert!(evaluator
            .evaluate("file:write", "any/path")
            .needs_approval());
    }
}
