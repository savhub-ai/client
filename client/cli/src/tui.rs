//! ratatui-based interactive multi-select for `savhub apply`.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, MouseButton,
    MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{self, EnterAlternateScreen, LeaveAlternateScreen};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

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

// ── Self-contained render row ──

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    Install,
    Remove,
}

#[derive(Clone)]
enum RowKind {
    SectionHeader {
        label: &'static str,
        color: Color,
    },
    Separator,
    RepoGroup {
        label: String,
        collapsed: bool,
    },
    Selector {
        index: usize,
        checked: bool,
        label: String,
    },
    Flock {
        slug: String,
        checked: bool,
        display: String,
        skill_count: usize,
    },
    RemovedSelector {
        label: String,
    },
    RemovedFlock {
        display: String,
    },
}

#[derive(Clone)]
struct Row {
    kind: RowKind,
    section: Section,
}

impl Row {
    fn is_interactive(&self) -> bool {
        matches!(
            self.kind,
            RowKind::Selector { .. } | RowKind::Flock { .. } | RowKind::RepoGroup { .. }
        )
    }

    fn matches_filter(&self, filter: &str) -> bool {
        if filter.is_empty() {
            return true;
        }
        let q = filter.to_ascii_lowercase();
        match &self.kind {
            RowKind::Selector { label, .. } => label.to_ascii_lowercase().contains(&q),
            RowKind::Flock { slug, display, .. } => {
                slug.to_ascii_lowercase().contains(&q) || display.to_ascii_lowercase().contains(&q)
            }
            RowKind::RepoGroup { label, .. } => label.to_ascii_lowercase().contains(&q),
            RowKind::RemovedSelector { label } => label.to_ascii_lowercase().contains(&q),
            RowKind::RemovedFlock { display } => display.to_ascii_lowercase().contains(&q),
            _ => true, // headers/separators always visible
        }
    }
}

// ── Helpers ──

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

fn flock_display(slug: &str) -> &str {
    slug.strip_prefix(repo_group(slug))
        .and_then(|s| s.strip_prefix('/'))
        .unwrap_or(slug)
}

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

fn is_flock_checked(flock: &str, overrides: &HashMap<String, bool>) -> bool {
    overrides.get(flock).copied().unwrap_or(true)
}

// ── Navigation ──

fn first_interactive(rows: &[Row]) -> Option<usize> {
    rows.iter().position(|r| r.is_interactive())
}

fn last_interactive(rows: &[Row]) -> Option<usize> {
    rows.iter().rposition(|r| r.is_interactive())
}

fn move_up(rows: &[Row], cursor: usize) -> usize {
    if cursor == 0 {
        return cursor;
    }
    let mut pos = cursor - 1;
    while pos > 0 && !rows[pos].is_interactive() {
        pos -= 1;
    }
    if rows[pos].is_interactive() {
        pos
    } else {
        cursor
    }
}

fn move_down(rows: &[Row], cursor: usize) -> usize {
    let last = rows.len().saturating_sub(1);
    if cursor >= last {
        return cursor;
    }
    let mut pos = cursor + 1;
    while pos < last && !rows[pos].is_interactive() {
        pos += 1;
    }
    if rows[pos].is_interactive() {
        pos
    } else {
        cursor
    }
}

fn page_up(rows: &[Row], cursor: usize, page_height: usize) -> usize {
    let mut pos = cursor;
    for _ in 0..page_height {
        let next = move_up(rows, pos);
        if next == pos {
            break;
        }
        pos = next;
    }
    pos
}

fn page_down(rows: &[Row], cursor: usize, page_height: usize) -> usize {
    let mut pos = cursor;
    for _ in 0..page_height {
        let next = move_down(rows, pos);
        if next == pos {
            break;
        }
        pos = next;
    }
    pos
}

