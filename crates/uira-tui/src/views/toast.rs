//! Toast notification system for temporary messages
//!
//! Toasts appear at the bottom-right corner and auto-dismiss after a duration.
//! Supports multiple variants (Success, Error, Warning, Info) with themed colors.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ratatui::{
    layout::Rect,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::Theme;

/// Toast notification variant with associated styling
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastVariant {
    /// Success message (green)
    Success,
    /// Error message (red)
    Error,
    /// Warning message (yellow)
    Warning,
    /// Info message (accent color)
    Info,
}

impl ToastVariant {
    /// Get the color for this variant based on theme
    fn color(&self, theme: &Theme) -> Color {
        match self {
            ToastVariant::Success => theme.success,
            ToastVariant::Error => theme.error,
            ToastVariant::Warning => theme.warning,
            ToastVariant::Info => theme.accent,
        }
    }
}

/// A single toast notification
#[derive(Debug, Clone)]
pub struct Toast {
    /// Message text
    pub message: String,
    /// Toast variant (determines color)
    pub variant: ToastVariant,
    /// When this toast was created
    created_at: Instant,
    /// How long to display this toast
    duration: Duration,
}

impl Toast {
    /// Create a new toast
    pub fn new(message: String, variant: ToastVariant, duration_ms: u64) -> Self {
        Self {
            message,
            variant,
            created_at: Instant::now(),
            duration: Duration::from_millis(duration_ms),
        }
    }

    /// Check if this toast has expired
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.duration
    }
}

/// Manages a queue of toast notifications
#[derive(Debug)]
pub struct ToastManager {
    /// Queue of active toasts (oldest first)
    toasts: VecDeque<Toast>,
    /// Maximum number of visible toasts
    max_visible: usize,
}

impl ToastManager {
    /// Create a new toast manager
    pub fn new() -> Self {
        Self {
            toasts: VecDeque::new(),
            max_visible: 3,
        }
    }

    /// Show a toast notification
    pub fn show(&mut self, message: String, variant: ToastVariant, duration_ms: u64) {
        let toast = Toast::new(message, variant, duration_ms);
        self.toasts.push_back(toast);

        // Remove oldest toasts if we exceed max_visible
        while self.toasts.len() > self.max_visible {
            self.toasts.pop_front();
        }
    }

    /// Remove expired toasts (call before rendering)
    pub fn tick(&mut self) {
        self.toasts.retain(|toast| !toast.is_expired());
    }

    /// Render toasts at the bottom-right corner of the given area
    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.toasts.is_empty() {
            return;
        }

        // Calculate toast dimensions
        let toast_width = std::cmp::min(40, area.width / 3);
        let toast_height = 3; // 1 line content + 2 for borders

        // Stack toasts from bottom-right, going upward
        for (idx, toast) in self.toasts.iter().enumerate() {
            let idx = idx as u16;
            // Calculate position from bottom-right
            let toast_y = area.bottom().saturating_sub((idx + 1) * toast_height + idx);

            // Skip if toast would go off-screen
            if toast_y < area.top() {
                break;
            }

            let toast_x = area.right().saturating_sub(toast_width);

            let toast_area = Rect {
                x: toast_x,
                y: toast_y,
                width: toast_width,
                height: toast_height,
            };

            // Render the toast
            let color = toast.variant.color(theme);
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(color));

            let inner = block.inner(toast_area);

            // Create the toast content with colored left border indicator
            let content = Line::from(vec![
                Span::styled("█ ", Style::default().fg(color)),
                Span::raw(&toast.message),
            ]);

            let paragraph = Paragraph::new(content);

            frame.render_widget(block, toast_area);
            frame.render_widget(paragraph, inner);
        }
    }
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_toast_expiration() {
        let toast = Toast::new("test".to_string(), ToastVariant::Info, 1);
        assert!(!toast.is_expired());

        // Sleep to let it expire
        std::thread::sleep(Duration::from_millis(10));
        assert!(toast.is_expired());
    }

    #[test]
    fn test_toast_manager_max_visible() {
        let mut manager = ToastManager::new();
        manager.show("toast1".to_string(), ToastVariant::Info, 5000);
        manager.show("toast2".to_string(), ToastVariant::Info, 5000);
        manager.show("toast3".to_string(), ToastVariant::Info, 5000);
        manager.show("toast4".to_string(), ToastVariant::Info, 5000);

        // Should only keep 3 most recent
        assert_eq!(manager.toasts.len(), 3);
        assert_eq!(manager.toasts[0].message, "toast2");
        assert_eq!(manager.toasts[2].message, "toast4");
    }

    #[test]
    fn test_toast_manager_tick() {
        let mut manager = ToastManager::new();
        manager.show("short".to_string(), ToastVariant::Info, 1);
        manager.show("long".to_string(), ToastVariant::Info, 5000);

        std::thread::sleep(Duration::from_millis(10));
        manager.tick();

        // Short toast should be expired and removed
        assert_eq!(manager.toasts.len(), 1);
        assert_eq!(manager.toasts[0].message, "long");
    }
}
