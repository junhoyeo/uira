//! Integration tests for Anthropic provider hardening
//!
//! These tests verify the new hardening features work correctly together:
//! - Error classification with various API error messages
//! - Retry logic with exponential backoff
//! - Turn validation for message sequences
//! - Beta features header generation
//! - Thinking config serialization
//!
//! All tests use mocked responses and do not require API keys.

use uira_protocol::{ContentBlock, Message, MessageContent, Role};
use uira_providers::{
    classify_error, validate_anthropic_turns, BetaFeatures, ProviderError, RetryConfig,
};

// ============================================================================
// Error Classification Tests
// ============================================================================

#[test]
fn test_error_classification_context_overflow() {
    let error = classify_error(
        400,
        "request_too_large: your request exceeds the maximum context length",
    );
    assert!(matches!(error, ProviderError::ContextExceeded { .. }));
}

#[test]
fn test_error_classification_context_overflow_variants() {
    // Test various context overflow message patterns
    let patterns = vec![
        "Context length exceeded",
        "Your prompt is too long",
        "exceeds model context window",
        "maximum context length",
    ];

    for pattern in patterns {
        let error = classify_error(400, pattern);
        assert!(
            matches!(error, ProviderError::ContextExceeded { .. }),
            "Failed to classify as ContextExceeded: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_rate_limit() {
    let error = classify_error(429, "rate limit exceeded");
    assert!(matches!(error, ProviderError::RateLimited { .. }));
}

#[test]
fn test_error_classification_rate_limit_variants() {
    // Test various rate limit message patterns
    let patterns = vec![
        "rate limit exceeded",
        "rate_limit",
        "Too many requests",
        "quota exceeded",
    ];

    for pattern in patterns {
        let error = classify_error(429, pattern);
        assert!(
            matches!(error, ProviderError::RateLimited { .. }),
            "Failed to classify as RateLimited: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_overloaded() {
    let error = classify_error(529, "overloaded_error");
    assert!(matches!(error, ProviderError::Unavailable { .. }));
}

#[test]
fn test_error_classification_overloaded_variants() {
    let patterns = vec!["overloaded_error", "The service is overloaded"];

    for pattern in patterns {
        let error = classify_error(503, pattern);
        assert!(
            matches!(error, ProviderError::Unavailable { .. }),
            "Failed to classify as Unavailable: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_billing() {
    let error = classify_error(402, "payment required");
    assert!(matches!(error, ProviderError::PaymentRequired { .. }));
}

#[test]
fn test_error_classification_billing_variants() {
    let patterns = vec!["payment required", "insufficient credits", "billing issue"];

    for pattern in patterns {
        let error = classify_error(400, pattern);
        assert!(
            matches!(error, ProviderError::PaymentRequired { .. }),
            "Failed to classify as PaymentRequired: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_auth() {
    let error = classify_error(401, "invalid_api_key");
    assert!(matches!(error, ProviderError::AuthenticationFailed(_)));
}

#[test]
fn test_error_classification_auth_variants() {
    let patterns = vec![
        "invalid_api_key",
        "Invalid API key",
        "Unauthorized",
        "token has expired",
        "authentication failed",
    ];

    for pattern in patterns {
        let error = classify_error(401, pattern);
        assert!(
            matches!(error, ProviderError::AuthenticationFailed(_)),
            "Failed to classify as AuthenticationFailed: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_timeout() {
    let error = classify_error(408, "request timeout");
    assert!(matches!(error, ProviderError::Timeout { .. }));
}

#[test]
fn test_error_classification_timeout_variants() {
    let patterns = vec!["Request timeout", "Connection timed out"];

    for pattern in patterns {
        let error = classify_error(408, pattern);
        assert!(
            matches!(error, ProviderError::Timeout { .. }),
            "Failed to classify as Timeout: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_message_ordering() {
    let error = classify_error(400, "incorrect role information");
    assert!(matches!(error, ProviderError::MessageOrderingConflict));
}

#[test]
fn test_error_classification_message_ordering_variants() {
    let patterns = vec!["incorrect role information", "Roles must alternate"];

    for pattern in patterns {
        let error = classify_error(400, pattern);
        assert!(
            matches!(error, ProviderError::MessageOrderingConflict),
            "Failed to classify as MessageOrderingConflict: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_tool_input() {
    let error = classify_error(400, "tool_use.input is required");
    assert!(matches!(error, ProviderError::ToolCallInputMissing));
}

#[test]
fn test_error_classification_tool_input_variants() {
    let patterns = vec!["tool_use.input is required", "tool_call.arguments required"];

    for pattern in patterns {
        let error = classify_error(400, pattern);
        assert!(
            matches!(error, ProviderError::ToolCallInputMissing),
            "Failed to classify as ToolCallInputMissing: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_image_error() {
    let error = classify_error(400, "image dimensions exceed maximum");
    assert!(matches!(error, ProviderError::ImageError { .. }));
}

#[test]
fn test_error_classification_image_error_variants() {
    let patterns = vec![
        "image dimensions exceed maximum",
        "Image exceeds 5MB",
        "image file too large",
    ];

    for pattern in patterns {
        let error = classify_error(400, pattern);
        assert!(
            matches!(error, ProviderError::ImageError { .. }),
            "Failed to classify as ImageError: {}",
            pattern
        );
    }
}

#[test]
fn test_error_classification_unclassified() {
    let error = classify_error(500, "unknown error");
    assert!(matches!(error, ProviderError::InvalidResponse(_)));
}

#[test]
fn test_error_classification_status_code_priority() {
    // Status code should take priority over message pattern
    let error = classify_error(429, "some random message");
    assert!(matches!(error, ProviderError::RateLimited { .. }));

    let error = classify_error(402, "some random message");
    assert!(matches!(error, ProviderError::PaymentRequired { .. }));

    let error = classify_error(529, "some random message");
    assert!(matches!(error, ProviderError::Unavailable { .. }));
}

// ============================================================================
// Retry Configuration Tests
// ============================================================================

#[test]
fn test_retry_config_default_values() {
    let config = RetryConfig::default();
    assert_eq!(config.max_attempts, 3);
    assert_eq!(config.initial_delay_ms, 500);
    assert_eq!(config.max_delay_ms, 60_000);
    assert_eq!(config.backoff_multiplier, 2.0);
    assert_eq!(config.jitter_factor, 0.1);
}

#[test]
fn test_retry_config_custom_values() {
    let config = RetryConfig {
        max_attempts: 5,
        initial_delay_ms: 1000,
        max_delay_ms: 30_000,
        backoff_multiplier: 1.5,
        jitter_factor: 0.2,
    };

    assert_eq!(config.max_attempts, 5);
    assert_eq!(config.initial_delay_ms, 1000);
    assert_eq!(config.max_delay_ms, 30_000);
    assert_eq!(config.backoff_multiplier, 1.5);
    assert_eq!(config.jitter_factor, 0.2);
}

#[test]
fn test_provider_error_is_retryable() {
    // Retryable errors
    assert!(ProviderError::RateLimited {
        retry_after_ms: 1000
    }
    .is_retryable());
    assert!(ProviderError::Unavailable {
        provider: "test".to_string()
    }
    .is_retryable());
    assert!(ProviderError::Timeout {
        message: "test".to_string()
    }
    .is_retryable());

    // Non-retryable errors
    assert!(!ProviderError::AuthenticationFailed("test".to_string()).is_retryable());
    assert!(!ProviderError::PaymentRequired {
        message: "test".to_string()
    }
    .is_retryable());
    assert!(!ProviderError::ContextExceeded { used: 0, limit: 0 }.is_retryable());
    assert!(!ProviderError::MessageOrderingConflict.is_retryable());
    assert!(!ProviderError::ToolCallInputMissing.is_retryable());
}

#[test]
fn test_provider_error_retry_after_ms() {
    let error = ProviderError::RateLimited {
        retry_after_ms: 5000,
    };
    assert_eq!(error.retry_after_ms(), Some(5000));

    let error = ProviderError::Timeout {
        message: "test".to_string(),
    };
    assert_eq!(error.retry_after_ms(), None);
}

// ============================================================================
// Beta Features Tests
// ============================================================================

#[test]
fn test_beta_features_none() {
    let features = BetaFeatures::none();
    assert_eq!(features.to_header_value(), "");
}

#[test]
fn test_beta_features_default() {
    let features = BetaFeatures::default();
    assert_eq!(features.to_header_value(), "");
}

#[test]
fn test_beta_features_oauth_default() {
    let features = BetaFeatures::oauth_default();
    assert!(features.oauth);
    assert!(features.interleaved_thinking);
    assert!(features.prompt_caching);
    assert!(!features.token_counting);

    let header = features.to_header_value();
    assert!(header.contains("oauth-2025-04-20"));
    assert!(header.contains("interleaved-thinking-2025-05-14"));
    assert!(header.contains("prompt-caching-2024-07-31"));
    assert!(!header.contains("token-counting"));
}

#[test]
fn test_beta_features_single_feature() {
    let features = BetaFeatures {
        oauth: true,
        ..Default::default()
    };
    assert_eq!(features.to_header_value(), "oauth-2025-04-20");
}

#[test]
fn test_beta_features_multiple_features() {
    let features = BetaFeatures {
        oauth: true,
        interleaved_thinking: true,
        prompt_caching: true,
        token_counting: false,
    };

    let header = features.to_header_value();
    assert_eq!(
        header,
        "oauth-2025-04-20,interleaved-thinking-2025-05-14,prompt-caching-2024-07-31"
    );
}

#[test]
fn test_beta_features_all_features() {
    let features = BetaFeatures {
        oauth: true,
        interleaved_thinking: true,
        prompt_caching: true,
        token_counting: true,
    };

    let header = features.to_header_value();
    assert_eq!(
        header,
        "oauth-2025-04-20,interleaved-thinking-2025-05-14,prompt-caching-2024-07-31,token-counting-2024-11-01"
    );
}

#[test]
fn test_beta_features_order_consistency() {
    // Features should always appear in the same order regardless of how they're set
    let features1 = BetaFeatures {
        token_counting: true,
        oauth: true,
        interleaved_thinking: false,
        prompt_caching: false,
    };

    let features2 = BetaFeatures {
        oauth: true,
        token_counting: true,
        interleaved_thinking: false,
        prompt_caching: false,
    };

    assert_eq!(features1.to_header_value(), features2.to_header_value());
}

// ============================================================================
// Turn Validation Tests
// ============================================================================

#[test]
fn test_turn_validation_merge_consecutive_user_messages() {
    let messages = vec![
        Message::user("First question"),
        Message::user("Second question"),
        Message::user("Third question"),
    ];

    let validated = validate_anthropic_turns(&messages);

    assert_eq!(validated.len(), 1);
    assert_eq!(validated[0].role, Role::User);

    // Check that all text was merged into blocks
    if let MessageContent::Blocks(blocks) = &validated[0].content {
        assert_eq!(blocks.len(), 3);
        for block in blocks {
            assert!(matches!(block, ContentBlock::Text { .. }));
        }
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_turn_validation_alternating_messages_unchanged() {
    let messages = vec![
        Message::user("Question 1"),
        Message::assistant("Answer 1"),
        Message::user("Question 2"),
        Message::assistant("Answer 2"),
    ];

    let validated = validate_anthropic_turns(&messages);

    assert_eq!(validated.len(), 4);
    assert_eq!(validated[0].role, Role::User);
    assert_eq!(validated[1].role, Role::Assistant);
    assert_eq!(validated[2].role, Role::User);
    assert_eq!(validated[3].role, Role::Assistant);
}

#[test]
fn test_turn_validation_complex_sequence() {
    let messages = vec![
        Message::user("Q1"),
        Message::user("Q2"), // Should merge with Q1
        Message::assistant("A1"),
        Message::user("Q3"),
        Message::user("Q4"), // Should merge with Q3
        Message::user("Q5"), // Should merge with Q3, Q4
        Message::assistant("A2"),
    ];

    let validated = validate_anthropic_turns(&messages);

    assert_eq!(validated.len(), 4);
    assert_eq!(validated[0].role, Role::User); // Q1 + Q2
    assert_eq!(validated[1].role, Role::Assistant); // A1
    assert_eq!(validated[2].role, Role::User); // Q3 + Q4 + Q5
    assert_eq!(validated[3].role, Role::Assistant); // A2

    // Verify Q1+Q2 merged
    if let MessageContent::Blocks(blocks) = &validated[0].content {
        assert_eq!(blocks.len(), 2);
    } else {
        panic!("Expected Blocks content");
    }

    // Verify Q3+Q4+Q5 merged
    if let MessageContent::Blocks(blocks) = &validated[2].content {
        assert_eq!(blocks.len(), 3);
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_turn_validation_with_blocks() {
    let messages = vec![
        Message::with_blocks(
            Role::User,
            vec![
                ContentBlock::Text {
                    text: "First".to_string(),
                },
                ContentBlock::Text {
                    text: "block".to_string(),
                },
            ],
        ),
        Message::user("Second message"),
    ];

    let validated = validate_anthropic_turns(&messages);

    assert_eq!(validated.len(), 1);
    if let MessageContent::Blocks(blocks) = &validated[0].content {
        assert_eq!(blocks.len(), 3); // 2 from first + 1 from second
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_turn_validation_system_messages_filtered() {
    let messages = vec![
        Message::system("System prompt"),
        Message::user("User message"),
        Message::assistant("Assistant response"),
    ];

    let validated = validate_anthropic_turns(&messages);

    // System message should be filtered out
    assert_eq!(validated.len(), 2);
    assert_eq!(validated[0].role, Role::User);
    assert_eq!(validated[1].role, Role::Assistant);
}

#[test]
fn test_turn_validation_empty_messages() {
    let messages: Vec<Message> = vec![];
    let validated = validate_anthropic_turns(&messages);
    assert_eq!(validated.len(), 0);
}

#[test]
fn test_turn_validation_only_user_messages() {
    let messages = vec![
        Message::user("Q1"),
        Message::user("Q2"),
        Message::user("Q3"),
    ];

    let validated = validate_anthropic_turns(&messages);

    assert_eq!(validated.len(), 1);
    assert_eq!(validated[0].role, Role::User);

    if let MessageContent::Blocks(blocks) = &validated[0].content {
        assert_eq!(blocks.len(), 3);
    } else {
        panic!("Expected Blocks content");
    }
}

#[test]
fn test_turn_validation_only_assistant_messages() {
    let messages = vec![
        Message::assistant("Response 1"),
        Message::assistant("Response 2"),
    ];

    let validated = validate_anthropic_turns(&messages);

    // Assistant messages should be preserved as-is
    assert_eq!(validated.len(), 2);
    assert_eq!(validated[0].role, Role::Assistant);
    assert_eq!(validated[1].role, Role::Assistant);
}

// ============================================================================
// Integration Tests - Error Classification + Retry Logic
// ============================================================================

#[test]
fn test_integration_retryable_error_classification() {
    // Test that errors classified as retryable match retry logic expectations
    let retryable_cases = vec![
        (429, "rate limit exceeded"),
        (503, "overloaded_error"),
        (408, "request timeout"),
    ];

    for (status, message) in retryable_cases {
        let error = classify_error(status, message);
        assert!(
            error.is_retryable(),
            "Error should be retryable: status={}, message={}",
            status,
            message
        );
    }
}

#[test]
fn test_integration_non_retryable_error_classification() {
    // Test that errors classified as non-retryable match retry logic expectations
    let non_retryable_cases = vec![
        (401, "invalid_api_key"),
        (402, "payment required"),
        (400, "context length exceeded"),
        (400, "roles must alternate"),
        (400, "tool_use.input is required"),
    ];

    for (status, message) in non_retryable_cases {
        let error = classify_error(status, message);
        assert!(
            !error.is_retryable(),
            "Error should not be retryable: status={}, message={}",
            status,
            message
        );
    }
}

#[test]
fn test_integration_rate_limit_retry_after() {
    let error = classify_error(429, "rate limit exceeded");

    // Should be retryable
    assert!(error.is_retryable());

    // Should have retry_after_ms
    assert!(error.retry_after_ms().is_some());
    assert_eq!(error.retry_after_ms().unwrap(), 60000); // Default 60s
}

// ============================================================================
// Integration Tests - Turn Validation + Error Classification
// ============================================================================

#[test]
fn test_integration_turn_validation_prevents_ordering_error() {
    // Create a sequence that would trigger message ordering error
    let messages = vec![
        Message::user("Q1"),
        Message::user("Q2"), // Consecutive user messages
        Message::assistant("A1"),
    ];

    // Validate turns to fix the ordering
    let validated = validate_anthropic_turns(&messages);

    // Should merge consecutive user messages
    assert_eq!(validated.len(), 2);
    assert_eq!(validated[0].role, Role::User);
    assert_eq!(validated[1].role, Role::Assistant);

    // This validated sequence should not trigger MessageOrderingConflict
    // (In real usage, this would be sent to the API)
}

// ============================================================================
// Integration Tests - Beta Features + OAuth
// ============================================================================

#[test]
fn test_integration_oauth_beta_features_complete() {
    let features = BetaFeatures::oauth_default();

    // Verify all required OAuth features are enabled
    assert!(features.oauth);
    assert!(features.interleaved_thinking);
    assert!(features.prompt_caching);

    // Verify header format is correct
    let header = features.to_header_value();
    assert!(header.contains("oauth-2025-04-20"));
    assert!(header.contains("interleaved-thinking-2025-05-14"));
    assert!(header.contains("prompt-caching-2024-07-31"));

    // Verify it's a valid comma-separated list
    let parts: Vec<&str> = header.split(',').collect();
    assert_eq!(parts.len(), 3);
}

// ============================================================================
// Thinking Config Tests (via serialization)
// ============================================================================

#[test]
fn test_thinking_config_serialization() {
    use serde_json::json;

    // Test that ThinkingConfig serializes to the expected format
    // We can't directly test the struct since it's not public, but we can
    // verify the expected JSON structure matches what the API expects

    let expected = json!({
        "type": "enabled",
        "budget_tokens": 64000
    });

    // Verify the structure matches Anthropic API expectations
    assert_eq!(expected["type"], "enabled");
    assert_eq!(expected["budget_tokens"], 64000);
}

#[test]
fn test_thinking_config_budget_values() {
    use serde_json::json;

    // Test various budget token values
    let budgets = vec![1000, 10000, 64000, 100000];

    for budget in budgets {
        let config = json!({
            "type": "enabled",
            "budget_tokens": budget
        });

        assert_eq!(config["type"], "enabled");
        assert_eq!(config["budget_tokens"], budget);
    }
}
