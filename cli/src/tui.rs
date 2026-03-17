//! ratatui-based interactive multi-select for `savhub apply`.

use std::collections::BTreeSet;
use std::io;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};

/// A matched selector with its contributed flocks.
pub struct MatchedSelector {
    pub name: String,
    pub label: String,
    pub checked: bool,
    pub flocks: Vec<String>,
}

/// A previously matched selector that no longer matches.
pub struct UnmatchedSelector {
    pub name: String,
    pub flocks: Vec<String>,
}

/// Result of the apply TUI selection.
pub struct ApplySelection {
    pub selected_selectors: Vec<String>,
    pub skipped_selectors: Vec<String>,
    pub selected_flocks: Vec<String>,
    pub skipped_flocks: Vec<String>,
}

/// Extract the repository part from a flock sign.
///
/// `github.com/owner/repo/flock-slug` → `github.com/owner/repo`
/// `github.com/owner/repo`            → `github.com/owner/repo`
fn repo_group(flock_sign: &str) -> &str {
    let mut end = 0;
    let mut slashes = 0;
    for (i, c) in flock_sign.char_indices() {
        if c == '/' {
            slashes += 1;
            if slashes == 3 {
                end = i;
            }
        }
    }
    if slashes >= 3 && end > 0 {
        &flock_sign[..end]
    } else {
        flock_sign
    }
}

/// Short display name for a flock: strip repo prefix if present.
fn flock_display(slug: &str) -> &str {
    slug.strip_prefix(repo_group(slug))
        .and_then(|s| s.strip_prefix('/'))
        .unwrap_or(slug)
}

/// Group flocks by repository, preserving original order.
/// Returns `[(repo_label, [(flock_sign)])]`.
fn group_flocks_by_repo(flocks: &[String]) -> Vec<(String, Vec<String>)> {
    let mut groups: Vec<(String, Vec<String>)> = Vec::new();
    for flock in flocks {
        let repo = repo_group(flock).to_string();
        if let Some(group) = groups.iter_mut().find(|(r, _)| *r == repo) {
            group.1.push(flock.clone());
        } else {
            groups.push((repo, vec![flock.clone()]));
        }
    }
    groups
}