fn jump_section(rows: &[Row], cursor: usize) -> usize {
    let current_section = rows.get(cursor).map(|r| r.section);
    if let Some(target) = rows
        .iter()
        .enumerate()
        .find(|(_, r)| r.is_interactive() && Some(r.section) != current_section)
    {
        return target.0;
    }
    cursor
}

// ── Row building ──

fn build_rows(
    selectors: &[MatchedSelector],
    flock_overrides: &HashMap<String, bool>,
    flock_skill_counts: &HashMap<String, usize>,
    collapsed_repos: &HashSet<String>,
    unmatched: &[UnmatchedSelector],
    filter: &str,
) -> Vec<Row> {
    let derived_flocks = compute_derived(selectors);
    let grouped = group_flocks_by_repo(&derived_flocks);
    let mut rows = Vec::new();

    // -- Install section --
    if !selectors.is_empty() {
        rows.push(Row {
            kind: RowKind::SectionHeader {
                label: "  Install",
                color: Color::Green,
            },
            section: Section::Install,
        });
        for (i, sel) in selectors.iter().enumerate() {
            let row = Row {
                kind: RowKind::Selector {
                    index: i,
                    checked: sel.checked,
                    label: sel.label.clone(),
                },
                section: Section::Install,
            };
            if row.matches_filter(filter) {
                rows.push(row);
            }
        }
        if !derived_flocks.is_empty() {
            rows.push(Row {
                kind: RowKind::Separator,
                section: Section::Install,
            });
            for (repo, flocks) in &grouped {
                let is_collapsed = collapsed_repos.contains(repo);
                let flock_count = flocks.len();
                let repo_row = Row {
                    kind: RowKind::RepoGroup {
                        label: format!(
                            "{repo} ({flock_count} flocks){}",
                            if is_collapsed { " ..." } else { "" }
                        ),
                        collapsed: is_collapsed,
                    },
                    section: Section::Install,
                };
                if repo_row.matches_filter(filter) || !is_collapsed {
                    rows.push(repo_row);
                }
                if !is_collapsed {
                    for f in flocks {
                        let on = is_flock_checked(f, flock_overrides);
                        let count = flock_skill_counts.get(f.as_str()).copied().unwrap_or(0);
                        let flock_row = Row {
                            kind: RowKind::Flock {
                                slug: f.clone(),
                                checked: on,
                                display: flock_display(f).to_string(),
                                skill_count: count,
                            },
                            section: Section::Install,
                        };
                        if flock_row.matches_filter(filter) {
                            rows.push(flock_row);
                        }
                    }
                }
            }
        }
    }

    // -- Remove section --
    if !unmatched.is_empty() {
        if !rows.is_empty() {
            rows.push(Row {
                kind: RowKind::Separator,
                section: Section::Remove,
            });
        }
        rows.push(Row {
            kind: RowKind::SectionHeader {
                label: "  Remove",
                color: Color::Red,
            },
            section: Section::Remove,
        });
        for u in unmatched {
            let row = Row {
                kind: RowKind::RemovedSelector {
                    label: u.name.clone(),
                },
                section: Section::Remove,
            };
            if row.matches_filter(filter) {
                rows.push(row);
            }
            for f in &u.flocks {
                let row = Row {
                    kind: RowKind::RemovedFlock {
                        display: flock_display(f).to_string(),
                    },
                    section: Section::Remove,
                };
                if row.matches_filter(filter) {
                    rows.push(row);
                }
            }
        }
    }

    rows
}

// ── Context info for help bar ──

fn context_info(rows: &[Row], cursor: usize) -> String {
    let Some(row) = rows.get(cursor) else {
        return String::new();
    };
    match &row.kind {
        RowKind::Selector { label, checked, .. } => {
            let state = if *checked { "on" } else { "off" };
            format!(" Selector: {label} [{state}]")
        }
        RowKind::Flock {
            slug,
            skill_count,
            checked,
            ..
        } => {
            let state = if *checked { "on" } else { "off" };
            format!(" Flock: {slug} -- {skill_count} skills [{state}]")
        }
        RowKind::RepoGroup { label, collapsed } => {
            let state = if *collapsed { "collapsed" } else { "expanded" };
            format!(" Repo: {label} [{state}] -- Space to toggle")
        }
        _ => String::new(),
    }
}

