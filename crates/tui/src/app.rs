//! Application state and all state-transition logic. Deliberately free of
//! ratatui widget types (except the small, plain `ListState` selection helper)
//! so the logic is unit-testable headless.

use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use issue_core::core::{body_after_frontmatter, sort_by_id, Issue, STATUS_CLOSED, STATUS_OPEN};
use issue_core::ops::{self, EditIssue, NewIssue};
use issue_core::storage;

use crate::form::{Form, Mode};

/// Status dimension of the filter pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusFilter {
    Open,
    Closed,
    All,
}

/// Which of the three panes currently has focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Filters,
    Issues,
    Detail,
}

/// A filter-pane row, for selection in the Filters panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FilterRow {
    Status(StatusFilter),
    Label(String),
}

/// Returns the current unix time in seconds.
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The whole TUI application state.
pub struct App {
    pub dir: PathBuf,
    /// All issues, sorted by id.
    pub issues: Vec<Issue>,
    pub filter: StatusFilter,
    pub label_filter: Option<String>,
    /// `Some` while in search-input mode or while a query is active.
    pub search: Option<String>,
    /// `true` while actively typing a search query (vs. an applied one).
    pub searching: bool,
    /// Indices into `issues` after filter + search, in display order.
    pub visible: Vec<usize>,
    /// Selected position within `visible`.
    pub selected: usize,
    /// Selected row within the Filters panel.
    pub filter_selected: usize,
    pub focus: Panel,
    /// Cached (id, body) of the currently selected issue.
    pub detail: Option<(i64, String)>,
    /// (label, count) pairs for the filter pane.
    pub labels_with_counts: Vec<(String, usize)>,
    pub open_count: usize,
    pub closed_count: usize,
    pub all_count: usize,
    pub modal: Option<Form>,
    pub status_line: String,
    pub show_help: bool,
    pub should_quit: bool,
}

