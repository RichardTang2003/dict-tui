use std::io;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Modifier, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};

use crate::ai::AiClient;
use crate::app::Config;
use crate::cache::{
    DefinitionCache, QueryResultCache, DEFINITION_CACHE_CAPACITY, SEARCH_CACHE_CAPACITY,
};
use crate::dict::DictionaryStore;
use crate::render::{build_preview_html_file, html_to_plain_text, open_in_browser};
use crate::ui::config_editor::run_config_editor;

const PAGE_STEP: usize = 10;
const DETAIL_SCROLL_STEP: usize = 3;
const AI_SPINNER: &[&str] = &["-", "\\", "|", "/"];

type AiTaskResult = std::result::Result<String, String>;

struct PendingAiQuery {
    query: String,
    context: String,
    receiver: Receiver<AiTaskResult>,
    handle: tokio::task::JoinHandle<()>,
    tick: usize,
}

enum AiPollOutcome {
    Pending(String),
    Complete {
        query: String,
        context: String,
        result: AiTaskResult,
    },
    Disconnected(String),
}

#[derive(Debug)]
struct SearchState {
    query: String,
    result_indexes: Vec<usize>,
    selected: usize,
    detail_text: String,
    detail_entry_idx: Option<usize>,
    detail_scroll: usize,
    detail_line_count: usize,
    status_text: String,
}

impl SearchState {
    fn update_results(&mut self, dict: &DictionaryStore, result_cache: &mut QueryResultCache) {
        self.result_indexes = result_cache.query(dict, &self.query);
        self.selected = if self.result_indexes.is_empty() {
            0
        } else {
            self.selected.min(self.result_indexes.len() - 1)
        };
        self.detail_entry_idx = None;
        self.detail_scroll = 0;
    }

    fn selected_entry_index(&self) -> Option<usize> {
        if self.result_indexes.is_empty() {
            None
        } else {
            Some(self.result_indexes[self.selected])
        }
    }

    fn refresh_detail(
        &mut self,
        dict: &mut DictionaryStore,
        definition_cache: &mut DefinitionCache,
    ) {
        if self.query.trim().is_empty() {
            self.detail_text = "开始输入关键词后，会在每次输入/删除字符时自动查询。".to_string();
            self.detail_entry_idx = None;
            self.detail_scroll = 0;
            self.detail_line_count = count_lines(&self.detail_text);
            return;
        }

        let Some(entry_idx) = self.selected_entry_index() else {
            self.detail_text = "未找到匹配词条，请尝试修改或缩短关键词。".to_string();
            self.detail_entry_idx = None;
            self.detail_scroll = 0;
            self.detail_line_count = count_lines(&self.detail_text);
            return;
        };

        if self.detail_entry_idx == Some(entry_idx) {
            return;
        }

        match definition_cache.get_or_load(dict, entry_idx) {
            Ok(definition) => {
                let entry = &dict.entries[entry_idx];
                let plain_text = html_to_plain_text(&definition);
                let text_body = if plain_text.is_empty() {
                    "(词条内容为空)".to_string()
                } else {
                    plain_text
                };
                self.detail_text = format!(
                    "{}\n来源词典: {}\n\n{}",
                    entry.word, entry.source, text_body
                );
                self.detail_entry_idx = Some(entry_idx);
                self.detail_scroll = 0;
                self.detail_line_count = count_lines(&self.detail_text);
            }
            Err(err) => {
                self.detail_text = format!("读取词条失败: {err}");
                self.detail_entry_idx = None;
                self.detail_scroll = 0;
                self.detail_line_count = count_lines(&self.detail_text);
            }
        }
    }

    fn scroll_up(&mut self) {
        self.detail_scroll = self.detail_scroll.saturating_sub(DETAIL_SCROLL_STEP);
    }

    fn scroll_down(&mut self) {
        let max_scroll = self.detail_line_count.saturating_sub(1);
        self.detail_scroll = (self.detail_scroll + DETAIL_SCROLL_STEP).min(max_scroll);
    }
}

impl Default for SearchState {
    fn default() -> Self {
        let text = "开始输入关键词后，会在每次输入/删除字符时自动查询。".to_string();
        Self {
            query: String::new(),
            result_indexes: Vec::new(),
            selected: 0,
            detail_text: text.clone(),
            detail_entry_idx: None,
            detail_scroll: 0,
            detail_line_count: count_lines(&text),
            status_text: String::new(),
        }
    }
}

