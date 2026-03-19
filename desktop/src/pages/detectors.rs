use std::collections::BTreeSet;

use dioxus::prelude::*;

use savhub_local::selectors::{
    SelectorDefinition, SelectorRule, MatchMode, create_selector, delete_selector,
    generate_selector_id, read_selectors_store, update_selector,
};
use savhub_local::presets::read_presets_store;

use crate::components::pagination::{self, PaginationControls};
use crate::components::view_toggle::{ViewMode, ViewToggleButton};
use crate::i18n;
use crate::state::AppState;
use crate::theme::Theme;

const DETECTORS_PAGE_SIZE: usize = 4;

// ---------------------------------------------------------------------------
// Form state
// ---------------------------------------------------------------------------

/// Form rule: (kind, value1, value2).
/// value1 = path/pattern/name/command, value2 = contains text (only for file_contains).
type FormRule = (String, String, String);

#[derive(Clone, PartialEq)]
struct SelectorForm {
    editing_id: Option<String>,
    name: String,
    description: String,
    folder_scope: String,
    rules: Vec<FormRule>,
    match_mode: u8, // 0=AllMatch, 1=AnyMatch, 2=Custom
    custom_expr: String,
    presets: BTreeSet<String>,
    skills: BTreeSet<String>,
    flocks: BTreeSet<String>,
    priority: i32,
    error: String,
}

impl SelectorForm {
    fn blank() -> Self {
        Self {
            editing_id: None,
            name: String::new(),
            description: String::new(),
            folder_scope: ".".to_string(),
            rules: vec![("file_exists".to_string(), String::new(), String::new())],
            match_mode: 0,
            custom_expr: String::new(),
            presets: BTreeSet::new(),
            skills: BTreeSet::new(),
            flocks: BTreeSet::new(),
            priority: 0,
            error: String::new(),
        }
    }

    fn from_selector(d: &SelectorDefinition, as_template: bool) -> Self {
        let rules = d
            .rules
            .iter()
            .map(|r| match r {
                SelectorRule::FileExists { path } => ("file_exists".to_string(), path.clone(), String::new()),
                SelectorRule::FolderExists { path } => ("folder_exists".to_string(), path.clone(), String::new()),
                SelectorRule::GlobMatch { pattern } => ("glob_match".to_string(), pattern.clone(), String::new()),
                SelectorRule::FileContains { path, contains } => ("file_contains".to_string(), path.clone(), contains.clone()),
                SelectorRule::FileRegex { path, pattern } => ("file_regex".to_string(), path.clone(), pattern.clone()),
                SelectorRule::EnvVarSet { name } => ("env_var_set".to_string(), name.clone(), String::new()),
                SelectorRule::CommandExits { command } => ("command_exits".to_string(), command.clone(), String::new()),
            })
            .collect();
        Self {
            editing_id: if as_template { None } else { Some(d.id.clone()) },
            name: if as_template { format!("{} (copy)", d.name) } else { d.name.clone() },
            description: d.description.clone(),
            folder_scope: d.folder_scope.clone(),
            rules,
            match_mode: match d.match_mode { MatchMode::AllMatch => 0, MatchMode::AnyMatch => 1, MatchMode::Custom => 2 },
            custom_expr: d.custom_expression.clone(),
            presets: d.presets.iter().cloned().collect(),
            skills: d.add_skills.iter().cloned().collect(),
            flocks: d.add_flocks.iter().cloned().collect(),
            priority: d.priority,
            error: String::new(),
        }
    }

    fn to_definition(&self) -> SelectorDefinition {
        let rules = self
            .rules
            .iter()
            .filter(|(_, v1, _)| !v1.trim().is_empty())
            .map(|(kind, v1, v2)| match kind.as_str() {
                "folder_exists" => SelectorRule::FolderExists { path: v1.trim().to_string() },
                "glob_match" => SelectorRule::GlobMatch { pattern: v1.trim().to_string() },
                "file_contains" => SelectorRule::FileContains { path: v1.trim().to_string(), contains: v2.trim().to_string() },
                "file_regex" => SelectorRule::FileRegex { path: v1.trim().to_string(), pattern: v2.trim().to_string() },
                "env_var_set" => SelectorRule::EnvVarSet { name: v1.trim().to_string() },
                "command_exits" => SelectorRule::CommandExits { command: v1.trim().to_string() },
                _ => SelectorRule::FileExists { path: v1.trim().to_string() },
            })
            .collect();
        SelectorDefinition {
            id: self.editing_id.clone().unwrap_or_else(generate_selector_id),
            name: self.name.trim().to_string(),
            description: self.description.trim().to_string(),
            folder_scope: self.folder_scope.trim().to_string(),
            rules,
            match_mode: match self.match_mode { 1 => MatchMode::AnyMatch, 2 => MatchMode::Custom, _ => MatchMode::AllMatch },
            custom_expression: self.custom_expr.clone(),
            presets: self.presets.iter().cloned().collect(),
            add_skills: self.skills.iter().cloned().collect(),
            add_flocks: self.flocks.iter().cloned().collect(),
            priority: self.priority,
        }
    }
}

