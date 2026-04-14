use std::collections::HashMap;
use std::io;
use std::path::PathBuf;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Gauge, List, ListItem, Paragraph};

use super::widgets::key_badge_line;
use crate::infrastructure::aliases;
use crate::infrastructure::rollback::{self, RollbackEntry};

// ── Types ──────────────────────────────────────────────────────────────────

#[derive(Clone, PartialEq, Eq)]
enum PkgStatus {
    Pending,
    Removing,
    Ok,
    Failed,
}

struct BrewPackageEntry {
    brew_name: String,
    apt_names: Vec<String>,  // apt packages to reinstall
    snap_names: Vec<String>, // snap packages to reinstall
    is_selected: bool,
}

impl BrewPackageEntry {
    fn has_names(&self) -> bool {
        !self.apt_names.is_empty() || !self.snap_names.is_empty()
    }

    fn all_names(&self) -> Vec<&str> {
        self.apt_names
            .iter()
            .chain(self.snap_names.iter())
            .map(|s| s.as_str())
            .collect()
    }
}

struct ProgressEntry {
    apt_names: Vec<String>,  // packages to reinstall via apt
    snap_names: Vec<String>, // packages to reinstall via snap
    brew_name: String,
    status: PkgStatus,
}

enum Phase {
    BrowsePackages,
    ScriptsModal,
    BrewUninstall,
    AptInstall,
    Done(String),
}

struct RollbackState {
    packages: Vec<BrewPackageEntry>,
    cursor: usize,
    search_query: String,
    searching: bool,

    scripts: Vec<(PathBuf, Vec<RollbackEntry>)>,
    script_cursor: usize,

    progress: Vec<ProgressEntry>,
    progress_current: usize,
    phase: Phase,
}

// ── Entry point ────────────────────────────────────────────────────────────

