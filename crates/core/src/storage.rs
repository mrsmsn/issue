//! Filesystem-facing layer: issue-dir resolution, scanning `*.md`,
//! reading frontmatter concurrently, writing new issue files. The pure
//! logic lives in [`crate::core`]; this module only handles I/O and wires
//! data into those functions.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use crate::core::{self, Issue};

/// Resolves the issue directory: env var `ISSUE_DIR` when set and
/// non-empty, otherwise `./issue`.
pub fn resolve_issue_dir() -> PathBuf {
    match std::env::var("ISSUE_DIR") {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from("issue"),
    }
}

/// Lists the paths of all `*.md` files directly inside `dir` (non-recursive).
/// Returns an empty vec when the directory does not exist.
pub fn list_md_files(dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    let rd = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(paths),
        Err(e) => return Err(e),
    };
    for entry in rd {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("md") {
            // Skip README.md: it is not an issue file.
            if path.file_name().and_then(|s| s.to_str()) == Some("README.md") {
                continue;
            }
            if entry.file_type()?.is_file() {
                paths.push(path);
            }
        }
    }
    Ok(paths)
}

/// Reads and parses frontmatter for every issue file in `dir`, in parallel.
///
/// A thread pool sized to [`thread::available_parallelism`] reads files
/// concurrently; each worker reads a file and parses only its
/// frontmatter (stopping at the closing fence). Files that fail to read or
/// have no frontmatter are skipped. The returned vector is unordered;
/// callers sort as needed.
pub fn load_issues(dir: &Path) -> io::Result<Vec<Issue>> {
    let paths = list_md_files(dir)?;
    Ok(parse_paths_parallel(paths))
}

/// Like [`load_issues`] but also returns the source filename alongside
/// each parsed issue. Used by `lint` to report conflicting filenames.
pub fn load_issues_with_files(dir: &Path) -> io::Result<Vec<(Issue, String)>> {
    let paths = list_md_files(dir)?;
    Ok(parse_paths_parallel_with_files(paths))
}

/// Reads every issue file in `dir`, returning each parsed issue paired with
/// its markdown body (everything after the closing frontmatter fence). Used by
/// `export`. Files that fail to read or lack frontmatter are skipped. The
/// returned vector is unordered; callers sort as needed.
pub fn load_issues_with_bodies(dir: &Path) -> io::Result<Vec<(Issue, String)>> {
    let mut out = Vec::new();
    for path in list_md_files(dir)? {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(issue) = core::parse_frontmatter(&content) {
            let body = core::body_after_frontmatter(&content);
            out.push((issue, body));
        }
    }
    Ok(out)
}

fn worker_count(n_items: usize) -> usize {
    let avail = thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    avail.min(n_items).max(1)
}

/// Reads + parses the given paths concurrently, returning parsed issues.
fn parse_paths_parallel(paths: Vec<PathBuf>) -> Vec<Issue> {
    parse_generic(paths, |_path, issue| issue)
}

/// Reads + parses the given paths concurrently, returning (issue, filename).
fn parse_paths_parallel_with_files(paths: Vec<PathBuf>) -> Vec<(Issue, String)> {
    parse_generic(paths, |path, issue| {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        (issue, name)
    })
}

/// Shared concurrent read+parse engine. `map` turns a parsed
/// (path, issue) into the desired result item.
fn parse_generic<T, F>(paths: Vec<PathBuf>, map: F) -> Vec<T>
where
    T: Send,
    F: Fn(&Path, Issue) -> T + Send + Sync,
{
    if paths.is_empty() {
        return Vec::new();
    }

    let n = worker_count(paths.len());
    let total = paths.len();
    let (tx, rx) = mpsc::channel::<(usize, T)>();

    // Static chunking: contiguous slices keep memory locality and avoid
    // per-item locking. Index pairs preserve nothing about order (caller
    // sorts), but we keep the original index so output is deterministic
    // before any sort if a caller wants it.
    let chunk = total.div_ceil(n);

    thread::scope(|scope| {
        let map_ref = &map;
        for (w, chunk_paths) in paths.chunks(chunk).enumerate() {
            let tx = tx.clone();
            let base = w * chunk;
            scope.spawn(move || {
                for (j, path) in chunk_paths.iter().enumerate() {
                    if let Ok(content) = fs::read_to_string(path) {
                        if let Some(issue) = core::parse_frontmatter(&content) {
                            let item = map_ref(path, issue);
                            // Ignore send errors (receiver always alive here).
                            let _ = tx.send((base + j, item));
                        }
                    }
                }
            });
        }
        drop(tx); // close the original sender so rx ends after workers finish.

        let mut collected: Vec<(usize, T)> = rx.iter().collect();
        collected.sort_by_key(|(idx, _)| *idx);
        collected.into_iter().map(|(_, item)| item).collect()
    })
}