// ---------------------------------------------------------------------------
// Collect all known skill slugs
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Page component
// ---------------------------------------------------------------------------

#[component]
pub fn SelectorsPage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut version = use_signal(|| 0u32);
    let mut form = use_signal(|| Option::<SelectorForm>::None);
    let mut selectors_page = use_signal(|| 0usize);
    let mut form_key = use_signal(|| 0u32);
    let mut search = use_signal(String::new);
    let mut detail_selector = use_signal(|| Option::<SelectorDefinition>::None);
    let mut view_mode = use_signal(|| ViewMode::List);

    let _ = *version.read();

    let all_selectors = read_selectors_store().unwrap_or_default().selectors;
    let search_val = search.read().to_lowercase();
    let selectors: Vec<_> = if search_val.is_empty() {
        all_selectors
    } else {
        all_selectors.into_iter().filter(|d| {
            d.name.to_lowercase().contains(&search_val)
                || d.description.to_lowercase().contains(&search_val)
                || d.folder_scope.to_lowercase().contains(&search_val)
        }).collect()
    };
    let current_page = pagination::clamp_page(*selectors_page.read(), selectors.len(), DETECTORS_PAGE_SIZE);
    let visible = pagination::slice_for_page(&selectors, current_page, DETECTORS_PAGE_SIZE);
    let total_pages = pagination::total_pages(selectors.len(), DETECTORS_PAGE_SIZE);
    let form_is_open = form.read().is_some();

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%; position: relative;",
            // ── Sticky header ──
            div { style: "flex-shrink: 0; padding: 12px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10; display: flex; align-items: center; gap: 10px;",
                h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT}; white-space: nowrap;",
                    "{t.selectors_title}"
                }
                div { style: "flex: 1; max-width: 320px; margin-left: auto;",
                    input {
                        r#type: "text", value: "{search}", placeholder: "{t.selectors_search_skills}",
                        style: "width: 100%; padding: 6px 12px; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 13px; background: {Theme::PANEL}; color: {Theme::TEXT}; outline: none;",
                        oninput: move |e: Event<FormData>| search.set(e.value().to_string()),
                    }
                }
                // Refresh
                button {
                    title: "Refresh",
                    style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer; font-size: 16px;",
                    onclick: move |_| version += 1,
                    "\u{21BB}"
                }
                // View toggle
                ViewToggleButton {
                    mode: *view_mode.read(),
                    on_toggle: move |_| {
                        let cur = *view_mode.read();
                        view_mode.set(if cur == ViewMode::Cards { ViewMode::List } else { ViewMode::Cards });
                    },
                }
                // Create
                button {
                    disabled: form_is_open,
                    style: "padding: 7px 16px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 8px; font-size: 13px; font-weight: 700; cursor: pointer; white-space: nowrap;",
                    onclick: move |_| {
                        form_key += 1;
                        form.set(Some(SelectorForm::blank()));
                    },
                    "+ Create"
                }
            }

            // ── Scrollable content ──
            div { style: "flex: 1; overflow-y: auto; padding: 16px 32px 32px;",
            div { style: "max-width: 1180px; display: flex; flex-direction: column; gap: 8px;",

                // ── Selector list ──────────────────────────────────
                if selectors.is_empty() {
                    div { style: "background: {Theme::PANEL}; border: 1px dashed {Theme::LINE}; border-radius: 12px; padding: 32px; text-align: center;",
                        h2 { style: "font-size: 16px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 8px;",
                            "{t.selectors_empty_title}"
                        }
                        p { style: "font-size: 13px; color: {Theme::MUTED};",
                            "{t.selectors_empty_hint}"
                        }
                    }
                } else {
                    { let is_cards = *view_mode.read() == ViewMode::Cards;
                    let container_style = if is_cards {
                        "display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px;"
                    } else {
                        "display: flex; flex-direction: column; gap: 10px;"
                    };
                    rsx! {
                    div { style: "{container_style}",
                        for selector in visible.iter() {
                            SelectorRow {
                                key: "{selector.id}",
                                selector: selector.clone(),
                                form_is_open: form_is_open,
                                card_mode: is_cards,
                                on_click: {
                                    let selector = selector.clone();
                                    move |_| detail_selector.set(Some(selector.clone()))
                                },
                                on_template: {
                                    let selector = selector.clone();
                                    move |_| { form_key += 1; form.set(Some(SelectorForm::from_selector(&selector, true))); }
                                },
                                on_edit: {
                                    let selector = selector.clone();
                                    move |_| { form_key += 1; form.set(Some(SelectorForm::from_selector(&selector, false))); }
                                },
                                on_delete: {
                                    let id = selector.id.clone();
                                    move |_| { let _ = delete_selector(&id); version += 1; }
                                },
                            }
                        }
                        PaginationControls {
                            current_page, total_pages: Some(total_pages),
                            has_prev: current_page > 0, has_next: current_page + 1 < total_pages,
                            on_prev: move |_| selectors_page.set(current_page.saturating_sub(1)),
                            on_next: move |_| selectors_page.set(current_page + 1),
                        }
                    }
                    }}
                }
            } // max-width
            } // scrollable content

            // ── Detail popup ──────────────────────────────────
            if detail_selector.read().is_some() {
                SelectorDetailPopup { selector: detail_selector }
            }

            // ── Form modal ──────────────────────────────────
            if form_is_open {
                SelectorFormModal { key: "{form_key}", form: form, version: version }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Selector card
// ---------------------------------------------------------------------------

/// Selector item. Card mode = small grid card. List mode = full-width row with description.
#[component]
fn SelectorRow(
    selector: SelectorDefinition,
    form_is_open: bool,
    #[props(default = false)]
    card_mode: bool,
    on_click: EventHandler<()>,
    on_template: EventHandler<()>,
    on_edit: EventHandler<()>,
    on_delete: EventHandler<()>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut down_pos = use_signal(|| (0.0f64, 0.0f64));
    let rules_count = selector.rules.len();
    let skills_count = selector.add_skills.len();
    let presets_count = selector.presets.len();
    let flocks_count = selector.add_flocks.len();

    if card_mode {
        // Card view — compact card for grid layout, multiple per row
        rsx! {
            div {
                style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 10px; padding: 14px; cursor: pointer; display: flex; flex-direction: column; gap: 8px;",
                onmousedown: move |evt: Event<MouseData>| { let c = evt.client_coordinates(); down_pos.set((c.x, c.y)); },
                onclick: move |evt: Event<MouseData>| { let c = evt.client_coordinates(); let (dx, dy) = *down_pos.read(); if (c.x - dx).abs() < 5.0 && (c.y - dy).abs() < 5.0 { on_click.call(()); } },
                h3 { style: "font-size: 14px; font-weight: 700; color: {Theme::TEXT};", "{selector.name}" }
                if !selector.description.is_empty() {
                    p { style: "font-size: 12px; color: {Theme::MUTED}; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                        "{selector.description}"
                    }
                }
                div { style: "display: flex; flex-wrap: wrap; gap: 5px; margin-top: auto;",
                    span { style: "font-size: 10px; padding: 2px 7px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px;", "{selector.folder_scope}" }
                    span { style: "font-size: 10px; color: {Theme::MUTED};", "{rules_count}r \u{00B7} {presets_count}p \u{00B7} {skills_count}s \u{00B7} {flocks_count}f" }
                    if selector.priority != 0 {
                        span { style: "font-size: 10px; color: {Theme::MUTED};", "P{selector.priority}" }
                    }
                }
                div { style: "display: flex; gap: 4px;",
                    onclick: move |e: Event<MouseData>| e.stop_propagation(),
                    SmallButton { label: t.selectors_edit, disabled: form_is_open, onclick: move |_| on_edit.call(()) }
                    SmallButton { label: t.selectors_delete, disabled: form_is_open, accent: Theme::DANGER, onclick: move |_| on_delete.call(()) }
                }
            }
        }
    } else {
        // List view — full-width row with description, one per line
        rsx! {
            div {
                style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 12px; padding: 16px; cursor: pointer;",
                onmousedown: move |evt: Event<MouseData>| { let c = evt.client_coordinates(); down_pos.set((c.x, c.y)); },
                onclick: move |evt: Event<MouseData>| { let c = evt.client_coordinates(); let (dx, dy) = *down_pos.read(); if (c.x - dx).abs() < 5.0 && (c.y - dy).abs() < 5.0 { on_click.call(()); } },
                div { style: "display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; margin-bottom: 8px;",
                    div { style: "min-width: 0; flex: 1;",
                        h3 { style: "font-size: 15px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 4px;", "{selector.name}" }
                        if !selector.description.is_empty() {
                            p { style: "font-size: 12px; color: {Theme::MUTED}; line-height: 1.5; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                                "{selector.description}"
                            }
                        }
                    }
                    div { style: "display: flex; gap: 4px; flex-shrink: 0;",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        SmallButton { label: t.selectors_use_template, disabled: form_is_open, onclick: move |_| on_template.call(()) }
                        SmallButton { label: t.selectors_edit, disabled: form_is_open, onclick: move |_| on_edit.call(()) }
                        SmallButton { label: t.selectors_delete, disabled: form_is_open, accent: Theme::DANGER, onclick: move |_| on_delete.call(()) }
                    }
                }
                div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                    span { style: "font-size: 11px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px;", "{selector.folder_scope}" }
                    span { style: "font-size: 11px; color: {Theme::MUTED};", "{rules_count} rules \u{00B7} {presets_count} presets \u{00B7} {skills_count} skills \u{00B7} {flocks_count} flocks" }
                    if selector.priority != 0 {
                        span { style: "font-size: 11px; color: {Theme::MUTED};", "P{selector.priority}" }
                    }
                }
            }
        }
    }
}