pub fn run_rollback_tui() -> io::Result<()> {
    // Gather data before entering TUI
    eprintln!("Loading brew packages...");
    let formulae = rollback::brew_list_formulae();
    let casks = rollback::brew_list_casks();

    let scripts = match rollback::find_rollback_scripts() {
        Ok(s) => s,
        Err(_) => Vec::new(),
    };

    let mut parsed_scripts: Vec<(PathBuf, Vec<RollbackEntry>)> = Vec::new();
    for path in &scripts {
        let entries = rollback::parse_rollback_script(path).unwrap_or_default();
        parsed_scripts.push((path.clone(), entries));
    }
    parsed_scripts.reverse(); // newest first

    // Build rollback lookup: brew_name -> [(apt_name, is_snap)]
    let mut rollback_map: HashMap<String, Vec<(String, bool)>> = HashMap::new();
    for (_, entries) in &parsed_scripts {
        for entry in entries {
            let list = rollback_map.entry(entry.brew_name.clone()).or_default();
            if !list.iter().any(|(n, _)| n == &entry.apt_name) {
                list.push((entry.apt_name.clone(), entry.is_snap));
            }
        }
    }

    // Reverse alias: brew_name -> apt_name (for packages not in rollback scripts)
    let reverse_aliases = aliases::brew_to_apt_map();

    // Collect all brew package names: leaves + any from rollback scripts (deps included)
    let mut all_brew_names: Vec<String> = formulae;
    all_brew_names.extend(casks);
    for brew_name in rollback_map.keys() {
        if !all_brew_names.contains(brew_name) {
            all_brew_names.push(brew_name.clone());
        }
    }

    // Build package list: packages with known apt name first, then unknown
    let mut known: Vec<BrewPackageEntry> = Vec::new();
    let mut unknown: Vec<BrewPackageEntry> = Vec::new();

    for name in &all_brew_names {
        let pkg_info = rollback_map.remove(name).unwrap_or_default();
        let mut apt_names: Vec<String> = pkg_info
            .iter()
            .filter(|(_, s)| !s)
            .map(|(n, _)| n.clone())
            .collect();
        let snap_names: Vec<String> = pkg_info
            .iter()
            .filter(|(_, s)| *s)
            .map(|(n, _)| n.clone())
            .collect();

        if apt_names.is_empty() && snap_names.is_empty() {
            if let Some(alias_name) = reverse_aliases.get(name) {
                apt_names.push(alias_name.clone());
            }
        }

        let entry = BrewPackageEntry {
            brew_name: name.clone(),
            apt_names,
            snap_names,
            is_selected: false,
        };
        if !entry.apt_names.is_empty() || !entry.snap_names.is_empty() {
            known.push(entry);
        } else {
            unknown.push(entry);
        }
    }

    known.sort_by(|a, b| a.brew_name.cmp(&b.brew_name));
    unknown.sort_by(|a, b| a.brew_name.cmp(&b.brew_name));
    known.append(&mut unknown);

    known.append(&mut unknown);

    if known.is_empty() {
        println!("No Homebrew packages installed.");
        return Ok(());
    }

    let mut state = RollbackState {
        packages: known,
        cursor: 0,
        search_query: String::new(),
        searching: false,
        scripts: parsed_scripts,
        script_cursor: 0,
        progress: Vec::new(),
        progress_current: 0,
        phase: Phase::BrowsePackages,
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
                    state.phase = Phase::AptInstall;

                    disable_raw_mode()?;
                    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

                    let succeeded: Vec<_> = state
                        .progress
                        .iter()
                        .filter(|p| p.status == PkgStatus::Ok)
                        .collect();

                    let apt_names: Vec<&str> = succeeded
                        .iter()
                        .flat_map(|p| p.apt_names.iter().map(|s| s.as_str()))
                        .collect();

                    let snap_names: Vec<&str> = succeeded
                        .iter()
                        .flat_map(|p| p.snap_names.iter().map(|s| s.as_str()))
                        .collect();

                    let ok_count = succeeded.len();

                    if !apt_names.is_empty() || !snap_names.is_empty() {
                        let mut failed_count_reinstall = 0;

                        if !apt_names.is_empty() {
                            println!(
                                "\n  Reinstalling {} APT packages (requires sudo)...\n",
                                apt_names.len()
                            );
                            let failed = rollback::apt_install_batch(&apt_names);
                            failed_count_reinstall += failed.len();
                        }

                        if !snap_names.is_empty() {
                            println!(
                                "\n  Reinstalling {} snap packages (requires sudo)...\n",
                                snap_names.len()
                            );
                            let failed = rollback::snap_install_batch(&snap_names);
                            failed_count_reinstall += failed.len();
                        }

                        let failed_count = state
                            .progress
                            .iter()
                            .filter(|p| p.status == PkgStatus::Failed)
                            .count();

                        let summary = if failed_count_reinstall == 0 && failed_count == 0 {
                            format!("{ok_count} restored")
                        } else {
                            let mut parts = vec![format!("{ok_count} brew removed")];
                            if failed_count > 0 {
                                parts.push(format!("{failed_count} brew errors"));
                            }
                            if failed_count_reinstall > 0 {
                                parts.push(format!("{failed_count_reinstall} reinstall errors"));
                            }
                            parts.join(", ")
                        };

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
                continue;
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
                Phase::BrowsePackages => {
                    if state.searching {
                        handle_search_key(&mut state, key.code);
                    } else {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Up | KeyCode::Char('k') => {
                                if state.cursor > 0 {
                                    state.cursor -= 1;
                                }
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if state.cursor < state.packages.len().saturating_sub(1) {
                                    state.cursor += 1;
                                }
                            }
                            KeyCode::Char(' ') => {
                                let pkg = &mut state.packages[state.cursor];
                                if pkg.has_names() {
                                    pkg.is_selected = !pkg.is_selected;
                                    if state.cursor < state.packages.len().saturating_sub(1) {
                                        state.cursor += 1;
                                    }
                                }
                            }
                            KeyCode::Char('a') => {
                                for pkg in &mut state.packages {
                                    if pkg.has_names() {
                                        pkg.is_selected = true;
                                    }
                                }
                            }
                            KeyCode::Char('n') => {
                                for pkg in &mut state.packages {
                                    pkg.is_selected = false;
                                }
                            }
                            KeyCode::Char('/') => {
                                state.searching = true;
                                state.search_query.clear();
                            }
                            KeyCode::Char('s') => {
                                state.script_cursor = 0;
                                state.phase = Phase::ScriptsModal;
                            }
                            KeyCode::Enter => {
                                let chosen: Vec<_> = state
                                    .packages
                                    .iter()
                                    .filter(|p| p.is_selected && !p.apt_names.is_empty())
                                    .collect();

                                if !chosen.is_empty() {
                                    state.progress = chosen
                                        .iter()
                                        .map(|p| ProgressEntry {
                                            apt_names: p.apt_names.clone(),
                                            snap_names: p.snap_names.clone(),
                                            brew_name: p.brew_name.clone(),
                                            status: PkgStatus::Pending,
                                        })
                                        .collect();
                                    state.progress_current = 0;
                                    state.phase = Phase::BrewUninstall;
                                }
                            }
                            _ => {}
                        }
                    }
                }

                Phase::ScriptsModal => match key.code {
                    KeyCode::Esc | KeyCode::Char('s') => {
                        state.phase = Phase::BrowsePackages;
                    }
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
                        if !state.scripts.is_empty() {
                            let script_entries = &state.scripts[state.script_cursor].1;
                            let script_brew_names: Vec<&str> = script_entries
                                .iter()
                                .map(|e| e.brew_name.as_str())
                                .collect();

                            // Select packages that belong to this script
                            for pkg in &mut state.packages {
                                pkg.is_selected =
                                    script_brew_names.contains(&pkg.brew_name.as_str());
                            }

                            state.phase = Phase::BrowsePackages;
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

fn handle_search_key(state: &mut RollbackState, code: KeyCode) {
    match code {
        KeyCode::Esc => {
            state.searching = false;
            state.search_query.clear();
        }
        KeyCode::Enter => {
            state.searching = false;
            // Jump cursor to first match
            if !state.search_query.is_empty() {
                let query = state.search_query.to_lowercase();
                if let Some(pos) = state
                    .packages
                    .iter()
                    .position(|p| p.brew_name.to_lowercase().contains(&query))
                {
                    state.cursor = pos;
                }
            }
        }
        KeyCode::Backspace => {
            state.search_query.pop();
        }
        KeyCode::Char(c) => {
            state.search_query.push(c);
        }
        _ => {}
    }
}

// ── Drawing ────────────────────────────────────────────────────────────────

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
        Phase::BrowsePackages | Phase::ScriptsModal => {
            draw_browse(f, chunks[0], chunks[1], chunks[2], state);
            if matches!(state.phase, Phase::ScriptsModal) {
                draw_scripts_modal(f, state);
            }
        }
        Phase::BrewUninstall | Phase::AptInstall => {
            draw_progress(f, chunks[0], chunks[1], chunks[2], state);
        }
        Phase::Done(msg) => {
            draw_done(f, chunks[0], chunks[1], chunks[2], state, msg);
        }
    }
}

fn draw_browse(
    f: &mut ratatui::Frame,
    header_area: Rect,
    list_area: Rect,
    footer_area: Rect,
    state: &RollbackState,
) {
    let selected_count = state.packages.iter().filter(|p| p.is_selected).count();
    let eligible_count = state
        .packages
        .iter()
        .filter(|p| !p.apt_names.is_empty())
        .count();

    let header_text = if state.searching {
        format!(" Search: {}_", state.search_query)
    } else {
        format!(
            " {selected_count}/{eligible_count} rollback-eligible selected  ({} total brew packages)",
            state.packages.len()
        )
    };

    let header = Paragraph::new(header_text).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" apt2brew rollback "),
    );
    f.render_widget(header, header_area);

    // Package list with scroll
    let list_height = list_area.height.saturating_sub(2) as usize;
    let scroll = if state.cursor >= list_height {
        state.cursor.saturating_sub(list_height - 1)
    } else {
        0
    };

    let search_lower = state.search_query.to_lowercase();

    let items: Vec<ListItem> = state
        .packages
        .iter()
        .enumerate()
        .filter(|(_, pkg)| {
            if state.search_query.is_empty() {
                true
            } else {
                pkg.brew_name.to_lowercase().contains(&search_lower)
            }
        })
        .skip(scroll)
        .take(list_height)
        .map(|(i, pkg)| {
            let is_cursor = i == state.cursor;
            let has_rollback = pkg.has_names();

            let checkbox = if !has_rollback {
                "   "
            } else if pkg.is_selected {
                "[x]"
            } else {
                "[ ]"
            };

            let line_style = if is_cursor {
                Style::default()
                    .bg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD)
            } else if has_rollback {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let check_style = if pkg.is_selected {
                Style::default().fg(Color::Cyan)
            } else if has_rollback {
                Style::default()
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let apt_info = if pkg.has_names() {
                let names: Vec<&str> = pkg.all_names();
                format!("  (apt: {})", names.join(", "))
            } else {
                String::new()
            };

            ListItem::new(Line::from(vec![
                Span::styled(format!(" {checkbox} "), check_style),
                Span::styled(format!("{:<28}", pkg.brew_name), line_style),
                Span::styled(apt_info, Style::default().fg(Color::DarkGray)),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Packages "));
    f.render_widget(list, list_area);

    // Footer
    let keys: Vec<(&str, &str)> = if state.searching {
        vec![("Type", "search"), ("Enter", "jump"), ("Esc", "cancel")]
    } else {
        vec![
            ("j/k", "move"),
            ("Space", "toggle"),
            ("a", "all"),
            ("n", "none"),
            ("/", "search"),
            ("s", "scripts"),
            ("Enter", "rollback"),
            ("q", "quit"),
        ]
    };

    let footer =
        Paragraph::new(key_badge_line(&keys)).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, footer_area);
}

fn draw_scripts_modal(f: &mut ratatui::Frame, state: &RollbackState) {
    let area = f.area();
    let popup_width = 60.min(area.width.saturating_sub(4));
    let line_count = state.scripts.len().max(1);
    let popup_height = (line_count as u16 + 5).min(area.height.saturating_sub(4));

    let popup_area = Rect {
        x: (area.width.saturating_sub(popup_width)) / 2,
        y: (area.height.saturating_sub(popup_height)) / 2,
        width: popup_width,
        height: popup_height,
    };

    f.render_widget(Clear, popup_area);

    if state.scripts.is_empty() {
        let content = Paragraph::new(" No rollback scripts found.").block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Rollback Scripts ")
                .style(Style::default().bg(Color::Black)),
        );
        f.render_widget(content, popup_area);
        return;
    }

    let mut lines = Vec::new();
    for (i, (path, entries)) in state.scripts.iter().enumerate() {
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

        let count_style = if entries.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Cyan)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("  {name:<38}"), style),
            Span::styled(format!("{} packages", entries.len()), count_style),
        ]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " j/k: navigate  |  Enter: select  |  Esc: close ",
        Style::default().fg(Color::Yellow),
    )));

    let content = Paragraph::new(lines).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Rollback Scripts ")
            .style(Style::default().bg(Color::Black)),
    );

    f.render_widget(content, popup_area);
}

