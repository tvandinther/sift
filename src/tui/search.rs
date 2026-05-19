use crate::index;
use crate::search;
use crate::tui::theme;
use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;

/// Search view state.
pub struct SearchView {
    query: String,
    cursor_pos: usize,
    results: Vec<search::FileResult>,
    list_state: ListState,
    embedding_model: Option<Arc<index::embed::EmbeddingModel>>,
    error: Option<String>,
    input_focused: bool,
    is_searching: bool,
}

impl SearchView {
    pub fn new() -> Self {
        Self {
            query: String::new(),
            cursor_pos: 0,
            results: Vec::new(),
            list_state: ListState::default(),
            embedding_model: None,
            error: None,
            input_focused: true,
            is_searching: false,
        }
    }

    /// Check if currently searching.
    pub fn is_searching(&self) -> bool {
        self.is_searching
    }

    /// Start search (sets loading state).
    pub fn start_search(&mut self) {
        self.is_searching = true;
    }

    /// Check if input is focused.
    pub fn is_input_focused(&self) -> bool {
        self.input_focused
    }

    /// Focus on input.
    pub fn focus_input(&mut self) {
        self.input_focused = true;
    }

    /// Unfocus from input (focus on results).
    pub fn unfocus_input(&mut self) {
        self.input_focused = false;
    }

    /// Set error message.
    pub fn set_error(&mut self, msg: String) {
        self.error = Some(msg);
    }

    /// Handle character input.
    pub fn insert_char(&mut self, c: char) {
        self.query.insert(self.cursor_pos, c);
        self.cursor_pos += 1;
    }

