use crossterm::event::KeyCode;

use super::app::{AppState, TuiOutcome};

/// Handle a key press. Returns Some(outcome) to exit the TUI, None to continue.
pub fn handle_key(key: KeyCode, state: &mut AppState) -> Option<TuiOutcome> {
    // Summary screen has its own input handling
    if state.show_summary {
        return handle_summary_key(key, state);
    }

    // Search mode captures all printable keys
    if state.searching {
        return handle_search_key(key, state);
    }

    match key {
        KeyCode::Char('q') | KeyCode::Esc => Some(TuiOutcome::Cancelled),

        KeyCode::Up | KeyCode::Char('k') => {
            if state.cursor > 0 {
                state.cursor -= 1;
            }
            None
        }

        KeyCode::Down | KeyCode::Char('j') => {
            let visible_count = state.visible_indices().len();
            if visible_count > 0 && state.cursor < visible_count - 1 {
                state.cursor += 1;
            }
            None
        }

        KeyCode::Char(' ') => {
            state.toggle_selected();
            // Move down after toggle for quick batch selection
            let visible_count = state.visible_indices().len();
            if visible_count > 0 && state.cursor < visible_count - 1 {
                state.cursor += 1;
            }
            None
        }

        KeyCode::Char('a') => {
            // Select all visible
            let visible = state.visible_indices();
            for &idx in &visible {
                state.packages[idx].is_selected = true;
            }
            None
        }

        KeyCode::Char('n') => {
            // Deselect all visible
            let visible = state.visible_indices();
            for &idx in &visible {
                state.packages[idx].is_selected = false;
            }
            None
        }

        KeyCode::Tab => {
            state.filter = state.filter.next();
            state.cursor = 0;
            state.scroll_offset = 0;
            None
        }

        KeyCode::Char('/') => {
            state.searching = true;
            state.search_query.clear();
            None
        }

        KeyCode::Enter => {
            if state.selected_count() > 0 {
                state.show_summary = true;
            }
            None
        }

        _ => None,
    }
}

fn handle_search_key(key: KeyCode, state: &mut AppState) -> Option<TuiOutcome> {
    match key {
        KeyCode::Esc => {
            state.searching = false;
            state.search_query.clear();
            state.cursor = 0;
        }
        KeyCode::Enter => {
            state.searching = false;
            state.cursor = 0;
        }
        KeyCode::Backspace => {
            state.search_query.pop();
            state.cursor = 0;
        }
        KeyCode::Char(c) => {
            state.search_query.push(c);
            state.cursor = 0;
        }
        _ => {}
    }
    None
}

fn handle_summary_key(key: KeyCode, state: &mut AppState) -> Option<TuiOutcome> {
    match key {
        KeyCode::Char('y') | KeyCode::Enter => {
            let confirmed = state.packages.clone();
            Some(TuiOutcome::Confirmed(confirmed))
        }
        KeyCode::Esc | KeyCode::Char('n') => {
            state.show_summary = false;
            None
        }
        _ => None,
    }
}