/// Finds the issue file whose frontmatter id == `id`. Returns the path and
/// the full file contents. `None` when not found.
pub fn find_issue_by_id(dir: &Path, id: i64) -> io::Result<Option<(PathBuf, String)>> {
    for path in list_md_files(dir)? {
        let content = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if let Some(issue) = core::parse_frontmatter(&content) {
            if issue.id == id {
                return Ok(Some((path, content)));
            }
        }
    }
    Ok(None)
}

/// Serializes an issue to its full Markdown-with-frontmatter file content.
/// Schema is GitHub-aligned: `status` is open/closed, categorization is via
/// `labels`, and cross-references live in the body (no `type`/`related`).
pub fn render_issue_file(issue: &Issue, body: &str) -> String {
    let labels = issue.labels.join(", ");
    let mut out = String::new();
    out.push_str("---\n");
    out.push_str(&format!("id: {}\n", issue.id));
    out.push_str(&format!("title: \"{}\"\n", escape_double_quotes(&issue.title)));
    out.push_str(&format!("status: {}\n", issue.status));
    out.push_str(&format!("created: {}\n", issue.created));
    out.push_str(&format!("updated: {}\n", issue.updated));
    out.push_str(&format!("labels: [{labels}]\n"));
    out.push_str("---\n");
    if !body.is_empty() {
        out.push('\n');
        out.push_str(body);
        if !body.ends_with('\n') {
            out.push('\n');
        }
    }
    out
}

fn escape_double_quotes(s: &str) -> String {
    s.replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    /// Creates a unique temp directory for a test and returns its path.
    fn unique_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("issue-test-{tag}-{pid}-{nanos}-{n}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[test]
    fn resolve_uses_env_when_set() {
        // Note: env mutation; isolated key value, restored after.
        let key = "ISSUE_DIR";
        let prev = std::env::var(key).ok();
        std::env::set_var(key, "/tmp/custom-issue-dir");
        assert_eq!(resolve_issue_dir(), PathBuf::from("/tmp/custom-issue-dir"));
        std::env::set_var(key, "");
        assert_eq!(resolve_issue_dir(), PathBuf::from("issue"));
        match prev {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }

    #[test]
    fn list_md_skips_readme_and_nonmd() {
        let dir = unique_dir("listmd");
        write(&dir, "1-a.md", "---\nid: 1\n---\n");
        write(&dir, "README.md", "readme");
        write(&dir, "notes.txt", "txt");
        let mut names: Vec<String> = list_md_files(&dir)
            .unwrap()
            .iter()
            .map(|p| p.file_name().unwrap().to_str().unwrap().to_string())
            .collect();
        names.sort();
        assert_eq!(names, vec!["1-a.md"]);
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn list_md_missing_dir_is_empty() {
        let dir = std::env::temp_dir().join("issue-test-definitely-missing-xyz-0");
        assert!(list_md_files(&dir).unwrap().is_empty());
    }

    #[test]
    fn load_issues_parses_all_concurrently() {
        let dir = unique_dir("load");
        for id in 1..=20 {
            write(
                &dir,
                &format!("{id}-t.md"),
                &format!("---\nid: {id}\ntitle: t{id}\nstatus: open\nlabels: [x]\n---\nbody\n"),
            );
        }
        let mut issues = load_issues(&dir).unwrap();
        crate::core::sort_by_id(&mut issues);
        let ids: Vec<i64> = issues.iter().map(|i| i.id).collect();
        assert_eq!(ids, (1..=20).collect::<Vec<i64>>());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn find_by_id_returns_content() {
        let dir = unique_dir("find");
        write(&dir, "1-a.md", "---\nid: 1\ntitle: one\n---\nfirst\n");
        write(&dir, "2-b.md", "---\nid: 2\ntitle: two\n---\nsecond\n");
        let (path, content) = find_issue_by_id(&dir, 2).unwrap().unwrap();
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "2-b.md");
        assert!(content.contains("second"));
        assert!(find_issue_by_id(&dir, 999).unwrap().is_none());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn render_roundtrips_through_parser() {
        let issue = Issue {
            id: 12,
            title: "A \"tricky\" title".to_string(),
            status: "open".to_string(),
            created: "2026-06-14".to_string(),
            updated: "2026-06-14".to_string(),
            labels: vec!["cli".to_string(), "mvp".to_string()],
        };
        let text = render_issue_file(&issue, "Hello body");
        let parsed = core::parse_frontmatter(&text).unwrap();
        assert_eq!(parsed.id, 12);
        assert_eq!(parsed.title, "A \"tricky\" title");
        assert_eq!(parsed.status, "open");
        assert_eq!(parsed.labels, vec!["cli", "mvp"]);
        assert!(text.contains("Hello body"));
    }
}
