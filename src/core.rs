//! Pure, I/O-free logic of the issue CLI: frontmatter parsing, slug
//! generation, id allocation, sorting, filtering and duplicate (lint)
//! detection. Keeping it free of filesystem access makes it
//! straightforward to unit-test.

/// Valid status values for an issue.
pub const STATUS_OPEN: &str = "open";
pub const STATUS_CLOSED: &str = "closed";
pub const STATUS_IN_PROGRESS: &str = "in-progress";
pub const STATUS_WONTFIX: &str = "wontfix";

/// All allowed status values.
pub const VALID_STATUSES: [&str; 4] =
    [STATUS_OPEN, STATUS_CLOSED, STATUS_IN_PROGRESS, STATUS_WONTFIX];

/// Reports whether `s` is one of the allowed status values.
pub fn is_valid_status(s: &str) -> bool {
    VALID_STATUSES.contains(&s)
}

/// Parsed frontmatter of a single issue file. The body is intentionally
/// not stored here: list/parse only needs the frontmatter, and the body
/// is read separately by `view`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Issue {
    pub id: i64,
    pub title: String,
    pub status: String,
    pub r#type: String,
    pub created: String,
    pub updated: String,
    pub labels: Vec<String>,
    pub related: Vec<i64>,
}

/// Converts a title into a filename-safe slug:
///   - lowercased
///   - every run of non-alphanumeric characters collapses to a single "-"
///   - leading/trailing "-" trimmed
///
/// "Alphanumeric" is judged by Unicode letter/digit (via
/// [`char::is_alphanumeric`]), so accented or non-ASCII letters are
/// preserved (lowercased) rather than dropped. This mirrors the Go
/// implementation for a fair comparison.
pub fn slug(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut prev_dash = false;
    for c in title.chars() {
        if c.is_alphanumeric() {
            // to_lowercase can expand to multiple chars (e.g. 'İ'); push all.
            for lc in c.to_lowercase() {
                out.push(lc);
            }
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

/// Allocates the next id as `max(existing ids) + 1`, or `1` when there
/// are no existing ids. Gaps are preserved (never reused).
pub fn next_id(existing: &[i64]) -> i64 {
    existing.iter().copied().max().map_or(1, |m| m + 1)
}

// ---------------------------------------------------------------------------
// Frontmatter parsing
// ---------------------------------------------------------------------------

/// Parses YAML frontmatter from the start of a file's content. Only the
/// region between the first pair of `---` fences is parsed; parsing stops
/// at the closing fence. Returns `None` when there is no leading `---`
/// fence (i.e. the file is not an issue file).
///
/// This is a deliberately minimal parser. It handles exactly the keys the
/// schema defines: `id`, `title`, `status`, `type`, `created`, `updated`,
/// `labels` and `related`. Surrounding single/double quotes are stripped
/// from scalar string values. List values use the inline `[a, b]` form
/// (and the empty `[]` form).
pub fn parse_frontmatter(content: &str) -> Option<Issue> {
    let mut lines = content.lines();

    // The first non-empty content must be the opening fence.
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }

    let mut issue = Issue::default();
    for line in lines {
        if line.trim_end() == "---" {
            break; // closing fence: stop, ignore the markdown body.
        }
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some(colon) = trimmed.find(':') else {
            continue;
        };
        let key = trimmed[..colon].trim();
        let raw_value = trimmed[colon + 1..].trim();
        match key {
            "id" => {
                if let Ok(n) = strip_quotes(raw_value).parse::<i64>() {
                    issue.id = n;
                }
            }
            "title" => issue.title = strip_quotes(raw_value).to_string(),
            "status" => issue.status = strip_quotes(raw_value).to_string(),
            "type" => issue.r#type = strip_quotes(raw_value).to_string(),
            "created" => issue.created = strip_quotes(raw_value).to_string(),
            "updated" => issue.updated = strip_quotes(raw_value).to_string(),
            "labels" => issue.labels = parse_string_list(raw_value),
            "related" => {
                issue.related = parse_string_list(raw_value)
                    .iter()
                    .filter_map(|s| s.parse::<i64>().ok())
                    .collect();
            }
            _ => {}
        }
    }
    Some(issue)
}

/// Strips a single pair of matching surrounding quotes (single or double).
fn strip_quotes(s: &str) -> &str {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return &s[1..s.len() - 1];
        }
    }
    s
}

/// Parses an inline YAML list `[a, b, c]` (or `[]`) into trimmed,
/// quote-stripped string elements. Empty elements are dropped.
fn parse_string_list(raw: &str) -> Vec<String> {
    let inner = raw.strip_prefix('[').and_then(|s| s.strip_suffix(']'));
    let Some(inner) = inner else {
        return Vec::new();
    };
    inner
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| strip_quotes(s).to_string())
        .collect()
}

// ---------------------------------------------------------------------------
// Sort / filter / lint
// ---------------------------------------------------------------------------

