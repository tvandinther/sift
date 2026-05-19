use crate::index;
use crate::tui::theme;
use anyhow::Result;
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame,
};
use rusqlite::Connection;

/// Indexes view state.
pub struct IndexesView {
    sources: Vec<index::db::SourceSummary>,
    list_state: ListState,
    pub(crate) confirm_delete: Option<usize>,
    message: Option<String>,
    is_error: bool,
}

impl IndexesView {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
            list_state: ListState::default(),
            confirm_delete: None,
            message: None,
            is_error: false,
        }
    }

    /// Refresh the list of sources.
    pub fn refresh(&mut self, conn: &Connection) -> Result<()> {
        self.sources = index::db::list_sources(conn)?;
        if !self.sources.is_empty() && self.list_state.selected().is_none() {
            self.list_state.select(Some(0));
        }
        Ok(())
    }

    /// Navigate to previous source.
    pub fn previous(&mut self) {
        if !self.sources.is_empty() {
            let i = self.list_state.selected().unwrap_or(0);
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
            self.confirm_delete = None;
        }
    }

    /// Navigate to next source.
    pub fn next(&mut self) {
        if !self.sources.is_empty() {
            let i = self.list_state.selected().unwrap_or(0);
            if i < self.sources.len() - 1 {
                self.list_state.select(Some(i + 1));
            }
            self.confirm_delete = None;
        }
    }

    /// Request deletion confirmation for selected source.
    pub fn request_delete(&mut self) {
        if let Some(i) = self.list_state.selected() {
            self.confirm_delete = Some(i);
            self.message = None;
        }
    }

    /// Cancel deletion.
    pub fn cancel_delete(&mut self) {
        self.confirm_delete = None;
    }

    /// Confirm and execute deletion.
    pub fn confirm_delete(&mut self, conn: &Connection) -> Result<()> {
        if let Some(i) = self.confirm_delete {
            if let Some(source) = self.sources.get(i) {
                let identifier = source.label.as_deref().unwrap_or(&source.path);
                match index::db::delete_source(conn, identifier) {
                    Ok(true) => {
                        self.message = Some(format!("Deleted '{}'", identifier));
                        self.is_error = false;
                        self.refresh(conn)?;

                        // Adjust selection after deletion
                        if self.sources.is_empty() {
                            self.list_state.select(None);
                        } else if i >= self.sources.len() {
                            self.list_state.select(Some(self.sources.len() - 1));
                        }
                    }
                    Ok(false) => {
                        self.message = Some(format!("Source '{}' not found", identifier));
                        self.is_error = true;
                    }
                    Err(e) => {
                        self.message = Some(format!("Error deleting source: {}", e));
                        self.is_error = true;
                    }
                }
            }
        }
        self.confirm_delete = None;
        Ok(())
    }

    /// Run prune on selected source.
    pub fn prune_selected(&mut self, conn: &Connection) -> Result<()> {
        if let Some(i) = self.list_state.selected() {
            if let Some(source) = self.sources.get(i) {
                // Get files for this source
                let files = index::db::get_source_files(conn, source.id)?;
                let mut removed = 0;

                for file_path in files {
                    if !std::path::Path::new(&file_path).exists() {
                        index::db::delete_file(conn, &file_path)?;
                        removed += 1;
                    }
                }

                if removed == 0 {
                    self.message = Some(format!("No files to remove from '{}'", source.label.as_deref().unwrap_or(&source.path)));
                } else {
                    self.message = Some(format!("Removed {} files from '{}'", removed, source.label.as_deref().unwrap_or(&source.path)));
                }
                self.is_error = false;
                self.refresh(conn)?;
            }
        }
        Ok(())
    }

    /// Run prune on all sources.
    pub fn prune_all(&mut self, conn: &Connection) -> Result<()> {
        match index::db::gc(conn) {
            Ok(summary) => {
                if summary.files_removed == 0 {
                    self.message = Some("No files to remove".to_string());
                } else {
                    self.message = Some(format!(
                        "Removed {} files ({} chunks)",
                        summary.files_removed, summary.chunks_removed
                    ));
                }
                self.is_error = false;
                self.refresh(conn)?;
            }
            Err(e) => {
                self.message = Some(format!("Error running prune: {}", e));
                self.is_error = true;
            }
        }
        Ok(())
    }

    /// Refresh selected source.
    pub fn refresh_selected(&mut self, conn: &Connection, model_cache: &std::path::Path) -> Result<()> {
        if let Some(i) = self.list_state.selected() {
            if let Some(source) = self.sources.get(i) {
                let identifier = source.label.as_deref().unwrap_or(&source.path);
                match index::run_refresh(conn, Some(identifier), false, model_cache, false) {
                    Ok(stats) => {
                        self.message = Some(format!(
                            "Refreshed '{}': {} scanned, {} added, {} updated, {} removed",
                            identifier, stats.scanned, stats.added, stats.updated, stats.removed
                        ));
                        self.is_error = false;
                        self.refresh(conn)?;
                    }
                    Err(e) => {
                        self.message = Some(format!("Error refreshing '{}': {}", identifier, e));
                        self.is_error = true;
                    }
                }
            }
        }
        Ok(())
    }

    /// Refresh all sources.
    pub fn refresh_all(&mut self, conn: &Connection, model_cache: &std::path::Path) -> Result<()> {
        match index::run_refresh(conn, None, false, model_cache, false) {
            Ok(stats) => {
                self.message = Some(format!(
                    "Refreshed all: {} scanned, {} added, {} updated, {} removed",
                    stats.scanned, stats.added, stats.updated, stats.removed
                ));
                self.is_error = false;
                self.refresh(conn)?;
            }
            Err(e) => {
                self.message = Some(format!("Error refreshing: {}", e));
                self.is_error = true;
            }
        }
        Ok(())
    }

    /// Clear message.
    pub fn clear_message(&mut self) {
        self.message = None;
    }

    /// Render the indexes view.
    pub fn render(&mut self, frame: &mut Frame, area: Rect) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),    // List
                Constraint::Length(3), // Message/confirmation
                Constraint::Length(1), // Status bar
            ])
            .split(area);

        // Render list
        self.render_list(frame, chunks[0]);

        // Render message or confirmation
        if self.confirm_delete.is_some() {
            self.render_confirmation(frame, chunks[1]);
        } else if self.message.is_some() {
            self.render_message(frame, chunks[1]);
        }

        // Render status bar
        self.render_status_bar(frame, chunks[2]);
    }

    fn render_list(&mut self, frame: &mut Frame, area: Rect) {
        if self.sources.is_empty() {
            let empty = Paragraph::new("No indexed sources. Press 'q' to exit and run 'sift index add <path>' to get started.")
                .style(theme::help())
                .block(Block::default().borders(Borders::ALL).title(" Indexes ").border_style(theme::border()));
            frame.render_widget(empty, area);
            return;
        }

        let items: Vec<ListItem> = self
            .sources
            .iter()
            .map(|source| {
                let label = source.label.as_deref().unwrap_or("—");
                let size = format_size(source.total_size_bytes);
                let indexed = format_timestamp(&source.indexed_at);
                let embeddings = if source.embedding_count > 0 {
                    format!("{} chunks", source.embedding_count)
                } else {
                    "none".to_string()
                };

                let path = truncate(&source.path, 40);

                let content = vec![
                    Line::from(vec![
                        Span::styled(format!("{:<40}", path), theme::default()),
                        Span::raw("  "),
                        Span::styled(format!("{:<12}", truncate(label, 12)), theme::snippet()),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            format!(
                                "{} files  {}  {}  {}",
                                source.file_count, size, embeddings, indexed
                            ),
                            theme::help(),
                        ),
                    ]),
                ];

                ListItem::new(content)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(format!(" {} Indexes ", self.sources.len()))
                    .border_style(theme::border()),
            )
            .highlight_style(theme::selected());

        frame.render_stateful_widget(list, area, &mut self.list_state);
    }

    fn render_confirmation(&self, frame: &mut Frame, area: Rect) {
        if let Some(i) = self.confirm_delete {
            if let Some(source) = self.sources.get(i) {
                let identifier = source.label.as_deref().unwrap_or(&source.path);
                let msg = format!("Delete '{}'? [y/n]", identifier);
                let confirmation = Paragraph::new(msg)
                    .style(theme::error())
                    .block(Block::default().borders(Borders::ALL).border_style(theme::error()));
                frame.render_widget(confirmation, area);
            }
        }
    }

    fn render_message(&self, frame: &mut Frame, area: Rect) {
        if let Some(msg) = &self.message {
            let style = if self.is_error {
                theme::error()
            } else {
                theme::success()
            };
            let message = Paragraph::new(msg.as_str())
                .style(style)
                .block(Block::default().borders(Borders::ALL).border_style(style));
            frame.render_widget(message, area);
        }
    }

    fn render_status_bar(&self, frame: &mut Frame, area: Rect) {
        let status_text = " [d] delete  [p] prune  [P] prune all  [r] refresh  [R] refresh all  [s] search  [?] help  [q] quit ";
        let status = Paragraph::new(status_text).style(theme::status_bar());
        frame.render_widget(status, area);
    }

    /// Get the count of sources.
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

fn format_timestamp(timestamp: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) {
        dt.format("%Y-%m-%d %H:%M").to_string()
    } else {
        timestamp.to_string()
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len.saturating_sub(3)])
    }
}
