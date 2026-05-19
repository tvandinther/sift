pub mod indexes;
pub mod search;
pub mod theme;

use anyhow::{Context, Result};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use indexes::IndexesView;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use rusqlite::Connection;
use search::SearchView;
use std::io;
use std::path::Path;
use std::time::Duration;

enum View {
    Search,
    Indexes,
    Help,
}

/// Main TUI application state.
pub struct App {
    view: View,
    search_view: SearchView,
    indexes_view: IndexesView,
    should_quit: bool,
    pending_search: bool,
}

impl App {
    fn new() -> Self {
        Self {
            view: View::Search,
            search_view: SearchView::new(),
            indexes_view: IndexesView::new(),
            should_quit: false,
            pending_search: false,
        }
    }

    fn handle_event(&mut self, conn: &Connection) -> Result<()> {
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match self.view {
                    View::Search => self.handle_search_keys(key.code, key.modifiers, conn)?,
                    View::Indexes => self.handle_indexes_keys(key.code, conn)?,
                    View::Help => self.handle_help_keys(key.code),
                }
            }
        }

        Ok(())
    }

    fn handle_search_keys(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
        conn: &Connection,
    ) -> Result<()> {
        // Ctrl+C always quits
        if matches!(key, KeyCode::Char('c')) && modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return Ok(());
        }

        // If input is focused, only handle text input and Esc
        if self.search_view.is_input_focused() {
            match key {
                KeyCode::Esc => {
                    self.search_view.unfocus_input();
                }
                KeyCode::Enter => {
                    // Schedule search for next iteration (allows loading indicator to render)
                    self.pending_search = true;
                    self.search_view.start_search();
                }
                KeyCode::Char(c) => {
                    self.search_view.insert_char(c);
                }
                KeyCode::Backspace => {
                    self.search_view.delete_char();
                }
                KeyCode::Left => {
                    self.search_view.move_cursor_left();
                }
                KeyCode::Right => {
                    self.search_view.move_cursor_right();
                }
                KeyCode::Home => {
                    self.search_view.move_cursor_home();
                }
                KeyCode::End => {
                    self.search_view.move_cursor_end();
                }
                _ => {}
            }
        } else {
            // Input not focused - handle navigation and hotkeys
            match key {
                KeyCode::Char('q') => {
                    self.should_quit = true;
                }
                KeyCode::Char('?') => {
                    self.view = View::Help;
                }
                KeyCode::Char('i') => {
                    self.indexes_view.refresh(conn)?;
                    self.view = View::Indexes;
                }
                KeyCode::Char('/') | KeyCode::Char('s') => {
                    self.search_view.focus_input();
                }
                KeyCode::Char('o') => {
                    if let Some(path) = self.search_view.selected_path() {
                        if let Err(e) = self.open_with_system(&path) {
                            self.search_view.set_error(format!("Error opening file: {}", e));
                        }
                    }
                }
                KeyCode::Up => {
                    self.search_view.previous_result();
                }
                KeyCode::Down => {
                    self.search_view.next_result();
                }
                KeyCode::Enter => {
                    if let Some(path) = self.search_view.selected_path() {
                        if let Err(e) = self.view_in_pager(&path) {
                            self.search_view.set_error(format!("Error opening file: {}", e));
                        }
                    }
                }
                KeyCode::Esc => {
                    self.should_quit = true;
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn handle_indexes_keys(&mut self, key: KeyCode, conn: &Connection) -> Result<()> {
        // If in delete confirmation mode
        if self.indexes_view.confirm_delete.is_some() {
            match key {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.indexes_view.confirm_delete(conn)?;
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.indexes_view.cancel_delete();
                }
                _ => {}
            }
            return Ok(());
        }

        match key {
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Char('?') => {
                self.view = View::Help;
            }
            KeyCode::Char('s') => {
                self.view = View::Search;
            }
            KeyCode::Char('d') => {
                self.indexes_view.request_delete();
            }
            KeyCode::Char('p') => {
                self.indexes_view.prune_selected(conn)?;
            }
            KeyCode::Char('P') => {
                self.indexes_view.prune_all(conn)?;
            }
            KeyCode::Char('r') => {
                self.indexes_view.refresh_selected(conn)?;
            }
            KeyCode::Char('R') => {
                self.indexes_view.refresh_all(conn)?;
            }
            KeyCode::Up => {
                self.indexes_view.previous();
            }
            KeyCode::Down => {
                self.indexes_view.next();
            }
            KeyCode::Enter => {
                self.indexes_view.clear_message();
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_help_keys(&mut self, key: KeyCode) {
        match key {
            KeyCode::Char('q') | KeyCode::Esc | KeyCode::Char('?') => {
                self.view = View::Search;
            }
            _ => {}
        }
    }

    fn view_in_pager(&self, path: &Path) -> Result<()> {
        // Check if file exists first
        if !path.exists() {
            anyhow::bail!("File not found: {}", path.display());
        }

        // Temporarily leave the TUI to open the pager
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

        // Determine pager to use and parse arguments
        let pager_env = std::env::var("PAGER").unwrap_or_else(|_| {
            // Default to less on Unix-like systems
            #[cfg(not(windows))]
            {
                "less".to_string()
            }
            #[cfg(windows)]
            {
                "more".to_string()
            }
        });

        // Split pager command and arguments (e.g., "less -R" -> ["less", "-R"])
        let pager_parts: Vec<&str> = pager_env.split_whitespace().collect();
        let (pager_cmd, pager_args) = if pager_parts.is_empty() {
            ("less", vec![])
        } else {
            (pager_parts[0], pager_parts[1..].to_vec())
        };

        let result = std::process::Command::new(pager_cmd)
            .args(&pager_args)
            .arg(path)
            .status();

        // Re-enter the TUI
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        match result {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => anyhow::bail!("Pager '{}' exited with code: {:?}", pager_env, status.code()),
            Err(e) => anyhow::bail!("Failed to execute pager '{}': {} (try setting PAGER env var)", pager_env, e),
        }
    }

    fn open_with_system(&self, path: &Path) -> Result<()> {
        // Check if file exists first
        if !path.exists() {
            anyhow::bail!("File not found: {}", path.display());
        }

        // Open file with system default application
        #[cfg(target_os = "macos")]
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .with_context(|| "Failed to execute 'open' command")?;

        #[cfg(target_os = "linux")]
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .with_context(|| "Failed to execute 'xdg-open' command")?;

        #[cfg(target_os = "windows")]
        std::process::Command::new("cmd")
            .args(["/C", "start", "", path.to_str().unwrap()])
            .spawn()
            .with_context(|| "Failed to execute 'start' command")?;

        Ok(())
    }

    fn render(&mut self, frame: &mut Frame, source_count: usize) {
        match self.view {
            View::Search => {
                self.search_view.render(frame, frame.area(), source_count);
            }
            View::Indexes => {
                self.indexes_view.render(frame, frame.area());
            }
            View::Help => {
                self.render_help(frame);
            }
        }
    }

    fn render_help(&self, frame: &mut Frame) {
        let area = centered_rect(75, 75, frame.area());

        let help_text = vec![
            Line::from(""),
            Line::from("  Search View (Hybrid: Lexical + Semantic)"),
            Line::from("  ─────────────────────────────────────────"),
            Line::from("  / or s        Focus search input"),
            Line::from("  Type          Enter search query (when focused)"),
            Line::from("  Enter         Execute search (when focused) / View file in $PAGER (when unfocused)"),
            Line::from("  Esc           Unfocus input / Quit (when unfocused)"),
            Line::from("  ↑ / ↓         Navigate results (when unfocused)"),
            Line::from("  o             Open file with system default (when unfocused)"),
            Line::from("  i             Switch to indexes view (when unfocused)"),
            Line::from(""),
            Line::from("  Indexes View"),
            Line::from("  ────────────"),
            Line::from("  ↑ / ↓         Navigate sources"),
            Line::from("  d             Delete selected source"),
            Line::from("  p             Prune selected source (remove missing files)"),
            Line::from("  P             Prune all sources"),
            Line::from("  r             Refresh selected source (check hashes, add/remove files)"),
            Line::from("  R             Refresh all sources"),
            Line::from("  s             Switch to search view"),
            Line::from(""),
            Line::from("  Global"),
            Line::from("  ──────"),
            Line::from("  ?             Toggle this help"),
            Line::from("  q             Quit (when unfocused)"),
            Line::from("  Ctrl+C        Quit"),
            Line::from(""),
        ];

        let help = Paragraph::new(help_text)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Help ")
                    .border_style(theme::border()),
            )
            .style(theme::default());

        frame.render_widget(Clear, area);
        frame.render_widget(help, area);
    }
}

/// Run the TUI application.
pub fn run(conn: &Connection) -> Result<()> {
    // Check if there are any sources
    let source_count: i64 = conn.query_row("SELECT COUNT(*) FROM sources", [], |row| row.get(0))?;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Create app
    let mut app = App::new();
    app.indexes_view.refresh(conn)?;

    // Run event loop
    let result = run_app(&mut terminal, &mut app, conn, source_count as usize);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    result
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    conn: &Connection,
    source_count: usize,
) -> Result<()> {
    loop {
        terminal.draw(|f| app.render(f, source_count))?;

        // Execute pending search if scheduled
        if app.pending_search {
            app.pending_search = false;
            // Render once to show loading indicator
            terminal.draw(|f| app.render(f, source_count))?;
            // Execute search
            if let Err(e) = app.search_view.execute_search(conn, 50) {
                app.search_view.set_error(format!("Search error: {}", e));
            }
        }

        app.handle_event(conn)?;

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

/// Helper function to create a centered rect.
fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}