    /// Handle backspace.
    pub fn delete_char(&mut self) {
        if self.cursor_pos > 0 {
            self.query.remove(self.cursor_pos - 1);
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor left.
    pub fn move_cursor_left(&mut self) {
        if self.cursor_pos > 0 {
            self.cursor_pos -= 1;
        }
    }

    /// Move cursor right.
    pub fn move_cursor_right(&mut self) {
        if self.cursor_pos < self.query.len() {
            self.cursor_pos += 1;
        }
    }

    /// Move cursor to start.
    pub fn move_cursor_home(&mut self) {
        self.cursor_pos = 0;
    }

    /// Move cursor to end.
    pub fn move_cursor_end(&mut self) {
        self.cursor_pos = self.query.len();
    }

    /// Navigate to previous result.
    pub fn previous_result(&mut self) {
        if !self.results.is_empty() {
            let i = self.list_state.selected().unwrap_or(0);
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    /// Navigate to next result.
    pub fn next_result(&mut self) {
        if !self.results.is_empty() {
            let i = self.list_state.selected().unwrap_or(0);
            if i < self.results.len() - 1 {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    /// Get selected result path.
    pub fn selected_path(&self) -> Option<PathBuf> {
        self.list_state
            .selected()
            .and_then(|i| self.results.get(i))
            .map(|r| r.file_path.clone())
    }

    /// Execute search (always hybrid: lexical + semantic).
    pub fn execute_search(&mut self, conn: &Connection, model_cache: &std::path::Path, limit: usize) -> Result<()> {
        self.error = None;

        if self.query.trim().is_empty() {
            self.results.clear();
            self.list_state.select(None);
            self.is_searching = false;
            return Ok(());
        }

        // Load embedding model if not already loaded
        if self.embedding_model.is_none() {
            match index::embed::EmbeddingModel::load(model_cache, false) {
                Ok(model) => {
                    self.embedding_model = Some(Arc::new(model));
                }
                Err(e) => {
                    self.error = Some(format!("Failed to load embedding model: {}", e));
                    self.is_searching = false;
                    return Ok(());
                }
            }
        }

        // Perform hybrid search (lexical + semantic)
        let file_results = if let Some(model) = &self.embedding_model {
            let lex_results = search::lexical::search(conn, &self.query, limit * 3)?;
            let sem_results = search::semantic::search(conn, model, &self.query, limit * 3)?;
            let merged = search::fusion::merge_results(lex_results, sem_results);
            search::fusion::deduplicate_to_files(merged)
        } else {
            Vec::new()
        };

        self.results = file_results.into_iter().take(limit).collect();

        // Select first result if any and unfocus input
        if !self.results.is_empty() {
            self.list_state.select(Some(0));
            self.input_focused = false;
        } else {
            self.list_state.select(None);
            // Keep input focused if no results
        }

        self.is_searching = false;
        Ok(())
    }

    /// Render the search view.
    pub fn render(&mut self, frame: &mut Frame, area: Rect, source_count: usize) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3), // Search bar
                Constraint::Min(0),    // Results
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Render search bar
        self.render_search_bar(frame, chunks[0]);

        // Render results
        self.render_results(frame, chunks[1]);

        // Render status bar
        self.render_status_bar(frame, chunks[2], source_count);
    }

    fn render_search_bar(&self, frame: &mut Frame, area: Rect) {
        let title = " sift ";

        let search_text = if self.query.is_empty() {
            if self.input_focused {
                Span::styled("Search...", theme::help())
            } else {
                Span::styled("Press / or s to search", theme::help())
            }
        } else {
            Span::styled(&self.query, theme::search_bar())
        };

        let mut line = vec![Span::raw("> "), search_text];

        // Add cursor if focused
        if self.input_focused && self.cursor_pos == self.query.len() {
            line.push(Span::styled(" ", theme::cursor()));
        }

        let border_style = if self.input_focused {
            theme::search_bar()
        } else {
            theme::border()
        };

        let paragraph = Paragraph::new(Line::from(line))
            .block(Block::default().borders(Borders::ALL).title(title).border_style(border_style));

        frame.render_widget(paragraph, area);
    }

    fn render_results(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(err) = &self.error {
            let error_msg = Paragraph::new(err.as_str())
                .style(theme::error())
                .block(Block::default().borders(Borders::ALL).border_style(theme::border()));
            frame.render_widget(error_msg, area);
            return;
        }

        if self.is_searching {
            let loading = Paragraph::new("Searching...")
                .style(theme::mode_indicator())
                .block(Block::default().borders(Borders::ALL).border_style(theme::border()));
            frame.render_widget(loading, area);
            return;
        }

        if self.results.is_empty() {
            let msg = if self.query.trim().is_empty() {
                "Type a query and press Enter to search"
            } else {
                "No results found"
            };
            let empty = Paragraph::new(msg)
                .style(theme::help())
                .block(Block::default().borders(Borders::ALL).border_style(theme::border()));
            frame.render_widget(empty, area);
            return;
        }

        let items: Vec<ListItem> = self
            .results
            .iter()
            .map(|result| {
                let path = result.file_path.display().to_string();
                let source = format!("[{}]", result.source);
                let snippet = result.snippet.chars().take(80).collect::<String>();

                let content = vec![
                    Line::from(vec![
                        Span::raw(path),
                        Span::raw("  "),
                        Span::styled(source, theme::mode_indicator()),
                    ]),
                    Line::from(Span::styled(snippet, theme::snippet())),
                ];

                ListItem::new(content)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} results ", self.results.len()))
                    .border_style(theme::border()),
            )
            .highlight_style(theme::selected());

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect, source_count: usize) {
        let sources_text = if source_count == 1 {
            "1 source"
        } else {
            "sources"
        };

        let status_text = if self.input_focused {
            format!(
                " {} {}  [enter] search  [esc] unfocus  [ctrl+c] quit ",
                source_count, sources_text
            )
        } else {
            format!(
                " {} {}  [/] focus  [enter] view  [o] open  [i] indexes  [?] help  [q] quit ",
                source_count, sources_text
            )
        };

        let status = Paragraph::new(status_text).style(theme::status_bar());
        frame.render_widget(status, area);
    }
}
