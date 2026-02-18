use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use std::collections::HashMap;
use uira_core::AgentState;

use crate::widgets::tool_renderers::{ToolRenderContext, ToolState};
use crate::widgets::{
    diff::WrapMode,
    markdown::{render_markdown_with_options, MarkdownRenderOptions},
    tool_renderers, ChatMessage,
};
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
    agent_color_map: HashMap<String, usize>,
    line_message_index: Vec<Option<usize>>,
    cached_static_line_message_index: Vec<Option<usize>>,
    cached_static_entry_count: usize,
    cached_message_count: usize,
    cache_dirty: bool,
    show_tool_details: bool,
    pub agent_state: AgentState,
    diff_wrap_mode: WrapMode,
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
            agent_color_map: HashMap::new(),
            line_message_index: Vec::new(),
            cached_static_line_message_index: Vec::new(),
            cached_static_entry_count: 0,
            cached_message_count: 0,
            cache_dirty: true,
            show_tool_details: true,
            agent_state: AgentState::Idle,
            diff_wrap_mode: WrapMode::Word,
        }
    }

    pub fn set_diff_wrap_mode(&mut self, wrap_mode: WrapMode) {
        self.diff_wrap_mode = wrap_mode;
        self.invalidate_render_cache();
    }

    pub fn toggle_tool_details(&mut self) -> String {
        self.show_tool_details = !self.show_tool_details;
        self.invalidate_render_cache();
        if self.show_tool_details {
            "Tool details shown".to_string()
        } else {
            "Tool details hidden".to_string()
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
        let inner_width = area.width;
        self.viewport_height = area.height;
        if self.cached_message_count != self.messages.len() {
            self.cache_dirty = true;
        }

        self.rebuild_render_cache_if_needed(inner_width);
        self.clamp_scroll_offset();
        // Sync follow state after clamping - if viewport resize moved us to bottom,
        // re-enable auto-follow so new messages scroll correctly
        self.sync_follow_with_position();

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

        self.append_runtime_status_indicators(&mut visible_lines);

        if visible_lines.len() > self.viewport_height as usize {
            let keep = self.viewport_height as usize;
            visible_lines = visible_lines[visible_lines.len().saturating_sub(keep)..].to_vec();
        }

        if visible_lines.is_empty() {
            visible_lines.push(Line::from(""));
        }

        let paragraph = Paragraph::new(Text::from(visible_lines))
            .style(Style::default().bg(self.theme.bg))
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

    pub fn push_message(&mut self, role: &str, content: String, agent_name: Option<String>) {
        let msg = ChatMessage::new(role, content).with_agent(agent_name);
        self.messages.push(msg);
        self.invalidate_render_cache();
        self.auto_scroll_to_bottom();
    }

    pub fn push_tool_message(
        &mut self,
        role: &str,
        tool_name: String,
        content: String,
        is_collapsed: bool,
        agent_name: Option<String>,
    ) {
        let summary = summarize_tool_output(&content);
        let msg = ChatMessage::tool(role, content, tool_name, summary, is_collapsed)
            .with_agent(agent_name);
        self.messages.push(msg);
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

    pub fn get_message_index_at_line(&self, line: usize) -> Option<usize> {
        self.line_message_index.get(line).and_then(|&idx| idx)
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
        self.cached_static_line_message_index.clear();

        let wrap_width = width as usize;
        for message_index in 0..self.messages.len() {
            let msg = self.messages[message_index].clone();
            let lines = self.render_message_lines(msg, wrap_width);
            self.line_message_index
                .extend(std::iter::repeat_n(Some(message_index), lines.len()));
            self.rendered_lines.push(lines);

            if message_index + 1 < self.messages.len() {
                self.line_message_index.push(None);
                self.rendered_lines.push(vec![Line::from("")]);
            }
        }

        self.cached_static_entry_count = self.rendered_lines.len();
        self.cached_static_line_message_index = self.line_message_index.clone();

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

        self.rendered_lines.truncate(self.cached_static_entry_count);
        self.line_message_index = self.cached_static_line_message_index.clone();

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

    fn append_runtime_status_indicators(&self, visible_lines: &mut Vec<Line<'static>>) {
        if matches!(
            self.agent_state,
            AgentState::Thinking | AgentState::ExecutingTool
        ) && self.has_pending_turn_after_last_assistant()
        {
            visible_lines.push(Line::from(vec![
                Span::styled("┃ ", Style::default().fg(self.theme.accent)),
                Span::styled(
                    " QUEUED ",
                    Style::default()
                        .bg(self.theme.accent)
                        .fg(self.theme.bg)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        }

        if self.thinking_buffer.is_some() {
            visible_lines.push(Line::from(vec![
                Span::styled("┃ ", Style::default().fg(self.theme.borders)),
                Span::styled(
                    "thinking...",
                    Style::default()
                        .fg(self.theme.borders)
                        .add_modifier(Modifier::ITALIC),
                ),
            ]));
        }
    }

    fn has_pending_turn_after_last_assistant(&self) -> bool {
        if self.messages.is_empty() {
            return false;
        }

        match self
            .messages
            .iter()
            .rposition(|message| message.role == "assistant")
        {
            Some(last_assistant_index) => last_assistant_index + 1 < self.messages.len(),
            None => true,
        }
    }

    fn border_color_for_message(&mut self, msg: &ChatMessage) -> Color {
        if let Some(ref agent_name) = msg.agent_name {
            let idx = self.agent_color_index(agent_name);
            if !self.theme.agent_colors.is_empty() {
                return self.theme.agent_colors[idx % self.theme.agent_colors.len()];
            }
        }

        match msg.role.as_str() {
            "user" => self.theme.accent,
            "assistant" => self.theme.fg,
            "tool" => self.theme.text_muted,
            "system" => self.theme.warning,
            "error" => self.theme.error,
            "thinking" => self.theme.borders,
            _ => self.theme.fg,
        }
    }

    fn agent_color_index(&mut self, agent_name: &str) -> usize {
        let next_idx = self.agent_color_map.len();
        *self
            .agent_color_map
            .entry(agent_name.to_string())
            .or_insert(next_idx)
    }

    fn render_message_lines(&mut self, msg: ChatMessage, inner_width: usize) -> Vec<Line<'static>> {
        let content_width = inner_width.saturating_sub(2);
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
            if tool_output.collapsed || !self.show_tool_details {
                let body = format!(
                    "▶ {}: {} [Tab/Enter to expand]",
                    tool_output.tool_name, tool_output.summary
                );
                let lines = wrap_message(&role_prefix, &body, content_width, style);
                return self.add_message_border(lines, &msg);
            }

            let header = format!("▼ {}:", tool_output.tool_name);
            let mut lines = vec![Line::from(vec![
                Span::styled(role_prefix, style.add_modifier(Modifier::BOLD)),
                Span::styled(header, style),
            ])];
            lines.extend(tool_renderers::render_tool_output(
                &tool_output.tool_name,
                &msg.content,
                content_width,
                &self.theme,
                ToolRenderContext {
                    state: tool_state_from_message(&msg),
                    expanded: !tool_output.collapsed,
                    wide: inner_width > 120,
                    wrap_mode: self.diff_wrap_mode,
                },
            ));
            return self.add_message_border(lines, &msg);
        }

        if msg.role == "assistant" {
            let segments = split_thinking_blocks(&msg.content);
            let has_thinking_blocks = segments.iter().any(|(is_thinking, _)| *is_thinking);

            if has_thinking_blocks {
                let thinking_style = Style::default()
                    .fg(self.theme.borders)
                    .add_modifier(Modifier::ITALIC);
                let mut lines = Vec::new();
                let mut has_rendered_assistant_content = false;

                for (is_thinking, segment) in segments {
                    if segment.is_empty() {
                        continue;
                    }

                    if is_thinking {
                        lines.extend(wrap_message(
                            "thinking: ",
                            &segment,
                            content_width,
                            thinking_style,
                        ));
                    } else {
                        let segment_prefix = if has_rendered_assistant_content {
                            ""
                        } else {
                            role_prefix.as_str()
                        };
                        lines.extend(render_message_markdown(
                            segment_prefix,
                            &segment,
                            content_width,
                            style,
                            &self.theme,
                        ));
                        has_rendered_assistant_content = true;
                    }
                }

                if lines.is_empty() {
                    lines = render_message_markdown(
                        &role_prefix,
                        &msg.content,
                        content_width,
                        style,
                        &self.theme,
                    );
                }

                let border_color = self.border_color_for_message(&msg);
                if let Some(footer) = self.render_turn_footer(&msg, border_color) {
                    lines.push(footer);
                }

                return self.add_message_border(lines, &msg);
            }
        }

        if msg.role == "system" {
            if is_compaction_marker(&msg.content) {
                let mut lines = render_compaction_marker(content_width, &self.theme);
                lines.extend(render_attachment_badges(&msg.content, &self.theme));
                return self.add_message_border(lines, &msg);
            }
            if let Some(revert_lines) =
                render_revert_banner(&msg.content, content_width, &self.theme)
            {
                return self.add_message_border(revert_lines, &msg);
            }
        }

        if msg.role == "assistant" || msg.role == "system" {
            let mut lines = render_message_markdown(
                &role_prefix,
                &msg.content,
                content_width,
                style,
                &self.theme,
            );
            if msg.role == "assistant" {
                let border_color = self.border_color_for_message(&msg);
                if let Some(footer) = self.render_turn_footer(&msg, border_color) {
                    lines.push(footer);
                }
            }
            lines.extend(render_attachment_badges(&msg.content, &self.theme));
            return self.add_message_border(lines, &msg);
        }

        let mut lines = wrap_message(&role_prefix, &msg.content, content_width, style);
        lines.extend(render_attachment_badges(&msg.content, &self.theme));
        self.add_message_border(lines, &msg)
    }

    fn render_turn_footer(&self, msg: &ChatMessage, border_color: Color) -> Option<Line<'static>> {
        let agent_name = msg.agent_name.as_ref()?;
        let mut spans = vec![
            Span::styled(
                "▣ ",
                Style::default()
                    .fg(border_color)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("Mode", Style::default().fg(border_color)),
        ];

        spans.push(Span::styled(
            " · ",
            Style::default().fg(self.theme.text_muted),
        ));
        spans.push(Span::styled(
            agent_name.clone(),
            Style::default().fg(border_color),
        ));

        if let Some(ref model_id) = msg.session_id {
            spans.push(Span::styled(
                " · ",
                Style::default().fg(self.theme.text_muted),
            ));
            spans.push(Span::styled(
                model_id.clone(),
                Style::default().fg(self.theme.text_muted),
            ));
        }

        if let Some(timestamp) = msg.timestamp {
            spans.push(Span::styled(
                " · ",
                Style::default().fg(self.theme.text_muted),
            ));
            spans.push(Span::styled(
                format!("{}ms", timestamp),
                Style::default().fg(self.theme.text_muted),
            ));
        }

        if msg.content.to_lowercase().contains("interrupt") {
            spans.push(Span::styled(
                " · ",
                Style::default().fg(self.theme.text_muted),
            ));
            spans.push(Span::styled(
                "interrupted",
                Style::default()
                    .fg(self.theme.warning)
                    .add_modifier(Modifier::ITALIC),
            ));
        }

        Some(Line::from(spans))
    }

    fn add_message_border(
        &mut self,
        lines: Vec<Line<'static>>,
        msg: &ChatMessage,
    ) -> Vec<Line<'static>> {
        let border_style = Style::default().fg(self.border_color_for_message(msg));
        lines
            .into_iter()
            .map(|line| {
                let mut spans = Vec::with_capacity(line.spans.len() + 1);
                spans.push(Span::styled("┃ ", border_style));
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect()
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

    /// Compute the line offset where each message starts.
    /// Returns a vector where index i contains the line offset of message i.
    pub(crate) fn message_line_offsets(&self) -> Vec<usize> {
        let mut offsets = Vec::new();
        let mut current_offset = 0;

        for lines in &self.rendered_lines {
            offsets.push(current_offset);
            current_offset += lines.len();
        }

        offsets
    }

    /// Scroll to the previous user message before the current scroll position.
    /// Sets user_scrolled=true and auto_follow=false.
    pub fn scroll_to_prev_user_message(&mut self) {
        let offsets = self.message_line_offsets();
        let current_line = self.scroll_offset;

        // Find the message index that contains the current scroll position
        let mut current_msg_idx = None;
        for (idx, &offset) in offsets.iter().enumerate() {
            if offset <= current_line {
                current_msg_idx = Some(idx);
            } else {
                break;
            }
        }

        // Search backwards from the current message (or before it) for a user message
        if let Some(start_idx) = current_msg_idx {
            for idx in (0..start_idx).rev() {
                if self.messages.get(idx).map(|m| m.role.as_str()) == Some("user") {
                    if let Some(&offset) = offsets.get(idx) {
                        self.scroll_offset = offset;
                        self.user_scrolled = true;
                        self.auto_follow = false;
                    }
                    return;
                }
            }
        }
    }

    /// Scroll to the next user message after the current scroll position.
    /// Sets user_scrolled=true and auto_follow=false.
    pub fn scroll_to_next_user_message(&mut self) {
        let offsets = self.message_line_offsets();
        let current_line = self.scroll_offset;

        // Find the message index that contains the current scroll position
        let mut current_msg_idx = None;
        for (idx, &offset) in offsets.iter().enumerate() {
            if offset <= current_line {
                current_msg_idx = Some(idx);
            } else {
                break;
            }
        }

        // Search forwards from the next message for a user message
        let start_idx = current_msg_idx.map(|idx| idx + 1).unwrap_or(0);
        for idx in start_idx..self.messages.len() {
            if self.messages.get(idx).map(|m| m.role.as_str()) == Some("user") {
                if let Some(&offset) = offsets.get(idx) {
                    self.scroll_offset = offset;
                    self.user_scrolled = true;
                    self.auto_follow = false;
                }
                return;
            }
        }
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

fn render_message_markdown(
    prefix: &str,
    content: &str,
    max_width: usize,
    style: Style,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let prefix_style = style.add_modifier(Modifier::BOLD);
    let prefix_len = prefix.chars().count();
    let markdown_width = max_width.saturating_sub(prefix_len).max(1);
    let mut markdown_lines = render_markdown_with_options(
        content,
        markdown_width,
        theme,
        MarkdownRenderOptions {
            conceal: true,
            streaming_friendly: true,
        },
    );

    if markdown_lines.is_empty() {
        return vec![Line::from(Span::styled(prefix.to_string(), prefix_style))];
    }

    if let Some(first_line) = markdown_lines.first_mut() {
        first_line
            .spans
            .insert(0, Span::styled(prefix.to_string(), prefix_style));
    }

    markdown_lines
}

fn split_thinking_blocks(content: &str) -> Vec<(bool, String)> {
    if content.contains("&lt;thinking&gt;") || content.contains("&lt;/thinking&gt;") {
        return split_thinking_blocks_with_markers(
            content,
            "&lt;thinking&gt;",
            "&lt;/thinking&gt;",
        );
    }

    split_thinking_blocks_with_markers(content, "<thinking>", "</thinking>")
}

fn split_thinking_blocks_with_markers(
    content: &str,
    open_tag: &str,
    close_tag: &str,
) -> Vec<(bool, String)> {
    let mut segments = Vec::new();
    let mut remaining = content;

    while let Some(start) = remaining.find(open_tag) {
        if start > 0 {
            segments.push((false, remaining[..start].to_string()));
        }

        remaining = &remaining[start + open_tag.len()..];
        if let Some(end) = remaining.find(close_tag) {
            segments.push((true, remaining[..end].to_string()));
            remaining = &remaining[end + close_tag.len()..];
        } else {
            segments.push((true, remaining.to_string()));
            remaining = "";
            break;
        }
    }

    if !remaining.is_empty() {
        segments.push((false, remaining.to_string()));
    }

    if segments.is_empty() {
        segments.push((false, content.to_string()));
    }

    segments
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

fn tool_state_from_message(msg: &ChatMessage) -> ToolState {
    if msg.role == "error" {
        let denied = msg.content.contains("rejected permission")
            || msg.content.contains("specified a rule")
            || msg.content.contains("user dismissed");
        return if denied {
            ToolState::Denied
        } else {
            ToolState::Error
        };
    }

    if msg.content.trim().is_empty() {
        ToolState::Pending
    } else {
        ToolState::Completed
    }
}

fn is_compaction_marker(content: &str) -> bool {
    let lower = content.to_lowercase();
    lower.contains("compaction") || lower.contains("compacted")
}

fn render_compaction_marker(width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let rule_width = width.saturating_sub(14).max(8);
    let left = "─".repeat(rule_width / 2);
    let right = "─".repeat(rule_width.saturating_sub(rule_width / 2));
    vec![Line::from(vec![
        Span::styled(left, Style::default().fg(theme.border_subtle)),
        Span::styled(
            " Compaction ",
            Style::default()
                .fg(theme.text_muted)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(right, Style::default().fg(theme.border_subtle)),
    ])]
}

fn render_revert_banner(content: &str, _width: usize, theme: &Theme) -> Option<Vec<Line<'static>>> {
    let lower = content.to_lowercase();
    if !lower.contains("revert") && !lower.contains("reverted") {
        return None;
    }

    let mut lines = vec![Line::from(vec![
        Span::styled("  ", Style::default().bg(theme.bg_panel)),
        Span::styled(
            content.to_string(),
            Style::default()
                .fg(theme.warning)
                .bg(theme.bg_panel)
                .add_modifier(Modifier::BOLD),
        ),
    ])];

    let files: Vec<&str> = content
        .lines()
        .filter(|line| line.trim_start().starts_with('+') || line.trim_start().starts_with('-'))
        .collect();
    for file in files {
        let style = if file.trim_start().starts_with('+') {
            Style::default().fg(theme.diff_added)
        } else {
            Style::default().fg(theme.diff_removed)
        };
        lines.push(Line::from(Span::styled(format!("  {file}"), style)));
    }

    Some(lines)
}

fn render_attachment_badges(content: &str, theme: &Theme) -> Vec<Line<'static>> {
    let mut badges = Vec::new();
    for token in content.split_whitespace() {
        let token_lower = token.to_lowercase();
        let (label, color) = if token_lower.ends_with(".png")
            || token_lower.ends_with(".jpg")
            || token_lower.ends_with(".jpeg")
            || token_lower.ends_with(".webp")
        {
            (" img ", theme.accent)
        } else if token_lower.ends_with(".pdf") {
            (" pdf ", theme.warning)
        } else if token_lower.ends_with('/') {
            (" dir ", theme.borders)
        } else if token_lower.ends_with(".txt")
            || token_lower.ends_with(".md")
            || token_lower.ends_with(".rs")
            || token_lower.ends_with(".json")
            || token_lower.ends_with(".toml")
        {
            (" txt ", theme.success)
        } else {
            continue;
        };
        badges.push(Line::from(vec![Span::styled(
            label,
            Style::default()
                .bg(color)
                .fg(theme.bg)
                .add_modifier(Modifier::BOLD),
        )]));
    }
    badges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_message_lines_adds_border_with_role_color() {
        let mut view = ChatView::new(Theme::default());
        let message = ChatMessage::new("user", "hello").with_agent(Some("executor".to_string()));

        let lines = view.render_message_lines(message, 20);

        assert!(!lines.is_empty());
        let first = &lines[0];
        assert_eq!(first.spans[0].content, "┃ ");
        assert_eq!(first.spans[0].style.fg, Some(view.theme.agent_colors[0]));
    }

    #[test]
    fn rebuild_render_cache_adds_separator_line_between_messages() {
        let mut view = ChatView::new(Theme::default());
        view.push_message("user", "one".to_string(), None);
        view.push_message("assistant", "two".to_string(), None);

        view.rebuild_render_cache_if_needed(40);

        assert_eq!(view.rendered_lines.len(), 3);
        assert_eq!(view.line_message_index.len(), 3);
        assert_eq!(view.line_message_index[0], Some(0));
        assert_eq!(view.line_message_index[1], None);
        assert_eq!(view.line_message_index[2], Some(1));
        assert_eq!(view.total_lines, 3);
    }

    #[test]
    fn test_agent_color_cycling() {
        let mut view = ChatView::new(Theme::default());
        let names = [
            "explore",
            "architect",
            "executor",
            "writer",
            "critic",
            "planner",
            "qa",
        ];

        let first = view.agent_color_index(names[0]);
        let repeated = view.agent_color_index(names[0]);
        assert_eq!(first, repeated);

        for (i, name) in names.iter().enumerate() {
            assert_eq!(view.agent_color_index(name), i);
        }

        let idx = view.agent_color_index(names[6]);
        let color = view.theme.agent_colors[idx % view.theme.agent_colors.len()];
        assert_eq!(color, view.theme.agent_colors[0]);
    }

    #[test]
    fn test_turn_footer_rendering() {
        let mut view = ChatView::new(Theme::default());
        let with_agent =
            ChatMessage::new("assistant", "hello").with_agent(Some("executor".to_string()));
        let without_agent = ChatMessage::new("assistant", "hello");

        let lines_with_footer = view.render_message_lines(with_agent, 40);
        let footer = lines_with_footer
            .iter()
            .find(|line| line.spans.iter().any(|span| span.content.contains("▣ ")));
        assert!(footer.is_some());

        let lines_without_footer = view.render_message_lines(without_agent, 40);
        let footer_missing = lines_without_footer
            .iter()
            .any(|line| line.spans.iter().any(|span| span.content.contains("▣ ")));
        assert!(!footer_missing);
    }

    #[test]
    fn test_agent_name_border_color_integration() {
        let mut view = ChatView::new(Theme::default());

        let alpha_first =
            ChatMessage::new("assistant", "first").with_agent(Some("alpha".to_string()));
        let beta = ChatMessage::new("assistant", "second").with_agent(Some("beta".to_string()));
        let alpha_again =
            ChatMessage::new("assistant", "third").with_agent(Some("alpha".to_string()));

        let alpha_first_color = view.render_message_lines(alpha_first, 40)[0].spans[0]
            .style
            .fg;
        let beta_color = view.render_message_lines(beta, 40)[0].spans[0].style.fg;
        let alpha_again_color = view.render_message_lines(alpha_again, 40)[0].spans[0]
            .style
            .fg;

        assert_eq!(alpha_first_color, alpha_again_color);
        assert_ne!(alpha_first_color, beta_color);
    }

    #[test]
    fn test_separator_lines_between_messages() {
        let mut view = ChatView::new(Theme::default());
        view.push_message("user", "one".to_string(), None);
        view.push_message("assistant", "two".to_string(), None);
        view.push_message("user", "three".to_string(), None);

        view.rebuild_render_cache_if_needed(40);

        assert_eq!(view.rendered_lines.len(), 5);
        assert_eq!(view.line_message_index[1], None);
        assert_eq!(view.line_message_index[3], None);
        assert!(view.rendered_lines[1][0].spans.is_empty());
        assert!(view.rendered_lines[3][0].spans.is_empty());
    }

    #[test]
    fn queued_and_thinking_indicators_render_for_active_turn() {
        let mut view = ChatView::new(Theme::default());
        view.messages.push(ChatMessage::new("assistant", "done"));
        view.messages.push(ChatMessage::new("user", "next"));
        view.agent_state = AgentState::Thinking;
        view.thinking_buffer = Some("reasoning".to_string());

        let mut lines = Vec::new();
        view.append_runtime_status_indicators(&mut lines);

        assert_eq!(lines.len(), 2);
        let queued = lines[0]
            .spans
            .iter()
            .any(|span| span.content.contains("QUEUED"));
        let thinking = lines[1]
            .spans
            .iter()
            .any(|span| span.content.contains("thinking..."));
        assert!(queued);
        assert!(thinking);
    }

    #[test]
    fn split_thinking_blocks_handles_mixed_content() {
        let segments = split_thinking_blocks("before<thinking>inner</thinking>after");

        assert_eq!(
            segments,
            vec![
                (false, "before".to_string()),
                (true, "inner".to_string()),
                (false, "after".to_string())
            ]
        );
    }

    #[test]
    fn split_thinking_blocks_handles_multiple_blocks() {
        let segments = split_thinking_blocks("<thinking>a</thinking>x<thinking>b</thinking>");

        assert_eq!(
            segments,
            vec![
                (true, "a".to_string()),
                (false, "x".to_string()),
                (true, "b".to_string())
            ]
        );
    }

    #[test]
    fn split_thinking_blocks_handles_unclosed_tag() {
        let segments = split_thinking_blocks("hello <thinking>draft");

        assert_eq!(
            segments,
            vec![(false, "hello ".to_string()), (true, "draft".to_string())]
        );
    }

    #[test]
    fn split_thinking_blocks_handles_escaped_tags() {
        let segments = split_thinking_blocks("start&lt;thinking&gt;idea&lt;/thinking&gt;end");

        assert_eq!(
            segments,
            vec![
                (false, "start".to_string()),
                (true, "idea".to_string()),
                (false, "end".to_string())
            ]
        );
    }
}
