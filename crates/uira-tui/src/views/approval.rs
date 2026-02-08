//! Approval overlay with queue-based modal (Codex pattern)
//!
//! This module implements the ApprovalOverlay which displays tool approval
//! requests as a centered modal with keyboard shortcuts.

use crossterm::event::KeyCode;
use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph, Widget, Wrap},
    Frame,
};
use tokio::sync::oneshot;
use uira_protocol::ReviewDecision;

use crate::Theme;

/// Approval request with response channel
pub struct ApprovalRequest {
    /// Unique request ID
    pub id: String,
    /// Name of the tool requesting approval
    pub tool_name: String,
    /// Tool input parameters
    pub input: serde_json::Value,
    /// Reason for requiring approval
    pub reason: String,
    /// Channel to send the decision
    pub response_tx: oneshot::Sender<ReviewDecision>,
}

impl std::fmt::Debug for ApprovalRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ApprovalRequest")
            .field("id", &self.id)
            .field("tool_name", &self.tool_name)
            .field("input", &self.input)
            .field("reason", &self.reason)
            .field("response_tx", &"<oneshot::Sender>")
            .finish()
    }
}

/// Queue-based approval overlay (Codex pattern)
pub struct ApprovalOverlay {
    /// Current request being displayed
    current: Option<ApprovalRequest>,
    /// Queued requests waiting to be shown
    queue: Vec<ApprovalRequest>,
    /// Currently selected option (0=Approve, 1=ApproveAll, 2=Deny)
    selected: usize,
    /// Scroll offset for long input display
    scroll: u16,
    /// Current theme
    theme: Theme,
}

impl ApprovalOverlay {
    /// Create a new approval overlay
    pub fn new() -> Self {
        Self {
            current: None,
            queue: Vec::new(),
            selected: 0,
            scroll: 0,
            theme: Theme::default(),
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    /// Add an approval request to the queue
    pub fn enqueue(&mut self, request: ApprovalRequest) {
        if self.current.is_none() {
            self.current = Some(request);
            self.selected = 0;
            self.scroll = 0;
        } else {
            self.queue.push(request);
        }
    }

    /// Check if the overlay is currently active
    pub fn is_active(&self) -> bool {
        self.current.is_some()
    }

    /// Get the number of pending approvals (including current)
    pub fn pending_count(&self) -> usize {
        if self.current.is_some() {
            1 + self.queue.len()
        } else {
            self.queue.len()
        }
    }

    /// Handle a key event, returns true if consumed
    pub fn handle_key(&mut self, key: KeyCode) -> bool {
        if self.current.is_none() {
            return false;
        }

        match key {
            // Quick approve shortcuts
            KeyCode::Char('y') | KeyCode::Char('Y') => {
                self.submit(ReviewDecision::Approve);
                true
            }
            // Approve all (session-wide)
            KeyCode::Char('a') | KeyCode::Char('A') => {
                self.submit(ReviewDecision::ApproveAll);
                true
            }
            // Deny shortcuts
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                self.submit(ReviewDecision::Deny { reason: None });
                true
            }
            // Navigation between options
            KeyCode::Left | KeyCode::Char('h') => {
                self.selected = self.selected.saturating_sub(1);
                true
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.selected = (self.selected + 1).min(2);
                true
            }
            KeyCode::Tab => {
                self.selected = (self.selected + 1) % 3;
                true
            }
            KeyCode::BackTab => {
                self.selected = if self.selected == 0 {
                    2
                } else {
                    self.selected - 1
                };
                true
            }
            // Confirm selected option
            KeyCode::Enter => {
                let decision = match self.selected {
                    0 => ReviewDecision::Approve,
                    1 => ReviewDecision::ApproveAll,
                    _ => ReviewDecision::Deny { reason: None },
                };
                self.submit(decision);
                true
            }
            // Scroll input display
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                true
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_add(1);
                true
            }
            _ => false,
        }
    }

    /// Submit a decision and advance to next request
    fn submit(&mut self, decision: ReviewDecision) {
        if let Some(request) = self.current.take() {
            // Send decision (ignore error if receiver dropped)
            let _ = request.response_tx.send(decision);
        }

        // Advance to next in queue
        self.current = self.queue.pop();
        self.selected = 0;
        self.scroll = 0;
    }

    /// Deny all pending approvals (e.g., on quit)
    pub fn deny_all(&mut self) {
        if let Some(request) = self.current.take() {
            let _ = request.response_tx.send(ReviewDecision::Deny {
                reason: Some("Application closed".to_string()),
            });
        }

        for request in self.queue.drain(..) {
            let _ = request.response_tx.send(ReviewDecision::Deny {
                reason: Some("Application closed".to_string()),
            });
        }
    }

    /// Render the overlay
    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if let Some(ref request) = self.current {
            // Calculate centered modal dimensions
            let width = 70.min(area.width.saturating_sub(4));
            let height = 18.min(area.height.saturating_sub(4));
            let x = (area.width.saturating_sub(width)) / 2;
            let y = (area.height.saturating_sub(height)) / 2;
            let modal_area = Rect::new(x, y, width, height);

            // Clear the background
            frame.render_widget(Clear, modal_area);

            // Render the modal
            frame.render_widget(
                ApprovalModal {
                    request,
                    selected: self.selected,
                    scroll: self.scroll,
                    queue_count: self.queue.len(),
                    theme: &self.theme,
                },
                modal_area,
            );
        }
    }
}