/// Show an interactive TUI for `savhub apply`.
///
/// Selectors are directly togglable. Flocks are derived from checked
/// selectors and can also be individually toggled.
///
/// Returns `None` if cancelled.
pub fn apply_select(
    selectors: &mut [MatchedSelector],
    flock_skip_set: &BTreeSet<String>,
    flock_skill_counts: &dyn Fn(&str) -> usize,
    unmatched: &[UnmatchedSelector],
) -> anyhow::Result<Option<ApplySelection>> {
    if selectors.is_empty() && unmatched.is_empty() {
        return Ok(Some(ApplySelection {
            selected_selectors: Vec::new(),
            skipped_selectors: Vec::new(),
            selected_flocks: Vec::new(),
            skipped_flocks: Vec::new(),
        }));
    }

    // Derived flocks with user overrides
    let mut flock_overrides: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();

    // Initialize overrides from existing skip sets
    for f in flock_skip_set {
        flock_overrides.insert(f.clone(), false);
    }

    // Recompute derived flocks from checked selectors
    fn compute_derived(selectors: &[MatchedSelector]) -> Vec<String> {
        let mut flocks = Vec::new();
        for sel in selectors {
            if !sel.checked {
                continue;
            }
            for f in &sel.flocks {
                if !flocks.contains(f) {
                    flocks.push(f.clone());
                }
            }
        }
        flocks
    }

    fn is_flock_checked(flock: &str, overrides: &std::collections::HashMap<String, bool>) -> bool {
        overrides.get(flock).copied().unwrap_or(true)
    }

    // Row types
    #[derive(Clone)]
    enum Row {
        SectionHeader(&'static str, Color), // label, color
        Separator,
        RepoGroup(String),
        Selector(usize),
        Flock(String),
        RemovedSelector(usize),
        RemovedFlock(String),
    }

    fn is_interactive(row: &Row) -> bool {
        matches!(row, Row::Selector(_) | Row::Flock(_))
    }

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut cursor = 0usize;
    let mut cancelled = false;

    loop {
        let derived_flocks = compute_derived(selectors);
        let grouped = group_flocks_by_repo(&derived_flocks);

        // Build row list
        let mut rows: Vec<Row> = Vec::new();

        // ── Install section ──
        if !selectors.is_empty() {
            rows.push(Row::SectionHeader("  Install", Color::Green));
            for i in 0..selectors.len() {
                rows.push(Row::Selector(i));
            }
            if !derived_flocks.is_empty() {
                rows.push(Row::Separator);
                for (repo, flocks) in &grouped {
                    rows.push(Row::RepoGroup(repo.clone()));
                    for f in flocks {
                        rows.push(Row::Flock(f.clone()));
                    }
                }
            }
        }

        // ── Remove section ──
        if !unmatched.is_empty() {
            if !rows.is_empty() {
                rows.push(Row::Separator);
            }
            rows.push(Row::SectionHeader("  Remove", Color::Red));
            for (ui, u) in unmatched.iter().enumerate() {
                rows.push(Row::RemovedSelector(ui));
                for f in &u.flocks {
                    rows.push(Row::RemovedFlock(f.clone()));
                }
            }
        }

        // Clamp cursor and find first interactive row
        if cursor >= rows.len() {
            cursor = rows.len().saturating_sub(1);
        }
        if !rows.is_empty() && !is_interactive(&rows[cursor]) {
            // find next interactive
            let mut found = false;
            for i in 0..rows.len() {
                if is_interactive(&rows[i]) {
                    cursor = i;
                    found = true;
                    break;
                }
            }
            if !found {
                cursor = 0;
            }
        }

        // Counts for title
        let add_sel = selectors.iter().filter(|s| s.checked).count();
        let add_flock = derived_flocks
            .iter()
            .filter(|f| is_flock_checked(f, &flock_overrides))
            .count();
        let rm_sel = unmatched.len();

        let rows_snapshot = rows.clone();

        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);

            // ── Title bar ──
            let mut title_spans = vec![
                Span::styled(
                    " savhub apply ",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            if add_sel > 0 || add_flock > 0 {
                title_spans.push(Span::styled(
                    format!(" +{add_sel} selectors  +{add_flock} flocks"),
                    Style::default().fg(Color::Green),
                ));
            }
            if rm_sel > 0 {
                title_spans.push(Span::styled(
                    format!("  -{rm_sel} selectors"),
                    Style::default().fg(Color::Red),
                ));
            }
            let title = Paragraph::new(Line::from(title_spans))
                .block(
                    Block::default()
                        .borders(Borders::BOTTOM)
                        .border_style(Style::default().fg(Color::DarkGray)),
                );
            frame.render_widget(title, chunks[0]);

            // ── Main list ──
            let list_items: Vec<ListItem> = rows_snapshot
                .iter()
                .map(|row| match row {
                    Row::SectionHeader(label, color) => {
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!(" ── {label} "),
                                Style::default()
                                    .fg(*color)
                                    .add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                "─".repeat(40),
                                Style::default().fg(Color::DarkGray),
                            ),
                        ]))
                    }
                    Row::Separator => {
                        ListItem::new(Line::from(Span::styled("", Style::default())))
                    }
                    Row::RepoGroup(repo) => ListItem::new(Line::from(Span::styled(
                        format!("    {repo}"),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ))),
                    Row::Selector(i) => {
                        let sel = &selectors[*i];
                        let (marker, mc, lc) = if sel.checked {
                            ("[+]", Color::Green, Color::White)
                        } else {
                            ("[-]", Color::Red, Color::DarkGray)
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("   {marker} "),
                                Style::default().fg(mc).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(&sel.label, Style::default().fg(lc)),
                        ]))
                    }
                    Row::Flock(slug) => {
                        let on = is_flock_checked(slug, &flock_overrides);
                        let count = flock_skill_counts(slug);
                        let (marker, mc, lc) = if on {
                            ("[+]", Color::Green, Color::White)
                        } else {
                            ("[-]", Color::Red, Color::DarkGray)
                        };
                        let display = flock_display(slug);
                        let label = format!("{display} ({count} skills)");
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("      {marker} "),
                                Style::default().fg(mc).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(label, Style::default().fg(lc)),
                        ]))
                    }
                    Row::RemovedSelector(i) => {
                        let u = &unmatched[*i];
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                "   [✕] ",
                                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                &u.name,
                                Style::default()
                                    .fg(Color::DarkGray)
                                    .add_modifier(Modifier::CROSSED_OUT),
                            ),
                        ]))
                    }
                    Row::RemovedFlock(slug) => {
                        let display = flock_display(slug);
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                "      [✕] ",
                                Style::default().fg(Color::Rgb(120, 50, 50)),
                            ),
                            Span::styled(
                                display,
                                Style::default()
                                    .fg(Color::Rgb(120, 80, 80))
                                    .add_modifier(Modifier::CROSSED_OUT),
                            ),
                        ]))
                    }
                })
                .collect();

            let mut list_state = ListState::default();
            list_state.select(Some(cursor));

            let list = List::new(list_items)
                .highlight_style(Style::default().bg(Color::Rgb(40, 60, 35)))
                .highlight_symbol("▸");
            frame.render_stateful_widget(list, chunks[1], &mut list_state);

            // ── Help bar ──
            let help = Paragraph::new(Line::from(vec![
                Span::styled(" Space", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" toggle  ", Style::default().fg(Color::DarkGray)),
                Span::styled("a", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" all  ", Style::default().fg(Color::DarkGray)),
                Span::styled("n", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" none  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Enter", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" confirm  ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(" cancel", Style::default().fg(Color::DarkGray)),
            ]));
            frame.render_widget(help, chunks[2]);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    cancelled = true;
                    break;
                }
                KeyCode::Enter => break,
                KeyCode::Up | KeyCode::Char('k') => {
                    if cursor > 0 {
                        cursor -= 1;
                        while cursor > 0 && !is_interactive(&rows[cursor]) {
                            cursor -= 1;
                        }
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if cursor + 1 < rows.len() {
                        cursor += 1;
                        while cursor + 1 < rows.len() && !is_interactive(&rows[cursor]) {
                            cursor += 1;
                        }
                    }
                }
                KeyCode::Char(' ') => match &rows[cursor] {
                    Row::Selector(i) => {
                        selectors[*i].checked = !selectors[*i].checked;
                    }
                    Row::Flock(slug) => {
                        let current = is_flock_checked(slug, &flock_overrides);
                        flock_overrides.insert(slug.clone(), !current);
                    }
                    _ => {}
                },
                KeyCode::Char('a') => {
                    for sel in selectors.iter_mut() {
                        sel.checked = true;
                    }
                    flock_overrides.values_mut().for_each(|v| *v = true);
                }
                KeyCode::Char('n') => {
                    for sel in selectors.iter_mut() {
                        sel.checked = false;
                    }
                    flock_overrides.values_mut().for_each(|v| *v = false);
                }
                _ => {}
            }
        }
    }

    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;

    if cancelled {
        return Ok(None);
    }

    // Build final result
    let final_flocks = compute_derived(selectors);

    let mut result = ApplySelection {
        selected_selectors: Vec::new(),
        skipped_selectors: Vec::new(),
        selected_flocks: Vec::new(),
        skipped_flocks: Vec::new(),
    };

    for sel in selectors.iter() {
        if sel.checked {
            result.selected_selectors.push(sel.name.clone());
        } else {
            result.skipped_selectors.push(sel.name.clone());
        }
    }
    for f in &final_flocks {
        if is_flock_checked(f, &flock_overrides) {
            result.selected_flocks.push(f.clone());
        } else {
            result.skipped_flocks.push(f.clone());
        }
    }

    Ok(Some(result))
}
