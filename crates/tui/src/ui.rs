//! Pure rendering. The only module that touches ratatui widgets/layout.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::app::{App, FilterRow, Panel, StatusFilter};
use crate::form::Field;

/// Draws the entire UI for the current app state.
pub fn draw(frame: &mut Frame, app: &App) {
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(frame.area());

    let panes = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(22),
            Constraint::Percentage(40),
            Constraint::Min(20),
        ])
        .split(root[0]);

    draw_filters(frame, app, panes[0]);
    draw_issues(frame, app, panes[1]);
    draw_detail(frame, app, panes[2]);
    draw_footer(frame, app, root[1]);

    if app.modal.is_some() {
        draw_modal(frame, app);
    }
    if app.show_help {
        draw_help(frame);
    }
}

fn focused_border(app: &App, panel: Panel) -> Style {
    if app.focus == panel {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

fn draw_filters(frame: &mut Frame, app: &App, area: Rect) {
    let rows = app.filter_rows();
    let mut items: Vec<ListItem> = Vec::new();
    for row in &rows {
        let (label, active) = match row {
            FilterRow::Status(s) => {
                let (name, count) = match s {
                    StatusFilter::Open => ("Open", app.open_count),
                    StatusFilter::Closed => ("Closed", app.closed_count),
                    StatusFilter::All => ("All", app.all_count),
                };
                let active = app.label_filter.is_none() && app.filter == *s;
                (format!("{name} ({count})"), active)
            }
            FilterRow::Label(l) => {
                let count = app
                    .labels_with_counts
                    .iter()
                    .find(|(k, _)| k == l)
                    .map(|(_, c)| *c)
                    .unwrap_or(0);
                let active = app.label_filter.as_deref() == Some(l.as_str());
                (format!("# {l} ({count})"), active)
            }
        };
        let style = if active {
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        items.push(ListItem::new(Line::from(Span::styled(label, style))));
    }

    let block = Block::default()
        .title("Filters")
        .borders(Borders::ALL)
        .border_style(focused_border(app, Panel::Filters));

    let mut state = ListState::default();
    if !rows.is_empty() {
        state.select(Some(app.filter_selected.min(rows.len() - 1)));
    }
    let list = List::default()
        .items(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_issues(frame: &mut Frame, app: &App, area: Rect) {
    let title = format!("Issues ({})", app.visible.len());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(focused_border(app, Panel::Issues));

    if app.visible.is_empty() {
        let hint = Paragraph::new("No issues match.\nPress 'n' to create one.")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(hint, area);
        return;
    }

    let items: Vec<ListItem> = app
        .visible
        .iter()
        .map(|&idx| {
            let issue = &app.issues[idx];
            let status_style = match issue.status.as_str() {
                "closed" => Style::default().fg(Color::Red),
                _ => Style::default().fg(Color::Green),
            };
            let line = Line::from(vec![
                Span::styled(format!("#{:<4}", issue.id), Style::default().fg(Color::Yellow)),
                Span::styled(format!("{:<7}", issue.status), status_style),
                Span::raw(" "),
                Span::raw(issue.title.clone()),
            ]);
            ListItem::new(line)
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(app.selected.min(app.visible.len() - 1)));

    let list = List::default()
        .items(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol("> ");
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_detail(frame: &mut Frame, app: &App, area: Rect) {
    let block = Block::default()
        .title("Detail")
        .borders(Borders::ALL)
        .border_style(focused_border(app, Panel::Detail));

    let Some(issue) = app.selected_issue() else {
        let p = Paragraph::new("No issue selected.")
            .block(block)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(p, area);
        return;
    };

    let body = app
        .detail
        .as_ref()
        .filter(|(id, _)| *id == issue.id)
        .map(|(_, b)| b.as_str())
        .unwrap_or("");

    let mut lines = vec![
        meta_line("id", &format!("#{}", issue.id)),
        meta_line("status", &issue.status),
        meta_line("labels", &issue.labels.join(", ")),
        meta_line("created", &issue.created),
        meta_line("updated", &issue.updated),
        Line::from(""),
        Line::from(Span::styled(
            issue.title.clone(),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
    ];
    for l in body.lines() {
        lines.push(Line::from(l.to_string()));
    }

    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

fn meta_line<'a>(key: &'a str, val: &str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{key:>8}: "),
            Style::default().fg(Color::DarkGray),
        ),
        Span::raw(val.to_string()),
    ])
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let hints = if app.searching {
        format!("/{}", app.search.as_deref().unwrap_or(""))
    } else if app.modal.is_some() {
        "Tab: field  Space: toggle status  Enter: save  Esc: cancel".to_string()
    } else {
        "j/k move  n new  e edit  b body  c close  o open  / search  R reload  ? help  q quit"
            .to_string()
    };
    let text = Line::from(vec![
        Span::styled(hints, Style::default().fg(Color::DarkGray)),
        Span::raw("  |  "),
        Span::styled(
            app.status_line.clone(),
            Style::default().fg(Color::White).add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(Paragraph::new(text), area);
}

fn draw_modal(frame: &mut Frame, app: &App) {
    let Some(form) = &app.modal else { return };
    let area = centered_rect(60, 40, frame.area());
    frame.render_widget(Clear, area);

    let block = Block::default()
        .title(form.heading())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    frame.render_widget(block, area);

    let inner = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Length(2),
            Constraint::Min(1),
        ])
        .split(area);

    frame.render_widget(field_widget("Title", &form.title, form.focus == Field::Title), inner[0]);
    frame.render_widget(
        field_widget("Labels (comma)", &form.labels, form.focus == Field::Labels),
        inner[1],
    );
    frame.render_widget(
        field_widget("Status", &form.status, form.focus == Field::Status),
        inner[2],
    );
    frame.render_widget(
        Paragraph::new("Tab/Shift-Tab: move  Space: toggle status  Enter: save  Esc: cancel")
            .style(Style::default().fg(Color::DarkGray))
            .wrap(Wrap { trim: true }),
        inner[3],
    );
}

fn field_widget<'a>(label: &'a str, value: &str, focused: bool) -> Paragraph<'a> {
    let marker = if focused { "> " } else { "  " };
    let cursor = if focused { "_" } else { "" };
    let style = if focused {
        Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
    } else {
        Style::default()
    };
    let line = Line::from(vec![
        Span::styled(format!("{marker}{label}: "), Style::default().fg(Color::Yellow)),
        Span::styled(format!("{value}{cursor}"), style),
    ]);
    Paragraph::new(line)
}

fn draw_help(frame: &mut Frame) {
    let area = centered_rect(60, 70, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .title("Help")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD));
    let lines = vec![
        Line::from(Span::styled(
            format!("lazyissue {}", env!("CARGO_PKG_VERSION")),
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Navigation"),
        Line::from("  j / Down, k / Up   move selection"),
        Line::from("  g / G              first / last"),
        Line::from("  Ctrl-d / Ctrl-u    half page down / up"),
        Line::from("  Tab / Shift-Tab    cycle panes"),
        Line::from("  h / l, Left/Right  move focus"),
        Line::from(""),
        Line::from("Filters pane"),
        Line::from("  Up/Down + Enter    apply Open/Closed/All/label"),
        Line::from(""),
        Line::from("Actions"),
        Line::from("  n  new issue       e  edit issue"),
        Line::from("  b  edit body ($EDITOR)"),
        Line::from("  c  close           o  reopen"),
        Line::from("  /  search          R  reload"),
        Line::from("  ?  toggle help     q  quit"),
        Line::from(""),
        Line::from("Esc: close modal -> exit search -> close help -> quit"),
    ];
    let p = Paragraph::new(lines).block(block).wrap(Wrap { trim: false });
    frame.render_widget(p, area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::App;
    use issue_core::core::Issue;
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    use std::path::PathBuf;

    fn issue(id: i64, title: &str, status: &str, labels: &[&str]) -> Issue {
        Issue {
            id,
            title: title.to_string(),
            status: status.to_string(),
            created: "2026-06-14".to_string(),
            updated: "2026-06-14".to_string(),
            labels: labels.iter().map(|s| s.to_string()).collect(),
        }
    }

    /// Renders the app to an off-screen buffer and returns it as text.
    fn render(app: &App, w: u16, h: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
        terminal.draw(|f| draw(f, app)).unwrap();
        let buffer = terminal.backend().buffer().clone();
        let mut s = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                s.push_str(buffer[(x, y)].symbol());
            }
            s.push('\n');
        }
        s
    }

    fn sample_app() -> App {
        App::with_issues(
            PathBuf::from("/tmp/x"),
            vec![
                issue(1, "CLI MVP epic", "open", &["cli", "mvp"]),
                issue(2, "Fix the parser", "closed", &["bug"]),
                issue(3, "日本語タイトルの確認", "open", &["docs"]),
            ],
        )
    }

    #[test]
    fn renders_three_panes_with_content() {
        let out = render(&sample_app(), 100, 24);
        assert!(out.contains("Filters"));
        assert!(out.contains("Issues"));
        assert!(out.contains("Detail"));
        assert!(out.contains("Open"));
        assert!(out.contains("CLI MVP epic")); // an open issue shows under the default Open filter
        assert!(out.contains("j/k move")); // footer hints
    }

    #[test]
    fn renders_cjk_title_without_panic() {
        // Switch to the All filter so the CJK issue is visible, then render.
        let mut app = sample_app();
        app.filter = StatusFilter::All;
        app.recompute_visible();
        let out = render(&app, 100, 24);
        // The double-width chars land in non-adjacent buffer cells (the second
        // half is a continuation cell), so assert on a single CJK char: it
        // proves the title reached the buffer and rendering didn't panic.
        assert!(out.contains('日'));
    }

    #[test]
    fn renders_empty_list_hint() {
        let app = App::with_issues(PathBuf::from("/tmp/x"), vec![]);
        let out = render(&app, 80, 20);
        assert!(out.contains("No issues match") || out.contains("Press 'n'"));
    }

    #[test]
    fn renders_modal_when_open() {
        let mut app = sample_app();
        app.open_create_form();
        let out = render(&app, 100, 24);
        assert!(out.contains("Title"));
        assert!(out.contains("Labels"));
        assert!(out.contains("Status"));
    }

    #[test]
    fn renders_help_overlay() {
        let mut app = sample_app();
        app.show_help = true;
        let out = render(&app, 100, 24);
        assert!(out.contains("Help"));
        assert!(out.contains("quit"));
    }

    #[test]
    fn renders_in_a_small_terminal_without_panic() {
        // Tiny size must not panic (layout/area math stays in bounds).
        let _ = render(&sample_app(), 20, 6);
    }
}

/// A centered rect taking `pct_x` / `pct_y` percent of `r`.
fn centered_rect(pct_x: u16, pct_y: u16, r: Rect) -> Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - pct_y) / 2),
            Constraint::Percentage(pct_y),
            Constraint::Percentage((100 - pct_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - pct_x) / 2),
            Constraint::Percentage(pct_x),
            Constraint::Percentage((100 - pct_x) / 2),
        ])
        .split(vertical[1])[1]
}