/// Detail popup showing full selector info.
#[component]
fn SelectorDetailPopup(selector: Signal<Option<SelectorDefinition>>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut backdrop_pressed = use_signal(|| false);
    let guard = selector.read();
    let Some(d) = guard.as_ref() else { return rsx! {}; };

    let name = d.name.clone();
    let desc = d.description.clone();
    let scope = d.folder_scope.clone();
    let expr = d.display_expression();
    let mode = match d.match_mode { MatchMode::AllMatch => t.selectors_match_all, MatchMode::AnyMatch => t.selectors_match_any, MatchMode::Custom => t.selectors_match_custom };
    let rules = d.rules.clone();
    let presets = d.presets.clone();
    let skills = d.add_skills.clone();
    let flocks = d.add_flocks.clone();
    let priority = d.priority;
    drop(guard);

    rsx! {
        div {
            style: "position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); z-index: 1000; display: flex; align-items: center; justify-content: center; padding: 24px;",
            onmousedown: move |_| backdrop_pressed.set(true),
            onmouseup: move |_| { if *backdrop_pressed.read() { selector.set(None); } backdrop_pressed.set(false); },
            div {
                style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 18px; padding: 24px; width: 640px; max-width: 92vw; max-height: 85vh; overflow-y: auto; box-shadow: 0 30px 80px rgba(0, 0, 0, 0.25);",
                onmousedown: move |evt: Event<MouseData>| evt.stop_propagation(),
                onmouseup: move |evt: Event<MouseData>| evt.stop_propagation(),
                // Header
                div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 16px;",
                    h2 { style: "font-size: 18px; font-weight: 800; color: {Theme::TEXT};", "{name}" }
                    button { style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: transparent; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 16px; color: {Theme::MUTED}; cursor: pointer;",
                        onclick: move |_| selector.set(None), "\u{00D7}"
                    }
                }
                if !desc.is_empty() {
                    p { style: "font-size: 13px; color: {Theme::MUTED}; line-height: 1.6; margin-bottom: 14px;", "{desc}" }
                }
                // Badges
                div { style: "display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 14px;",
                    InfoBadge { label: format!("{}: {scope}", t.selectors_scope_label), color: Theme::ACCENT_STRONG }
                    InfoBadge { label: format!("{mode}"), color: Theme::TEXT }
                    InfoBadge { label: format!("{}: {expr}", t.selectors_expr_label), color: Theme::ACCENT_STRONG }
                    if priority != 0 {
                        InfoBadge { label: format!("{}: {priority}", t.selectors_priority), color: Theme::TEXT }
                    }
                }
                // Rules
                div { style: "padding: 14px; background: rgba(90, 158, 63, 0.06); border: 1px solid rgba(90, 158, 63, 0.12); border-radius: 14px; margin-bottom: 14px;",
                    p { style: "font-size: 11px; font-weight: 700; color: {Theme::ACCENT_STRONG}; margin-bottom: 10px;", "{t.selectors_rules_label}" }
                    div { style: "display: flex; flex-direction: column; gap: 6px;",
                        for (idx, rule) in rules.iter().enumerate() {
                            div { style: "display: flex; align-items: flex-start; gap: 8px;",
                                span { style: "min-width: 18px; height: 18px; display: inline-flex; align-items: center; justify-content: center; background: rgba(90, 158, 63, 0.14); color: {Theme::ACCENT_STRONG}; border-radius: 999px; font-size: 11px; font-weight: 700; flex-shrink: 0;", "{idx + 1}" }
                                p { style: "font-size: 12px; color: {Theme::TEXT}; line-height: 1.55;", "{rule.display()}" }
                            }
                        }
                    }
                }
                // Presets + Skills
                div { style: "display: flex; flex-direction: column; gap: 10px;",
                    if !presets.is_empty() {
                        TagGroup { label: t.selectors_presets_label, items: presets, bg: "rgba(90, 158, 63, 0.12)", color: Theme::ACCENT_STRONG, border: "rgba(90, 158, 63, 0.16)" }
                    }
                    if !skills.is_empty() {
                        TagGroup { label: t.selectors_add_skills_label, items: skills, bg: "rgba(46, 139, 87, 0.10)", color: Theme::SUCCESS, border: "rgba(46, 139, 87, 0.16)" }
                    }
                    if !flocks.is_empty() {
                        TagGroup { label: t.selectors_add_flocks_label, items: flocks, bg: "rgba(90, 120, 200, 0.10)", color: "rgba(50, 80, 160, 0.9)", border: "rgba(90, 120, 200, 0.16)" }
                    }
                }
            }
        }
    }
}