// ── Rendering ──

fn render_row(row: &Row, term_width: u16) -> ListItem<'static> {
    match &row.kind {
        RowKind::SectionHeader { label, color } => {
            let rule_len = (term_width as usize).saturating_sub(label.len() + 4);
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" -- {label} "),
                    Style::default().fg(*color).add_modifier(Modifier::BOLD),
                ),
                Span::styled("-".repeat(rule_len), Style::default().fg(Color::DarkGray)),
            ]))
        }
        RowKind::Separator => ListItem::new(Line::from(Span::raw(""))),
        RowKind::RepoGroup { label, collapsed } => {
            let icon = if *collapsed { ">" } else { "v" };
            ListItem::new(Line::from(vec![
                Span::styled(format!("   {icon} "), Style::default().fg(Color::DarkGray)),
                Span::styled(
                    label.clone(),
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
        }
        RowKind::Selector { checked, label, .. } => {
            let (marker, mc, lc) = if *checked {
                ("*", Color::Green, Color::White)
            } else {
                (" ", Color::DarkGray, Color::DarkGray)
            };
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("  [{marker}] "),
                    Style::default().fg(mc).add_modifier(Modifier::BOLD),
                ),
                Span::styled(label.clone(), Style::default().fg(lc)),
            ]))
        }
        RowKind::Flock {
            checked,
            display,
            skill_count,
            ..
        } => {
            let (marker, mc, lc) = if *checked {
                ("+", Color::Cyan, Color::White)
            } else {
                ("-", Color::Red, Color::DarkGray)
            };
            let label = format!("{display} ({skill_count} skills)");
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!("      [{marker}] "),
                    Style::default().fg(mc).add_modifier(Modifier::BOLD),
                ),
                Span::styled(label, Style::default().fg(lc)),
            ]))
        }
        RowKind::RemovedSelector { label } => ListItem::new(Line::from(vec![
            Span::styled(
                "  [x] ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                label.clone(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
        ])),
        RowKind::RemovedFlock { display } => ListItem::new(Line::from(vec![
            Span::styled("      [x] ", Style::default().fg(Color::Rgb(120, 50, 50))),
            Span::styled(
                display.clone(),
                Style::default()
                    .fg(Color::Rgb(120, 80, 80))
                    .add_modifier(Modifier::CROSSED_OUT),
            ),
        ])),
    }
}

fn help_key(key: &str, desc: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(
            format!(" {key}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(format!(" {desc} "), Style::default().fg(Color::DarkGray)),
    ]
}

fn render_search_overlay(filter: &str, area: Rect, frame: &mut ratatui::Frame<'_>) {
    let width = 40.min(area.width.saturating_sub(4)) as u16;
    let x = area.width.saturating_sub(width + 2);
    let overlay = Rect::new(x, 1, width + 2, 3);
    let text = format!(" / {filter}_");
    let p = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title(" Search "),
    );
    frame.render_widget(Clear, overlay);
    frame.render_widget(p, overlay);
}

// ── Terminal guard ──

struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

// ── Main entry point ──