fn count_lines(text: &str) -> usize {
    text.lines().count().max(1)
}

fn handle_query_backspace(
    state: &mut SearchState,
    store: &mut DictionaryStore,
    result_cache: &mut QueryResultCache,
    definition_cache: &mut DefinitionCache,
) {
    if state.query.pop().is_none() {
        return;
    }

    state.selected = 0;
    state.update_results(store, result_cache);
    state.refresh_detail(store, definition_cache);
}

fn is_prev_key(ch: char) -> bool {
    matches!(ch, ',' | '<')
}

fn is_next_key(ch: char) -> bool {
    matches!(ch, '.' | '>')
}

pub fn run_search(store: &mut DictionaryStore, mut config: Config) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    with_tui(|terminal| {
        let mut state = SearchState::default();
        let mut result_cache = QueryResultCache::new(SEARCH_CACHE_CAPACITY);
        let mut definition_cache = DefinitionCache::new(DEFINITION_CACHE_CAPACITY);
        let mut ai_task: Option<PendingAiQuery> = None;

        loop {
            poll_ai_task(&mut state, &mut ai_task);
            terminal.draw(|frame| draw_ui(frame, store, &state))?;

            if !event::poll(Duration::from_millis(100))? {
                continue;
            }

            let event = event::read()?;
            if let Event::Key(key) = event {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::F(2) => {
                        match open_in_browser_action(&state, store, &mut definition_cache, &config)
                        {
                            Ok(()) => state.status_text = "已打开浏览器预览".to_string(),
                            Err(err) => state.status_text = format!("打开网页失败: {err}"),
                        }
                    }
                    KeyCode::Char('o') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        match open_in_browser_action(&state, store, &mut definition_cache, &config)
                        {
                            Ok(()) => state.status_text = "已打开浏览器预览".to_string(),
                            Err(err) => state.status_text = format!("打开网页失败: {err}"),
                        }
                    }
                    KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        start_ai_query(&rt, &config, &mut state, &mut ai_task, store);
                    }
                    KeyCode::F(4) => {
                        match run_config_editor(terminal, config.clone()) {
                            Ok(new_config) => {
                                config = new_config;
                                state.status_text = "配置已更新".to_string();
                            }
                            Err(err) => {
                                state.status_text = format!("配置页错误: {err}");
                            }
                        }
                        terminal.clear()?;
                    }
                    KeyCode::Char(ch) if is_prev_key(ch) => {
                        state.selected = state.selected.saturating_sub(1);
                        state.refresh_detail(store, &mut definition_cache);
                    }
                    KeyCode::Char(ch)
                        if is_next_key(ch) && state.selected + 1 < state.result_indexes.len() =>
                    {
                        state.selected += 1;
                        state.refresh_detail(store, &mut definition_cache);
                    }
                    KeyCode::Backspace => {
                        handle_query_backspace(
                            &mut state,
                            store,
                            &mut result_cache,
                            &mut definition_cache,
                        );
                    }
                    KeyCode::Char(ch) if !ch.is_control() => {
                        state.query.push(ch);
                        state.selected = 0;
                        state.update_results(store, &mut result_cache);
                        state.refresh_detail(store, &mut definition_cache);
                    }
                    KeyCode::Up => state.scroll_up(),
                    KeyCode::Down => state.scroll_down(),
                    KeyCode::Home => {
                        state.selected = 0;
                        state.refresh_detail(store, &mut definition_cache);
                    }
                    KeyCode::End if !state.result_indexes.is_empty() => {
                        state.selected = state.result_indexes.len() - 1;
                        state.refresh_detail(store, &mut definition_cache);
                    }
                    KeyCode::PageUp => {
                        state.selected = state.selected.saturating_sub(PAGE_STEP);
                        state.refresh_detail(store, &mut definition_cache);
                    }
                    KeyCode::PageDown if !state.result_indexes.is_empty() => {
                        state.selected =
                            (state.selected + PAGE_STEP).min(state.result_indexes.len() - 1);
                        state.refresh_detail(store, &mut definition_cache);
                    }
                    _ => {}
                }
            }
        }

        abort_ai_task(&mut ai_task);
        Ok(())
    })
}

