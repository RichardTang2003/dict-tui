use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::{backend::CrosstermBackend, Frame, Terminal};

use crate::app::Config;

const FIELD_COUNT: usize = 7;
const FIELD_WEB_SEARCH: usize = 4;
const FIELD_JAVASCRIPT: usize = 5;
const FIELD_SYSTEM_PROMPT: usize = 6;

#[derive(Debug)]
struct EditorState {
    config: Config,
    selected: usize,
    editing: bool,
    edit_buffer: String,
    cursor: usize,
    prompt_scroll: usize,
    status: String,
}

impl EditorState {
    fn new(config: Config) -> Self {
        Self {
            config,
            selected: 0,
            editing: false,
            edit_buffer: String::new(),
            cursor: 0,
            prompt_scroll: 0,
            status: "F4 打开配置；Esc 返回搜索".to_string(),
        }
    }

    fn selected_label(&self) -> &'static str {
        field_label(self.selected)
    }

    fn selected_value(&self) -> String {
        field_value(&self.config, self.selected)
    }

    fn begin_edit(&mut self) {
        self.editing = true;
        self.edit_buffer = self.selected_value();
        self.cursor = self.edit_buffer.len();
        self.status = format!(
            "正在编辑 {}。Ctrl+S 保存，Esc 取消。",
            self.selected_label()
        );
    }

    fn commit_edit(&mut self) {
        set_field_value(&mut self.config, self.selected, self.edit_buffer.clone());
        self.editing = false;
        self.status = format!("已更新 {}，按 Ctrl+S 写入配置文件。", self.selected_label());
    }

    fn cancel_edit(&mut self) {
        self.editing = false;
        self.edit_buffer.clear();
        self.cursor = 0;
        self.status = "已取消编辑。".to_string();
    }

    fn commit_and_save(&mut self) {
        if self.editing {
            set_field_value(&mut self.config, self.selected, self.edit_buffer.clone());
        }

        match self.config.save() {
            Ok(()) => {
                self.editing = false;
                self.status = "配置已保存。".to_string();
            }
            Err(err) => {
                self.status = format!("保存失败: {err}");
            }
        }
    }

    fn select_prev(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    fn select_next(&mut self) {
        if self.selected + 1 < FIELD_COUNT {
            self.selected += 1;
        }
    }
}

pub fn run_config_editor(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    initial_config: Config,
) -> Result<Config> {
    let mut state = EditorState::new(initial_config);

    loop {
        terminal.draw(|frame| draw_config_ui(frame, &state))?;

        if !event::poll(Duration::from_millis(100))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('s') {
            state.commit_and_save();
            continue;
        }

        if state.editing {
            handle_edit_key(&mut state, key.code);
        } else if handle_navigation_key(&mut state, key.code) {
            return Ok(state.config);
        }
    }
}

fn handle_navigation_key(state: &mut EditorState, code: KeyCode) -> bool {
    match code {
        KeyCode::Esc => return true,
        KeyCode::Up => state.select_prev(),
        KeyCode::Down => state.select_next(),
        KeyCode::PageUp if state.selected == FIELD_SYSTEM_PROMPT => {
            state.prompt_scroll = state.prompt_scroll.saturating_sub(6);
        }
        KeyCode::PageDown if state.selected == FIELD_SYSTEM_PROMPT => {
            state.prompt_scroll = state.prompt_scroll.saturating_add(6);
        }
        KeyCode::Home => state.selected = 0,
        KeyCode::End => state.selected = FIELD_COUNT - 1,
        KeyCode::Enter => state.begin_edit(),
        _ => {}
    }

    false
}

