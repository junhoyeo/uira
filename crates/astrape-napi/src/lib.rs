#![deny(clippy::all)]

use astrape_core::HookOutput;
use astrape_hook::KeywordDetector;
use napi_derive::napi;

#[napi(object)]
pub struct JsHookOutput {
    #[napi(js_name = "continue")]
    pub continue_processing: bool,
    pub message: Option<String>,
    pub stop_reason: Option<String>,
    pub decision: Option<String>,
    pub reason: Option<String>,
    pub additional_context: Option<String>,
    pub suppress_output: Option<bool>,
    pub system_message: Option<String>,
}

impl From<HookOutput> for JsHookOutput {
    fn from(output: HookOutput) -> Self {
        Self {
            continue_processing: output.continue_processing,
            message: output.message,
            stop_reason: output.stop_reason,
            decision: output.decision.map(|d| format!("{:?}", d)),
            reason: output.reason,
            additional_context: output.additional_context,
            suppress_output: output.suppress_output,
            system_message: output.system_message,
        }
    }
}

impl Default for JsHookOutput {
    fn default() -> Self {
        Self {
            continue_processing: true,
            message: None,
            stop_reason: None,
            decision: None,
            reason: None,
            additional_context: None,
            suppress_output: None,
            system_message: None,
        }
    }
}

#[napi(object)]
pub struct DetectedKeyword {
    pub keyword_type: String,
    pub message: String,
}

#[napi]
pub fn detect_keywords(prompt: String, agent: Option<String>) -> Option<JsHookOutput> {
    let detector = KeywordDetector::new();
    detector
        .detect(&prompt, agent.as_deref())
        .map(JsHookOutput::from)
}

#[napi]
pub fn detect_all_keywords(prompt: String, agent: Option<String>) -> Vec<DetectedKeyword> {
    let detector = KeywordDetector::new();
    detector
        .detect_all(&prompt, agent.as_deref())
        .into_iter()
        .map(|(keyword_type, message)| DetectedKeyword {
            keyword_type: keyword_type.to_string(),
            message,
        })
        .collect()
}

#[napi]
pub fn create_hook_output_with_message(message: String) -> JsHookOutput {
    JsHookOutput {
        continue_processing: true,
        message: Some(message),
        ..Default::default()
    }
}

#[napi]
pub fn create_hook_output_deny(reason: String) -> JsHookOutput {
    JsHookOutput {
        continue_processing: false,
        decision: Some("Deny".to_string()),
        reason: Some(reason),
        ..Default::default()
    }
}

#[napi]
pub fn create_hook_output_stop(reason: String) -> JsHookOutput {
    JsHookOutput {
        continue_processing: false,
        stop_reason: Some(reason),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_ultrawork() {
        let result = detect_keywords("ultrawork: do something".to_string(), None);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.continue_processing);
        assert!(output.message.unwrap().contains("ultrawork-mode"));
    }

    #[test]
    fn test_detect_search() {
        let result = detect_keywords("search for files".to_string(), None);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.message.unwrap().contains("search-mode"));
    }

    #[test]
    fn test_detect_analyze() {
        let result = detect_keywords("analyze this code".to_string(), None);
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.message.unwrap().contains("analyze-mode"));
    }

    #[test]
    fn test_no_keyword() {
        let result = detect_keywords("just a normal message".to_string(), None);
        assert!(result.is_none());
    }

    #[test]
    fn test_detect_all() {
        let result = detect_all_keywords("ultrawork search analyze".to_string(), None);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn test_planner_agent() {
        let result = detect_keywords(
            "ultrawork: plan".to_string(),
            Some("prometheus".to_string()),
        );
        assert!(result.is_some());
        let output = result.unwrap();
        assert!(output.message.unwrap().contains("PLANNER"));
    }
}
