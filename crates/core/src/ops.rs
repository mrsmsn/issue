//! Service layer: the create/edit/close-reopen mutations, composed from
//! [`crate::core`] (pure logic) and [`crate::storage`] (I/O). Both the `issue`
//! CLI and the `lazyissue` TUI call these so the orchestration lives in exactly
//! one place. Functions take `now` (unix seconds) rather than reading the clock,
//! so they are deterministic and unit-testable.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::core::{
    self, apply_label_changes, format_date, is_valid_status, next_id, parse_frontmatter,
    replace_body, slug, update_frontmatter, Issue,
};
use crate::storage;

/// Fields for creating a new issue. `status` defaults are the caller's job.
pub struct NewIssue {
    pub title: String,
    pub labels: Vec<String>,
    pub status: String,
    pub body: String,
}

/// Fields for an in-place edit. `None` / empty means "leave unchanged".
#[derive(Default)]
pub struct EditIssue {
    pub title: Option<String>,
    pub status: Option<String>,
    pub add_labels: Vec<String>,
    pub remove_labels: Vec<String>,
    pub body: Option<String>,
}

/// Errors a mutation can fail with. `Display` matches the strings the CLI has
/// always printed, so callers can `eprintln!("error: {e}")` unchanged.
#[derive(Debug)]
pub enum OpError {
    NotFound(i64),
    InvalidStatus(String),
    Malformed,
    Io(io::Error),
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpError::NotFound(id) => write!(f, "no issue found with id {id}"),
            OpError::InvalidStatus(s) => write!(
                f,
                "invalid status '{s}' (allowed: {})",
                core::VALID_STATUSES.join(", ")
            ),
            OpError::Malformed => write!(f, "issue file has malformed frontmatter"),
            OpError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for OpError {}

impl From<io::Error> for OpError {
    fn from(e: io::Error) -> Self {
        OpError::Io(e)
    }
}

/// The on-disk filename for an issue: `<id>-<slug>.md`, or `<id>.md` when the
/// title yields an empty slug. Shared by create and import.
pub fn issue_filename(id: i64, title: &str) -> String {
    let s = slug(title);
    if s.is_empty() {
        format!("{id}.md")
    } else {
        format!("{id}-{s}.md")
    }
}

/// Creates a new issue file in `dir`, allocating `id = max(existing) + 1`.
/// Creates the directory if needed. Returns the new issue and its path.
pub fn create_issue(dir: &Path, new: NewIssue, now: i64) -> Result<(Issue, PathBuf), OpError> {
    if !is_valid_status(&new.status) {
        return Err(OpError::InvalidStatus(new.status));
    }
    fs::create_dir_all(dir)?;
    let existing = storage::load_issues(dir)?;
    let ids: Vec<i64> = existing.iter().map(|i| i.id).collect();
    let id = next_id(&ids);

    let today = format_date(now);
    let issue = Issue {
        id,
        title: new.title,
        status: new.status,
        created: today.clone(),
        updated: today,
        labels: new.labels,
    };
    let path = dir.join(issue_filename(id, &issue.title));
    fs::write(&path, storage::render_issue_file(&issue, &new.body))?;
    Ok((issue, path))
}

/// Sets `status` on the issue with `id` and bumps `updated`. The file is never
/// renamed. Returns the file path. `status` is assumed valid (callers pass the
/// open/closed constants).
pub fn set_status(dir: &Path, id: i64, status: &str, now: i64) -> Result<PathBuf, OpError> {
    let (path, content) = storage::find_issue_by_id(dir, id)?.ok_or(OpError::NotFound(id))?;
    let updated = format_date(now);
    let new = update_frontmatter(
        &content,
        &[("status", status.to_string()), ("updated", updated)],
    )
    .ok_or(OpError::Malformed)?;
    fs::write(&path, new)?;
    Ok(path)
}

/// Applies an in-place edit to the issue with `id`, bumping `updated`. Unknown
/// frontmatter keys and the body (unless `body` is set) are preserved; the file
/// is never renamed. Returns the file path.
pub fn edit_issue(dir: &Path, id: i64, edit: EditIssue, now: i64) -> Result<PathBuf, OpError> {
    if let Some(s) = &edit.status {
        if !is_valid_status(s) {
            return Err(OpError::InvalidStatus(s.clone()));
        }
    }
    let (path, content) = storage::find_issue_by_id(dir, id)?.ok_or(OpError::NotFound(id))?;
    let issue = parse_frontmatter(&content).ok_or(OpError::Malformed)?;

    let mut updates: Vec<(&str, String)> = Vec::new();
    if let Some(t) = &edit.title {
        updates.push(("title", quote(t)));
    }
    if let Some(s) = &edit.status {
        updates.push(("status", s.clone()));
    }
    if !edit.add_labels.is_empty() || !edit.remove_labels.is_empty() {
        let labels = apply_label_changes(&issue.labels, &edit.add_labels, &edit.remove_labels);
        updates.push(("labels", format!("[{}]", labels.join(", "))));
    }
    updates.push(("updated", format_date(now)));

    let mut out = update_frontmatter(&content, &updates).ok_or(OpError::Malformed)?;
    if let Some(b) = &edit.body {
        out = replace_body(&out, b).ok_or(OpError::Malformed)?;
    }
    fs::write(&path, out)?;
    Ok(path)
}

/// Renders a frontmatter scalar value, quoting and escaping like a title.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    // 2026-06-14T00:00:00Z.
    const NOW: i64 = 1_781_395_200;