impl Default for ApprovalOverlay {
    fn default() -> Self {
        Self::new()
    }
}

/// The approval modal widget
struct ApprovalModal<'a> {
    request: &'a ApprovalRequest,
    selected: usize,
    scroll: u16,
    queue_count: usize,
    theme: &'a Theme,
}

impl Widget for ApprovalModal<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Modal border with title
        let title = if self.queue_count > 0 {
            format!(
                " Approve: {} (+{} more) ",
                self.request.tool_name, self.queue_count
            )
        } else {
            format!(" Approve: {} ", self.request.tool_name)
        };

        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.borders));

        let inner = block.inner(area);
        block.render(area, buf);

        // Split inner area into sections
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Reason
                Constraint::Min(5),    // Input JSON
                Constraint::Length(1), // Separator
                Constraint::Length(2), // Buttons
                Constraint::Length(1), // Help text
            ])
            .split(inner);

        // Reason section
        let reason_text = format!("Reason: {}", self.request.reason);
        let reason = Paragraph::new(reason_text)
            .style(Style::default().fg(self.theme.fg))
            .wrap(Wrap { trim: true });
        reason.render(chunks[0], buf);

        // Input JSON section
        let input_str = serde_json::to_string_pretty(&self.request.input)
            .unwrap_or_else(|_| self.request.input.to_string());
        let input_lines: Vec<Line> = input_str
            .lines()
            .skip(self.scroll as usize)
            .take(chunks[1].height as usize)
            .map(|line| Line::from(Span::styled(line, Style::default().fg(self.theme.accent))))
            .collect();

        let input_block = Block::default()
            .title(" Input ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(self.theme.borders));

        let input_inner = input_block.inner(chunks[1]);
        input_block.render(chunks[1], buf);

        let input_para = Paragraph::new(input_lines);
        input_para.render(input_inner, buf);

        // Buttons section
        self.render_buttons(chunks[3], buf);

        // Help text
        let help = Paragraph::new(
            "↑↓/jk: scroll | ←→/hl/Tab: select | Enter: confirm | y: yes | a: all | n/Esc: no",
        )
        .style(Style::default().fg(self.theme.borders))
        .alignment(Alignment::Center);
        help.render(chunks[4], buf);
    }
}

impl ApprovalModal<'_> {
    fn render_buttons(&self, area: Rect, buf: &mut Buffer) {
        let buttons = [("[Y]es", 0), ("[A]ll", 1), ("[N]o", 2)];

        let button_width = 12;
        let total_width = buttons.len() as u16 * button_width;
        let start_x = area.x + (area.width.saturating_sub(total_width)) / 2;

        for (i, (label, idx)) in buttons.iter().enumerate() {
            let x = start_x + (i as u16 * button_width);

            let style = if self.selected == *idx {
                Style::default()
                    .fg(Theme::contrast_text(self.theme.accent))
                    .bg(self.theme.accent)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(self.theme.fg)
            };

            // Pad label to center in button width
            let padded = format!("{:^width$}", label, width = button_width as usize - 2);
            buf.set_string(x, area.y, &padded, style);
        }
    }
}

/// Simple approval view widget (for inline display)
pub struct ApprovalView<'a> {
    tool_name: &'a str,
    description: &'a str,
    selected: usize, // 0 = Approve, 1 = Deny, 2 = Edit
    theme: Theme,
}

impl<'a> ApprovalView<'a> {
    pub fn new(tool_name: &'a str, description: &'a str) -> Self {
        Self {
            tool_name,
            description,
            selected: 0,
            theme: Theme::default(),
        }
    }

    pub fn with_theme(mut self, theme: Theme) -> Self {
        self.theme = theme;
        self
    }

    pub fn select_next(&mut self) {
        self.selected = (self.selected + 1) % 3;
    }

    pub fn select_prev(&mut self) {
        self.selected = if self.selected == 0 {
            2
        } else {
            self.selected - 1
        };
    }

    pub fn selected(&self) -> usize {
        self.selected
    }
}

