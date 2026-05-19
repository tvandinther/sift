pub mod indexes;
pub mod search;
pub mod theme;

use anyhow::Result;
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
}

impl App {
    fn new() -> Self {
        Self {
            view: View::Search,
            search_view: SearchView::new(),
            indexes_view: IndexesView::new(),
            should_quit: false,
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

        // Check if search should be executed
        if matches!(self.view, View::Search) && self.search_view.should_search() {
            self.search_view.execute_search(conn, 50)?;
        }

        Ok(())
    }

    fn handle_search_keys(
        &mut self,
        key: KeyCode,
        modifiers: KeyModifiers,
        conn: &Connection,
    ) -> Result<()> {
        match key {
            KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.should_quit = true;
            }
            KeyCode::Char('?') => {
                self.view = View::Help;
            }
            KeyCode::Char('i') => {
                self.indexes_view.refresh(conn)?;
                self.view = View::Indexes;
            }
            KeyCode::Tab => {
                self.search_view.toggle_mode();
            }
            KeyCode::Up => {
                self.search_view.previous_result();
            }
            KeyCode::Down => {
                self.search_view.next_result();
            }
            KeyCode::Enter => {
                if let Some(path) = self.search_view.selected_path() {
                    self.open_in_editor(&path)?;
                }
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
            KeyCode::Char('g') => {
                self.indexes_view.run_gc(conn)?;
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

    fn open_in_editor(&self, path: &Path) -> Result<()> {
        // Temporarily leave the TUI to open the editor
        disable_raw_mode()?;
        execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture)?;

        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
        let status = std::process::Command::new(editor)
            .arg(path)
            .status()?;

        // Re-enter the TUI
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen, EnableMouseCapture)?;

        if !status.success() {
            anyhow::bail!("Editor exited with non-zero status");
        }

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
        let area = centered_rect(60, 60, frame.area());

        let help_text = vec![
            Line::from(""),
            Line::from("  Search View"),
            Line::from("  ───────────"),
            Line::from("  Type          Search for text"),
            Line::from("  ↑ / ↓         Navigate results"),
            Line::from("  Enter         Open file in $EDITOR"),
            Line::from("  Tab           Toggle search mode (hybrid/lexical/semantic)"),
            Line::from("  i             Switch to indexes view"),
            Line::from(""),
            Line::from("  Indexes View"),
            Line::from("  ────────────"),
            Line::from("  ↑ / ↓         Navigate sources"),
            Line::from("  d             Delete selected source"),
            Line::from("  g             Run garbage collection"),
            Line::from("  s             Switch to search view"),
            Line::from(""),
            Line::from("  Global"),
            Line::from("  ──────"),
            Line::from("  ?             Toggle this help"),
            Line::from("  q / Esc       Quit"),
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
