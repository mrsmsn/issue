//! `issue` — a local-first issue-management CLI (Rust prototype, std-only).
//!
//! Architecture: [`core`] holds pure logic (parse, slug, id-alloc, sort,
//! filter, lint, date) and is fully unit-tested; [`storage`] handles all
//! filesystem I/O; this file is the thin CLI shell (arg parsing + wiring).

mod core;
mod storage;

use std::io::{self, BufRead, BufWriter, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use core::Issue;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> io::Result<ExitCode> {
    let Some(cmd) = args.first() else {
        print_usage();
        return Ok(ExitCode::FAILURE);
    };

    if cmd == "--help" || cmd == "-h" {
        print_usage();
        return Ok(ExitCode::SUCCESS);
    }

    let rest = &args[1..];
    match cmd.as_str() {
        "init" => cmd_init(rest),
        "create" => cmd_create(rest),
        "list" => cmd_list(rest),
        "view" => cmd_view(rest),
        "edit" => cmd_edit(rest),
        "close" => cmd_close(rest),
        "reopen" => cmd_reopen(rest),
        "lint" => cmd_lint(rest),
        other => {
            eprintln!("error: unknown command '{other}'");
            print_usage();
            Ok(ExitCode::FAILURE)
        }
    }
}

fn wants_help(args: &[String]) -> bool {
    args.iter().any(|a| a == "--help" || a == "-h")
}

// ---------------------------------------------------------------------------
// init
// ---------------------------------------------------------------------------

const README_BODY: &str = "# Issues\n\n\
This directory holds the project's issues, one Markdown file per issue\n\
(`<id>-<slug>.md`) with YAML frontmatter. It is managed by the `issue`\n\
CLI (a local-first, gh-issue-like tool). Issues live alongside the code\n\
they track; there is no server or remote backend.\n\n\
Run `issue list` to see open issues, `issue create` to add one.\n";

fn cmd_init(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue init\n\nCreate the issue directory (idempotent). Never overwrites existing files.");
        return Ok(ExitCode::SUCCESS);
    }
    let dir = storage::resolve_issue_dir();
    std::fs::create_dir_all(&dir)?;
    let readme = dir.join("README.md");
    if !readme.exists() {
        std::fs::write(&readme, README_BODY)?;
        println!("created {}", readme.display());
    }
    println!("initialized issue directory at {}", dir.display());
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// create
// ---------------------------------------------------------------------------

struct CreateFlags {
    title: Option<String>,
    labels: Vec<String>,
    status: Option<String>,
    body: Option<String>,
}

fn parse_create_flags(args: &[String]) -> Result<CreateFlags, String> {
    let mut f = CreateFlags {
        title: None,
        labels: Vec::new(),
        status: None,
        body: None,
    };
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--title" => f.title = Some(value_after(args, &mut i, "--title")?),
            "--label" => f.labels.push(value_after(args, &mut i, "--label")?),
            "--status" => f.status = Some(value_after(args, &mut i, "--status")?),
            "--body" => f.body = Some(value_after(args, &mut i, "--body")?),
            other => return Err(format!("unknown flag '{other}'")),
        }
        i += 1;
    }
    Ok(f)
}

/// True when no create flags were supplied (run interactively).
fn is_interactive(flags: &CreateFlags) -> bool {
    flags.title.is_none()
        && flags.labels.is_empty()
        && flags.status.is_none()
        && flags.body.is_none()
}

fn cmd_create(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue create [--title T] [--label L]... [--status S] [--body TEXT]\n\nWith no flags, prompts interactively for title/labels on stdin.\nStatus is open|closed; categorize with labels (there is no `type`).");
        return Ok(ExitCode::SUCCESS);
    }

    let mut flags = match parse_create_flags(args) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(ExitCode::FAILURE);
        }
    };

    if is_interactive(&flags) {
        let stdin = io::stdin();
        let mut lines = stdin.lock().lines();
        let mut prompt = |label: &str| -> io::Result<String> {
            print!("{label}: ");
            io::stdout().flush()?;
            Ok(lines.next().transpose()?.unwrap_or_default().trim().to_string())
        };
        let title = prompt("title")?;
        let labels_line = prompt("labels (comma-separated)")?;
        flags.title = Some(title);
        flags.labels = labels_line
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();
    }

    let title = match flags.title {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => {
            eprintln!("error: --title is required");
            return Ok(ExitCode::FAILURE);
        }
    };

    let status = flags.status.unwrap_or_else(|| core::STATUS_OPEN.to_string());
    if !core::is_valid_status(&status) {
        eprintln!(
            "error: invalid status '{status}' (allowed: {})",
            core::VALID_STATUSES.join(", ")
        );
        return Ok(ExitCode::FAILURE);
    }

    let dir = storage::resolve_issue_dir();
    std::fs::create_dir_all(&dir)?;

    let existing = storage::load_issues(&dir)?;
    let ids: Vec<i64> = existing.iter().map(|i| i.id).collect();
    let id = core::next_id(&ids);

    let today = core::format_date(now_unix());
    let issue = Issue {
        id,
        title: title.clone(),
        status,
        created: today.clone(),
        updated: today,
        labels: flags.labels,
    };

    let slug = core::slug(&title);
    let filename = if slug.is_empty() {
        format!("{id}.md")
    } else {
        format!("{id}-{slug}.md")
    };
    let path = dir.join(&filename);
    let content = storage::render_issue_file(&issue, flags.body.as_deref().unwrap_or(""));
    std::fs::write(&path, content)?;

    println!("created issue #{id} at {}", path.display());
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// list (performance-critical)
// ---------------------------------------------------------------------------

