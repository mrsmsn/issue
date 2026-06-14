//! `issue` — a local-first issue-management CLI (Rust prototype, std-only).
//!
//! Architecture: [`core`] holds pure logic (parse, slug, id-alloc, sort,
//! filter, lint, date) and is fully unit-tested; [`storage`] handles all
//! filesystem I/O; this file is the thin CLI shell (arg parsing + wiring).

mod completions;

use std::io::{self, BufRead, BufWriter, Read, Write};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use issue_core::core::{self, Issue};
use issue_core::ops::{self, EditIssue, NewIssue};
use issue_core::{json, storage};

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

    if cmd == "--version" || cmd == "-V" {
        return cmd_version();
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
        "export" => cmd_export(rest),
        "import" => cmd_import(rest),
        "completions" => cmd_completions(rest),
        "version" => cmd_version(),
        // Hidden helpers used by the completion scripts for dynamic values.
        "__complete-ids" => cmd_complete_ids(),
        "__complete-labels" => cmd_complete_labels(),
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

    let new = NewIssue {
        title,
        labels: flags.labels,
        status: flags.status.unwrap_or_else(|| core::STATUS_OPEN.to_string()),
        body: flags.body.unwrap_or_default(),
    };
    let dir = storage::resolve_issue_dir();
    match ops::create_issue(&dir, new, now_unix()) {
        Ok((issue, path)) => {
            println!("created issue #{} at {}", issue.id, path.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("error: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
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

/// Sets status on the issue with `id` via the shared ops layer, printing the
/// outcome with the given verb (e.g. "closed issue #3 (path)").
fn set_status(id: i64, status: &str, verb: &str) -> io::Result<ExitCode> {
    let dir = storage::resolve_issue_dir();
    match ops::set_status(&dir, id, status, now_unix()) {
        Ok(path) => {
            println!("{verb} issue #{id} ({})", path.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("error: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
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

    let dir = storage::resolve_issue_dir();
    let edit = EditIssue {
        title,
        status,
        add_labels: add,
        remove_labels: remove,
        body,
    };
    match ops::edit_issue(&dir, id, edit, now_unix()) {
        Ok(path) => {
            println!("edited issue #{id} ({})", path.display());
            Ok(ExitCode::SUCCESS)
        }
        Err(e) => {
            eprintln!("error: {e}");
            Ok(ExitCode::FAILURE)
        }
    }
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
// export
// ---------------------------------------------------------------------------

fn cmd_export(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue export\n\nPrint all issues as a GitHub-shaped JSON array on stdout,\nsorted by id ascending. Round-trips with `issue import`.");
        return Ok(ExitCode::SUCCESS);
    }

    let dir = storage::resolve_issue_dir();
    let mut issues = storage::load_issues_with_bodies(&dir)?;
    issues.sort_by(|a, b| a.0.id.cmp(&b.0.id));
    let json = core::issues_to_json_array(&issues);

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    out.write_all(json.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// import
// ---------------------------------------------------------------------------

fn cmd_import(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue import [FILE]\n\nImport issues from a GitHub-shaped JSON array (REST or `gh` form).\nReads FILE if given, otherwise stdin. Each issue is written to a new\n`<id>-<slug>.md` file; existing files are never overwritten. Ids are\nreconciled: a source `number` is kept when free, else a fresh id is\nassigned (max+1).");
        return Ok(ExitCode::SUCCESS);
    }

    // Optional positional FILE (the only non-flag argument).
    let file = args.iter().find(|a| !a.starts_with('-'));
    let input = match file {
        Some(path) => std::fs::read_to_string(path)?,
        None => {
            let mut buf = String::new();
            io::stdin().lock().read_to_string(&mut buf)?;
            buf
        }
    };

    let root = match json::parse(&input) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: invalid JSON: {e}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let today = core::format_date(now_unix());
    let imported = match core::parse_imported(&root, &today) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("error: {e}");
            return Ok(ExitCode::FAILURE);
        }
    };

    let dir = storage::resolve_issue_dir();
    std::fs::create_dir_all(&dir)?;

    // Seed the used-id set from existing issues so we never collide on disk.
    let existing = storage::load_issues(&dir)?;
    let mut used: Vec<i64> = existing.iter().map(|i| i.id).collect();

    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    let mut count = 0usize;

    for imp in &imported {
        // Id reconciliation: keep the source number when present and free,
        // otherwise assign max(used)+1. Track each assignment so the batch
        // stays internally consistent.
        let (id, remapped) = match imp.number {
            Some(n) if !used.contains(&n) => (n, false),
            _ => (core::next_id(&used), true),
        };
        used.push(id);

        // Status is open/closed by construction, but validate defensively.
        let status = if core::is_valid_status(&imp.status) {
            imp.status.clone()
        } else {
            core::STATUS_OPEN.to_string()
        };

        let issue = Issue {
            id,
            title: imp.title.clone(),
            status,
            created: imp.created.clone(),
            updated: imp.updated.clone(),
            labels: imp.labels.clone(),
        };

        let path = dir.join(ops::issue_filename(id, &issue.title));
        if path.exists() {
            eprintln!("warning: skipping #{id}: {} already exists", path.display());
            continue;
        }
        let content = storage::render_issue_file(&issue, &imp.body);
        std::fs::write(&path, content)?;

        match (remapped, imp.number) {
            (true, Some(orig)) => {
                writeln!(out, "imported #{id} (was #{orig}) \"{}\"", issue.title)?;
            }
            _ => {
                writeln!(out, "imported #{id} \"{}\"", issue.title)?;
            }
        }
        count += 1;
    }

    writeln!(out, "{count} issue(s) imported")?;
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// completions (+ hidden dynamic-completion helpers)
// ---------------------------------------------------------------------------

fn cmd_completions(args: &[String]) -> io::Result<ExitCode> {
    let shell = args.iter().find(|a| !a.starts_with('-'));
    if wants_help(args) || shell.is_none() {
        println!(
            "Usage: issue completions <{}>\n\n\
Print a shell completion script to stdout (Tab-completes subcommands, flags,\n\
and — dynamically — issue ids, labels, and statuses).\n\n\
  bash:  issue completions bash > /usr/local/etc/bash_completion.d/issue\n\
  zsh:   issue completions zsh > \"${{fpath[1]}}/_issue\"   # then restart zsh\n\
  fish:  issue completions fish > ~/.config/fish/completions/issue.fish\n\n\
Or, quickly for the current shell: source <(issue completions zsh)",
            completions::SHELLS.join("|")
        );
        return Ok(if shell.is_none() && !wants_help(args) {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        });
    }
    let shell = shell.unwrap();
    match completions::script(shell) {
        Some(s) => {
            print!("{s}");
            Ok(ExitCode::SUCCESS)
        }
        None => {
            eprintln!(
                "error: unsupported shell '{shell}' (supported: {})",
                completions::SHELLS.join(", ")
            );
            Ok(ExitCode::FAILURE)
        }
    }
}

/// Hidden: prints `<id>:<title>` per issue (id ascending) for shell completion.
fn cmd_complete_ids() -> io::Result<ExitCode> {
    let dir = storage::resolve_issue_dir();
    let mut issues = storage::load_issues(&dir)?;
    core::sort_by_id(&mut issues);
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for issue in &issues {
        // Titles are single-line; strip any stray control chars defensively.
        let title: String = issue.title.replace(['\n', '\r', '\t'], " ");
        writeln!(out, "{}:{}", issue.id, title)?;
    }
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

/// Hidden: prints each distinct label (sorted) for shell completion.
fn cmd_complete_labels() -> io::Result<ExitCode> {
    let dir = storage::resolve_issue_dir();
    let issues = storage::load_issues(&dir)?;
    let mut labels: Vec<String> = issues.into_iter().flat_map(|i| i.labels).collect();
    labels.sort();
    labels.dedup();
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());
    for l in &labels {
        writeln!(out, "{l}")?;
    }
    out.flush()?;
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

/// Prints the version. Backs both the `version` subcommand and `--version`/`-V`.
fn cmd_version() -> io::Result<ExitCode> {
    println!("issue {}", env!("CARGO_PKG_VERSION"));
    Ok(ExitCode::SUCCESS)
}

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
  export           Print all issues as a GitHub-shaped JSON array.\n\
  import [FILE]    Import issues from GitHub-shaped JSON (file or stdin).\n\
  completions <sh> Print a shell completion script (bash|zsh|fish).\n\
  version          Print the version (also: --version, -V).\n\
\n\
Issue dir: $ISSUE_DIR if set, else ./issue\n\
Run `issue <command> --help` for command-specific options."
    );
}
