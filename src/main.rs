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
    r#type: Option<String>,
    labels: Vec<String>,
    status: Option<String>,
    body: Option<String>,
}

fn parse_create_flags(args: &[String]) -> Result<CreateFlags, String> {
    let mut f = CreateFlags {
        title: None,
        r#type: None,
        labels: Vec::new(),
        status: None,
        body: None,
    };
    // Returns the value that follows the flag at index `i`, advancing `i`
    // past it. Errors when no value is present.
    fn value_after(args: &[String], i: &mut usize, name: &str) -> Result<String, String> {
        if *i + 1 >= args.len() {
            return Err(format!("flag {name} requires a value"));
        }
        *i += 1;
        Ok(args[*i].clone())
    }

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--title" => f.title = Some(value_after(args, &mut i, "--title")?),
            "--type" => f.r#type = Some(value_after(args, &mut i, "--type")?),
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
        && flags.r#type.is_none()
        && flags.labels.is_empty()
        && flags.status.is_none()
        && flags.body.is_none()
}

fn cmd_create(args: &[String]) -> io::Result<ExitCode> {
    if wants_help(args) {
        println!("Usage: issue create [--title T] [--type X] [--label L]... [--status S] [--body TEXT]\n\nWith no flags, prompts interactively for title/type/labels on stdin.");
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
        let typ = prompt("type")?;
        let labels_line = prompt("labels (comma-separated)")?;
        flags.title = Some(title);
        if !typ.is_empty() {
            flags.r#type = Some(typ);
        }
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
        r#type: flags.r#type.unwrap_or_default(),
        created: today.clone(),
        updated: today,
        labels: flags.labels,
        related: Vec::new(),
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
  lint             Detect duplicate ids; exit non-zero if any.\n\
\n\
Issue dir: $ISSUE_DIR if set, else ./issue\n\
Run `issue <command> --help` for command-specific options."
    );
}
