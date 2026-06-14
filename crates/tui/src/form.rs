//! Modal form state for creating and editing issues. Pure state: no ratatui
//! types, so it is unit-testable headless.

use issue_core::core::{Issue, STATUS_CLOSED, STATUS_OPEN};

/// Which form field currently has focus. Ordered for `Tab` cycling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Title,
    Labels,
    Status,
}

const FIELD_ORDER: [Field; 3] = [Field::Title, Field::Labels, Field::Status];

/// Whether the form creates a new issue or edits an existing one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Create,
    Edit,
}

/// Modal form state. `title` and `labels` are free text (labels is a
/// comma-separated string); `status` is a toggle between open/closed.
#[derive(Debug, Clone)]
pub struct Form {
    pub mode: Mode,
    /// Set when editing: the id of the issue being edited.
    pub edit_id: Option<i64>,
    /// Original labels of the edited issue (for add/remove diffing).
    pub original_labels: Vec<String>,
    pub title: String,
    pub labels: String,
    pub status: String,
    pub focus: Field,
}

impl Form {
    /// A blank create form (status defaults to open).
    pub fn new_create() -> Self {
        Form {
            mode: Mode::Create,
            edit_id: None,
            original_labels: Vec::new(),
            title: String::new(),
            labels: String::new(),
            status: STATUS_OPEN.to_string(),
            focus: Field::Title,
        }
    }

    /// An edit form prefilled from an existing issue.
    pub fn from_issue(issue: &Issue) -> Self {
        Form {
            mode: Mode::Edit,
            edit_id: Some(issue.id),
            original_labels: issue.labels.clone(),
            title: issue.title.clone(),
            labels: issue.labels.join(", "),
            status: issue.status.clone(),
            focus: Field::Title,
        }
    }

    /// The title for the popup, e.g. "New issue" / "Edit #3".
    pub fn heading(&self) -> String {
        match self.mode {
            Mode::Create => "New issue".to_string(),
            Mode::Edit => format!("Edit #{}", self.edit_id.unwrap_or(0)),
        }
    }

    /// Moves focus to the next field (wraps).
    pub fn focus_next(&mut self) {
        let idx = FIELD_ORDER.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = FIELD_ORDER[(idx + 1) % FIELD_ORDER.len()];
    }

    /// Moves focus to the previous field (wraps).
    pub fn focus_prev(&mut self) {
        let idx = FIELD_ORDER.iter().position(|f| *f == self.focus).unwrap_or(0);
        self.focus = FIELD_ORDER[(idx + FIELD_ORDER.len() - 1) % FIELD_ORDER.len()];
    }

    /// Appends a printable char to the focused text field. No-op on the
    /// status field (which is toggled, not typed).
    pub fn input_char(&mut self, c: char) {
        match self.focus {
            Field::Title => self.title.push(c),
            Field::Labels => self.labels.push(c),
            Field::Status => {}
        }
    }

    /// Deletes the last char of the focused text field.
    pub fn backspace(&mut self) {
        match self.focus {
            Field::Title => {
                self.title.pop();
            }
            Field::Labels => {
                self.labels.pop();
            }
            Field::Status => {}
        }
    }

    /// Toggles the status between open and closed (used by Space on the
    /// status field, and also as a generic toggle).
    pub fn toggle_status(&mut self) {
        self.status = if self.status == STATUS_OPEN {
            STATUS_CLOSED.to_string()
        } else {
            STATUS_OPEN.to_string()
        };
    }

    /// Parses the comma-separated labels field into trimmed, non-empty labels.
    pub fn parsed_labels(&self) -> Vec<String> {
        self.labels
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect()
    }

    /// For an edit: labels present in the form but not in the original issue.
    pub fn add_labels(&self) -> Vec<String> {
        let parsed = self.parsed_labels();
        parsed
            .into_iter()
            .filter(|l| !self.original_labels.iter().any(|o| o == l))
            .collect()
    }

    /// For an edit: labels present in the original issue but not in the form.
    pub fn remove_labels(&self) -> Vec<String> {
        let parsed = self.parsed_labels();
        self.original_labels
            .iter()
            .filter(|o| !parsed.iter().any(|l| l == *o))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue(id: i64, labels: &[&str]) -> Issue {
        Issue {
            id,
            title: "orig".to_string(),
            status: STATUS_OPEN.to_string(),
            created: "2026-06-14".to_string(),
            updated: "2026-06-14".to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parsed_labels_splits_and_trims() {
        let mut f = Form::new_create();
        f.labels = " cli ,  mvp,, perf ".to_string();
        assert_eq!(f.parsed_labels(), vec!["cli", "mvp", "perf"]);
    }

    #[test]
    fn parsed_labels_empty_is_empty() {
        let f = Form::new_create();
        assert!(f.parsed_labels().is_empty());
    }

    #[test]
    fn toggle_status_flips() {
        let mut f = Form::new_create();
        assert_eq!(f.status, STATUS_OPEN);
        f.toggle_status();
        assert_eq!(f.status, STATUS_CLOSED);
        f.toggle_status();
        assert_eq!(f.status, STATUS_OPEN);
    }

    #[test]
    fn input_and_backspace_on_focused_field() {
        let mut f = Form::new_create();
        f.focus = Field::Title;
        f.input_char('h');
        f.input_char('i');
        assert_eq!(f.title, "hi");
        f.backspace();
        assert_eq!(f.title, "h");
        // status field ignores typing.
        f.focus = Field::Status;
        f.input_char('x');
        assert_eq!(f.title, "h");
    }

    #[test]
    fn focus_cycles_both_ways() {
        let mut f = Form::new_create();
        assert_eq!(f.focus, Field::Title);
        f.focus_next();
        assert_eq!(f.focus, Field::Labels);
        f.focus_next();
        assert_eq!(f.focus, Field::Status);
        f.focus_next();
        assert_eq!(f.focus, Field::Title);
        f.focus_prev();
        assert_eq!(f.focus, Field::Status);
    }

    #[test]
    fn edit_label_diff_computes_add_and_remove() {
        let f = {
            let mut f = Form::from_issue(&issue(3, &["cli", "mvp"]));
            f.labels = "cli, perf".to_string();
            f
        };
        assert_eq!(f.add_labels(), vec!["perf"]);
        assert_eq!(f.remove_labels(), vec!["mvp"]);
    }

    #[test]
    fn edit_label_diff_no_change_is_empty() {
        let f = Form::from_issue(&issue(1, &["a", "b"]));
        assert!(f.add_labels().is_empty());
        assert!(f.remove_labels().is_empty());
    }

    #[test]
    fn from_issue_prefills_fields() {
        let f = Form::from_issue(&issue(5, &["x", "y"]));
        assert_eq!(f.mode, Mode::Edit);
        assert_eq!(f.edit_id, Some(5));
        assert_eq!(f.title, "orig");
        assert_eq!(f.labels, "x, y");
        assert_eq!(f.heading(), "Edit #5");
    }
}
