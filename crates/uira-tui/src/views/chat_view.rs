use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
};
use std::collections::HashMap;

use crate::widgets::ChatMessage;
use crate::Theme;

pub struct ChatView {
    pub(crate) messages: Vec<ChatMessage>,
    pub(crate) streaming_buffer: Option<String>,
    pub(crate) thinking_buffer: Option<String>,
    pub(crate) tool_call_names: HashMap<String, String>,
    pub(crate) theme: Theme,
    pub(crate) scroll_offset: usize,
    pub(crate) viewport_height: u16,
    pub(crate) total_lines: usize,
    pub(crate) user_scrolled: bool,
    pub(crate) auto_follow: bool,
    pub(crate) rendered_lines: Vec<Vec<Line<'static>>>,
    pub(crate) last_render_width: u16,
    line_message_index: Vec<Option<usize>>,
    cached_message_count: usize,
    cache_dirty: bool,
}

impl ChatView {
    pub fn new(theme: Theme) -> Self {
        Self {
            messages: Vec::new(),
            streaming_buffer: None,
            thinking_buffer: None,
            tool_call_names: HashMap::new(),
            theme,
            scroll_offset: 0,
            viewport_height: 0,
            total_lines: 0,
            user_scrolled: false,
            auto_follow: true,
            rendered_lines: Vec::new(),
            last_render_width: 0,
            line_message_index: Vec::new(),
            cached_message_count: 0,
            cache_dirty: true,
        }
    }

    pub fn set_theme(&mut self, theme: Theme) {
        self.theme = theme;
    }

    pub fn message_style(&self, role: &str) -> Style {
        match role {
            "user" => Style::default().fg(self.theme.accent),
            "assistant" => Style::default().fg(self.theme.fg),
            "tool" => Style::default().fg(self.theme.accent),
            "error" => Style::default().fg(self.theme.error),
            "system" => Style::default().fg(self.theme.warning),
            _ => Style::default().fg(self.theme.fg),
        }
    }

    pub fn render_chat(&mut self, frame: &mut ratatui::Frame, area: Rect) {
        let block = Block::default()
            .title(" Uira ")
            .borders(Borders::ALL)
            .style(Style::default().fg(self.theme.borders).bg(self.theme.bg));

        let inner_width = area.width.saturating_sub(2);
        self.viewport_height = area.height.saturating_sub(2);
        if self.cached_message_count != self.messages.len() {
            self.cache_dirty = true;
        }

        self.rebuild_render_cache_if_needed(inner_width);
        self.clamp_scroll_offset();

        let start = self.scroll_offset.min(self.total_lines);
        let end = (start + self.viewport_height as usize).min(self.total_lines);

        let mut visible_lines: Vec<Line<'static>> = self
            .rendered_lines
            .iter()
            .flatten()
            .skip(start)
            .take(end.saturating_sub(start))
            .cloned()
            .collect();

        if visible_lines.is_empty() {
            visible_lines.push(Line::from(""));
        }

        let paragraph = Paragraph::new(Text::from(visible_lines))
            .block(block)
            .wrap(Wrap { trim: false });
        frame.render_widget(paragraph, area);
    }

    pub fn scroll_up(&mut self) {
        if self.scroll_offset > 0 {
            self.scroll_offset -= 1;
        }
        self.user_scrolled = true;
        self.auto_follow = false;
    }

    pub fn scroll_down(&mut self) {
        let max_offset = self.max_scroll_offset();
        if self.scroll_offset < max_offset {
            self.scroll_offset += 1;
        }
        self.sync_follow_with_position();
    }

    pub fn page_up(&mut self) {
        let amount = (self.viewport_height as usize).max(1);
        self.scroll_offset = self.scroll_offset.saturating_sub(amount);
        self.user_scrolled = true;
        self.auto_follow = false;
    }

    pub fn page_down(&mut self) {
        let amount = (self.viewport_height as usize).max(1);
        self.scroll_offset = (self.scroll_offset + amount).min(self.max_scroll_offset());
        self.sync_follow_with_position();
    }

    pub fn scroll_to_top(&mut self) {
        self.scroll_offset = 0;
        self.user_scrolled = true;
        self.auto_follow = false;
    }

    pub fn scroll_to_bottom(&mut self) {
        self.scroll_to_bottom_internal();
        self.user_scrolled = false;
        self.auto_follow = true;
    }

    pub fn auto_scroll_to_bottom(&mut self) {
        if self.auto_follow {
            self.scroll_to_bottom_internal();
            self.user_scrolled = false;
        }
    }

    pub fn total_items(&self) -> usize {
        self.total_lines
    }

    pub fn push_message(&mut self, role: &str, content: String) {
        self.messages.push(ChatMessage::new(role, content));
        self.invalidate_render_cache();
        self.auto_scroll_to_bottom();
    }

    pub fn push_tool_message(
        &mut self,
        role: &str,
        tool_name: String,
        content: String,
        is_collapsed: bool,
    ) {
        let summary = summarize_tool_output(&content);
        self.messages.push(ChatMessage::tool(
            role,
            content,
            tool_name,
            summary,
            is_collapsed,
        ));
        self.invalidate_render_cache();
        self.auto_scroll_to_bottom();
    }

