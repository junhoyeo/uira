#[derive(Debug, Clone, PartialEq)]
pub enum ModelPath {
    Anthropic,
    DirectProvider,
}

pub fn route_model(model: &str) -> ModelPath {
    if model.starts_with("claude-") || model.starts_with("anthropic/") {
        ModelPath::Anthropic
    } else {
        ModelPath::DirectProvider
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_claude_prefix() {
        assert_eq!(
            route_model("claude-3-5-sonnet-20241022"),
            ModelPath::Anthropic
        );
    }

    #[test]
    fn test_anthropic_prefix() {
        assert_eq!(route_model("anthropic/claude-3-opus"), ModelPath::Anthropic);
    }

    #[test]
    fn test_openai_model() {
        assert_eq!(route_model("openai/gpt-4"), ModelPath::DirectProvider);
    }

    #[test]
    fn test_opencode_model() {
        assert_eq!(
            route_model("opencode/big-pickle"),
            ModelPath::DirectProvider
        );
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(route_model(""), ModelPath::DirectProvider);
    }
}
