use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// Build a footer line with inverted key badges (black-on-white bold key + white description).
pub fn key_badge_line(keys: &[(&str, &str)]) -> Line<'static> {
    let spans: Vec<Span> = keys
        .iter()
        .enumerate()
        .flat_map(|(i, (key, desc))| {
            let mut v = vec![
                Span::styled(
                    format!(" {key} "),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!(" {desc} "), Style::default().fg(Color::White)),
            ];
            if i < keys.len() - 1 {
                v.push(Span::styled(" ", Style::default()));
            }
            v
        })
        .collect();
    Line::from(spans)
}