    pub fn toggle_selected_tool_output(&mut self) -> Option<String> {
        let index = self.selected_message_index()?;

        let (collapsed, tool_name) = {
            let tool_output = self
                .messages
                .get_mut(index)
                .and_then(|msg| msg.tool_output.as_mut())?;
            tool_output.collapsed = !tool_output.collapsed;
            (tool_output.collapsed, tool_output.tool_name.clone())
        };

        self.invalidate_render_cache();
        let action = if collapsed { "Collapsed" } else { "Expanded" };
        Some(format!("{} {} output", action, tool_name))
    }

    pub fn collapse_all_tool_outputs(&mut self) -> String {
        let updated = self.set_all_tool_outputs_collapsed(true);
        if updated == 0 {
            "No expanded tool output to collapse".to_string()
        } else {
            format!("Collapsed {} tool output item(s)", updated)
        }
    }

    pub fn expand_all_tool_outputs(&mut self) -> String {
        let updated = self.set_all_tool_outputs_collapsed(false);
        if updated == 0 {
            "No collapsed tool output to expand".to_string()
        } else {
            format!("Expanded {} tool output item(s)", updated)
        }
    }

    pub fn set_all_tool_outputs_collapsed(&mut self, collapsed: bool) -> usize {
        let mut updated = 0;
        for message in &mut self.messages {
            if let Some(tool_output) = message.tool_output.as_mut() {
                if tool_output.collapsed != collapsed {
                    tool_output.collapsed = collapsed;
                    updated += 1;
                }
            }
        }
        if updated > 0 {
            self.invalidate_render_cache();
        }
        updated
    }

    pub fn selected_message_index(&self) -> Option<usize> {
        if self.messages.is_empty() {
            return None;
        }

        let start = self.scroll_offset.min(self.total_lines);
        let end = (start + self.viewport_height as usize).min(self.total_lines);

        if end > start {
            for idx in (start..end).rev() {
                if let Some(Some(message_index)) = self.line_message_index.get(idx) {
                    return Some(*message_index);
                }
            }
        }

        Some(self.messages.len() - 1)
    }

    pub fn append_streaming_delta(&mut self, delta: &str, max_size: usize) {
        if let Some(ref mut buffer) = self.streaming_buffer {
            if buffer.len() + delta.len() <= max_size {
                buffer.push_str(delta);
            }
        } else {
            self.streaming_buffer = Some(delta.to_string());
        }
        self.rerender_dynamic_entries();
        self.auto_scroll_to_bottom();
    }

    pub fn append_thinking_delta(&mut self, thinking: &str, max_size: usize) {
        if let Some(ref mut buffer) = self.thinking_buffer {
            if buffer.len() + thinking.len() <= max_size {
                buffer.push_str(thinking);
            }
        } else {
            self.thinking_buffer = Some(thinking.to_string());
        }
        self.rerender_dynamic_entries();
        self.auto_scroll_to_bottom();
    }

    pub fn set_streaming_buffer(&mut self, content: String) {
        self.streaming_buffer = Some(content);
        self.rerender_dynamic_entries();
        self.auto_scroll_to_bottom();
    }

    pub fn clear_streaming_buffer(&mut self) -> Option<String> {
        let taken = self.streaming_buffer.take();
        self.rerender_dynamic_entries();
        taken
    }

    pub fn take_thinking_buffer(&mut self) -> Option<String> {
        let taken = self.thinking_buffer.take();
        self.rerender_dynamic_entries();
        taken
    }

    pub fn invalidate_render_cache(&mut self) {
        self.cache_dirty = true;
    }

    fn rebuild_render_cache_if_needed(&mut self, width: u16) {
        let width_changed = width != self.last_render_width;
        if !self.cache_dirty && !width_changed {
            return;
        }

        let old_total_lines = self.total_lines;
        let old_offset = self.scroll_offset;

        self.rendered_lines.clear();
        self.line_message_index.clear();

        let wrap_width = width as usize;
        for (message_index, msg) in self.messages.iter().enumerate() {
            let lines = self.render_message_lines(msg, wrap_width);
            self.line_message_index
                .extend(std::iter::repeat_n(Some(message_index), lines.len()));
            self.rendered_lines.push(lines);
        }

        self.append_dynamic_entries(wrap_width);

        self.cached_message_count = self.messages.len();
        self.last_render_width = width;
        self.cache_dirty = false;
        self.total_lines = self.rendered_lines.iter().map(Vec::len).sum();

        if self.auto_follow {
            self.scroll_to_bottom_internal();
        } else if width_changed && old_total_lines > 1 && self.total_lines > 1 {
            self.scroll_offset =
                old_offset.saturating_mul(self.total_lines - 1) / (old_total_lines - 1);
            self.clamp_scroll_offset();
        }

        self.sync_follow_with_position();
    }