    fn tmp(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let d = std::env::temp_dir().join(format!("ops-test-{tag}-{}-{nanos}-{n}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn new(title: &str) -> NewIssue {
        NewIssue {
            title: title.to_string(),
            labels: vec![],
            status: "open".to_string(),
            body: String::new(),
        }
    }

    #[test]
    fn create_allocates_sequential_ids() {
        let d = tmp("create");
        let (a, _) = create_issue(&d, new("First"), NOW).unwrap();
        let (b, p) = create_issue(&d, new("Second"), NOW).unwrap();
        assert_eq!(a.id, 1);
        assert_eq!(b.id, 2);
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), "2-second.md");
        let parsed = parse_frontmatter(&fs::read_to_string(&p).unwrap()).unwrap();
        assert_eq!(parsed.created, "2026-06-14");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn create_empty_slug_falls_back_to_id() {
        let d = tmp("slug");
        let (i, p) = create_issue(&d, new("!!! ???"), NOW).unwrap();
        assert_eq!(p.file_name().unwrap().to_str().unwrap(), format!("{}.md", i.id));
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn create_rejects_invalid_status() {
        let d = tmp("badstatus");
        let mut n = new("x");
        n.status = "in-progress".to_string();
        match create_issue(&d, n, NOW) {
            Err(OpError::InvalidStatus(s)) => assert_eq!(s, "in-progress"),
            other => panic!("expected InvalidStatus, got {other:?}"),
        }
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn set_status_preserves_body_and_unknown_keys() {
        let d = tmp("status");
        let path = d.join("1-x.md");
        fs::write(
            &path,
            "---\nid: 1\ntitle: \"x\"\nstatus: open\npriority: P1\ncreated: 2026-01-01\nupdated: 2026-01-01\nlabels: [cli]\n---\n\n## Body\nkeep\n",
        )
        .unwrap();
        set_status(&d, 1, "closed", NOW).unwrap();
        let s = fs::read_to_string(&path).unwrap();
        assert!(s.contains("status: closed"));
        assert!(s.contains("updated: 2026-06-14"));
        assert!(s.contains("priority: P1")); // unknown key survives
        assert!(s.contains("## Body\nkeep"));
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn set_status_not_found() {
        let d = tmp("nf");
        match set_status(&d, 99, "closed", NOW) {
            Err(OpError::NotFound(99)) => {}
            other => panic!("expected NotFound, got {other:?}"),
        }
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn edit_applies_partial_changes() {
        let d = tmp("edit");
        let (i, _) = create_issue(
            &d,
            NewIssue {
                title: "orig".into(),
                labels: vec!["cli".into(), "mvp".into()],
                status: "open".into(),
                body: "old".into(),
            },
            NOW,
        )
        .unwrap();
        let path = edit_issue(
            &d,
            i.id,
            EditIssue {
                title: Some("renamed".into()),
                add_labels: vec!["perf".into()],
                remove_labels: vec!["mvp".into()],
                body: Some("new body".into()),
                ..Default::default()
            },
            NOW,
        )
        .unwrap();
        let parsed = parse_frontmatter(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(parsed.title, "renamed");
        assert_eq!(parsed.labels, vec!["cli", "perf"]);
        let body = crate::core::body_after_frontmatter(&fs::read_to_string(&path).unwrap());
        assert_eq!(body, "new body");
        fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn edit_rejects_invalid_status() {
        let d = tmp("editbad");
        let (i, _) = create_issue(&d, new("x"), NOW).unwrap();
        let r = edit_issue(
            &d,
            i.id,
            EditIssue {
                status: Some("wontfix".into()),
                ..Default::default()
            },
            NOW,
        );
        assert!(matches!(r, Err(OpError::InvalidStatus(_))));
        fs::remove_dir_all(&d).ok();
    }
}