fn draw_progress(
    f: &mut ratatui::Frame,
    header_area: Rect,
    list_area: Rect,
    footer_area: Rect,
    state: &RollbackState,
) {
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
    f.render_widget(gauge, header_area);

    let list_height = list_area.height.saturating_sub(2) as usize;
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
                    {
                        let all: Vec<&str> = entry
                            .apt_names
                            .iter()
                            .chain(entry.snap_names.iter())
                            .map(|s| s.as_str())
                            .collect();
                        format!("(apt: {})", all.join(", "))
                    },
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

    let list = List::new(visible).block(Block::default().borders(Borders::ALL).title(" Progress "));
    f.render_widget(list, list_area);

    let footer = Paragraph::new(key_badge_line(&[])).block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, footer_area);
}

fn draw_done(
    f: &mut ratatui::Frame,
    header_area: Rect,
    list_area: Rect,
    footer_area: Rect,
    state: &RollbackState,
    msg: &str,
) {
    let header = Paragraph::new(format!(" {msg}")).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" apt2brew rollback — complete "),
    );
    f.render_widget(header, header_area);

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
                Span::styled(
                    format!("{:<24}", {
                        let all: Vec<&str> = entry
                            .apt_names
                            .iter()
                            .chain(entry.snap_names.iter())
                            .map(|s| s.as_str())
                            .collect();
                        all.join(", ")
                    }),
                    style,
                ),
                Span::styled(
                    format!("brew:{}", entry.brew_name),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
        })
        .collect();

    let list = List::new(items).block(Block::default().borders(Borders::ALL).title(" Results "));
    f.render_widget(list, list_area);

    let footer = Paragraph::new(key_badge_line(&[("Enter", "exit"), ("q", "quit")]))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, footer_area);
}
