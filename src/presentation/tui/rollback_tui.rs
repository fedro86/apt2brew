use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, List, ListItem, Paragraph};

use crate::infrastructure::rollback::{self, RollbackEntry};

#[derive(Clone, PartialEq, Eq)]
enum PkgStatus {
    Pending,
    Removing,
    Ok,
    Failed,
}

struct ProgressEntry {
    apt_name: String,
    brew_name: String,
    status: PkgStatus,
}

enum Phase {
    SelectScript,
    SelectPackages,
    BrewUninstall, // TUI with progress bar
    AptInstall,    // leaves TUI for sudo
    Done(String),
}

struct RollbackState {
    scripts: Vec<(PathBuf, Vec<RollbackEntry>)>,
    script_cursor: usize,
    selected_entries: Vec<(RollbackEntry, bool)>,
    pkg_cursor: usize,
    progress: Vec<ProgressEntry>,
    progress_current: usize,
    phase: Phase,
}

pub fn run_rollback_tui() -> io::Result<()> {
    let scripts = match rollback::find_rollback_scripts() {
        Ok(s) if s.is_empty() => {
            println!("No rollback scripts found in ~/.apt2brew/");
            return Ok(());
        }
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error: {e}");
            return Ok(());
        }
    };

    let mut parsed: Vec<(PathBuf, Vec<RollbackEntry>)> = Vec::new();
    for path in scripts {
        let entries = rollback::parse_rollback_script(&path).unwrap_or_default();
        parsed.push((path, entries));
    }
    parsed.reverse();

    let mut state = RollbackState {
        scripts: parsed,
        script_cursor: 0,
        selected_entries: Vec::new(),
        pkg_cursor: 0,
        progress: Vec::new(),
        progress_current: 0,
        phase: Phase::SelectScript,
    };

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| draw(f, &state))?;

        match &state.phase {
            Phase::BrewUninstall => {
                let total = state.progress.len();
                if state.progress_current < total {
                    let i = state.progress_current;
                    state.progress[i].status = PkgStatus::Removing;
                    terminal.draw(|f| draw(f, &state))?;

                    let ok = rollback::brew_uninstall(&state.progress[i].brew_name).is_ok();
                    state.progress[i].status = if ok { PkgStatus::Ok } else { PkgStatus::Failed };
                    state.progress_current += 1;
                } else {
                    // All brew uninstalls done, leave TUI for sudo apt install
                    state.phase = Phase::AptInstall;

                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                    let apt_names: Vec<&str> = state
                        .progress
                        .iter()
                        .filter(|p| p.status == PkgStatus::Ok)
                        .map(|p| p.apt_name.as_str())
                        .collect();

                    let ok_count = apt_names.len();

                    if !apt_names.is_empty() {
                        println!(
                            "\n  Reinstalling {} packages via APT (requires sudo)...\n",
                            apt_names.len()
                        );
                        let apt_ok = rollback::apt_install_batch(&apt_names).is_ok();

                        let failed_count = state
                            .progress
                            .iter()
                            .filter(|p| p.status == PkgStatus::Failed)
                            .count();

                        let summary = if apt_ok {
                            format!("{ok_count} restored, {failed_count} brew errors")
                        } else {
                            format!("{ok_count} brew removed, but APT reinstall failed")
                        };

                        // Re-enter TUI for summary
                        enable_raw_mode()?;
                        execute!(io::stdout(), EnterAlternateScreen)?;
                        terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                        state.phase = Phase::Done(summary);
                    } else {
                        enable_raw_mode()?;
                        execute!(io::stdout(), EnterAlternateScreen)?;
                        terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                        state.phase = Phase::Done("All brew uninstalls failed".to_string());
                    }
                }
                continue; // don't wait for key during progress
            }

            Phase::AptInstall => continue,

            _ => {}
        }

        // Wait for key input in interactive phases
        if let Ok(Event::Key(key)) = event::read() {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match &state.phase {
                Phase::SelectScript => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.script_cursor > 0 {
                            state.script_cursor -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.script_cursor < state.scripts.len().saturating_sub(1) {
                            state.script_cursor += 1;
                        }
                    }
                    KeyCode::Enter => {
                        let entries = &state.scripts[state.script_cursor].1;
                        if !entries.is_empty() {
                            state.selected_entries =
                                entries.iter().map(|e| (e.clone(), true)).collect();
                            state.pkg_cursor = 0;
                            state.phase = Phase::SelectPackages;
                        }
                    }
                    _ => {}
                },

                Phase::SelectPackages => match key.code {
                    KeyCode::Esc => state.phase = Phase::SelectScript,
                    KeyCode::Char('q') => break,
                    KeyCode::Up | KeyCode::Char('k') => {
                        if state.pkg_cursor > 0 {
                            state.pkg_cursor -= 1;
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if state.pkg_cursor < state.selected_entries.len().saturating_sub(1) {
                            state.pkg_cursor += 1;
                        }
                    }
                    KeyCode::Char(' ') => {
                        state.selected_entries[state.pkg_cursor].1 =
                            !state.selected_entries[state.pkg_cursor].1;
                        if state.pkg_cursor < state.selected_entries.len().saturating_sub(1) {
                            state.pkg_cursor += 1;
                        }
                    }
                    KeyCode::Char('a') => {
                        for entry in &mut state.selected_entries {
                            entry.1 = true;
                        }
                    }
                    KeyCode::Char('n') => {
                        for entry in &mut state.selected_entries {
                            entry.1 = false;
                        }
                    }
                    KeyCode::Enter => {
                        let chosen: Vec<RollbackEntry> = state
                            .selected_entries
                            .iter()
                            .filter(|(_, sel)| *sel)
                            .map(|(e, _)| e.clone())
                            .collect();

                        if !chosen.is_empty() {
                            state.progress = chosen
                                .iter()
                                .map(|e| ProgressEntry {
                                    apt_name: e.apt_name.clone(),
                                    brew_name: e.brew_name.clone(),
                                    status: PkgStatus::Pending,
                                })
                                .collect();
                            state.progress_current = 0;
                            state.phase = Phase::BrewUninstall;
                        }
                    }
                    _ => {}
                },

                Phase::Done(_) => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => break,
                    _ => {}
                },

                _ => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    Ok(())
}