fn start_ai_query(
    rt: &tokio::runtime::Runtime,
    config: &Config,
    state: &mut SearchState,
    ai_task: &mut Option<PendingAiQuery>,
    store: &DictionaryStore,
) {
    let query = state.query.trim().to_string();
    if query.is_empty() {
        state.status_text = "请先输入要查询的内容".to_string();
        return;
    }

    let ai_context = state
        .selected_entry_index()
        .map(|idx| {
            let entry = &store.entries[idx];
            format!("[词典词条] {} ({})\n", entry.word, entry.source)
        })
        .unwrap_or_default();

    abort_ai_task(ai_task);

    state.status_text = format!("AI 查询 '{}' ...", query);
    state.detail_text = build_ai_pending_detail(&query, &ai_context, 0);
    state.detail_scroll = 0;
    state.detail_line_count = count_lines(&state.detail_text);

    *ai_task = Some(spawn_ai_query(rt, config.clone(), query, ai_context));
}

fn abort_ai_task(ai_task: &mut Option<PendingAiQuery>) {
    if let Some(task) = ai_task.take() {
        task.handle.abort();
    }
}

fn spawn_ai_query(
    rt: &tokio::runtime::Runtime,
    config: Config,
    query: String,
    context: String,
) -> PendingAiQuery {
    let (sender, receiver) = mpsc::channel();
    let task_query = query.clone();
    let task_context = context.clone();
    let handle = rt.spawn(async move {
        let ai = AiClient::new(config);
        let result = ai
            .query_with_context(&task_query, &task_context)
            .await
            .map_err(|err| err.to_string());
        let _ = sender.send(result);
    });

    PendingAiQuery {
        query,
        context,
        receiver,
        handle,
        tick: 0,
    }
}

fn poll_ai_task(state: &mut SearchState, ai_task: &mut Option<PendingAiQuery>) {
    let outcome = {
        let Some(task) = ai_task.as_mut() else {
            return;
        };

        match task.receiver.try_recv() {
            Ok(result) => AiPollOutcome::Complete {
                query: task.query.clone(),
                context: task.context.clone(),
                result,
            },
            Err(TryRecvError::Empty) => {
                task.tick = task.tick.wrapping_add(1);
                let spinner = AI_SPINNER[(task.tick / 2) % AI_SPINNER.len()];
                AiPollOutcome::Pending(format!("AI 查询中 {} {}", spinner, task.query))
            }
            Err(TryRecvError::Disconnected) => AiPollOutcome::Disconnected(task.query.clone()),
        }
    };

    match outcome {
        AiPollOutcome::Pending(status) => {
            state.status_text = status;
        }
        AiPollOutcome::Complete {
            query,
            context,
            result,
        } => {
            match result {
                Ok(answer) => {
                    state.detail_text = build_ai_answer_detail(&query, &context, &answer);
                    state.status_text = format!("AI 查询完成: {query}");
                }
                Err(err) => {
                    state.detail_text = format!("--- AI 查询失败: {query} ---\n\n{err}");
                    state.status_text = format!("AI 查询失败: {query}");
                }
            }
            state.detail_scroll = 0;
            state.detail_line_count = count_lines(&state.detail_text);
            *ai_task = None;
        }
        AiPollOutcome::Disconnected(query) => {
            state.detail_text = format!("--- AI 查询失败: {query} ---\n\nAI 任务已中断。");
            state.detail_scroll = 0;
            state.detail_line_count = count_lines(&state.detail_text);
            state.status_text = format!("AI 查询中断: {query}");
            *ai_task = None;
        }
    }
}

fn build_ai_pending_detail(query: &str, context: &str, tick: usize) -> String {
    let spinner = AI_SPINNER[(tick / 2) % AI_SPINNER.len()];
    let header = if context.trim().is_empty() {
        String::new()
    } else {
        format!("{}\n", context.trim())
    };

    format!(
        "{}--- AI 回答: {} ---\n\nAI 查询中 {} \n\n可以继续输入、滚动或打开配置页；结果返回后会自动更新到这里。",
        header, query, spinner
    )
}

