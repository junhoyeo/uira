//! Beta feature management for Anthropic API
//!
//! This module provides dynamic construction of the `anthropic-beta` header
//! based on which features are enabled.

/// Beta feature: OAuth 2.0 authentication
pub const BETA_OAUTH: &str = "oauth-2025-04-20";

/// Beta feature: Interleaved thinking (extended thinking)
pub const BETA_INTERLEAVED_THINKING: &str = "interleaved-thinking-2025-05-14";

/// Beta feature: Prompt caching
pub const BETA_PROMPT_CACHING: &str = "prompt-caching-2024-07-31";

/// Beta feature: Token counting
pub const BETA_TOKEN_COUNTING: &str = "token-counting-2024-11-01";

/// Tracks which beta features are enabled and builds the header value
#[derive(Debug, Default, Clone)]
pub struct BetaFeatures {
    /// Enable OAuth 2.0 authentication
    pub oauth: bool,
    /// Enable interleaved thinking
    pub interleaved_thinking: bool,
    /// Enable prompt caching
    pub prompt_caching: bool,
    /// Enable token counting
    pub token_counting: bool,
}

impl BetaFeatures {
    /// Build the comma-separated header value from enabled features
    ///
    /// # Returns
    /// A comma-separated string of enabled beta features, or empty string if none enabled
    ///
    /// # Example
    /// ```
    /// use uira_providers::BetaFeatures;
    ///
    /// let features = BetaFeatures {
    ///     oauth: true,
    ///     interleaved_thinking: true,
    ///     ..Default::default()
    /// };
    /// assert_eq!(
    ///     features.to_header_value(),
    ///     "oauth-2025-04-20,interleaved-thinking-2025-05-14"
    /// );
    /// ```
    pub fn to_header_value(&self) -> String {
        let mut features = Vec::new();

        if self.oauth {
            features.push(BETA_OAUTH);
        }
        if self.interleaved_thinking {
            features.push(BETA_INTERLEAVED_THINKING);
        }
        if self.prompt_caching {
            features.push(BETA_PROMPT_CACHING);
        }
        if self.token_counting {
            features.push(BETA_TOKEN_COUNTING);
        }

        features.join(",")
    }

    /// Create a BetaFeatures configuration for OAuth mode
    ///
    /// OAuth mode requires:
    /// - OAuth authentication
    /// - Interleaved thinking (extended thinking)
    /// - Prompt caching
    ///
    /// # Example
    /// ```
    /// use uira_providers::BetaFeatures;
    ///
    /// let features = BetaFeatures::oauth_default();
    /// assert!(features.oauth);
    /// assert!(features.interleaved_thinking);
    /// assert!(features.prompt_caching);
    /// assert!(!features.token_counting);
    /// ```
    pub fn oauth_default() -> Self {
        Self {
            oauth: true,
            interleaved_thinking: true,
            prompt_caching: true,
            token_counting: false,
        }
    }

    /// Create a BetaFeatures configuration with no features enabled
    ///
    /// This is the default for non-OAuth API key mode.
    ///
    /// # Example
    /// ```
    /// use uira_providers::BetaFeatures;
    ///
    /// let features = BetaFeatures::none();
    /// assert_eq!(features.to_header_value(), "");
    /// ```
    pub fn none() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_features() {
        let features = BetaFeatures::default();
        assert_eq!(features.to_header_value(), "");
    }

    #[test]
    fn test_single_feature() {
        let features = BetaFeatures {
            oauth: true,
            ..Default::default()
        };
        assert_eq!(features.to_header_value(), BETA_OAUTH);
    }

    #[test]
    fn test_multiple_features() {
        let features = BetaFeatures {
            oauth: true,
            interleaved_thinking: true,
            prompt_caching: true,
            ..Default::default()
        };
        assert_eq!(
            features.to_header_value(),
            "oauth-2025-04-20,interleaved-thinking-2025-05-14,prompt-caching-2024-07-31"
        );
    }

    #[test]
    fn test_all_features() {
        let features = BetaFeatures {
            oauth: true,
            interleaved_thinking: true,
            prompt_caching: true,
            token_counting: true,
        };
        assert_eq!(
            features.to_header_value(),
            "oauth-2025-04-20,interleaved-thinking-2025-05-14,prompt-caching-2024-07-31,token-counting-2024-11-01"
        );
    }

    #[test]
    fn test_oauth_default() {
        let features = BetaFeatures::oauth_default();
        assert!(features.oauth);
        assert!(features.interleaved_thinking);
        assert!(features.prompt_caching);
        assert!(!features.token_counting);

        assert_eq!(
            features.to_header_value(),
            "oauth-2025-04-20,interleaved-thinking-2025-05-14,prompt-caching-2024-07-31"
        );
    }

    #[test]
    fn test_none() {
        let features = BetaFeatures::none();
        assert!(!features.oauth);
        assert!(!features.interleaved_thinking);
        assert!(!features.prompt_caching);
        assert!(!features.token_counting);
        assert_eq!(features.to_header_value(), "");
    }

    #[test]
    fn test_feature_order() {
        let features1 = BetaFeatures {
            token_counting: true,
            oauth: true,
            ..Default::default()
        };
        let features2 = BetaFeatures {
            oauth: true,
            token_counting: true,
            ..Default::default()
        };
        assert_eq!(features1.to_header_value(), features2.to_header_value());
    }
}