    fn rerender_dynamic_entries(&mut self) {
        if self.cache_dirty || self.last_render_width == 0 {
            return;
        }

        self.rendered_lines.truncate(self.messages.len());
        self.line_message_index.clear();
        for (message_index, lines) in self.rendered_lines.iter().enumerate() {
            self.line_message_index
                .extend(std::iter::repeat_n(Some(message_index), lines.len()));
        }

        self.append_dynamic_entries(self.last_render_width as usize);
        self.total_lines = self.rendered_lines.iter().map(Vec::len).sum();
        self.clamp_scroll_offset();
        self.sync_follow_with_position();
    }

    fn append_dynamic_entries(&mut self, wrap_width: usize) {
        if let Some(ref buffer) = self.thinking_buffer {
            if !buffer.is_empty() {
                let style = Style::default()
                    .fg(self.theme.borders)
                    .add_modifier(Modifier::ITALIC);
                let lines = wrap_message("> Thinking: ", buffer, wrap_width, style);
                self.line_message_index
                    .extend(std::iter::repeat_n(None, lines.len()));
                self.rendered_lines.push(lines);
            }
        }

        if let Some(ref buffer) = self.streaming_buffer {
            if !buffer.is_empty() {
                let style = self.message_style("assistant");
                let mut lines = wrap_message("assistant: ", buffer, wrap_width, style);
                if let Some(last) = lines.last_mut() {
                    last.spans.push(Span::styled(
                        "▌",
                        Style::default()
                            .fg(self.theme.warning)
                            .add_modifier(Modifier::SLOW_BLINK),
                    ));
                }
                self.line_message_index
                    .extend(std::iter::repeat_n(None, lines.len()));
                self.rendered_lines.push(lines);
            }
        }
    }

    fn render_message_lines(&self, msg: &ChatMessage, inner_width: usize) -> Vec<Line<'static>> {
        let (prefix, style) = if msg.role == "thinking" {
            (
                "thinking: ",
                Style::default()
                    .fg(self.theme.borders)
                    .add_modifier(Modifier::ITALIC),
            )
        } else {
            let s = self.message_style(msg.role.as_str());
            ("", s)
        };

        let role_prefix = if prefix.is_empty() {
            format!("{}: ", msg.role)
        } else {
            prefix.to_string()
        };

        if let Some(tool_output) = &msg.tool_output {
            let body = if tool_output.collapsed {
                format!(
                    "▶ {}: {} [Tab/Enter to expand]",
                    tool_output.tool_name, tool_output.summary
                )
            } else {
                format!("▼ {}:\n{}", tool_output.tool_name, msg.content)
            };

            return wrap_message(&role_prefix, &body, inner_width, style);
        }

        wrap_message(&role_prefix, &msg.content, inner_width, style)
    }

    fn scroll_to_bottom_internal(&mut self) {
        self.scroll_offset = self.max_scroll_offset();
    }

    fn max_scroll_offset(&self) -> usize {
        self.total_lines
            .saturating_sub(self.viewport_height as usize)
    }

    fn clamp_scroll_offset(&mut self) {
        self.scroll_offset = self.scroll_offset.min(self.max_scroll_offset());
    }

    fn sync_follow_with_position(&mut self) {
        if self.is_at_bottom() {
            self.user_scrolled = false;
            self.auto_follow = true;
        }
    }

    fn is_at_bottom(&self) -> bool {
        self.scroll_offset >= self.max_scroll_offset()
    }
}

fn wrap_message(prefix: &str, content: &str, max_width: usize, style: Style) -> Vec<Line<'static>> {
    let prefix_len = prefix.chars().count();
    let content_width = max_width.saturating_sub(prefix_len);

    if content_width == 0 {
        return vec![Line::from(Span::styled(prefix.to_string(), style))];
    }

    let mut lines = Vec::new();
    let mut first = true;

    for paragraph in content.split('\n') {
        let chars: Vec<char> = paragraph.chars().collect();
        if chars.is_empty() {
            let line_prefix = if first { prefix } else { "" };
            lines.push(Line::from(Span::styled(line_prefix.to_string(), style)));
            first = false;
            continue;
        }

        let mut i = 0;
        while i < chars.len() {
            let width = if first { content_width } else { max_width };
            let end = (i + width).min(chars.len());
            let chunk: String = chars[i..end].iter().collect();

            let line = if first {
                Line::from(vec![
                    Span::styled(prefix.to_string(), style.add_modifier(Modifier::BOLD)),
                    Span::styled(chunk, style),
                ])
            } else {
                Line::from(Span::styled(chunk, style))
            };

            lines.push(line);
            first = false;
            i = end;
        }
    }

    if lines.is_empty() {
        lines.push(Line::from(Span::styled(prefix.to_string(), style)));
    }

    lines
}

fn truncate_chars(input: &str, max_chars: usize) -> String {
    let mut chars = input.chars();
    let mut output: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        output.push_str("...");
    }
    output
}

fn summarize_tool_output(output: &str) -> String {
    let total_lines = output.lines().count();
    if total_lines == 0 {
        return "no output".to_string();
    }

    let first_non_empty = output
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or("(empty lines)");

    let preview = truncate_chars(first_non_empty, 80);
    if total_lines == 1 {
        preview
    } else {
        format!(
            "{} (+{} more lines)",
            preview,
            total_lines.saturating_sub(1)
        )
    }
}