impl App {
    /// Builds an app by loading issues from `dir`.
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        let mut issues = storage::load_issues(&dir)?;
        sort_by_id(&mut issues);
        Ok(Self::from_parts(dir, issues))
    }

    /// Test/injection constructor: build an app from a fixed issues vec
    /// without touching the filesystem. Issues are sorted here.
    #[allow(dead_code)] // used by the headless unit tests
    pub fn with_issues(dir: PathBuf, mut issues: Vec<Issue>) -> Self {
        sort_by_id(&mut issues);
        Self::from_parts(dir, issues)
    }

    fn from_parts(dir: PathBuf, issues: Vec<Issue>) -> Self {
        let mut app = App {
            dir,
            issues,
            filter: StatusFilter::Open,
            label_filter: None,
            search: None,
            searching: false,
            visible: Vec::new(),
            selected: 0,
            filter_selected: 0,
            focus: Panel::Issues,
            detail: None,
            labels_with_counts: Vec::new(),
            open_count: 0,
            closed_count: 0,
            all_count: 0,
            modal: None,
            status_line: String::from("Press ? for help"),
            show_help: false,
            should_quit: false,
        };
        app.recompute_counts();
        app.recompute_visible();
        app.refresh_detail();
        app
    }

    // -- derived data ------------------------------------------------------

    fn recompute_counts(&mut self) {
        self.all_count = self.issues.len();
        self.open_count = self.issues.iter().filter(|i| i.status == STATUS_OPEN).count();
        self.closed_count = self
            .issues
            .iter()
            .filter(|i| i.status == STATUS_CLOSED)
            .count();

        use std::collections::BTreeMap;
        let mut counts: BTreeMap<String, usize> = BTreeMap::new();
        for issue in &self.issues {
            for label in &issue.labels {
                *counts.entry(label.clone()).or_insert(0) += 1;
            }
        }
        self.labels_with_counts = counts.into_iter().collect();
    }

    fn status_matches(&self, issue: &Issue) -> bool {
        match self.filter {
            StatusFilter::Open => issue.status == STATUS_OPEN,
            StatusFilter::Closed => issue.status == STATUS_CLOSED,
            StatusFilter::All => true,
        }
    }

    /// Rebuilds `visible` from the active status filter, label filter, and
    /// case-insensitive title-substring search, then clamps selection while
    /// trying to keep the same issue selected.
    pub fn recompute_visible(&mut self) {
        let prev_id = self.selected_issue().map(|i| i.id);
        let search_lc = self.search.as_ref().map(|s| s.to_lowercase());

        self.visible = self
            .issues
            .iter()
            .enumerate()
            .filter(|(_, issue)| self.status_matches(issue))
            .filter(|(_, issue)| {
                self.label_filter
                    .as_ref()
                    .is_none_or(|l| issue.labels.iter().any(|x| x == l))
            })
            .filter(|(_, issue)| {
                search_lc
                    .as_ref()
                    .is_none_or(|q| issue.title.to_lowercase().contains(q))
            })
            .map(|(idx, _)| idx)
            .collect();

        // Try to restore selection on the previously selected issue.
        if let Some(id) = prev_id {
            if let Some(pos) = self
                .visible
                .iter()
                .position(|&idx| self.issues[idx].id == id)
            {
                self.selected = pos;
            }
        }
        self.clamp_selection();
    }

    fn clamp_selection(&mut self) {
        if self.visible.is_empty() {
            self.selected = 0;
        } else if self.selected >= self.visible.len() {
            self.selected = self.visible.len() - 1;
        }
    }

    /// The currently selected issue, if any.
    pub fn selected_issue(&self) -> Option<&Issue> {
        self.visible
            .get(self.selected)
            .and_then(|&idx| self.issues.get(idx))
    }

    /// Lazily (re)loads the detail body for the selected issue, only when the
    /// selected id differs from the cached one.
    pub fn refresh_detail(&mut self) {
        let Some(id) = self.selected_issue().map(|i| i.id) else {
            self.detail = None;
            return;
        };
        if self.detail.as_ref().map(|(cid, _)| *cid) == Some(id) {
            return;
        }
        let body = match storage::find_issue_by_id(&self.dir, id) {
            Ok(Some((_, content))) => body_after_frontmatter(&content),
            _ => String::new(),
        };
        self.detail = Some((id, body));
    }

    /// Reloads issues from disk, preserving the selected issue by id when
    /// possible, and refreshes counts and detail.
    pub fn reload(&mut self) {
        let prev_id = self.selected_issue().map(|i| i.id);
        match storage::load_issues(&self.dir) {
            Ok(mut issues) => {
                sort_by_id(&mut issues);
                self.issues = issues;
            }
            Err(e) => {
                self.status_line = format!("reload failed: {e}");
                return;
            }
        }
        self.recompute_counts();
        self.recompute_visible();
        // Restore selection by id if it still exists in the visible set.
        if let Some(id) = prev_id {
            if let Some(pos) = self
                .visible
                .iter()
                .position(|&idx| self.issues[idx].id == id)
            {
                self.selected = pos;
            }
        }
        // Force detail refresh in case the body changed for the same id.
        self.detail = None;
        self.refresh_detail();
    }

    // -- filter pane -------------------------------------------------------

    /// The ordered list of selectable filter rows.
    pub fn filter_rows(&self) -> Vec<FilterRow> {
        let mut rows = vec![
            FilterRow::Status(StatusFilter::Open),
            FilterRow::Status(StatusFilter::Closed),
            FilterRow::Status(StatusFilter::All),
        ];
        for (label, _) in &self.labels_with_counts {
            rows.push(FilterRow::Label(label.clone()));
        }
        rows
    }

    /// Applies the filter row at `filter_selected`.
    pub fn apply_selected_filter(&mut self) {
        let rows = self.filter_rows();
        let Some(row) = rows.get(self.filter_selected) else {
            return;
        };
        match row {
            FilterRow::Status(s) => {
                self.filter = *s;
                self.label_filter = None;
            }
            FilterRow::Label(l) => {
                self.label_filter = Some(l.clone());
            }
        }
        self.recompute_visible();
        self.refresh_detail();
    }

    // -- selection movement ------------------------------------------------

    /// Number of selectable rows in the focused list.
    fn focused_len(&self) -> usize {
        match self.focus {
            Panel::Filters => self.filter_rows().len(),
            _ => self.visible.len(),
        }
    }

    fn focused_pos(&mut self) -> &mut usize {
        match self.focus {
            Panel::Filters => &mut self.filter_selected,
            _ => &mut self.selected,
        }
    }

    pub fn move_down(&mut self) {
        let len = self.focused_len();
        if len == 0 {
            return;
        }
        let pos = self.focused_pos();
        if *pos + 1 < len {
            *pos += 1;
        }
        self.after_move();
    }

    pub fn move_up(&mut self) {
        let len = self.focused_len();
        if len == 0 {
            return;
        }
        let pos = self.focused_pos();
        if *pos > 0 {
            *pos -= 1;
        }
        self.after_move();
    }

    pub fn move_first(&mut self) {
        *self.focused_pos() = 0;
        self.after_move();
    }

    pub fn move_last(&mut self) {
        let len = self.focused_len();
        if len > 0 {
            *self.focused_pos() = len - 1;
        }
        self.after_move();
    }

    pub fn half_page_down(&mut self) {
        let len = self.focused_len();
        if len == 0 {
            return;
        }
        let pos = self.focused_pos();
        *pos = (*pos + 10).min(len - 1);
        self.after_move();
    }

    pub fn half_page_up(&mut self) {
        let pos = self.focused_pos();
        *pos = pos.saturating_sub(10);
        self.after_move();
    }

    fn after_move(&mut self) {
        if self.focus != Panel::Filters {
            self.refresh_detail();
        }
    }

    // -- focus -------------------------------------------------------------

    pub fn focus_next(&mut self) {
        self.focus = match self.focus {
            Panel::Filters => Panel::Issues,
            Panel::Issues => Panel::Detail,
            Panel::Detail => Panel::Filters,
        };
    }

    pub fn focus_prev(&mut self) {
        self.focus = match self.focus {
            Panel::Filters => Panel::Detail,
            Panel::Issues => Panel::Filters,
            Panel::Detail => Panel::Issues,
        };
    }

    // -- search ------------------------------------------------------------

    /// Enters search-input mode (starts an empty query).
    pub fn start_search(&mut self) {
        self.searching = true;
        self.search = Some(String::new());
        self.focus = Panel::Issues;
    }

    pub fn search_push(&mut self, c: char) {
        if let Some(q) = self.search.as_mut() {
            q.push(c);
            self.recompute_visible();
            self.refresh_detail();
        }
    }

    pub fn search_backspace(&mut self) {
        if let Some(q) = self.search.as_mut() {
            q.pop();
            self.recompute_visible();
            self.refresh_detail();
        }
    }

    /// Confirms the current query (leaves input mode, keeps filtering).
    pub fn confirm_search(&mut self) {
        self.searching = false;
        if self.search.as_deref() == Some("") {
            self.search = None;
        }
    }

    /// Clears the search entirely (query and input mode).
    pub fn clear_search(&mut self) {
        self.searching = false;
        self.search = None;
        self.recompute_visible();
        self.refresh_detail();
    }

    // -- mutations ---------------------------------------------------------

    /// Closes the selected issue.
    pub fn close_selected(&mut self) {
        self.set_selected_status(STATUS_CLOSED);
    }

    /// Reopens the selected issue.
    pub fn reopen_selected(&mut self) {
        self.set_selected_status(STATUS_OPEN);
    }

    fn set_selected_status(&mut self, status: &str) {
        let Some(id) = self.selected_issue().map(|i| i.id) else {
            self.status_line = "no issue selected".to_string();
            return;
        };
        match ops::set_status(&self.dir, id, status, now_secs()) {
            Ok(_) => {
                let verb = if status == STATUS_CLOSED { "closed" } else { "reopened" };
                self.status_line = format!("{verb} #{id}");
                self.reload();
            }
            Err(e) => self.status_line = format!("error: {e}"),
        }
    }

    /// Opens a blank create-form modal.
    pub fn open_create_form(&mut self) {
        self.modal = Some(Form::new_create());
    }

    /// Opens an edit-form modal prefilled from the selected issue.
    pub fn open_edit_form(&mut self) {
        match self.selected_issue() {
            Some(issue) => self.modal = Some(Form::from_issue(issue)),
            None => self.status_line = "no issue selected".to_string(),
        }
    }

    pub fn cancel_modal(&mut self) {
        self.modal = None;
    }

    /// Saves the active modal: create -> create_issue, edit -> edit_issue.
    pub fn submit_modal(&mut self) {
        let Some(form) = self.modal.take() else {
            return;
        };
        let now = now_secs();
        match form.mode {
            Mode::Create => {
                let new = NewIssue {
                    title: form.title.clone(),
                    labels: form.parsed_labels(),
                    status: form.status.clone(),
                    body: String::new(),
                };
                match ops::create_issue(&self.dir, new, now) {
                    Ok((issue, _)) => {
                        self.status_line = format!("created #{}", issue.id);
                        self.reload();
                        self.select_by_id(issue.id);
                    }
                    Err(e) => {
                        self.status_line = format!("error: {e}");
                        self.modal = Some(form);
                    }
                }
            }
            Mode::Edit => {
                let Some(id) = form.edit_id else {
                    return;
                };
                let edit = EditIssue {
                    title: Some(form.title.clone()),
                    status: Some(form.status.clone()),
                    add_labels: form.add_labels(),
                    remove_labels: form.remove_labels(),
                    body: None,
                };
                match ops::edit_issue(&self.dir, id, edit, now) {
                    Ok(_) => {
                        self.status_line = format!("updated #{id}");
                        self.reload();
                    }
                    Err(e) => {
                        self.status_line = format!("error: {e}");
                        self.modal = Some(form);
                    }
                }
            }
        }
    }

    /// Applies a freshly edited body (from $EDITOR) to the selected issue.
    pub fn apply_body_edit(&mut self, body: String) {
        let Some(id) = self.selected_issue().map(|i| i.id) else {
            return;
        };
        let edit = EditIssue {
            body: Some(body),
            ..Default::default()
        };
        match ops::edit_issue(&self.dir, id, edit, now_secs()) {
            Ok(_) => {
                self.status_line = format!("edited body #{id}");
                self.reload();
            }
            Err(e) => self.status_line = format!("error: {e}"),
        }
    }

    /// The body text to seed $EDITOR for the selected issue.
    pub fn selected_body(&self) -> String {
        self.detail.as_ref().map(|(_, b)| b.clone()).unwrap_or_default()
    }

    fn select_by_id(&mut self, id: i64) {
        if let Some(pos) = self
            .visible
            .iter()
            .position(|&idx| self.issues[idx].id == id)
        {
            self.selected = pos;
            self.refresh_detail();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(id: i64, status: &str, title: &str, labels: &[&str]) -> Issue {
        Issue {
            id,
            title: title.to_string(),
            status: status.to_string(),
            created: "2026-06-14".to_string(),
            updated: "2026-06-14".to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
        }
    }

    fn sample() -> App {
        let issues = vec![
            mk(1, "open", "Add login", &["cli"]),
            mk(2, "closed", "Fix bug", &["bug"]),
            mk(3, "open", "Improve docs", &["docs", "cli"]),
            mk(4, "open", "Login redesign", &[]),
        ];
        App::with_issues(PathBuf::from("/nonexistent"), issues)
    }

    #[test]
    fn default_filter_shows_only_open() {
        let app = sample();
        let ids: Vec<i64> = app.visible.iter().map(|&i| app.issues[i].id).collect();
        assert_eq!(ids, vec![1, 3, 4]);
    }

    #[test]
    fn closed_filter_shows_only_closed() {
        let mut app = sample();
        app.filter = StatusFilter::Closed;
        app.recompute_visible();
        let ids: Vec<i64> = app.visible.iter().map(|&i| app.issues[i].id).collect();
        assert_eq!(ids, vec![2]);
    }

    #[test]
    fn all_filter_shows_everything() {
        let mut app = sample();
        app.filter = StatusFilter::All;
        app.recompute_visible();
        assert_eq!(app.visible.len(), 4);
    }

    #[test]
    fn label_filter_intersects_with_status() {
        let mut app = sample();
        app.filter = StatusFilter::All;
        app.label_filter = Some("cli".to_string());
        app.recompute_visible();
        let ids: Vec<i64> = app.visible.iter().map(|&i| app.issues[i].id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn search_is_case_insensitive_substring_on_title() {
        let mut app = sample();
        app.filter = StatusFilter::All;
        app.search = Some("login".to_string());
        app.recompute_visible();
        let ids: Vec<i64> = app.visible.iter().map(|&i| app.issues[i].id).collect();
        assert_eq!(ids, vec![1, 4]);
    }

    #[test]
    fn search_composes_with_status_filter() {
        let mut app = sample();
        // default Open filter + "login"
        app.search = Some("LOGIN".to_string());
        app.recompute_visible();
        let ids: Vec<i64> = app.visible.iter().map(|&i| app.issues[i].id).collect();
        assert_eq!(ids, vec![1, 4]);
    }

    #[test]
    fn selection_clamps_when_filter_shrinks_visible() {
        let mut app = sample();
        app.filter = StatusFilter::All;
        app.recompute_visible();
        app.selected = 3; // last of 4
        app.filter = StatusFilter::Closed; // only 1 visible
        app.recompute_visible();
        assert_eq!(app.selected, 0);
    }

    #[test]
    fn move_down_up_respects_bounds() {
        let mut app = sample(); // 3 visible (open)
        app.focus = Panel::Issues;
        assert_eq!(app.selected, 0);
        app.move_up(); // can't go below 0
        assert_eq!(app.selected, 0);
        app.move_down();
        app.move_down();
        assert_eq!(app.selected, 2);
        app.move_down(); // can't exceed last
        assert_eq!(app.selected, 2);
    }

    #[test]
    fn move_on_empty_list_does_not_panic() {
        let mut app = App::with_issues(PathBuf::from("/x"), vec![]);
        app.focus = Panel::Issues;
        app.move_down();
        app.move_up();
        app.move_last();
        app.half_page_down();
        assert_eq!(app.selected, 0);
        assert!(app.selected_issue().is_none());
    }

    #[test]
    fn recompute_preserves_selection_by_id() {
        let mut app = sample();
        app.filter = StatusFilter::All;
        app.recompute_visible();
        app.selected = 2; // issue id 3
        assert_eq!(app.selected_issue().unwrap().id, 3);
        // Re-run recompute; selection should still point at id 3.
        app.recompute_visible();
        assert_eq!(app.selected_issue().unwrap().id, 3);
    }

    #[test]
    fn confirm_empty_search_clears_query() {
        let mut app = sample();
        app.start_search();
        assert!(app.searching);
        app.confirm_search();
        assert!(!app.searching);
        assert_eq!(app.search, None);
    }

    #[test]
    fn confirm_nonempty_search_keeps_query() {
        let mut app = sample();
        app.start_search();
        app.search_push('a');
        app.confirm_search();
        assert!(!app.searching);
        assert_eq!(app.search.as_deref(), Some("a"));
    }

    #[test]
    fn filter_rows_lists_statuses_then_labels() {
        let app = sample();
        let rows = app.filter_rows();
        assert_eq!(rows[0], FilterRow::Status(StatusFilter::Open));
        assert_eq!(rows[1], FilterRow::Status(StatusFilter::Closed));
        assert_eq!(rows[2], FilterRow::Status(StatusFilter::All));
        // labels sorted: bug, cli, docs
        assert_eq!(rows[3], FilterRow::Label("bug".to_string()));
        assert_eq!(rows[4], FilterRow::Label("cli".to_string()));
        assert_eq!(rows[5], FilterRow::Label("docs".to_string()));
    }

    #[test]
    fn apply_label_filter_row_sets_label_filter() {
        let mut app = sample();
        app.focus = Panel::Filters;
        app.filter_selected = 4; // cli
        app.apply_selected_filter();
        assert_eq!(app.label_filter.as_deref(), Some("cli"));
    }

    #[test]
    fn counts_are_computed() {
        let app = sample();
        assert_eq!(app.open_count, 3);
        assert_eq!(app.closed_count, 1);
        assert_eq!(app.all_count, 4);
        assert_eq!(app.labels_with_counts.len(), 3); // bug, cli, docs
    }

    #[test]
    fn focus_cycles_both_directions() {
        let mut app = sample();
        app.focus = Panel::Filters;
        app.focus_next();
        assert_eq!(app.focus, Panel::Issues);
        app.focus_next();
        assert_eq!(app.focus, Panel::Detail);
        app.focus_next();
        assert_eq!(app.focus, Panel::Filters);
        app.focus_prev();
        assert_eq!(app.focus, Panel::Detail);
    }
}