fn cmd_list(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue list [--status S] [--label L]\n\nPrints tab-separated: <id>\\t<status>\\t<title>\\t<labels>");
        return Ok(ExitCode::SUCCESS);
    }

    let mut status: Option<String> = None;
    let mut label: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--status" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --status requires a value");
                    return Ok(ExitCode::FAILURE);
                }
                status = Some(args[i].clone());
            }
            "--label" => {
                i += 1;
                if i >= args.len() {
                    eprintln!("error: --label requires a value");
                    return Ok(ExitCode::FAILURE);
                }
                label = Some(args[i].clone());
            }
            other => {
                eprintln!("error: unknown flag '{other}'");
                return Ok(ExitCode::FAILURE);
            }
        }
        i += 1;
    }

    let dir = storage::resolve_issue_dir();
    let mut issues = storage::load_issues(&dir)?;
    core::sort_by_id(&mut issues);
    let selected = core::filter_issues(&issues, status.as_deref(), label.as_deref());

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for issue in selected {
        out.write_all(core::format_list_line(issue).as_bytes())?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// view
// ---------------------------------------------------------------------------

fn cmd_view(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue view <id>\n\nPrint the full issue file for the given id.");
        return Ok(ExitCode::SUCCESS);
    }
    let Some(id_arg) = args.first() else {
        eprintln!("error: view requires an <id> argument");
        return Ok(ExitCode::FAILURE);
    };
    let id: i64 = match id_arg.parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("error: invalid id '{id_arg}'");
            return Ok(ExitCode::FAILURE);
        }
    };

    let dir = storage::resolve_issue_dir();
    match storage::find_issue_by_id(&dir, id)? {
        Some((_, content)) => {
            let stdout = io::stdout();
            let mut out = BufWriter::new(stdout.lock());
            out.write_all(content.as_bytes())?;
            out.flush()?;
            Ok(ExitCode::SUCCESS)
        }
        None => {
            eprintln!("error: no issue found with id {id}");
            Ok(ExitCode::FAILURE)
        }
    }
}

// ---------------------------------------------------------------------------
// edit / close / reopen (in-place frontmatter edits)
// ---------------------------------------------------------------------------

/// Returns the value following the flag at index `*i`, advancing `*i` past
/// it. Errors when no value is present. (Free function to avoid a closure
/// holding a mutable borrow of the index across the arg loop.)
fn value_after(args: &[String], i: &mut usize, name: &str) -> Result<String, String> {
    if *i + 1 >= args.len() {
        return Err(format!("flag {name} requires a value"));
    }
    *i += 1;
    Ok(args[*i].clone())
}

/// First positional (non-flag) argument parsed as an id.
fn first_id(args: &[String]) -> Result<i64, String> {
    let arg = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .ok_or("an <id> argument is required")?;
    arg.parse::<i64>().map_err(|_| format!("invalid id '{arg}'"))
}

