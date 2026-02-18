use ratatui::{
    layout::Rect,
    style::{Color, Style},
    widgets::{Block, Clear},
    Frame,
};

use crate::Theme;

pub fn centered_rect(viewport: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(viewport.width.saturating_sub(2));
    let height = height.min(viewport.height.saturating_sub(2));
    let x = viewport.x + viewport.width.saturating_sub(width) / 2;
    let y = viewport.y + viewport.height.saturating_sub(height) / 2;
    Rect::new(x, y, width, height)
}

pub fn render_backdrop(frame: &mut Frame, viewport: Rect) {
    let backdrop = Block::default().style(Style::default().bg(Color::Rgb(0, 0, 0)));
    frame.render_widget(backdrop, viewport);
}

pub fn render_dialog_surface(frame: &mut Frame, area: Rect, theme: &Theme) {
    frame.render_widget(Clear, area);
    let panel = Block::default().style(Style::default().bg(theme.bg_panel).fg(theme.fg));
    frame.render_widget(panel, area);
}