fn handle_edit_key(state: &mut EditorState, code: KeyCode) {
    match code {
        KeyCode::Esc => state.cancel_edit(),
        KeyCode::F(2) => state.commit_edit(),
        KeyCode::Tab => {
            state.commit_edit();
            state.select_next();
        }
        KeyCode::BackTab => {
            state.commit_edit();
            state.select_prev();
        }
        KeyCode::Enter if state.selected == FIELD_SYSTEM_PROMPT => {
            insert_char(&mut state.edit_buffer, &mut state.cursor, '\n');
        }
        KeyCode::Enter => state.commit_edit(),
        KeyCode::Backspace => backspace(&mut state.edit_buffer, &mut state.cursor),
        KeyCode::Delete => delete_char(&mut state.edit_buffer, state.cursor),
        KeyCode::Left => move_left(&state.edit_buffer, &mut state.cursor),
        KeyCode::Right => move_right(&state.edit_buffer, &mut state.cursor),
        KeyCode::Up if state.selected == FIELD_SYSTEM_PROMPT => {
            move_cursor_vertically(&state.edit_buffer, &mut state.cursor, -1);
        }
        KeyCode::Down if state.selected == FIELD_SYSTEM_PROMPT => {
            move_cursor_vertically(&state.edit_buffer, &mut state.cursor, 1);
        }
        KeyCode::Home => move_line_start(&state.edit_buffer, &mut state.cursor),
        KeyCode::End => move_line_end(&state.edit_buffer, &mut state.cursor),
        KeyCode::Char(ch) if !ch.is_control() => {
            insert_char(&mut state.edit_buffer, &mut state.cursor, ch);
        }
        _ => {}
    }
}

fn draw_config_ui(frame: &mut Frame, state: &EditorState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(10),
            Constraint::Length(1),
        ])
        .split(frame.area());

    let header_text = if state.editing {
        "AI 配置 | 编辑中: Enter/F2 完成，Tab 保存并切换，Ctrl+S 保存配置，Esc 取消"
    } else {
        "AI 配置 | ↑/↓ 选择，Enter 编辑，PageUp/PageDown 滚动提示词，Ctrl+S 保存，Esc 返回"
    };
    frame.render_widget(
        Paragraph::new(header_text).block(Block::default().title("dict-tui").borders(Borders::ALL)),
        rows[0],
    );

    frame.render_widget(
        Paragraph::new(format!("当前字段: {}", state.selected_label())),
        rows[1],
    );

    let content_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(3),
        ])
        .split(rows[2]);

    for (idx, area) in content_rows.iter().take(FIELD_SYSTEM_PROMPT).enumerate() {
        draw_single_line_field(frame, *area, state, idx);
    }
    draw_prompt_field(frame, content_rows[FIELD_SYSTEM_PROMPT], state);

    frame.render_widget(Paragraph::new(state.status.as_str()), rows[3]);
}

fn draw_single_line_field(frame: &mut Frame, area: Rect, state: &EditorState, idx: usize) {
    let selected = state.selected == idx;
    let editing = selected && state.editing;
    let border_style = field_border_style(selected, editing);
    let inner = inner_rect(area);
    let display_value;
    let value = if editing {
        state.edit_buffer.as_str()
    } else {
        display_value = field_display_value(state, idx);
        display_value.as_str()
    };

    let (text, cursor_x) = if editing {
        single_line_view(value, state.cursor, inner.width)
    } else {
        (truncate_chars(value, inner.width as usize), 0)
    };

    frame.render_widget(
        Paragraph::new(text).block(
            Block::default()
                .title(field_label(idx))
                .borders(Borders::ALL)
                .border_style(border_style),
        ),
        area,
    );

    if editing && inner.width > 0 {
        frame.set_cursor_position(Position::new(
            inner.x + cursor_x.min(inner.width.saturating_sub(1)),
            inner.y,
        ));
    }
}

