use ratatui::Frame;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};

use crate::domain::package::RiskLevel;

use super::app::AppState;

pub fn draw(f: &mut Frame, state: &mut AppState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),    // package list
            Constraint::Length(3), // footer / status bar
        ])
        .split(f.area());

    draw_header(f, chunks[0], state);
    draw_package_list(f, chunks[1], state);
    draw_footer(f, chunks[2], state);

    if state.show_summary {
        draw_summary_overlay(f, state);
    }
}

fn draw_header(f: &mut Frame, area: Rect, state: &AppState) {
    let selected = state.selected_count();
    let total = state.packages.len();
    let visible = state.visible_indices().len();

    let title = format!(
        " apt2brew  |  {} selected  |  {} visible / {} total  |  Filter: {}",
        selected,
        visible,
        total,
        state.filter.label()
    );

    let search_info = if state.searching {
        format!("  |  Search: {}_", state.search_query)
    } else if !state.search_query.is_empty() {
        format!("  |  Search: {}", state.search_query)
    } else {
        String::new()
    };

    let header = Paragraph::new(format!("{title}{search_info}")).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Scan Results "),
    );
    f.render_widget(header, area);
}

fn draw_package_list(f: &mut Frame, area: Rect, state: &mut AppState) {
    let visible = state.visible_indices();
    let list_height = area.height.saturating_sub(2) as usize; // borders

    // Adjust scroll to keep cursor visible
    if state.cursor < state.scroll_offset {
        state.scroll_offset = state.cursor;
    }
    if state.cursor >= state.scroll_offset + list_height {
        state.scroll_offset = state.cursor.saturating_sub(list_height - 1);
    }

    let items: Vec<ListItem> = visible
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(list_height)
        .map(|(display_idx, &pkg_idx)| {
            let pkg = &state.packages[pkg_idx];
            let reason = state.risk_reasons[pkg_idx];

            let has_match = pkg.brew_name.is_some();
            let brew_display = pkg.brew_name.as_deref().unwrap_or("-");

            let is_cursor = display_idx == state.cursor;

            // No match → entire line is dim gray, not selectable
            if !has_match {
                let dim = Style::default().fg(Color::DarkGray);
                let line_style = if is_cursor {
                    dim.bg(Color::DarkGray).add_modifier(Modifier::BOLD)
                } else {
                    dim
                };

                let line = Line::from(vec![
                    Span::styled("     ", dim),
                    Span::styled(format!("{:<26}", truncate(&pkg.name, 25)), line_style),
                    Span::styled(format!("{:<16}", truncate(&pkg.apt_version, 15)), dim),
                    Span::styled(format!("{:<16}", "-"), dim),
                    Span::styled(format!("{:<6}", "N/A"), dim),
                    Span::styled(format!("  {reason}"), dim),
                ]);

                return ListItem::new(line);
            }

            let checkbox = if pkg.is_selected { "[x]" } else { "[ ]" };

            let risk_style = match pkg.risk {
                RiskLevel::Low => Style::default().fg(Color::Green),
                RiskLevel::High => Style::default().fg(Color::Red),
            };

            let line_style = if is_cursor {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            let line = Line::from(vec![
                Span::styled(
                    format!(" {checkbox} "),
                    if pkg.is_selected {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    },
                ),
                Span::styled(format!("{:<26}", truncate(&pkg.name, 25)), line_style),
                Span::styled(
                    format!("{:<16}", truncate(&pkg.apt_version, 15)),
                    line_style,
                ),
                Span::styled(format!("{:<16}", truncate(brew_display, 15)), line_style),
                Span::styled(
                    format!(
                        "{:<6}",
                        match pkg.risk {
                            RiskLevel::Low => "Low",
                            RiskLevel::High => "HIGH",
                        }
                    ),
                    risk_style,
                ),
                Span::styled(format!("  {reason}"), Style::default().fg(Color::DarkGray)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let column_header = Line::from(vec![
        Span::raw("     "),
        Span::styled(
            format!(
                "{:<26}{:<16}{:<16}{:<6}  Reason",
                "Package", "APT Ver", "Brew", "Risk"
            ),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
    ]);

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(column_header));

    f.render_widget(list, area);
}

fn draw_footer(f: &mut Frame, area: Rect, state: &AppState) {
    let keys: Vec<(&str, &str)> = if state.searching {
        vec![("Type", "search"), ("Enter", "apply"), ("Esc", "cancel")]
    } else {
        vec![
            ("j/k", "move"),
            ("Space", "toggle"),
            ("a", "all"),
            ("n", "none"),
            ("Tab", "filter"),
            ("/", "search"),
            ("Enter", "confirm"),
            ("q", "quit"),
        ]
    };

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

    let footer = Paragraph::new(Line::from(spans)).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, area);
}

fn draw_summary_overlay(f: &mut Frame, state: &AppState) {
    let area = f.area();
    let popup_width = 60.min(area.width.saturating_sub(4));
    let selected: Vec<_> = state.packages.iter().filter(|p| p.is_selected).collect();
    let popup_height = (selected.len() as u16 + 7).min(area.height.saturating_sub(4));

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    let mut lines = vec![
        Line::from(Span::styled(
            format!(" {} packages selected for migration:", selected.len()),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];

    let max_items = popup_height.saturating_sub(6) as usize;
    for (i, pkg) in selected.iter().enumerate() {
        if i >= max_items {
            lines.push(Line::from(Span::styled(
                format!("  ... and {} more", selected.len() - max_items),
                Style::default().fg(Color::DarkGray),
            )));
            break;
        }
        let brew = pkg.brew_name.as_deref().unwrap_or("?");
        lines.push(Line::from(format!("  {} -> brew:{brew}", pkg.name)));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " [Enter/y] Confirm  |  [Esc/n] Go back ",
        Style::default().fg(Color::Yellow),
    )));

    let summary = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Migration Summary ")
            .style(Style::default().bg(Color::Black)),
    );

    f.render_widget(summary, popup_area);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}
