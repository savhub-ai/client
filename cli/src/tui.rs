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

/// A matched selector with its contributed presets and flocks.
pub struct MatchedSelector {
    pub name: String,
    pub label: String,
    pub checked: bool,
    pub presets: Vec<String>,
    pub flocks: Vec<String>,
}

/// Result of the apply TUI selection.
pub struct ApplySelection {
    pub selected_selectors: Vec<String>,
    pub skipped_selectors: Vec<String>,
    pub selected_presets: Vec<String>,
    pub skipped_presets: Vec<String>,
    pub selected_flocks: Vec<String>,
    pub skipped_flocks: Vec<String>,
}

/// Show an interactive TUI for `savhub apply`.
///
/// Selectors are directly togglable. Presets and flocks are derived from checked
/// selectors and can also be individually toggled.
///
/// Returns `None` if cancelled.
pub fn apply_select(
    selectors: &mut [MatchedSelector],
    preset_skip_set: &BTreeSet<String>,
    flock_skip_set: &BTreeSet<String>,
    flock_skill_counts: &dyn Fn(&str) -> usize,
) -> anyhow::Result<Option<ApplySelection>> {
    if selectors.is_empty() {
        return Ok(Some(ApplySelection {
            selected_selectors: Vec::new(),
            skipped_selectors: Vec::new(),
            selected_presets: Vec::new(),
            skipped_presets: Vec::new(),
            selected_flocks: Vec::new(),
            skipped_flocks: Vec::new(),
        }));
    }

    // Derived presets/flocks with user overrides
    let mut preset_overrides: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();
    let mut flock_overrides: std::collections::HashMap<String, bool> =
        std::collections::HashMap::new();

    // Initialize overrides from existing skip sets
    for p in preset_skip_set {
        preset_overrides.insert(p.clone(), false);
    }
    for f in flock_skip_set {
        flock_overrides.insert(f.clone(), false);
    }

    // Recompute derived presets/flocks from checked selectors
    fn compute_derived(selectors: &[MatchedSelector]) -> (Vec<String>, Vec<String>) {
        let mut presets = Vec::new();
        let mut flocks = Vec::new();
        for sel in selectors {
            if !sel.checked {
                continue;
            }
            for p in &sel.presets {
                if !presets.contains(p) {
                    presets.push(p.clone());
                }
            }
            for f in &sel.flocks {
                if !flocks.contains(f) {
                    flocks.push(f.clone());
                }
            }
        }
        (presets, flocks)
    }

    fn is_preset_checked(
        preset: &str,
        overrides: &std::collections::HashMap<String, bool>,
    ) -> bool {
        overrides.get(preset).copied().unwrap_or(true)
    }

    fn is_flock_checked(flock: &str, overrides: &std::collections::HashMap<String, bool>) -> bool {
        overrides.get(flock).copied().unwrap_or(true)
    }

    // Entry types for the flat list
    #[derive(Clone)]
    enum Row {
        Header(&'static str),
        Selector(usize),
        Preset(String),
        Flock(String),
    }

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut cursor = 1usize; // skip first header
    let mut cancelled = false;

    loop {
        let (derived_presets, derived_flocks) = compute_derived(selectors);

        // Build row list dynamically
        let mut rows: Vec<Row> = Vec::new();
        rows.push(Row::Header("Selectors"));
        for i in 0..selectors.len() {
            rows.push(Row::Selector(i));
        }
        if !derived_presets.is_empty() {
            rows.push(Row::Header("Presets"));
            for p in &derived_presets {
                rows.push(Row::Preset(p.clone()));
            }
        }
        if !derived_flocks.is_empty() {
            rows.push(Row::Header("Flocks"));
            for f in &derived_flocks {
                rows.push(Row::Flock(f.clone()));
            }
        }

        // Clamp cursor
        if cursor >= rows.len() {
            cursor = rows.len().saturating_sub(1);
        }

        // Count totals for title
        let sel_count = selectors.iter().filter(|s| s.checked).count();
        let preset_count = derived_presets
            .iter()
            .filter(|p| is_preset_checked(p, &preset_overrides))
            .count();
        let flock_count = derived_flocks
            .iter()
            .filter(|f| is_flock_checked(f, &flock_overrides))
            .count();

        let rows_snapshot = rows.clone();

        terminal.draw(|frame| {
            let area = frame.area();
            let chunks = Layout::vertical([
                Constraint::Length(3),
                Constraint::Min(1),
                Constraint::Length(2),
            ])
            .split(area);

            // Title
            let title = Paragraph::new(Line::from(vec![
                Span::styled(
                    " savhub apply",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(
                        "  {sel_count} selectors, {preset_count} presets, {flock_count} flocks"
                    ),
                    Style::default().fg(Color::DarkGray),
                ),
            ]))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
            frame.render_widget(title, chunks[0]);

            // List
            let list_items: Vec<ListItem> = rows_snapshot
                .iter()
                .map(|row| match row {
                    Row::Header(name) => ListItem::new(Line::from(vec![
                        Span::styled(
                            format!(" ── {name} "),
                            Style::default()
                                .fg(Color::Cyan)
                                .add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            "──────────────────────────────",
                            Style::default().fg(Color::DarkGray),
                        ),
                    ])),
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
                    Row::Preset(name) => {
                        let on = is_preset_checked(name, &preset_overrides);
                        let (marker, mc, lc) = if on {
                            ("[+]", Color::Green, Color::White)
                        } else {
                            ("[-]", Color::Red, Color::DarkGray)
                        };
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("   {marker} "),
                                Style::default().fg(mc).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(name.as_str(), Style::default().fg(lc)),
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
                        ListItem::new(Line::from(vec![
                            Span::styled(
                                format!("   {marker} "),
                                Style::default().fg(mc).add_modifier(Modifier::BOLD),
                            ),
                            Span::styled(
                                format!("{slug} ({count} skills)"),
                                Style::default().fg(lc),
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

            // Help bar
            let help = Paragraph::new(Line::from(vec![
                Span::styled(
                    " Space",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" toggle  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "a",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" all  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "n",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" none  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "Enter",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(" confirm  ", Style::default().fg(Color::DarkGray)),
                Span::styled(
                    "Esc",
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
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
                        if matches!(rows[cursor], Row::Header(_)) && cursor > 0 {
                            cursor -= 1;
                        }
                    }
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    if cursor + 1 < rows.len() {
                        cursor += 1;
                        if matches!(rows[cursor], Row::Header(_)) && cursor + 1 < rows.len() {
                            cursor += 1;
                        }
                    }
                }
                KeyCode::Char(' ') => match &rows[cursor] {
                    Row::Selector(i) => {
                        selectors[*i].checked = !selectors[*i].checked;
                    }
                    Row::Preset(name) => {
                        let current = is_preset_checked(name, &preset_overrides);
                        preset_overrides.insert(name.clone(), !current);
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
                    preset_overrides.values_mut().for_each(|v| *v = true);
                    flock_overrides.values_mut().for_each(|v| *v = true);
                }
                KeyCode::Char('n') => {
                    for sel in selectors.iter_mut() {
                        sel.checked = false;
                    }
                    preset_overrides.values_mut().for_each(|v| *v = false);
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
    let (final_presets, final_flocks) = compute_derived(selectors);

    let mut result = ApplySelection {
        selected_selectors: Vec::new(),
        skipped_selectors: Vec::new(),
        selected_presets: Vec::new(),
        skipped_presets: Vec::new(),
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
    for p in &final_presets {
        if is_preset_checked(p, &preset_overrides) {
            result.selected_presets.push(p.clone());
        } else {
            result.skipped_presets.push(p.clone());
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