fn draw_prompt_field(frame: &mut Frame, area: Rect, state: &EditorState) {
    let selected = state.selected == FIELD_SYSTEM_PROMPT;
    let editing = selected && state.editing;
    let border_style = field_border_style(selected, editing);
    let inner = inner_rect(area);
    let value = if editing {
        state.edit_buffer.as_str()
    } else {
        state.config.system_prompt.as_str()
    };

    let (text, cursor_x, cursor_y) = if editing {
        multiline_view_at_cursor(value, state.cursor, inner.width, inner.height)
    } else {
        multiline_view_at_scroll(value, state.prompt_scroll, inner.width, inner.height)
    };

    let title = if editing {
        "系统提示词 (Enter 换行，F2 完成)"
    } else {
        "系统提示词"
    };
    frame.render_widget(
        Paragraph::new(text).wrap(Wrap { trim: false }).block(
            Block::default()
                .title(title)
                .borders(Borders::ALL)
                .border_style(border_style),
        ),
        area,
    );

    if editing && inner.width > 0 && inner.height > 0 {
        frame.set_cursor_position(Position::new(
            inner.x + cursor_x.min(inner.width.saturating_sub(1)),
            inner.y + cursor_y.min(inner.height.saturating_sub(1)),
        ));
    }
}

fn field_label(idx: usize) -> &'static str {
    match idx {
        0 => "API 地址",
        1 => "API Key",
        2 => "模型",
        3 => "回答语言",
        FIELD_WEB_SEARCH => "网页搜索",
        FIELD_JAVASCRIPT => "浏览器 JS",
        FIELD_SYSTEM_PROMPT => "系统提示词",
        _ => "",
    }
}

fn field_value(config: &Config, idx: usize) -> String {
    match idx {
        0 => config.api_endpoint.clone(),
        1 => config.api_key.clone(),
        2 => config.model.clone(),
        3 => config.answer_language.clone(),
        FIELD_WEB_SEARCH => bool_to_text(config.enable_web_search).to_string(),
        FIELD_JAVASCRIPT => bool_to_text(config.enable_javascript).to_string(),
        FIELD_SYSTEM_PROMPT => config.system_prompt.clone(),
        _ => String::new(),
    }
}

fn set_field_value(config: &mut Config, idx: usize, value: String) {
    match idx {
        0 => config.api_endpoint = value,
        1 => config.api_key = value,
        2 => config.model = value,
        3 => config.answer_language = value,
        FIELD_WEB_SEARCH => config.enable_web_search = parse_bool(&value),
        FIELD_JAVASCRIPT => config.enable_javascript = parse_bool(&value),
        FIELD_SYSTEM_PROMPT => config.system_prompt = value,
        _ => {}
    }
}

fn field_display_value(state: &EditorState, idx: usize) -> String {
    if idx == 1 && !state.config.api_key.is_empty() {
        "*".repeat(state.config.api_key.chars().count().min(16))
    } else {
        field_value(&state.config, idx)
    }
}

fn bool_to_text(value: bool) -> &'static str {
    if value {
        "on"
    } else {
        "off"
    }
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "y" | "on" | "enable" | "enabled"
    ) || matches!(value.trim(), "是" | "开" | "开启" | "启用")
}

fn field_border_style(selected: bool, editing: bool) -> Style {
    if editing {
        Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD)
    } else if selected {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default()
    }
}

fn inner_rect(area: Rect) -> Rect {
    Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    )
}

fn single_line_view(text: &str, cursor: usize, width: u16) -> (String, u16) {
    let width = width as usize;
    if width == 0 {
        return (String::new(), 0);
    }

    let cursor_chars = text[..cursor].chars().count();
    let scroll = cursor_chars.saturating_add(1).saturating_sub(width);
    let visible = text.chars().skip(scroll).take(width).collect::<String>();
    let cursor_x = cursor_chars
        .saturating_sub(scroll)
        .min(width.saturating_sub(1));
    (visible, cursor_x as u16)
}

