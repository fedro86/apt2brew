use std::io;

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

use crate::domain::package::{MigrationResult, PackageMigration};
use crate::infrastructure::filesystem;
use crate::infrastructure::migrate as infra_migrate;

/// RAII guard for the alternate-screen + raw-mode terminal state.
///
/// Without this, a panic (or an early `?` return) mid-migration leaves the
/// user's terminal in raw mode with scrollback hidden — the shell becomes
/// effectively unusable until they `reset`. The guard restores terminal
/// state on Drop even under unwinding.
struct TerminalGuard {
    active: bool,
}

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        execute!(io::stdout(), EnterAlternateScreen)?;
        Ok(Self { active: true })
    }

    /// Leave the alternate screen temporarily (e.g. to run an interactive
    /// sudo prompt on the real terminal). No-op if already inactive.
    fn suspend(&mut self) -> io::Result<()> {
        if self.active {
            execute!(io::stdout(), LeaveAlternateScreen)?;
            disable_raw_mode()?;
            self.active = false;
        }
        Ok(())
    }

    /// Re-enter raw mode + alternate screen after a suspend.
    fn resume(&mut self) -> io::Result<()> {
        if !self.active {
            enable_raw_mode()?;
            execute!(io::stdout(), EnterAlternateScreen)?;
            self.active = true;
        }
        Ok(())
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if self.active {
            // Best-effort: errors here are not actionable (process is exiting).
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            let _ = disable_raw_mode();
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
enum PackageStatus {
    Pending,
    Installing,
    Ok,
    Failed(String),
    AptRemoved,
}

struct ProgressEntry {
    apt_name: String,
    brew_name: String,
    brew_type: crate::domain::package::BrewType,
    source: crate::domain::package::PackageSource,
    status: PackageStatus,
}

enum Phase {
    BrewInstall,
    AptRemove,
    Done,
}

/// Run the migration with a TUI progress display.
pub fn run_migration_tui(packages: &[PackageMigration]) -> io::Result<()> {
    let selected: Vec<_> = packages
        .iter()
        .filter(|p| p.is_selected && p.brew_name.is_some())
        .collect();

    if selected.is_empty() {
        println!("No packages selected for migration.");
        return Ok(());
    }

    let mut entries: Vec<ProgressEntry> = selected
        .iter()
        .map(|p| ProgressEntry {
            apt_name: p.name.clone(),
            brew_name: p.brew_name.clone().unwrap(),
            brew_type: p
                .brew_type
                .clone()
                .unwrap_or(crate::domain::package::BrewType::Formula),
            source: p.source.clone(),
            status: PackageStatus::Pending,
        })
        .collect();

    // Reserve rollback path *before* any system modification.
    let rollback_path = filesystem::rollback_script_path()?;

    let mut guard = TerminalGuard::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;

    let mut current = 0;
    let total = entries.len();
    let mut phase = Phase::BrewInstall;
    let mut results: Vec<MigrationResult> = Vec::new();
    let mut scroll_offset = 0;
    let mut apt_status: Option<Result<(), String>> = None;

    loop {
        terminal.draw(|f| {
            draw_progress(f, &entries, current, total, &phase, &apt_status);
        })?;

        match phase {
            Phase::BrewInstall => {
                if current < total {
                    entries[current].status = PackageStatus::Installing;
                    // Redraw with "Installing" status
                    terminal.draw(|f| {
                        draw_progress(f, &entries, current, total, &phase, &apt_status);
                    })?;

                    let result = infra_migrate::brew_install_and_verify(
                        &entries[current].apt_name,
                        &entries[current].brew_name,
                        &entries[current].brew_type,
                        entries[current].source.clone(),
                    );

                    entries[current].status = if result.error.is_some() {
                        PackageStatus::Failed(result.error.clone().unwrap())
                    } else {
                        PackageStatus::Ok
                    };

                    results.push(result);

                    // Rewrite the on-disk rollback after every install so a Ctrl-C
                    // or panic leaves behind a script matching the actual brew state.
                    let _ = filesystem::write_rollback_script_at(&rollback_path, &results);

                    // Auto-scroll to keep current visible
                    let visible_height = terminal.size()?.height.saturating_sub(10) as usize;
                    if current >= scroll_offset + visible_height {
                        scroll_offset = current.saturating_sub(visible_height - 1);
                    }

                    current += 1;
                } else {
                    // All brew installs done, move to apt remove
                    phase = Phase::AptRemove;
                }
            }

            Phase::AptRemove => {
                use crate::domain::package::PackageSource;

                let succeeded_apt: Vec<String> = entries
                    .iter()
                    .filter(|e| e.status == PackageStatus::Ok && e.source != PackageSource::Snap)
                    .map(|e| e.apt_name.clone())
                    .collect();

                let succeeded_snap: Vec<String> = entries
                    .iter()
                    .filter(|e| e.status == PackageStatus::Ok && e.source == PackageSource::Snap)
                    .map(|e| e.apt_name.clone())
                    .collect();

                if succeeded_apt.is_empty() && succeeded_snap.is_empty() {
                    apt_status = Some(Ok(()));
                    phase = Phase::Done;
                    continue;
                }

                // Need to leave TUI temporarily for sudo prompt
                guard.suspend()?;

                if !infra_migrate::warm_sudo() {
                    eprintln!("Failed to obtain sudo. Skipping package removal.");
                    apt_status = Some(Err("sudo authentication failed".to_string()));

                    guard.resume()?;
                    terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                    phase = Phase::Done;
                    continue;
                }

                let mut any_failure = false;

                if !succeeded_apt.is_empty() {
                    let apt_refs: Vec<&str> = succeeded_apt.iter().map(|s| s.as_str()).collect();
                    println!("\n  Removing {} packages from APT...\n", apt_refs.len());

                    match infra_migrate::apt_remove_batch(&apt_refs) {
                        Ok(()) => {
                            for r in results.iter_mut() {
                                if r.error.is_none() && succeeded_apt.contains(&r.package) {
                                    r.apt_removed = true;
                                }
                            }
                            for entry in entries.iter_mut() {
                                if entry.status == PackageStatus::Ok
                                    && entry.source != PackageSource::Snap
                                {
                                    entry.status = PackageStatus::AptRemoved;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("\n  APT removal failed: {e}");
                            any_failure = true;
                        }
                    }
                }

                if !succeeded_snap.is_empty() {
                    let snap_refs: Vec<&str> = succeeded_snap.iter().map(|s| s.as_str()).collect();
                    println!(
                        "\n  Removing {} snap packages (requires sudo)...\n",
                        snap_refs.len()
                    );

                    match infra_migrate::snap_remove_batch(&snap_refs) {
                        Ok(()) => {
                            for r in results.iter_mut() {
                                if r.error.is_none() && succeeded_snap.contains(&r.package) {
                                    r.apt_removed = true;
                                }
                            }
                            for entry in entries.iter_mut() {
                                if entry.status == PackageStatus::Ok
                                    && entry.source == PackageSource::Snap
                                {
                                    entry.status = PackageStatus::AptRemoved;
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("\n  Snap removal failed: {e}");
                            any_failure = true;
                        }
                    }
                }

                apt_status = if any_failure {
                    Some(Err("some removals failed".to_string()))
                } else {
                    Some(Ok(()))
                };

                // Re-enter TUI for final summary
                guard.resume()?;
                terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
                phase = Phase::Done;
            }

            Phase::Done => {
                // Wait for user to press a key to exit
                terminal.draw(|f| {
                    draw_progress(f, &entries, current, total, &phase, &apt_status);
                })?;

                if let Ok(Event::Key(key)) = event::read()
                    && key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter)
                {
                    break;
                }
            }
        }
    }

    // Restore the terminal before printing any artifacts. The Drop impl is a
    // belt-and-braces fallback for panics.
    guard.suspend()?;
    drop(terminal);

    // Generate artifacts
    if let Ok(path) = filesystem::write_brewfile(packages) {
        println!("  Brewfile: {}", path.display());
    }
    // Final rollback rewrite, now with apt_removed flags populated.
    if let Err(e) = filesystem::write_rollback_script_at(&rollback_path, &results) {
        eprintln!("  Warning: could not finalize rollback script: {e}");
    }
    let log_path =
        filesystem::write_log(&results).unwrap_or_else(|_| std::path::PathBuf::from("(failed)"));

    filesystem::print_results(&results, &rollback_path, &log_path);

    Ok(())
}

fn draw_progress(
    f: &mut ratatui::Frame,
    entries: &[ProgressEntry],
    current: usize,
    total: usize,
    phase: &Phase,
    apt_status: &Option<Result<(), String>>,
) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header + progress bar
            Constraint::Min(5),    // package list
            Constraint::Length(3), // footer
        ])
        .split(f.area());

    // Header with progress bar
    let progress_pct = if total > 0 {
        (current as f64 / total as f64) * 100.0
    } else {
        0.0
    };

    let phase_label = match phase {
        Phase::BrewInstall => format!("Installing via Homebrew... ({current}/{total})"),
        Phase::AptRemove => "Removing from APT...".to_string(),
        Phase::Done => {
            let ok = entries
                .iter()
                .filter(|e| matches!(e.status, PackageStatus::Ok | PackageStatus::AptRemoved))
                .count();
            let failed = entries
                .iter()
                .filter(|e| matches!(e.status, PackageStatus::Failed(_)))
                .count();
            format!("Done — {ok} succeeded, {failed} failed")
        }
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" apt2brew — {phase_label} ")),
        )
        .gauge_style(Style::default().fg(Color::Cyan))
        .percent(progress_pct as u16);
    f.render_widget(gauge, chunks[0]);

    // Package list
    let list_height = chunks[1].height.saturating_sub(2) as usize;
    let items: Vec<ListItem> = entries
        .iter()
        .map(|entry| {
            let (icon, style) = match &entry.status {
                PackageStatus::Pending => ("  ..", Style::default().fg(Color::DarkGray)),
                PackageStatus::Installing => ("  >>", Style::default().fg(Color::Yellow)),
                PackageStatus::Ok => ("  OK", Style::default().fg(Color::Green)),
                PackageStatus::AptRemoved => ("  OK", Style::default().fg(Color::Cyan)),
                PackageStatus::Failed(_) => ("  !!", Style::default().fg(Color::Red)),
            };

            let detail = match &entry.status {
                PackageStatus::AptRemoved => " (migrated)".to_string(),
                PackageStatus::Failed(e) => format!("  {e}"),
                _ => String::new(),
            };

            let line = Line::from(vec![
                Span::styled(format!("{icon} "), style),
                Span::styled(
                    format!("{:<24}", entry.apt_name),
                    style.add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("-> brew:{:<20}", entry.brew_name), style),
                Span::styled(truncate(&detail, 40), Style::default().fg(Color::DarkGray)),
            ]);

            ListItem::new(line)
        })
        .collect();

    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(if entries.len() > list_height {
            entries.len().saturating_sub(list_height)
        } else {
            0
        })
        .collect();

    let list =
        List::new(visible_items).block(Block::default().borders(Borders::ALL).title(" Packages "));
    f.render_widget(list, chunks[1]);

    // Footer
    let footer_text = match phase {
        Phase::Done => " Press Enter or q to exit ",
        _ => " Migration in progress... ",
    };

    let apt_info = match apt_status {
        Some(Ok(())) => "",
        Some(Err(_)) => " | APT removal failed — brew packages kept",
        None => "",
    };

    let footer = Paragraph::new(format!("{footer_text}{apt_info}"))
        .style(Style::default().fg(Color::DarkGray))
        .block(Block::default().borders(Borders::ALL));
    f.render_widget(footer, chunks[2]);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() > max {
        let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{truncated}…")
    } else {
        s.to_string()
    }
}