/// Sorts issues by id ascending (numeric), in place.
pub fn sort_by_id(issues: &mut [Issue]) {
    issues.sort_by(|a, b| a.id.cmp(&b.id));
}

/// Returns the issues matching the optional status and label filters.
/// `None` means "no filter on this dimension".
pub fn filter_issues<'a>(
    issues: &'a [Issue],
    status: Option<&str>,
    label: Option<&str>,
) -> Vec<&'a Issue> {
    issues
        .iter()
        .filter(|i| status.is_none_or(|s| i.status == s))
        .filter(|i| label.is_none_or(|l| i.labels.iter().any(|x| x == l)))
        .collect()
}

/// Formats one `list` output line for an issue:
/// `<id>\t<status>\t<title>\t<labels-joined-by-comma>`.
/// (No trailing newline; the caller adds it.)
pub fn format_list_line(issue: &Issue) -> String {
    format!(
        "{}\t{}\t{}\t{}",
        issue.id,
        issue.status,
        issue.title,
        issue.labels.join(",")
    )
}

/// A detected duplicate id with the filenames that carry it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Duplicate {
    pub id: i64,
    pub files: Vec<String>,
}

/// Detects ids appearing in more than one file. Input pairs are
/// `(id, filename)`. The result is sorted by id ascending; for each
/// duplicate the filenames are sorted for deterministic output.
pub fn find_duplicates(entries: &[(i64, String)]) -> Vec<Duplicate> {
    use std::collections::BTreeMap;
    let mut by_id: BTreeMap<i64, Vec<String>> = BTreeMap::new();
    for (id, file) in entries {
        by_id.entry(*id).or_default().push(file.clone());
    }
    by_id
        .into_iter()
        .filter(|(_, files)| files.len() > 1)
        .map(|(id, mut files)| {
            files.sort();
            Duplicate { id, files }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Date (UTC civil date from a unix timestamp) — std-only, no chrono.
// ---------------------------------------------------------------------------

/// Converts unix seconds (UTC) to a civil date `(year, month, day)` using
/// Howard Hinnant's days-from-civil inverse algorithm.
pub fn civil_from_unix(secs: i64) -> (i64, u32, u32) {
    let days = secs.div_euclid(86_400);
    civil_from_days(days)
}

/// days = number of days since 1970-01-01.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32; // [1, 12]
    let year = if m <= 2 { y + 1 } else { y };
    (year, m, d)
}

/// Formats a civil date as `YYYY-MM-DD`.
pub fn format_date(secs: i64) -> String {
    let (y, m, d) = civil_from_unix(secs);
    format!("{y:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- slug -------------------------------------------------------------

    #[test]
    fn slug_lowercases_and_dashes_spaces() {
        assert_eq!(slug("Hello World"), "hello-world");
    }

    #[test]
    fn slug_collapses_runs_of_punctuation() {
        assert_eq!(slug("Foo --- Bar!!!Baz"), "foo-bar-baz");
        assert_eq!(slug("a  b___c"), "a-b-c");
    }

    #[test]
    fn slug_trims_leading_and_trailing_dashes() {
        assert_eq!(slug("  !Hello!  "), "hello");
        assert_eq!(slug("---x---"), "x");
    }

    #[test]
    fn slug_keeps_digits_and_unicode_letters() {
        assert_eq!(slug("Issue 42"), "issue-42");
        assert_eq!(slug("Café Crème"), "café-crème");
    }

    #[test]
    fn slug_all_punctuation_is_empty() {
        assert_eq!(slug("!!! ???"), "");
    }

    // --- next_id ----------------------------------------------------------

    #[test]
    fn next_id_empty_is_one() {
        assert_eq!(next_id(&[]), 1);
    }

    #[test]
    fn next_id_with_gaps_uses_max_plus_one() {
        assert_eq!(next_id(&[1, 2, 5]), 6);
        assert_eq!(next_id(&[10]), 11);
        assert_eq!(next_id(&[3, 1, 2]), 4);
    }

    // --- frontmatter parse ------------------------------------------------

    #[test]
    fn parse_full_frontmatter() {
        let content = "---\nid: 1\ntitle: \"Some title\"\nstatus: open\ntype: feature\ncreated: 2026-06-14\nupdated: 2026-06-15\nlabels: [cli, mvp]\nrelated: [2, 3]\n---\n# Body\nhello\n";
        let i = parse_frontmatter(content).expect("should parse");
        assert_eq!(i.id, 1);
        assert_eq!(i.title, "Some title");
        assert_eq!(i.status, "open");
        assert_eq!(i.r#type, "feature");
        assert_eq!(i.created, "2026-06-14");
        assert_eq!(i.updated, "2026-06-15");
        assert_eq!(i.labels, vec!["cli", "mvp"]);
        assert_eq!(i.related, vec![2, 3]);
    }

    #[test]
    fn parse_strips_single_quotes_from_title() {
        let content = "---\nid: 7\ntitle: 'Quoted title'\nstatus: closed\n---\n";
        let i = parse_frontmatter(content).unwrap();
        assert_eq!(i.title, "Quoted title");
        assert_eq!(i.id, 7);
        assert_eq!(i.status, "closed");
    }

    #[test]
    fn parse_empty_labels_and_related() {
        let content = "---\nid: 3\ntitle: t\nlabels: []\nrelated: []\n---\n";
        let i = parse_frontmatter(content).unwrap();
        assert!(i.labels.is_empty());
        assert!(i.related.is_empty());
    }

    #[test]
    fn parse_inline_labels_with_quotes_and_spaces() {
        let content = "---\nid: 9\nlabels: [ \"a b\", c , 'd' ]\n---\n";
        let i = parse_frontmatter(content).unwrap();
        assert_eq!(i.labels, vec!["a b", "c", "d"]);
    }

    #[test]
    fn parse_returns_none_without_opening_fence() {
        assert!(parse_frontmatter("# Just markdown\nno frontmatter\n").is_none());
        assert!(parse_frontmatter("").is_none());
    }

    #[test]
    fn parse_stops_at_closing_fence() {
        // An `id:`-looking line in the body must NOT be parsed.
        let content = "---\nid: 5\ntitle: real\n---\nid: 999\ntitle: fake\n";
        let i = parse_frontmatter(content).unwrap();
        assert_eq!(i.id, 5);
        assert_eq!(i.title, "real");
    }

    // --- sort / filter ----------------------------------------------------

    fn mk(id: i64, status: &str, labels: &[&str]) -> Issue {
        Issue {
            id,
            title: format!("t{id}"),
            status: status.to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn sort_orders_numerically_ascending() {
        let mut v = vec![mk(10, "open", &[]), mk(2, "open", &[]), mk(1, "open", &[])];
        sort_by_id(&mut v);
        let ids: Vec<i64> = v.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![1, 2, 10]);
    }

    #[test]
    fn filter_by_status() {
        let v = vec![mk(1, "open", &[]), mk(2, "closed", &[]), mk(3, "open", &[])];
        let got = filter_issues(&v, Some("open"), None);
        let ids: Vec<i64> = got.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn filter_by_label() {
        let v = vec![
            mk(1, "open", &["cli"]),
            mk(2, "open", &["docs"]),
            mk(3, "open", &["cli", "mvp"]),
        ];
        let got = filter_issues(&v, None, Some("cli"));
        let ids: Vec<i64> = got.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn filter_by_status_and_label() {
        let v = vec![
            mk(1, "open", &["cli"]),
            mk(2, "closed", &["cli"]),
            mk(3, "open", &["cli"]),
        ];
        let got = filter_issues(&v, Some("open"), Some("cli"));
        let ids: Vec<i64> = got.iter().map(|i| i.id).collect();
        assert_eq!(ids, vec![1, 3]);
    }

    #[test]
    fn filter_none_returns_all() {
        let v = vec![mk(1, "open", &[]), mk(2, "closed", &[])];
        assert_eq!(filter_issues(&v, None, None).len(), 2);
    }

    #[test]
    fn format_line_matches_spec() {
        let i = mk(4, "in-progress", &["a", "b"]);
        assert_eq!(format_list_line(&i), "4\tin-progress\tt4\ta,b");
        let j = mk(5, "open", &[]);
        assert_eq!(format_list_line(&j), "5\topen\tt5\t");
    }

    // --- lint -------------------------------------------------------------

    #[test]
    fn find_duplicates_none_when_clean() {
        let entries = vec![
            (1, "1-a.md".to_string()),
            (2, "2-b.md".to_string()),
            (3, "3-c.md".to_string()),
        ];
        assert!(find_duplicates(&entries).is_empty());
    }

    #[test]
    fn find_duplicates_detects_collisions() {
        let entries = vec![
            (1, "1-a.md".to_string()),
            (2, "2-b.md".to_string()),
            (1, "1-z.md".to_string()),
            (2, "2-y.md".to_string()),
            (2, "2-x.md".to_string()),
        ];
        let dups = find_duplicates(&entries);
        assert_eq!(dups.len(), 2);
        assert_eq!(dups[0].id, 1);
        assert_eq!(dups[0].files, vec!["1-a.md", "1-z.md"]);
        assert_eq!(dups[1].id, 2);
        assert_eq!(dups[1].files, vec!["2-b.md", "2-x.md", "2-y.md"]);
    }

    // --- date -------------------------------------------------------------

    #[test]
    fn date_epoch_is_1970_01_01() {
        assert_eq!(format_date(0), "1970-01-01");
    }

    #[test]
    fn date_known_value() {
        // 2026-06-14 00:00:00 UTC == 1781395200
        assert_eq!(format_date(1_781_395_200), "2026-06-14");
        // a time later in the same day stays on the same date
        assert_eq!(format_date(1_781_395_200 + 86_399), "2026-06-14");
    }

    #[test]
    fn date_leap_day() {
        // 2024-02-29 00:00:00 UTC == 1709164800
        assert_eq!(format_date(1_709_164_800), "2024-02-29");
    }
}