fn multiline_view_at_cursor(
    text: &str,
    cursor: usize,
    width: u16,
    height: u16,
) -> (String, u16, u16) {
    let width = width as usize;
    let height = height as usize;
    if width == 0 || height == 0 {
        return (String::new(), 0, 0);
    }

    let (cursor_line, cursor_col) = cursor_line_col(text, cursor);
    let vertical_scroll = cursor_line.saturating_add(1).saturating_sub(height);
    let lines = visible_lines(text, vertical_scroll, width, height);
    let cursor_y = cursor_line
        .saturating_sub(vertical_scroll)
        .min(height.saturating_sub(1));
    let cursor_x = cursor_col.min(width.saturating_sub(1));
    (lines, cursor_x as u16, cursor_y as u16)
}

fn multiline_view_at_scroll(
    text: &str,
    scroll: usize,
    width: u16,
    height: u16,
) -> (String, u16, u16) {
    (
        visible_lines(text, scroll, width as usize, height as usize),
        0,
        0,
    )
}

fn visible_lines(text: &str, scroll: usize, width: usize, height: usize) -> String {
    if width == 0 || height == 0 {
        return String::new();
    }

    text.lines()
        .chain(if text.ends_with('\n') { Some("") } else { None })
        .skip(scroll)
        .take(height)
        .map(|line| truncate_chars(line, width))
        .collect::<Vec<_>>()
        .join("\n")
}

fn truncate_chars(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

fn insert_char(text: &mut String, cursor: &mut usize, ch: char) {
    text.insert(*cursor, ch);
    *cursor += ch.len_utf8();
}

fn backspace(text: &mut String, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }

    let prev = text[..*cursor]
        .char_indices()
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(0);
    text.replace_range(prev..*cursor, "");
    *cursor = prev;
}

fn delete_char(text: &mut String, cursor: usize) {
    if cursor >= text.len() {
        return;
    }

    let next = text[cursor..]
        .char_indices()
        .nth(1)
        .map(|(idx, _)| cursor + idx)
        .unwrap_or(text.len());
    text.replace_range(cursor..next, "");
}

fn move_left(text: &str, cursor: &mut usize) {
    if *cursor == 0 {
        return;
    }
    *cursor = text[..*cursor]
        .char_indices()
        .last()
        .map(|(idx, _)| idx)
        .unwrap_or(0);
}

fn move_right(text: &str, cursor: &mut usize) {
    if *cursor >= text.len() {
        return;
    }
    *cursor = text[*cursor..]
        .char_indices()
        .nth(1)
        .map(|(idx, _)| *cursor + idx)
        .unwrap_or(text.len());
}

fn move_line_start(text: &str, cursor: &mut usize) {
    *cursor = text[..*cursor].rfind('\n').map(|idx| idx + 1).unwrap_or(0);
}

fn move_line_end(text: &str, cursor: &mut usize) {
    *cursor = text[*cursor..]
        .find('\n')
        .map(|idx| *cursor + idx)
        .unwrap_or(text.len());
}

fn move_cursor_vertically(text: &str, cursor: &mut usize, direction: i32) {
    let (line, col) = cursor_line_col(text, *cursor);
    let lines = line_ranges(text);
    let target_line = if direction < 0 {
        line.saturating_sub(1)
    } else {
        (line + 1).min(lines.len().saturating_sub(1))
    };

    if target_line == line {
        return;
    }

    let (start, end) = lines[target_line];
    *cursor = byte_index_for_col(&text[start..end], start, col);
}

fn cursor_line_col(text: &str, cursor: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (idx, ch) in text.char_indices() {
        if idx >= cursor {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

fn line_ranges(text: &str) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut start = 0;

    for (idx, ch) in text.char_indices() {
        if ch == '\n' {
            ranges.push((start, idx));
            start = idx + ch.len_utf8();
        }
    }

    ranges.push((start, text.len()));
    ranges
}

fn byte_index_for_col(line: &str, line_start: usize, target_col: usize) -> usize {
    line.char_indices()
        .nth(target_col)
        .map(|(idx, _)| line_start + idx)
        .unwrap_or(line_start + line.len())
}
