use super::types::RateLimitStatus;
use chrono::{DateTime, Duration, Utc};

/// Check current rate limit status by examining environment/state
pub async fn check_rate_limit_status() -> Option<RateLimitStatus> {
    // This is a placeholder implementation
    // In a real scenario, this would:
    // 1. Check Claude API response headers
    // 2. Read from a shared state file
    // 3. Call an API endpoint that tracks rate limits

    // For now, we'll check if any blocked panes exist as a proxy
    let blocked_panes = super::tmux::scan_for_blocked_panes(100);

    if blocked_panes.is_empty() {
        Some(RateLimitStatus {
            is_limited: false,
            last_checked_at: Utc::now(),
            five_hour_resets_at: None,
            weekly_resets_at: None,
            next_reset_at: None,
        })
    } else {
        // Estimate next reset time (this would come from API headers in reality)
        let next_reset = Utc::now() + Duration::minutes(30);

        Some(RateLimitStatus {
            is_limited: true,
            last_checked_at: Utc::now(),
            five_hour_resets_at: Some(next_reset),
            weekly_resets_at: None,
            next_reset_at: Some(next_reset),
        })
    }
}

/// Format time until reset in human-readable form
pub fn format_time_until_reset(reset_at: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = reset_at.signed_duration_since(now);

    if duration.num_seconds() < 0 {
        return "Reset time has passed".to_string();
    }

    let hours = duration.num_hours();
    let minutes = duration.num_minutes() % 60;
    let seconds = duration.num_seconds() % 60;

    if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format rate limit status as a human-readable string
pub fn format_rate_limit_status(status: &RateLimitStatus) -> String {
    if !status.is_limited {
        return "No active rate limits".to_string();
    }

    let mut parts = vec!["Rate limit active".to_string()];

    if let Some(reset_at) = status.next_reset_at {
        let time_until = format_time_until_reset(reset_at);
        parts.push(format!("Resets in: {}", time_until));
        parts.push(format!(
            "Reset time: {}",
            reset_at.format("%Y-%m-%d %H:%M:%S UTC")
        ));
    }

    if let Some(five_hour_reset) = status.five_hour_resets_at {
        parts.push(format!(
            "5-hour window resets: {} ({})",
            five_hour_reset.format("%H:%M:%S UTC"),
            format_time_until_reset(five_hour_reset)
        ));
    }

    if let Some(weekly_reset) = status.weekly_resets_at {
        parts.push(format!(
            "Weekly limit resets: {} ({})",
            weekly_reset.format("%Y-%m-%d %H:%M UTC"),
            format_time_until_reset(weekly_reset)
        ));
    }

    parts.join("\n")
}

/// Parse rate limit information from Claude API error response
pub fn parse_rate_limit_from_error(error_text: &str) -> Option<RateLimitStatus> {
    // Look for common rate limit error patterns
    let error_lower = error_text.to_lowercase();

    if !error_lower.contains("rate limit")
        && !error_lower.contains("429")
        && !error_lower.contains("too many requests")
    {
        return None;
    }

    // Try to extract reset time if present in error message
    // This is a simplified parser - real implementation would be more sophisticated

    Some(RateLimitStatus {
        is_limited: true,
        last_checked_at: Utc::now(),
        five_hour_resets_at: None,
        weekly_resets_at: None,
        next_reset_at: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn test_format_time_until_reset() {
        let future =
            Utc::now() + Duration::hours(2) + Duration::minutes(30) + Duration::seconds(45);
        let formatted = format_time_until_reset(future);
        assert!(formatted.contains("2h"));
        assert!(formatted.contains("30m"));
    }

    #[test]
    fn test_format_time_minutes_only() {
        let future = Utc::now() + Duration::minutes(5) + Duration::seconds(30);
        let formatted = format_time_until_reset(future);
        assert!(formatted.contains("5m"));
        assert!(!formatted.contains("h"));
    }

    #[test]
    fn test_format_time_past() {
        let past = Utc::now() - Duration::hours(1);
        let formatted = format_time_until_reset(past);
        assert!(formatted.contains("passed"));
    }

    #[test]
    fn test_parse_rate_limit_from_error() {
        let error = "Rate limit reached. Please try again later.";
        let status = parse_rate_limit_from_error(error);
        assert!(status.is_some());
        assert!(status.unwrap().is_limited);
    }

    #[test]
    fn test_parse_non_rate_limit_error() {
        let error = "Network connection failed";
        let status = parse_rate_limit_from_error(error);
        assert!(status.is_none());
    }
}