/// Loads the issue file for `id`, hands its content and parsed frontmatter to
/// `apply`, and writes the returned content back to the same path. The
/// filename is never changed — the id is the stable identity. On success
/// prints `msg`; reports a not-found / malformed error otherwise.
fn edit_issue_file(
    id: i64,
    msg: &str,
    apply: impl FnOnce(String, &Issue) -> Result<String, String>,
) -> io::Result<ExitCode> {
    let dir = storage::resolve_issue_dir();
    let Some((path, content)) = storage::find_issue_by_id(&dir, id)? else {
        eprintln!("error: no issue found with id {id}");
        return Ok(ExitCode::FAILURE);
    };
    // find_issue_by_id already parsed it once, so this cannot fail.
    let issue = core::parse_frontmatter(&content).expect("issue frontmatter parses");
    match apply(content, &issue) {
        Ok(new_content) => {
            std::fs::write(&path, new_content)?;
            println!("{msg} ({})", path.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("error: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Renders a frontmatter scalar value, quoting/escaping like a title.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('"', "\\\""))
}

fn set_status(id: i64, status: &str, verb: &str) -> io::Result<ExitCode> {
    edit_issue_file(id, &format!("{verb} issue #{id}"), |content, _| {
        let today = core::format_date(now_unix());
        core::update_frontmatter(
            &content,
            &[("status", status.to_string()), ("updated", today)],
        )
        .ok_or_else(|| "issue file has malformed frontmatter".to_string())
    })
}

fn cmd_close(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue close <id>\n\nSet status to `closed` and bump `updated`.");
        return Ok(ExitCode::SUCCESS);
    }
    match first_id(args) {
        Ok(id) => set_status(id, core::STATUS_CLOSED, "closed"),
        Err(e) => {
            eprintln!("error: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn cmd_reopen(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue reopen <id>\n\nSet status to `open` and bump `updated`.");
        return Ok(ExitCode::SUCCESS);
    }
    match first_id(args) {
        Ok(id) => set_status(id, core::STATUS_OPEN, "reopened"),
        Err(e) => {
            eprintln!("error: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn cmd_edit(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!(
            "Usage: issue edit <id> [--title T] [--status S] \
[--add-label L]... [--remove-label L]... [--body TEXT]\n\n\
Status is open|closed; categorize with labels (there is no `type`).\n\
Updates the given fields in place (filename is not renamed) and bumps \
`updated`. Unknown frontmatter keys and the body are preserved."
        );
        return Ok(ExitCode::SUCCESS);
    }

    let id = match first_id(args) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let mut title: Option<String> = None;
    let mut status: Option<String> = None;
    let mut add: Vec<String> = Vec::new();
    let mut remove: Vec<String> = Vec::new();
    let mut body: Option<String> = None;

    let mut i = 0;
    while i < args.len() {
        let a = args[i].clone();
        // Skip the positional id (the only non-flag argument).
        if !a.starts_with('-') {
            i += 1;
            continue;
        }
        let res = match a.as_str() {
            "--title" => value_after(args, &mut i, "--title").map(|v| title = Some(v)),
            "--status" => value_after(args, &mut i, "--status").map(|v| status = Some(v)),
            "--add-label" => value_after(args, &mut i, "--add-label").map(|v| add.push(v)),
            "--remove-label" => value_after(args, &mut i, "--remove-label").map(|v| remove.push(v)),
            "--body" => value_after(args, &mut i, "--body").map(|v| body = Some(v)),
            other => Err(format!("unknown flag '{other}'")),
        };
        if let Err(e) = res {
            eprintln!("error: {e}");
            return Ok(ExitCode::FAILURE);
        }
        i += 1;
    }

    if let Some(s) = &status {
        if !core::is_valid_status(s) {
            eprintln!(
                "error: invalid status '{s}' (allowed: {})",
                core::VALID_STATUSES.join(", ")
            );
            return Ok(ExitCode::FAILURE);
        }
    }

    if title.is_none()
        && status.is_none()
        && add.is_empty()
        && remove.is_empty()
        && body.is_none()
    {
        eprintln!(
            "error: nothing to edit (specify --title/--status/--add-label/--remove-label/--body)"
        );
        return Ok(ExitCode::FAILURE);
    }

    edit_issue_file(id, &format!("edited issue #{id}"), |content, issue| {
        let mut updates: Vec<(&str, String)> = Vec::new();
        if let Some(t) = &title {
            updates.push(("title", quote(t)));
        }
        if let Some(s) = &status {
            updates.push(("status", s.clone()));
        }
        if !add.is_empty() || !remove.is_empty() {
            let labels = core::apply_label_changes(&issue.labels, &add, &remove);
            updates.push(("labels", format!("[{}]", labels.join(", "))));
        }
        updates.push(("updated", core::format_date(now_unix())));

        let mut out = core::update_frontmatter(&content, &updates)
            .ok_or_else(|| "issue file has malformed frontmatter".to_string())?;
        if let Some(b) = &body {
            out = core::replace_body(&out, b)
                .ok_or_else(|| "issue file has malformed frontmatter".to_string())?;
        }
        Ok(out)
    })
}

// ---------------------------------------------------------------------------
// lint
// ---------------------------------------------------------------------------

fn cmd_lint(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue lint\n\nDetect duplicate ids across issue files. Exit non-zero if any found.");
        return Ok(ExitCode::SUCCESS);
    }
    let dir = storage::resolve_issue_dir();
    let pairs = storage::load_issues_with_files(&dir)?;
    let entries: Vec<(i64, String)> = pairs.into_iter().map(|(i, f)| (i.id, f)).collect();
    let dups = core::find_duplicates(&entries);
    if dups.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    for d in &dups {
        eprintln!("duplicate id {}: {}", d.id, d.files.join(", "));
    }
    Ok(ExitCode::FAILURE)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn now_unix() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

fn print_usage() {
    println!(
        "issue — local-first issue-management CLI\n\
\n\
Usage: issue <command> [options]\n\
\n\
Commands:\n\
  init             Create the issue directory (idempotent).\n\
  create           Create an issue (interactive, or via flags).\n\
  list             List issues (tab-separated), with optional filters.\n\
  view <id>        Print a single issue file.\n\
  edit <id>        Edit fields in place (title/status/labels/body).\n\
  close <id>       Set status to closed.\n\
  reopen <id>      Set status to open.\n\
  lint             Detect duplicate ids; exit non-zero if any.\n\
\n\
Issue dir: $ISSUE_DIR if set, else ./issue\n\
Run `issue <command> --help` for command-specific options."
    );
}