fn build_ai_answer_detail(query: &str, context: &str, answer: &str) -> String {
    if context.trim().is_empty() {
        format!("--- AI 回答: {} ---\n\n{}", query, answer)
    } else {
        format!(
            "{}\n--- AI 回答: {} ---\n\n{}",
            context.trim(),
            query,
            answer
        )
    }
}

fn draw_ui(frame: &mut Frame, store: &DictionaryStore, state: &SearchState) {
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(frame.area());

    let input = Paragraph::new(state.query.as_str()).block(
        Block::default()
            .title("输入(动态查词)")
            .borders(Borders::ALL),
    );
    frame.render_widget(input, rows[0]);

    let tip = Paragraph::new(format!(
        "输入/退格实时查询 | ,/. 切换词条 | ↑/↓ 滚动详情 | Ctrl+G AI查询 | F4 配置 | Ctrl+O/F2 网页 | Esc 退出 | 命中 {} 条",
        state.result_indexes.len(),
    ));
    frame.render_widget(tip, rows[1]);

    let columns = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(35), Constraint::Percentage(65)])
        .split(rows[2]);

    let list_items: Vec<ListItem> = if state.query.trim().is_empty() {
        vec![ListItem::new("请输入关键词...")]
    } else if state.result_indexes.is_empty() {
        vec![ListItem::new("没有匹配结果")]
    } else {
        state
            .result_indexes
            .iter()
            .map(|idx| {
                let entry = &store.entries[*idx];
                ListItem::new(format!("{}  [{}]", entry.word, entry.source))
            })
            .collect()
    };

    let list = List::new(list_items)
        .block(Block::default().title("搜索结果").borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol(">> ");

    let mut list_state = ListState::default();
    if !state.result_indexes.is_empty() {
        list_state.select(Some(state.selected));
    }
    frame.render_stateful_widget(list, columns[0], &mut list_state);

    let scroll = state.detail_scroll.min(u16::MAX as usize) as u16;
    let detail_title = build_detail_title(state, columns[1].width);
    let detail = Paragraph::new(state.detail_text.as_str())
        .block(Block::default().title(detail_title).borders(Borders::ALL))
        .scroll((scroll, 0))
        .wrap(Wrap { trim: false });
    frame.render_widget(detail, columns[1]);
}

fn build_detail_title(state: &SearchState, area_width: u16) -> String {
    let title = if state.status_text.is_empty() {
        "词条详情".to_string()
    } else {
        format!("词条详情 | {}", state.status_text)
    };
    let max_chars = area_width.saturating_sub(2) as usize;
    truncate_with_ellipsis(&title, max_chars)
}

fn truncate_with_ellipsis(text: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let total = text.chars().count();
    if total <= max_chars {
        return text.to_string();
    }
    if max_chars == 1 {
        return "…".to_string();
    }
    let mut truncated: String = text.chars().take(max_chars - 1).collect();
    truncated.push('…');
    truncated
}

fn open_in_browser_action(
    state: &SearchState,
    dict: &mut DictionaryStore,
    definition_cache: &mut DefinitionCache,
    config: &Config,
) -> Result<()> {
    let Some(entry_idx) = state.selected_entry_index() else {
        bail!("当前没有可打开的词条");
    };

    let raw_definition = definition_cache.get_or_load(dict, entry_idx)?;
    let (word, asset_dir) = dict.entry_web_context(entry_idx)?;
    let preview_file =
        build_preview_html_file(&word, &raw_definition, &asset_dir, config.enable_javascript)?;
    open_in_browser(&preview_file)?;
    Ok(())
}

fn with_tui<F>(mut app: F) -> Result<()>
where
    F: FnMut(&mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()>,
{
    enable_raw_mode().context("无法开启 raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("无法进入备用屏幕")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).context("无法创建终端")?;

    let run_result = app(&mut terminal);
    let restore_result = restore_terminal(&mut terminal);

    match (run_result, restore_result) {
        (Err(err), _) => Err(err),
        (Ok(_), Err(err)) => Err(err),
        (Ok(_), Ok(_)) => Ok(()),
    }
}

fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>) -> Result<()> {
    disable_raw_mode().context("无法关闭 raw mode")?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen).context("无法退出备用屏幕")?;
    terminal.show_cursor().context("无法恢复光标")?;
    Ok(())
}
