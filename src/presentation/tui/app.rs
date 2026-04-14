use std::io;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::application::scan::ScanResult;
use crate::domain::package::PackageMigration;

use super::input::handle_key;
use super::render::draw;

/// Which filter is active in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    All,
    Migratable,
    HighRisk,
    NoMatch,
}

impl Filter {
    pub fn label(self) -> &'static str {
        match self {
            Filter::All => "All",
            Filter::Migratable => "Migratable",
            Filter::HighRisk => "High Risk",
            Filter::NoMatch => "No Match",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Filter::All => Filter::Migratable,
            Filter::Migratable => Filter::HighRisk,
            Filter::HighRisk => Filter::NoMatch,
            Filter::NoMatch => Filter::All,
        }
    }
}

/// What the TUI resolved to when the user exits.
pub enum TuiOutcome {
    /// User confirmed their selection.
    Confirmed(Vec<PackageMigration>),
    /// User cancelled (Esc/q).
    Cancelled,
}

/// TUI application state.
pub struct AppState {
    pub packages: Vec<PackageMigration>,
    pub risk_reasons: Vec<&'static str>,
    pub cursor: usize,
    pub filter: Filter,
    pub search_query: String,
    pub searching: bool,
    pub scroll_offset: usize,
    pub show_summary: bool,
}

impl AppState {
    pub fn new(result: ScanResult) -> Self {
        Self {
            packages: result.migrations,
            risk_reasons: result.risk_reasons,
            cursor: 0,
            filter: Filter::All,
            search_query: String::new(),
            searching: false,
            scroll_offset: 0,
            show_summary: false,
        }
    }

    /// Get indices of packages matching the current filter and search.
    pub fn visible_indices(&self) -> Vec<usize> {
        self.packages
            .iter()
            .enumerate()
            .filter(|(_, pkg)| self.matches_filter(pkg) && self.matches_search(pkg))
            .map(|(i, _)| i)
            .collect()
    }

    fn matches_filter(&self, pkg: &PackageMigration) -> bool {
        use crate::domain::package::RiskLevel;
        match self.filter {
            Filter::All => true,
            Filter::Migratable => pkg.brew_name.is_some() && pkg.risk == RiskLevel::Low,
            Filter::HighRisk => pkg.risk == RiskLevel::High,
            Filter::NoMatch => pkg.brew_name.is_none(),
        }
    }

    fn matches_search(&self, pkg: &PackageMigration) -> bool {
        if self.search_query.is_empty() {
            return true;
        }
        let q = self.search_query.to_lowercase();
        pkg.name.to_lowercase().contains(&q)
            || pkg
                .brew_name
                .as_ref()
                .is_some_and(|b| b.to_lowercase().contains(&q))
    }

    pub fn toggle_selected(&mut self) {
        let visible = self.visible_indices();
        if let Some(&idx) = visible.get(self.cursor) {
            // Only allow toggle if package has a brew match
            if self.packages[idx].brew_name.is_some() {
                self.packages[idx].is_selected = !self.packages[idx].is_selected;
            }
        }
    }

    pub fn selected_count(&self) -> usize {
        self.packages.iter().filter(|p| p.is_selected).count()
    }
}

/// Run the TUI and return the user's outcome.
pub fn run_tui(result: ScanResult) -> io::Result<TuiOutcome> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut state = AppState::new(result);
    let outcome;

    loop {
        terminal.draw(|f| draw(f, &mut state))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }

            match handle_key(key.code, &mut state) {
                Some(TuiOutcome::Confirmed(pkgs)) => {
                    outcome = TuiOutcome::Confirmed(pkgs);
                    break;
                }
                Some(TuiOutcome::Cancelled) => {
                    outcome = TuiOutcome::Cancelled;
                    break;
                }
                None => {}
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    Ok(outcome)
}
