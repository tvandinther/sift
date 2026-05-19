use ratatui::style::{Color, Modifier, Style};

/// Default text style.
pub fn default() -> Style {
    Style::default().fg(Color::White)
}

/// Search bar style.
pub fn search_bar() -> Style {
    Style::default().fg(Color::Cyan)
}

/// Search bar cursor.
pub fn cursor() -> Style {
    Style::default().fg(Color::Black).bg(Color::Cyan)
}

/// Selected result.
pub fn selected() -> Style {
    Style::default()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

/// Result snippet.
pub fn snippet() -> Style {
    Style::default().fg(Color::Gray)
}

/// Status bar.
pub fn status_bar() -> Style {
    Style::default().fg(Color::Black).bg(Color::Gray)
}

/// Help text.
pub fn help() -> Style {
    Style::default().fg(Color::DarkGray)
}

/// Header text.
pub fn header() -> Style {
    Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)
}

/// Border style.
pub fn border() -> Style {
    Style::default().fg(Color::Gray)
}

/// Error message.
pub fn error() -> Style {
    Style::default().fg(Color::Red)
}

/// Success message.
pub fn success() -> Style {
    Style::default().fg(Color::Green)
}

/// Search mode indicator.
pub fn mode_indicator() -> Style {
    Style::default().fg(Color::Yellow)
}