/// Show an interactive TUI for `savhub apply`.
///
/// `flock_skill_counts` maps flock sign -> number of skills (pre-computed).
/// Returns `None` if cancelled.
pub fn apply_select(
    selectors: &mut [MatchedSelector],
    flock_skip_set: &BTreeSet<String>,
    flock_skill_counts: &HashMap<String, usize>,
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

    let mut flock_overrides: HashMap<String, bool> = HashMap::new();
    for f in flock_skip_set {
        flock_overrides.insert(f.clone(), false);
    }
    let mut collapsed_repos: HashSet<String> = HashSet::new();
    let mut filter = String::new();
    let mut search_mode = false;

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let _guard = TerminalGuard;

    let mut cursor = 0usize;
    let mut cancelled = false;
    let mut dirty = true;
    let mut rows: Vec<Row> = Vec::new();
    let mut list_area_height: u16 = 20;

    loop {
        if dirty {
            rows = build_rows(
                selectors,
                &flock_overrides,
                flock_skill_counts,
                &collapsed_repos,
                unmatched,
                &filter,
            );
            dirty = false;
        }

        // Clamp cursor
        if cursor >= rows.len() {
            cursor = rows.len().saturating_sub(1);
        }
        if !rows.is_empty() && !rows[cursor].is_interactive() {
            cursor = first_interactive(&rows).unwrap_or(0);
        }

        // Counts for title
        let derived_flocks = compute_derived(selectors);
        let add_sel = selectors.iter().filter(|s| s.checked).count();
        let add_flock = derived_flocks
            .iter()
            .filter(|f| is_flock_checked(f, &flock_overrides))
            .count();
        let add_skills: usize = derived_flocks
            .iter()
            .filter(|f| is_flock_checked(f, &flock_overrides))
            .map(|f| flock_skill_counts.get(f.as_str()).copied().unwrap_or(0))
            .sum();
        let rm_sel = unmatched.len();
        let ctx = context_info(&rows, cursor);
        let is_searching = search_mode;
        let filter_display = filter.clone();

        let rows_ref = &rows;

        terminal.draw(|frame| {
            let area = frame.area();
            list_area_height = area.height.saturating_sub(6);
            let chunks = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(3),
            ])
            .split(area);

            // -- Title bar --
            let mut title_spans = vec![Span::styled(
                " savhub apply ",
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            )];
            if add_sel > 0 || add_flock > 0 {
                title_spans.push(Span::styled(
                    format!(" +{add_sel} selectors  +{add_flock} flocks ({add_skills} skills)"),
                    Style::default().fg(Color::Green),
                ));
            }
            if rm_sel > 0 {
                title_spans.push(Span::styled(
                    format!("  -{rm_sel} selectors"),
                    Style::default().fg(Color::Red),
                ));
            }
            if !filter_display.is_empty() && !is_searching {
                title_spans.push(Span::styled(
                    format!("  [filter: {filter_display}]"),
                    Style::default().fg(Color::Yellow),
                ));
            }
            let title = Paragraph::new(Line::from(title_spans)).block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            frame.render_widget(title, chunks[0]);

            // -- Main list --
            let list_items: Vec<ListItem> = rows_ref
                .iter()
                .map(|row| render_row(row, area.width))
                .collect();

            let mut list_state = ListState::default();
            list_state.select(Some(cursor));

            let list = List::new(list_items)
                .highlight_style(Style::default().bg(Color::Rgb(40, 60, 35)))
                .highlight_symbol("> ");
            frame.render_stateful_widget(list, chunks[1], &mut list_state);

            // -- Help bar --
            let mut key_spans = Vec::new();
            key_spans.extend(help_key("Space", "toggle"));
            key_spans.extend(help_key("a", "all"));
            key_spans.extend(help_key("n", "none"));
            key_spans.extend(help_key("Tab", "section"));
            key_spans.extend(help_key("/", "search"));
            key_spans.extend(help_key("Enter", "confirm"));
            key_spans.extend(help_key("Esc", "cancel"));

            let ctx_line = if ctx.is_empty() {
                Line::from(Span::raw(""))
            } else {
                Line::from(Span::styled(ctx, Style::default().fg(Color::DarkGray)))
            };
            let help = Paragraph::new(vec![Line::from(key_spans), ctx_line]);
            frame.render_widget(help, chunks[2]);

            // -- Search overlay --
            if is_searching {
                render_search_overlay(&filter_display, area, frame);
            }
        })?;

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                if search_mode {
                    match key.code {
                        KeyCode::Esc => {
                            filter.clear();
                            search_mode = false;
                            dirty = true;
                        }
                        KeyCode::Enter => {
                            search_mode = false;
                            // keep filter active
                        }
                        KeyCode::Backspace => {
                            filter.pop();
                            dirty = true;
                        }
                        KeyCode::Char(c) => {
                            filter.push(c);
                            dirty = true;
                        }
                        _ => {}
                    }
                    continue;
                }

                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => {
                        if !filter.is_empty() {
                            filter.clear();
                            dirty = true;
                        } else {
                            cancelled = true;
                            break;
                        }
                    }
                    KeyCode::Enter => break,
                    KeyCode::Up | KeyCode::Char('k') => {
                        cursor = move_up(&rows, cursor);
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        cursor = move_down(&rows, cursor);
                    }
                    KeyCode::Home => {
                        if let Some(pos) = first_interactive(&rows) {
                            cursor = pos;
                        }
                    }
                    KeyCode::End => {
                        if let Some(pos) = last_interactive(&rows) {
                            cursor = pos;
                        }
                    }
                    KeyCode::PageUp => {
                        cursor = page_up(&rows, cursor, list_area_height as usize);
                    }
                    KeyCode::PageDown => {
                        cursor = page_down(&rows, cursor, list_area_height as usize);
                    }
                    KeyCode::Tab | KeyCode::BackTab => {
                        cursor = jump_section(&rows, cursor);
                    }
                    KeyCode::Char('/') => {
                        search_mode = true;
                    }
                    KeyCode::Char(' ') => match &rows[cursor].kind {
                        RowKind::Selector { index, .. } => {
                            selectors[*index].checked = !selectors[*index].checked;
                            dirty = true;
                        }
                        RowKind::Flock { slug, .. } => {
                            let current = is_flock_checked(slug, &flock_overrides);
                            flock_overrides.insert(slug.clone(), !current);
                            dirty = true;
                        }
                        RowKind::RepoGroup { label, collapsed } => {
                            // Extract the repo key (before the " (N flocks)" suffix)
                            let repo_key = label
                                .find(" (")
                                .map(|pos| &label[..pos])
                                .unwrap_or(label)
                                .to_string();
                            if *collapsed {
                                collapsed_repos.remove(&repo_key);
                            } else {
                                collapsed_repos.insert(repo_key);
                            }
                            dirty = true;
                        }
                        _ => {}
                    },
                    KeyCode::Char('a') => {
                        for sel in selectors.iter_mut() {
                            sel.checked = true;
                        }
                        flock_overrides.values_mut().for_each(|v| *v = true);
                        dirty = true;
                    }
                    KeyCode::Char('n') => {
                        for sel in selectors.iter_mut() {
                            sel.checked = false;
                        }
                        flock_overrides.values_mut().for_each(|v| *v = false);
                        dirty = true;
                    }
                    _ => {}
                }
            }
            Event::Mouse(mouse) => {
                if mouse.kind == MouseEventKind::Down(MouseButton::Left) {
                    // Map click y to list row (title=3 rows, list starts at y=3)
                    let click_y = mouse.row.saturating_sub(3) as usize;
                    // The list_state scroll offset determines which row is at the top
                    // We approximate: click_y + scroll_offset = row index
                    // Since we don't track offset explicitly, just select the row
                    let target = click_y;
                    if target < rows.len() && rows[target].is_interactive() {
                        cursor = target;
                        // Double-purpose: click to select, not toggle
                    }
                }
            }
            _ => {}
        }
    }

    // TerminalGuard handles cleanup on drop (including panic)
    drop(_guard);
    // Re-init since guard already cleaned up
    terminal::enable_raw_mode()?;
    terminal::disable_raw_mode()?;

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