impl Widget for ApprovalView<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        // Clear the area first
        Clear.render(area, buf);

        let block = Block::default()
            .title(format!(" Approve: {} ", self.tool_name))
            .borders(Borders::ALL)
            .style(Style::default().fg(self.theme.borders));

        let inner = block.inner(area);
        block.render(area, buf);

        let lines = vec![Line::from(self.description), Line::from(""), Line::from("")];

        let content = Paragraph::new(lines)
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Left);

        content.render(inner, buf);

        // Render buttons at the bottom
        let button_y = inner.y + inner.height.saturating_sub(2);
        let buttons = ["[A]pprove", "[D]eny", "[E]dit"];
        let mut x = inner.x + 2;

        for (i, button) in buttons.iter().enumerate() {
            let style = if i == self.selected {
                Style::default()
                    .fg(Theme::contrast_text(self.theme.accent))
                    .bg(self.theme.accent)
            } else {
                Style::default().fg(self.theme.fg)
            };

            buf.set_string(x, button_y, *button, style);
            x += button.len() as u16 + 2;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_approval_overlay_queue() {
        let mut overlay = ApprovalOverlay::new();
        assert!(!overlay.is_active());
        assert_eq!(overlay.pending_count(), 0);

        // Create mock requests
        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();

        overlay.enqueue(ApprovalRequest {
            id: "req1".to_string(),
            tool_name: "write_file".to_string(),
            input: serde_json::json!({"path": "/tmp/test.txt"}),
            reason: "Writes to filesystem".to_string(),
            response_tx: tx1,
        });

        assert!(overlay.is_active());
        assert_eq!(overlay.pending_count(), 1);

        overlay.enqueue(ApprovalRequest {
            id: "req2".to_string(),
            tool_name: "bash".to_string(),
            input: serde_json::json!({"command": "ls"}),
            reason: "Executes command".to_string(),
            response_tx: tx2,
        });

        assert_eq!(overlay.pending_count(), 2);

        // First is current, second is queued
        assert_eq!(overlay.current.as_ref().unwrap().id, "req1");
        assert_eq!(overlay.queue.len(), 1);
    }

    #[test]
    fn test_approval_overlay_navigation() {
        let mut overlay = ApprovalOverlay::new();
        let (tx, _rx) = oneshot::channel();

        overlay.enqueue(ApprovalRequest {
            id: "req1".to_string(),
            tool_name: "test".to_string(),
            input: serde_json::Value::Null,
            reason: "Test".to_string(),
            response_tx: tx,
        });

        assert_eq!(overlay.selected, 0);

        overlay.handle_key(KeyCode::Right);
        assert_eq!(overlay.selected, 1);

        overlay.handle_key(KeyCode::Right);
        assert_eq!(overlay.selected, 2);

        overlay.handle_key(KeyCode::Right);
        assert_eq!(overlay.selected, 2); // Max

        overlay.handle_key(KeyCode::Left);
        assert_eq!(overlay.selected, 1);

        overlay.handle_key(KeyCode::Tab);
        assert_eq!(overlay.selected, 2);

        overlay.handle_key(KeyCode::Tab);
        assert_eq!(overlay.selected, 0); // Wrap
    }

    #[test]
    fn test_approval_overlay_shortcuts() {
        let mut overlay = ApprovalOverlay::new();

        // Test 'y' shortcut
        let (tx1, rx1) = oneshot::channel();
        overlay.enqueue(ApprovalRequest {
            id: "req1".to_string(),
            tool_name: "test".to_string(),
            input: serde_json::Value::Null,
            reason: "Test".to_string(),
            response_tx: tx1,
        });

        overlay.handle_key(KeyCode::Char('y'));
        let decision = rx1.blocking_recv().unwrap();
        assert!(matches!(decision, ReviewDecision::Approve));
        assert!(!overlay.is_active());

        // Test 'n' shortcut
        let (tx2, rx2) = oneshot::channel();
        overlay.enqueue(ApprovalRequest {
            id: "req2".to_string(),
            tool_name: "test".to_string(),
            input: serde_json::Value::Null,
            reason: "Test".to_string(),
            response_tx: tx2,
        });

        overlay.handle_key(KeyCode::Char('n'));
        let decision = rx2.blocking_recv().unwrap();
        assert!(matches!(decision, ReviewDecision::Deny { .. }));

        // Test 'a' shortcut
        let (tx3, rx3) = oneshot::channel();
        overlay.enqueue(ApprovalRequest {
            id: "req3".to_string(),
            tool_name: "test".to_string(),
            input: serde_json::Value::Null,
            reason: "Test".to_string(),
            response_tx: tx3,
        });

        overlay.handle_key(KeyCode::Char('a'));
        let decision = rx3.blocking_recv().unwrap();
        assert!(matches!(decision, ReviewDecision::ApproveAll));
    }

    #[test]
    fn test_approval_overlay_advance_queue() {
        let mut overlay = ApprovalOverlay::new();

        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();

        overlay.enqueue(ApprovalRequest {
            id: "req1".to_string(),
            tool_name: "first".to_string(),
            input: serde_json::Value::Null,
            reason: "First".to_string(),
            response_tx: tx1,
        });

        overlay.enqueue(ApprovalRequest {
            id: "req2".to_string(),
            tool_name: "second".to_string(),
            input: serde_json::Value::Null,
            reason: "Second".to_string(),
            response_tx: tx2,
        });

        assert_eq!(overlay.current.as_ref().unwrap().tool_name, "first");

        // Approve first, should advance to second
        overlay.handle_key(KeyCode::Char('y'));
        assert!(overlay.is_active());
        assert_eq!(overlay.current.as_ref().unwrap().tool_name, "second");

        // Approve second, should become inactive
        overlay.handle_key(KeyCode::Char('y'));
        assert!(!overlay.is_active());
    }
}