#[component]
fn SmallButton(label: &'static str, disabled: bool, #[props(default = Theme::MUTED)] accent: &'static str, onclick: EventHandler<()>) -> Element {
    let opacity = if disabled { "0.5" } else { "1" };
    let cursor = if disabled { "not-allowed" } else { "pointer" };
    rsx! { button { disabled, style: "padding: 5px 10px; background: transparent; color: {accent}; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 11px; font-weight: 600; cursor: {cursor}; opacity: {opacity};", onclick: move |_| onclick.call(()), "{label}" } }
}

#[component]
fn InfoBadge(label: String, color: &'static str) -> Element {
    rsx! { span { style: "display: inline-flex; align-items: center; padding: 4px 10px; background: rgba(255, 255, 255, 0.78); color: {color}; border: 1px solid {Theme::LINE}; border-radius: 999px; font-size: 11px; font-weight: 600;", "{label}" } }
}

#[component]
fn TagGroup(label: &'static str, items: Vec<String>, bg: &'static str, color: &'static str, border: &'static str) -> Element {
    rsx! {
        div { style: "padding: 12px; background: rgba(255, 255, 255, 0.82); border: 1px solid {Theme::LINE}; border-radius: 14px;",
            p { style: "font-size: 11px; font-weight: 700; color: {Theme::MUTED}; margin-bottom: 8px;", "{label}" }
            div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                for item in items.iter() {
                    span { style: "display: inline-flex; align-items: center; padding: 5px 9px; background: {bg}; color: {color}; border: 1px solid {border}; border-radius: 999px; font-size: 11px; font-weight: 700;", "{item}" }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Form modal
// ---------------------------------------------------------------------------

#[component]
fn SelectorFormModal(form: Signal<Option<SelectorForm>>, version: Signal<u32>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut backdrop_pressed = use_signal(|| false);
    let mut skill_search = use_signal(String::new);
    let _skill_manual = use_signal(String::new);
    let mut flock_search = use_signal(String::new);

    let is_editing = form.read().as_ref().is_some_and(|f| f.editing_id.is_some());
    let title = if is_editing { t.selectors_edit_title } else { t.selectors_new_title };

    let mut set_field = move |mutator: Box<dyn FnOnce(&mut SelectorForm)>| {
        form.with_mut(|opt| { if let Some(f) = opt.as_mut() { mutator(f); } });
    };

    // Collect available presets and skills
    let all_presets: Vec<(String, Option<String>)> = read_presets_store()
        .map(|s| s.presets.into_iter().map(|(k, v)| (k, v.description)).collect())
        .unwrap_or_default();
    let mut installed_skills_sig = use_signal(Vec::<String>::new);
    use_effect(move || {
        spawn(async move {
            let slugs = tokio::task::spawn_blocking(|| {
                savhub_local::registry::list_installed_slugs().unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            installed_skills_sig.set(slugs);
        });
    });
    let installed_skills = installed_skills_sig.read().clone();

    let save = move |_: MouseEvent| {
        let guard = form.read();
        let Some(f) = guard.as_ref() else { return };
        if f.name.trim().is_empty() {
            drop(guard);
            set_field(Box::new(|f| f.error = "Name is required".to_string()));
            return;
        }
        let def = f.to_definition();
        if def.rules.is_empty() {
            drop(guard);
            set_field(Box::new(|f| f.error = "At least one rule is required".to_string()));
            return;
        }
        if let Err(e) = def.build_expression() {
            let msg = format!("{e}");
            drop(guard);
            set_field(Box::new(move |f| f.error = msg));
            return;
        }
        let is_edit = f.editing_id.is_some();
        drop(guard);
        let result = if is_edit { update_selector(def) } else { create_selector(def) };
        if let Err(e) = result {
            let msg = format!("{e}");
            set_field(Box::new(move |f| f.error = msg));
            return;
        }
        form.set(None);
        version.with_mut(|v| *v += 1);
    };

    // Read form snapshot
    let guard = form.read();
    let Some(f) = guard.as_ref() else { return rsx! {}; };
    let name_val = f.name.clone();
    let desc_val = f.description.clone();
    let scope_val = f.folder_scope.clone();
    let rules_snapshot = f.rules.clone();
    let rules_count = rules_snapshot.len();
    let match_mode = f.match_mode;
    let custom_expr_val = f.custom_expr.clone();
    let selected_presets = f.presets.clone();
    let selected_skills = f.skills.clone();
    let selected_flocks = f.flocks.clone();
    let priority_val = f.priority;
    let error_val = f.error.clone();
    drop(guard);

    let mut all_flock_slugs_sig = use_signal(Vec::<String>::new);
    use_effect(move || {
        spawn(async move {
            let slugs = tokio::task::spawn_blocking(|| {
                savhub_local::registry::list_flock_slugs().unwrap_or_default()
            })
            .await
            .unwrap_or_default();
            all_flock_slugs_sig.set(slugs);
        });
    });
    let all_flock_slugs = all_flock_slugs_sig.read().clone();

    let skill_search_val = skill_search.read().to_lowercase();
    // Only show unselected skills that match the search query; empty search = show nothing
    let skill_suggestions: Vec<&String> = if skill_search_val.is_empty() {
        Vec::new()
    } else {
        installed_skills.iter()
            .filter(|s| !selected_skills.contains(*s) && s.to_lowercase().contains(&skill_search_val))
            .take(20) // limit visible suggestions
            .collect()
    };

    let flock_search_val = flock_search.read().to_lowercase();
    let flock_suggestions: Vec<&String> = if flock_search_val.is_empty() {
        Vec::new()
    } else {
        all_flock_slugs.iter()
            .filter(|s| !selected_flocks.contains(*s) && s.to_lowercase().contains(&flock_search_val))
            .take(20)
            .collect()
    };

    let input_style = format!("width: 100%; padding: 8px 12px; border: 1px solid {}; border-radius: 8px; font-size: 13px; background: white; color: {};", Theme::LINE, Theme::TEXT);
    let label_style = format!("font-size: 12px; font-weight: 700; color: {}; margin-bottom: 4px;", Theme::MUTED);

    rsx! {
        // Backdrop
        div {
            style: "position: fixed; inset: 0; background: rgba(0, 0, 0, 0.4); z-index: 1000; display: flex; align-items: center; justify-content: center; padding: 24px;",
            onmousedown: move |_| backdrop_pressed.set(true),
            onmouseup: move |_| { if *backdrop_pressed.read() { form.set(None); } backdrop_pressed.set(false); },

            // Modal
            div {
                style: "background: {Theme::PANEL}; border: 2px solid {Theme::ACCENT}; border-radius: 18px; padding: 28px; width: 720px; max-width: 92vw; max-height: 92vh; overflow-y: auto; box-shadow: 0 30px 80px rgba(0, 0, 0, 0.25);",
                onmousedown: move |evt: Event<MouseData>| evt.stop_propagation(),
                onmouseup: move |evt: Event<MouseData>| evt.stop_propagation(),

                // Title + close button
                div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 18px;",
                    h2 { style: "font-size: 18px; font-weight: 800; color: {Theme::TEXT};", "{title}" }
                    button {
                        style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: transparent; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 16px; color: {Theme::MUTED}; cursor: pointer;",
                        onclick: move |_| form.set(None),
                        "\u{00D7}"
                    }
                }

                div { style: "display: flex; flex-direction: column; gap: 14px;",
                    // Row 1: Name + Description side by side
                    div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 12px;",
                        div {
                            p { style: "{label_style}", "{t.selectors_name_label}" }
                            input { r#type: "text", value: name_val, style: "{input_style}",
                                oninput: move |evt: Event<FormData>| { let v = evt.value().to_string(); set_field(Box::new(move |f| f.name = v)); },
                            }
                        }
                        div {
                            p { style: "{label_style}", "{t.selectors_desc_label}" }
                            input { r#type: "text", value: desc_val, style: "{input_style}",
                                oninput: move |evt: Event<FormData>| { let v = evt.value().to_string(); set_field(Box::new(move |f| f.description = v)); },
                            }
                        }
                    }
                    // Row 2: Folder Scope + Expression mode + Priority
                    div { style: "display: grid; grid-template-columns: 1fr 1fr auto; gap: 12px;",
                        div {
                            p { style: "{label_style}", "{t.selectors_scope_label}" }
                            input { r#type: "text", value: scope_val, placeholder: ".", style: "{input_style}",
                                oninput: move |evt: Event<FormData>| { let v = evt.value().to_string(); set_field(Box::new(move |f| f.folder_scope = v)); },
                            }
                        }
                        div {
                            p { style: "{label_style}", "{t.selectors_expr_label}" }
                            div { style: "display: flex; gap: 6px;",
                                ModeButton { label: t.selectors_match_all, active: match_mode == 0, onclick: move |_| set_field(Box::new(|f| f.match_mode = 0)) }
                                ModeButton { label: t.selectors_match_any, active: match_mode == 1, onclick: move |_| set_field(Box::new(|f| f.match_mode = 1)) }
                                ModeButton { label: t.selectors_match_custom, active: match_mode == 2, onclick: move |_| set_field(Box::new(|f| f.match_mode = 2)) }
                            }
                        }
                        div {
                            p { style: "{label_style}", "{t.selectors_priority}" }
                            input {
                                r#type: "number", value: "{priority_val}",
                                style: "width: 70px; padding: 8px 12px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 13px; background: white; color: {Theme::TEXT};",
                                oninput: move |evt: Event<FormData>| {
                                    let v = evt.value().parse::<i32>().unwrap_or(0);
                                    set_field(Box::new(move |f| f.priority = v));
                                },
                            }
                        }
                    }
                    // Custom expression (only if Custom mode)
                    if match_mode == 2 {
                        div {
                            input { r#type: "text", value: custom_expr_val, placeholder: "(1 && 2) || !3", style: "{input_style}",
                                oninput: move |evt: Event<FormData>| { let v = evt.value().to_string(); set_field(Box::new(move |f| f.custom_expr = v)); },
                            }
                            p { style: "font-size: 11px; color: {Theme::MUTED}; margin-top: 3px;", "{t.selectors_expr_hint}" }
                        }
                    }
                    // Rules
                    div {
                        p { style: "{label_style}", "{t.selectors_rules_label}" }
                        div { style: "display: flex; flex-direction: column; gap: 6px;",
                            for (idx, (kind, val1, val2)) in rules_snapshot.iter().enumerate() {
                                div { key: "{idx}", style: "display: flex; align-items: center; gap: 8px; flex-wrap: wrap;",
                                    span { style: "min-width: 20px; font-size: 12px; font-weight: 700; color: {Theme::ACCENT_STRONG};", "{idx + 1}." }
                                    select {
                                        value: kind.clone(),
                                        style: "padding: 6px 8px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                                        onchange: move |evt: Event<FormData>| { let v = evt.value().to_string(); set_field(Box::new(move |f| { if let Some(r) = f.rules.get_mut(idx) { r.0 = v; } })); },
                                        option { value: "file_exists", "{t.selectors_file_exists}" }
                                        option { value: "folder_exists", "{t.selectors_folder_exists}" }
                                        option { value: "glob_match", "{t.selectors_glob_match}" }
                                        option { value: "file_contains", "{t.selectors_file_contains}" }
                                        option { value: "file_regex", "{t.selectors_file_regex}" }
                                        option { value: "env_var_set", "{t.selectors_env_var_set}" }
                                        option { value: "command_exits", "{t.selectors_command_exits}" }
                                    }
                                    input {
                                        r#type: "text", value: val1.clone(),
                                        placeholder: match kind.as_str() {
                                            "glob_match" => "**/*.rs",
                                            "env_var_set" => "ENV_VAR_NAME",
                                            "command_exits" => "rustc --version",
                                            "file_contains" | "file_regex" => "path/to/file",
                                            _ => "path/to/file",
                                        },
                                        style: "flex: 1; min-width: 120px; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                                        oninput: move |evt: Event<FormData>| { let v = evt.value().to_string(); set_field(Box::new(move |f| { if let Some(r) = f.rules.get_mut(idx) { r.1 = v; } })); },
                                    }
                                    // Second input for file_contains / file_regex
                                    if kind == "file_contains" || kind == "file_regex" {
                                        input {
                                            r#type: "text", value: val2.clone(),
                                            placeholder: if kind == "file_regex" { "regex pattern..." } else { "search text..." },
                                            style: "flex: 1; min-width: 120px; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                                            oninput: move |evt: Event<FormData>| { let v = evt.value().to_string(); set_field(Box::new(move |f| { if let Some(r) = f.rules.get_mut(idx) { r.2 = v; } })); },
                                        }
                                    }
                                    if rules_count > 1 {
                                        button { style: "padding: 4px 8px; background: transparent; color: {Theme::DANGER}; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 11px; cursor: pointer;",
                                            onclick: move |_| set_field(Box::new(move |f| { f.rules.remove(idx); })), "x"
                                        }
                                    }
                                }
                            }
                            button {
                                style: "align-self: flex-start; padding: 5px 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: 1px solid rgba(90, 158, 63, 0.18); border-radius: 8px; font-size: 12px; font-weight: 600; cursor: pointer;",
                                onclick: move |_| set_field(Box::new(|f| f.rules.push(("file_exists".to_string(), String::new(), String::new())))),
                                "+ {t.selectors_add_rule}"
                            }
                        }
                    }
                    // Presets (full width)
                    if !all_presets.is_empty() {
                        div {
                            p { style: "{label_style}", "{t.selectors_presets_label}" }
                            div { style: "display: flex; flex-direction: column; gap: 4px; max-height: 180px; overflow-y: auto; padding: 8px; background: rgba(255,255,255,0.6); border: 1px solid {Theme::LINE}; border-radius: 10px;",
                                for (preset_name, preset_desc) in all_presets.iter() {
                                    { let name = preset_name.clone();
                                      let checked = selected_presets.contains(&name);
                                      let desc_text = preset_desc.as_deref().unwrap_or("").to_string();
                                      rsx! {
                                        label { style: "display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: 6px; cursor: pointer; font-size: 13px; color: {Theme::TEXT};",
                                            input {
                                                r#type: "checkbox", checked: checked,
                                                onchange: { let name = name.clone(); move |_| { let n = name.clone(); set_field(Box::new(move |f| { if f.presets.contains(&n) { f.presets.remove(&n); } else { f.presets.insert(n); } })); } },
                                            }
                                            span { style: "font-weight: 600;", "{name}" }
                                            if !desc_text.is_empty() {
                                                span { style: "color: {Theme::MUTED}; font-size: 11px;", "— {desc_text}" }
                                            }
                                        }
                                    }}
                                }
                            }
                        }
                    }
                    // Skills — search-to-add from installed skills
                    div {
                        p { style: "{label_style}", "{t.selectors_add_skills_label}" }
                        // Selected skills as removable tags
                        if !selected_skills.is_empty() {
                            div { style: "display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px;",
                                for skill in selected_skills.iter() {
                                    { let slug = skill.clone();
                                      rsx! {
                                        span { style: "display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: 1px solid rgba(90, 158, 63, 0.18); border-radius: 999px; font-size: 12px; font-weight: 600;",
                                            "{slug}"
                                            button {
                                                style: "background: none; border: none; color: {Theme::DANGER}; font-size: 13px; cursor: pointer; padding: 0 2px; line-height: 1;",
                                                onclick: { let slug = slug.clone(); move |_| { let s = slug.clone(); set_field(Box::new(move |f| { f.skills.remove(&s); })); } },
                                                "\u{00D7}"
                                            }
                                        }
                                    }}
                                }
                            }
                        }
                        // Search input
                        input {
                            r#type: "text", value: "{skill_search}", placeholder: t.selectors_search_skills,
                            style: "width: 100%; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                            oninput: move |evt: Event<FormData>| skill_search.set(evt.value().to_string()),
                        }
                        // Search results — only unselected skills matching the query
                        if !skill_suggestions.is_empty() {
                            div { style: "display: flex; flex-direction: column; gap: 2px; max-height: 160px; overflow-y: auto; padding: 6px; margin-top: 4px; background: rgba(255,255,255,0.6); border: 1px solid {Theme::LINE}; border-radius: 8px;",
                                for slug in skill_suggestions.iter() {
                                    { let s = (*slug).clone();
                                      rsx! {
                                        button {
                                            style: "display: flex; align-items: center; gap: 6px; padding: 5px 8px; background: transparent; border: none; border-radius: 6px; cursor: pointer; font-size: 12px; color: {Theme::TEXT}; text-align: left; width: 100%;",
                                            onclick: { let s = s.clone(); move |_| {
                                                let slug = s.clone();
                                                set_field(Box::new(move |f| { f.skills.insert(slug); }));
                                                skill_search.set(String::new());
                                            }},
                                            span { style: "color: {Theme::ACCENT_STRONG}; font-size: 14px;", "+" }
                                            "{s}"
                                        }
                                    }}
                                }
                            }
                        }
                        if installed_skills.is_empty() {
                            p { style: "font-size: 12px; color: {Theme::MUTED}; margin-top: 4px;", "No installed skills." }
                        }
                    }
                    // Flocks — search-to-add from registry flocks
                    div {
                        p { style: "{label_style}", "{t.selectors_add_flocks_label}" }
                        // Selected flocks as removable tags
                        if !selected_flocks.is_empty() {
                            div { style: "display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px;",
                                for flock in selected_flocks.iter() {
                                    { let slug = flock.clone();
                                      rsx! {
                                        span { style: "display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; background: rgba(90, 120, 200, 0.10); color: rgba(50, 80, 160, 0.9); border: 1px solid rgba(90, 120, 200, 0.18); border-radius: 999px; font-size: 12px; font-weight: 600;",
                                            "{slug}"
                                            button {
                                                style: "background: none; border: none; color: {Theme::DANGER}; font-size: 13px; cursor: pointer; padding: 0 2px; line-height: 1;",
                                                onclick: { let slug = slug.clone(); move |_| { let s = slug.clone(); set_field(Box::new(move |f| { f.flocks.remove(&s); })); } },
                                                "\u{00D7}"
                                            }
                                        }
                                    }}
                                }
                            }
                        }
                        // Search input
                        input {
                            r#type: "text", value: "{flock_search}", placeholder: t.selectors_search_flocks,
                            style: "width: 100%; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                            oninput: move |evt: Event<FormData>| flock_search.set(evt.value().to_string()),
                        }
                        // Search results
                        if !flock_suggestions.is_empty() {
                            div { style: "display: flex; flex-direction: column; gap: 2px; max-height: 160px; overflow-y: auto; padding: 6px; margin-top: 4px; background: rgba(255,255,255,0.6); border: 1px solid {Theme::LINE}; border-radius: 8px;",
                                for slug in flock_suggestions.iter() {
                                    { let s = (*slug).clone();
                                      rsx! {
                                        button {
                                            style: "display: flex; align-items: center; gap: 6px; padding: 5px 8px; background: transparent; border: none; border-radius: 6px; cursor: pointer; font-size: 12px; color: {Theme::TEXT}; text-align: left; width: 100%;",
                                            onclick: { let s = s.clone(); move |_| {
                                                let slug = s.clone();
                                                set_field(Box::new(move |f| { f.flocks.insert(slug); }));
                                                flock_search.set(String::new());
                                            }},
                                            span { style: "color: rgba(50, 80, 160, 0.9); font-size: 14px;", "+" }
                                            "{s}"
                                        }
                                    }}
                                }
                            }
                        }
                        if all_flock_slugs.is_empty() {
                            p { style: "font-size: 12px; color: {Theme::MUTED}; margin-top: 4px;", "No flocks in registry. Sync first." }
                        }
                    }
                    // Error
                    if !error_val.is_empty() {
                        p { style: "font-size: 12px; color: {Theme::DANGER}; padding: 8px 12px; background: rgba(139, 30, 30, 0.06); border-radius: 8px;", "{error_val}" }
                    }
                    // Actions
                    div { style: "display: flex; gap: 10px; justify-content: flex-end;",
                        button { style: "padding: 8px 20px; background: transparent; color: {Theme::MUTED}; border: 1px solid {Theme::LINE}; border-radius: 10px; font-size: 13px; font-weight: 600; cursor: pointer;",
                            onclick: move |_| form.set(None), "{t.selectors_cancel}"
                        }
                        button { style: "padding: 8px 20px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 10px; font-size: 13px; font-weight: 700; cursor: pointer;",
                            onclick: save, "{t.selectors_save}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn ModeButton(label: &'static str, active: bool, onclick: EventHandler<()>) -> Element {
    let bg = if active { Theme::ACCENT } else { "transparent" };
    let color = if active { "white" } else { Theme::TEXT };
    let border = if active { Theme::ACCENT } else { Theme::LINE };
    rsx! { button { style: "padding: 6px 14px; background: {bg}; color: {color}; border: 1px solid {border}; border-radius: 8px; font-size: 12px; font-weight: 600; cursor: pointer;", onclick: move |_| onclick.call(()), "{label}" } }
}