fn draw(f: &mut ratatui::Frame, state: &RollbackState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(f.area());

    match &state.phase {
        Phase::SelectScript => {
            let header = Paragraph::new(" Select a rollback script").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" apt2brew rollback "),
            );
            f.render_widget(header, chunks[0]);

            let items: Vec<ListItem> = state
                .scripts
                .iter()
                .enumerate()
                .map(|(i, (path, entries))| {
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    let style = if i == state.script_cursor {
                        Style::default()
                            .bg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    let pkg_style = if entries.is_empty() {
                        Style::default().fg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::Cyan)
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(format!("  {name:<40}"), style),
                        Span::styled(format!("{} packages", entries.len()), pkg_style),
                    ]))
                })
                .collect();

            let list =
                List::new(items).block(Block::default().borders(Borders::ALL).title(" Scripts "));
            f.render_widget(list, chunks[1]);

            let footer = Paragraph::new(" j/k: navigate | Enter: select | q: quit ")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }

        Phase::SelectPackages => {
            let selected_count = state.selected_entries.iter().filter(|(_, s)| *s).count();
            let total = state.selected_entries.len();

            let header = Paragraph::new(format!(
                " {selected_count}/{total} packages selected for rollback"
            ))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" apt2brew rollback "),
            );
            f.render_widget(header, chunks[0]);

            let items: Vec<ListItem> = state
                .selected_entries
                .iter()
                .enumerate()
                .map(|(i, (entry, is_selected))| {
                    let checkbox = if *is_selected { "[x]" } else { "[ ]" };
                    let is_cursor = i == state.pkg_cursor;

                    let line_style = if is_cursor {
                        Style::default()
                            .bg(Color::DarkGray)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default()
                    };

                    let check_style = if *is_selected {
                        Style::default().fg(Color::Cyan)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(format!(" {checkbox} "), check_style),
                        Span::styled(format!("{:<24}", entry.apt_name), line_style),
                        Span::styled(
                            format!("-> brew:{}", entry.brew_name),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();

            let list =
                List::new(items).block(Block::default().borders(Borders::ALL).title(" Packages "));
            f.render_widget(list, chunks[1]);

            let footer = Paragraph::new(
                " j/k: move | Space: toggle | a: all | n: none | Enter: rollback | Esc: back ",
            )
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }

        Phase::BrewUninstall | Phase::AptInstall => {
            let total = state.progress.len();
            let current = state.progress_current;
            let pct = if total > 0 {
                (current as f64 / total as f64 * 100.0) as u16
            } else {
                0
            };

            let gauge = Gauge::default()
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(format!(" Removing from Homebrew... ({current}/{total}) ")),
                )
                .gauge_style(Style::default().fg(Color::Yellow))
                .percent(pct);
            f.render_widget(gauge, chunks[0]);

            let list_height = chunks[1].height.saturating_sub(2) as usize;
            let items: Vec<ListItem> = state
                .progress
                .iter()
                .map(|entry| {
                    let (icon, style) = match &entry.status {
                        PkgStatus::Pending => ("  ..", Style::default().fg(Color::DarkGray)),
                        PkgStatus::Removing => ("  >>", Style::default().fg(Color::Yellow)),
                        PkgStatus::Ok => ("  OK", Style::default().fg(Color::Green)),
                        PkgStatus::Failed => ("  !!", Style::default().fg(Color::Red)),
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{icon} "), style),
                        Span::styled(
                            format!("{:<24}", entry.brew_name),
                            style.add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            format!("({})", entry.apt_name),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();

            let visible: Vec<ListItem> = items
                .into_iter()
                .skip(if state.progress.len() > list_height {
                    state.progress.len().saturating_sub(list_height)
                } else {
                    0
                })
                .collect();

            let list = List::new(visible)
                .block(Block::default().borders(Borders::ALL).title(" Progress "));
            f.render_widget(list, chunks[1]);

            let footer = Paragraph::new(" Rollback in progress... ")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }

        Phase::Done(msg) => {
            let header = Paragraph::new(format!(" {msg}")).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" apt2brew rollback — complete "),
            );
            f.render_widget(header, chunks[0]);

            let items: Vec<ListItem> = state
                .progress
                .iter()
                .map(|entry| {
                    let (icon, style) = match &entry.status {
                        PkgStatus::Ok => ("  OK", Style::default().fg(Color::Green)),
                        PkgStatus::Failed => ("  !!", Style::default().fg(Color::Red)),
                        _ => ("  --", Style::default().fg(Color::DarkGray)),
                    };

                    ListItem::new(Line::from(vec![
                        Span::styled(format!("{icon} "), style),
                        Span::styled(format!("{:<24}", entry.apt_name), style),
                        Span::styled(
                            format!("brew:{}", entry.brew_name),
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                })
                .collect();

            let list =
                List::new(items).block(Block::default().borders(Borders::ALL).title(" Results "));
            f.render_widget(list, chunks[1]);

            let footer = Paragraph::new(" Press Enter or q to exit ")
                .style(Style::default().fg(Color::DarkGray))
                .block(Block::default().borders(Borders::ALL));
            f.render_widget(footer, chunks[2]);
        }
    }
}
