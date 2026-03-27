use std::collections::BTreeSet;

use dioxus::prelude::*;
use savhub_local::selectors::{
    MatchMode, SelectorDefinition, SelectorRule, clone_official_as_custom, create_selector,
    delete_selector, generate_selector_id, normalize_repo_url_to_sign,
    read_official_selectors_store, read_selector_prefs, read_selectors_store,
    set_all_custom_selectors_enabled, set_all_official_selectors_enabled, set_selector_enabled,
    update_selector,
};

use crate::components::view_toggle::{ViewMode, ViewToggleButton};
use crate::i18n;
use crate::state::AppState;
use crate::theme::Theme;

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
    skills: BTreeSet<String>,
    flocks: BTreeSet<String>,
    repos: BTreeSet<String>,
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
            skills: BTreeSet::new(),
            flocks: BTreeSet::new(),
            repos: BTreeSet::new(),
            priority: 0,
            error: String::new(),
        }
    }

    fn from_selector(d: &SelectorDefinition, as_template: bool) -> Self {
        let rules = d
            .rules
            .iter()
            .map(|r| match r {
                SelectorRule::FileExists { path } => {
                    ("file_exists".to_string(), path.clone(), String::new())
                }
                SelectorRule::FolderExists { path } => {
                    ("folder_exists".to_string(), path.clone(), String::new())
                }
                SelectorRule::GlobMatch { pattern } => {
                    ("glob_match".to_string(), pattern.clone(), String::new())
                }
                SelectorRule::FileContains { path, contains } => {
                    ("file_contains".to_string(), path.clone(), contains.clone())
                }
                SelectorRule::FileRegex { path, pattern } => {
                    ("file_regex".to_string(), path.clone(), pattern.clone())
                }
                SelectorRule::EnvVarSet { name } => {
                    ("env_var_set".to_string(), name.clone(), String::new())
                }
                SelectorRule::CommandExits { command } => {
                    ("command_exits".to_string(), command.clone(), String::new())
                }
            })
            .collect();
        Self {
            editing_id: if as_template {
                None
            } else {
                Some(d.sign.clone())
            },
            name: if as_template {
                format!("{} (copy)", d.name)
            } else {
                d.name.clone()
            },
            description: d.description.clone(),
            folder_scope: d.folder_scope.clone(),
            rules,
            match_mode: match d.match_mode {
                MatchMode::AllMatch => 0,
                MatchMode::AnyMatch => 1,
                MatchMode::Custom => 2,
            },
            custom_expr: d.custom_expression.clone(),
            skills: d.skills.iter().map(|s| s.to_string()).collect(),
            flocks: d.flocks.iter().map(|s| s.to_string()).collect(),
            repos: d.repos.iter().map(|r| r.git_url.clone()).collect(),
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
                "folder_exists" => SelectorRule::FolderExists {
                    path: v1.trim().to_string(),
                },
                "glob_match" => SelectorRule::GlobMatch {
                    pattern: v1.trim().to_string(),
                },
                "file_contains" => SelectorRule::FileContains {
                    path: v1.trim().to_string(),
                    contains: v2.trim().to_string(),
                },
                "file_regex" => SelectorRule::FileRegex {
                    path: v1.trim().to_string(),
                    pattern: v2.trim().to_string(),
                },
                "env_var_set" => SelectorRule::EnvVarSet {
                    name: v1.trim().to_string(),
                },
                "command_exits" => SelectorRule::CommandExits {
                    command: v1.trim().to_string(),
                },
                _ => SelectorRule::FileExists {
                    path: v1.trim().to_string(),
                },
            })
            .collect();
        SelectorDefinition {
            sign: self.editing_id.clone().unwrap_or_else(generate_selector_id),
            name: self.name.trim().to_string(),
            description: self.description.trim().to_string(),
            folder_scope: self.folder_scope.trim().to_string(),
            rules,
            match_mode: match self.match_mode {
                1 => MatchMode::AnyMatch,
                2 => MatchMode::Custom,
                _ => MatchMode::AllMatch,
            },
            custom_expression: self.custom_expr.clone(),
            skills: self
                .skills
                .iter()
                .map(|s| savhub_local::selectors::SelectorSkillRef::parse(s))
                .collect(),
            flocks: self
                .flocks
                .iter()
                .map(|s| savhub_local::selectors::SelectorSkillRef::parse(s))
                .collect(),
            repos: self
                .repos
                .iter()
                .map(|url| savhub_local::selectors::SelectorRepo::from_url(url))
                .collect(),
            priority: self.priority,
            match_count: 0,
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
    let mut form_key = use_signal(|| 0u32);
    let mut search = use_signal(String::new);
    let mut detail_selector = use_signal(|| Option::<SelectorDefinition>::None);
    let mut view_mode = use_signal(|| ViewMode::List);
    let mut syncing = use_signal(|| false);
    let mut show_official = use_signal(|| true); // true=Official, false=Custom

    let _ = *version.read();

    // Background push of custom selectors to server after any mutation.
    let push_selectors_to_server = move || {
        let api_base = state.api_base.read().clone();
        let token = state.token.read().clone();
        if let Some(token) = token {
            spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    savhub_local::selectors::push_custom_selectors(&api_base, &token)
                })
                .await;
            });
        }
    };

    // ── Official selectors ──
    let official_store = read_official_selectors_store().unwrap_or_default();
    let selector_prefs = read_selector_prefs().unwrap_or_default();
    let official_count = official_store.selectors.len();
    let custom_count = read_selectors_store()
        .map(|s| s.selectors.len())
        .unwrap_or(0);
    eprintln!(
        "[savhub] selectors page render: {official_count} official, {custom_count} custom, version={}",
        *version.peek()
    );
    let _all_official_disabled = official_count > 0
        && official_store
            .selectors
            .iter()
            .all(|e| selector_prefs.disabled.contains(&e.selector.sign));

    // ── Custom selectors ──
    let mut custom_selectors = read_selectors_store().unwrap_or_default().selectors;
    custom_selectors.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let search_val = search.read().to_lowercase();
    let custom_filtered: Vec<_> = if search_val.is_empty() {
        custom_selectors
    } else {
        custom_selectors
            .into_iter()
            .filter(|d| {
                d.name.to_lowercase().contains(&search_val)
                    || d.description.to_lowercase().contains(&search_val)
                    || d.folder_scope.to_lowercase().contains(&search_val)
            })
            .collect()
    };
    let official_filtered: Vec<_> = if search_val.is_empty() {
        official_store.selectors.clone()
    } else {
        official_store
            .selectors
            .iter()
            .filter(|e| {
                e.selector.name.to_lowercase().contains(&search_val)
                    || e.selector.description.to_lowercase().contains(&search_val)
                    || e.tags
                        .iter()
                        .any(|tag| tag.to_lowercase().contains(&search_val))
            })
            .cloned()
            .collect()
    };
    let form_is_open = form.read().is_some();
    let is_cards = *view_mode.read() == ViewMode::Cards;
    let container_style = if is_cards {
        "display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr)); gap: 12px;"
    } else {
        "display: flex; flex-direction: column; gap: 10px;"
    };

    // ── Sync official selectors on page mount (silent, no spinner) ──
    let mut sync_triggered = use_signal(|| false);
    if !*sync_triggered.read() {
        sync_triggered.set(true);
        let api_base = state.api_base.read().clone();
        eprintln!("[savhub] selectors page mount: syncing official selectors, api_base={api_base}");
        spawn(async move {
            let result = tokio::task::spawn_blocking(move || {
                savhub_local::selectors::sync_official_selectors(&api_base)
            })
            .await;
            match &result {
                Ok(Ok(updated)) => {
                    eprintln!("[savhub] selectors page sync done, updated={updated}")
                }
                Ok(Err(e)) => eprintln!("[savhub] selectors page sync failed: {e}"),
                Err(e) => eprintln!("[savhub] selectors page sync task panicked: {e}"),
            }
            version += 1;
        });
    }

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%; position: relative;",
            // ── Sticky header ──
            div { style: "flex-shrink: 0; padding: 12px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10; display: flex; align-items: center; gap: 10px;",
                h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT}; white-space: nowrap;",
                    "{t.selectors_title}"
                }
                // Official / Custom toggle
                {
                    let is_official = *show_official.read();
                    let active_style = format!("padding: 5px 14px; font-size: 12px; font-weight: 600; border: none; border-radius: 6px; cursor: pointer; background: {}; color: white;", Theme::ACCENT);
                    let inactive_style = format!("padding: 5px 14px; font-size: 12px; font-weight: 600; border: none; border-radius: 6px; cursor: pointer; background: transparent; color: {};", Theme::MUTED);
                    rsx! {
                        div { style: "display: inline-flex; align-items: center; background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 2px;",
                            button {
                                style: if is_official { "{active_style}" } else { "{inactive_style}" },
                                onclick: move |_| show_official.set(true),
                                "{t.selectors_official_title}"
                            }
                            button {
                                style: if !is_official { "{active_style}" } else { "{inactive_style}" },
                                onclick: move |_| show_official.set(false),
                                "{t.selectors_custom_title}"
                            }
                        }
                    }
                }
                // Enable All / Disable All
                button {
                    style: "padding: 4px 10px; font-size: 11px; border: 1px solid {Theme::LINE}; border-radius: 6px; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; cursor: pointer;",
                    onclick: {

                        move |_| {
                            if *show_official.read() {
                                let _ = set_all_official_selectors_enabled(true);
                            } else {
                                let _ = set_all_custom_selectors_enabled(true);
                                push_selectors_to_server();
                            }
                            version += 1;
                        }
                    },
                    "{t.selectors_enable_all}"
                }
                button {
                    style: "padding: 4px 10px; font-size: 11px; border: 1px solid {Theme::LINE}; border-radius: 6px; background: {Theme::PANEL}; color: {Theme::MUTED}; cursor: pointer;",
                    onclick: {

                        move |_| {
                            if *show_official.read() {
                                let _ = set_all_official_selectors_enabled(false);
                            } else {
                                let _ = set_all_custom_selectors_enabled(false);
                                push_selectors_to_server();
                            }
                            version += 1;
                        }
                    },
                    "{t.selectors_disable_all}"
                }
                div { style: "flex: 1; max-width: 200px; margin-left: auto;",
                    input {
                        r#type: "text", value: "{search}", placeholder: "{t.selectors_search_skills}",
                        style: "width: 100%; padding: 6px 12px; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 13px; background: {Theme::PANEL}; color: {Theme::TEXT}; outline: none;",
                        oninput: move |e: Event<FormData>| search.set(e.value().to_string()),
                    }
                }
                // Refresh / Sync
                button {
                    title: if *show_official.read() { "Sync with server" } else { "Refresh" },
                    disabled: *syncing.read(),
                    style: if *syncing.read() {
                        format!("display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {panel}; color: {muted}; border: 1px solid {line}; border-radius: 8px; cursor: not-allowed; font-size: 16px; opacity: 0.6;", panel = Theme::PANEL, muted = Theme::MUTED, line = Theme::LINE)
                    } else {
                        format!("display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {panel}; color: {accent}; border: 1px solid {line}; border-radius: 8px; cursor: pointer; font-size: 16px;", panel = Theme::PANEL, accent = Theme::ACCENT_STRONG, line = Theme::LINE)
                    },
                    onclick: {
                        let api_base = state.api_base.read().clone();
                        move |_| {
                            if *syncing.read() { return; }
                            syncing.set(true);
                            let api_base = api_base.clone();
                            let is_official = *show_official.read();
                            spawn(async move {
                                if is_official {
                                    let _ = tokio::task::spawn_blocking(move || {
                                        savhub_local::selectors::sync_official_selectors(&api_base)
                                    }).await;
                                }
                                syncing.set(false);
                                version += 1;
                            });
                        }
                    },
                    span {
                        style: if *syncing.read() { "display: inline-flex; animation: spin 1s linear infinite;" } else { "display: inline-flex;" },
                        crate::icons::LucideIcon { icon: crate::icons::Icon::RefreshCw, size: 14 }
                    }
                }
                // View toggle
                ViewToggleButton {
                    mode: *view_mode.read(),
                    on_toggle: move |_| {
                        let cur = *view_mode.read();
                        view_mode.set(if cur == ViewMode::Cards { ViewMode::List } else { ViewMode::Cards });
                    },
                }
                // Create custom selector (only in Custom tab)
                if !*show_official.read() {
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
            }

            // ── Scrollable content ──
            div { style: "flex: 1; overflow-y: auto; padding: 16px 32px 32px;",
            div { style: "max-width: 1180px; display: flex; flex-direction: column; gap: 20px;",

                // ══════════════════════════════════════════════
                // Official / Custom Selectors (toggled)
                // ══════════════════════════════════════════════
                if *show_official.read() {
                div { style: "display: flex; flex-direction: column; gap: 10px;",
                    // Official selector cards/list
                    if official_filtered.is_empty() {
                        div { style: "background: {Theme::PANEL}; border: 1px dashed {Theme::LINE}; border-radius: 10px; padding: 20px; text-align: center;",
                            p { style: "font-size: 12px; color: {Theme::MUTED};",
                                "No official selectors yet. Click the refresh button to sync from server."
                            }
                        }
                    } else {
                        div { style: "{container_style}",
                            for entry in official_filtered.iter() {
                                { let is_disabled = selector_prefs.disabled.contains(&entry.selector.sign);
                                let selector = entry.selector.clone();
                                let tags = entry.tags.clone();
                                rsx! {
                                SelectorRow {
                                    key: "{selector.sign}",
                                    selector: selector.clone(),
                                    form_is_open: form_is_open,
                                    card_mode: is_cards,
                                    is_official: true,
                                    tags: tags,
                                    is_enabled: !is_disabled,
                                    on_click: {
                                        let selector = selector.clone();
                                        move |_| detail_selector.set(Some(selector.clone()))
                                    },
                                    on_template: {
                                        let sign = selector.sign.clone();

                                        move |_| {
                                            if let Ok(cloned) = clone_official_as_custom(&sign) {
                                                form_key += 1;
                                                form.set(Some(SelectorForm::from_selector(&cloned, false)));
                                                version += 1;
                                                push_selectors_to_server();
                                            }
                                        }
                                    },
                                    on_edit: move |_| {},
                                    on_delete: move |_| {},
                                    on_toggle: {
                                        let sign = selector.sign.clone();
                                        move |_| {
                                            let _ = set_selector_enabled(&sign, is_disabled);
                                            version += 1;
                                        }
                                    },
                                }
                                }}
                            }
                        }
                    }
                }
                } else {
                // ══════════════════════════════════════════════
                // Custom Selectors
                // ══════════════════════════════════════════════
                div { style: "display: flex; flex-direction: column; gap: 10px;",
                    // Custom selector cards/list
                    if custom_filtered.is_empty() {
                        div { style: "background: {Theme::PANEL}; border: 1px dashed {Theme::LINE}; border-radius: 10px; padding: 20px; text-align: center;",
                            p { style: "font-size: 12px; color: {Theme::MUTED};",
                                "{t.selectors_empty_hint}"
                            }
                        }
                    } else {
                        div { style: "{container_style}",
                            for selector in custom_filtered.iter() {
                                { let is_disabled = selector_prefs.disabled.contains(&selector.sign);
                                rsx! {
                                SelectorRow {
                                    key: "{selector.sign}",
                                    selector: selector.clone(),
                                    form_is_open: form_is_open,
                                    card_mode: is_cards,
                                    is_official: false,
                                    is_enabled: !is_disabled,
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
                                        let id = selector.sign.clone();

                                        move |_| { let _ = delete_selector(&id); version += 1; push_selectors_to_server(); }
                                    },
                                    on_toggle: {
                                        let sign = selector.sign.clone();
                                        move |_| { let _ = set_selector_enabled(&sign, is_disabled); version += 1; }
                                    },
                                }
                                }}
                            }
                        }
                    }
                }
                } // end custom

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
///
/// When `is_official` is true, the card shows a lock badge and only toggle + clone buttons.
#[component]
fn SelectorRow(
    selector: SelectorDefinition,
    form_is_open: bool,
    #[props(default = false)] card_mode: bool,
    #[props(default = false)] is_official: bool,
    #[props(default = Vec::new())] tags: Vec<String>,
    #[props(default = true)] is_enabled: bool,
    on_click: EventHandler<()>,
    on_template: EventHandler<()>,
    on_edit: EventHandler<()>,
    on_delete: EventHandler<()>,
    on_toggle: EventHandler<()>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut down_pos = use_signal(|| (0.0f64, 0.0f64));
    let rules_count = selector.rules.len();
    let skills_count = selector.skills.len();
    let flocks_count = selector.flocks.len();
    let repos_count = selector.repos.len();
    let match_count = selector.match_count;
    let opacity = if is_enabled { "1" } else { "0.5" };
    let toggle_label = if is_enabled {
        t.selectors_disable
    } else {
        t.selectors_enable
    };
    let border_color = if is_official {
        "rgba(90, 158, 63, 0.25)"
    } else {
        Theme::LINE
    };

    if card_mode {
        // Card view — compact card for grid layout, multiple per row
        rsx! {
            div {
                style: "background: {Theme::PANEL}; border: 1px solid {border_color}; border-radius: 10px; padding: 14px; cursor: pointer; display: flex; flex-direction: column; gap: 8px; opacity: {opacity}; position: relative;",
                onmousedown: move |evt: Event<MouseData>| { let c = evt.client_coordinates(); down_pos.set((c.x, c.y)); },
                onclick: move |evt: Event<MouseData>| { let c = evt.client_coordinates(); let (dx, dy) = *down_pos.read(); if (c.x - dx).abs() < 5.0 && (c.y - dy).abs() < 5.0 { on_click.call(()); } },
                // Official badge
                if is_official {
                    span { style: "position: absolute; top: 8px; right: 8px; display: inline-flex; align-items: center; gap: 3px; font-size: 9px; padding: 2px 6px; background: rgba(90, 158, 63, 0.10); color: {Theme::ACCENT_STRONG}; border-radius: 999px;",
                        crate::icons::LucideIcon { icon: crate::icons::Icon::Lock, size: 9 }
                        "{t.selectors_official_badge}"
                    }
                }
                { let pr = if is_official { "60px" } else { "0" };
                rsx! { h3 { style: "font-size: 14px; font-weight: 700; color: {Theme::TEXT}; padding-right: {pr};", "{selector.name}" } }}
                if !selector.description.is_empty() {
                    p { style: "font-size: 12px; color: {Theme::MUTED}; line-height: 1.4; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                        "{selector.description}"
                    }
                }
                // Tags (official only)
                if !tags.is_empty() {
                    div { style: "display: flex; flex-wrap: wrap; gap: 3px;",
                        for tag in tags.iter() {
                            span { style: "font-size: 9px; padding: 1px 5px; background: rgba(90, 158, 63, 0.08); color: {Theme::MUTED}; border-radius: 999px;",
                                "{tag}"
                            }
                        }
                    }
                }
                div { style: "display: flex; flex-wrap: wrap; gap: 5px; margin-top: auto;",
                    span { style: "font-size: 10px; padding: 2px 7px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px;", "{selector.folder_scope}" }
                    span { style: "font-size: 10px; color: {Theme::MUTED};", "{rules_count}r \u{00B7} {skills_count}s \u{00B7} {flocks_count}f \u{00B7} {repos_count}rp" }
                    if selector.priority != 0 {
                        span { style: "font-size: 10px; color: {Theme::MUTED};", "P{selector.priority}" }
                    }
                    if match_count > 0 {
                        span { style: "font-size: 10px; padding: 1px 6px; background: rgba(90, 158, 63, 0.12); color: {Theme::ACCENT_STRONG}; border-radius: 999px; display: inline-flex; align-items: center; gap: 2px;",
                            crate::icons::LucideIcon { icon: crate::icons::Icon::Check, size: 10 }
                            "{match_count}"
                        }
                    }
                }
                div { style: "display: flex; gap: 4px;",
                    onclick: move |e: Event<MouseData>| e.stop_propagation(),
                    SmallButton { label: toggle_label, disabled: form_is_open, accent: if is_enabled { Theme::MUTED } else { Theme::ACCENT_STRONG }, onclick: move |_| on_toggle.call(()) }
                    if is_official {
                        SmallButton { label: t.selectors_clone_template, disabled: form_is_open, onclick: move |_| on_template.call(()) }
                    } else {
                        SmallButton { label: t.selectors_edit, disabled: form_is_open, onclick: move |_| on_edit.call(()) }
                        SmallButton { label: t.selectors_delete, disabled: form_is_open, accent: Theme::DANGER, onclick: move |_| on_delete.call(()) }
                    }
                }
            }
        }
    } else {
        // List view — full-width row with description, one per line
        rsx! {
            div {
                style: "background: {Theme::PANEL}; border: 1px solid {border_color}; border-radius: 12px; padding: 16px; cursor: pointer; opacity: {opacity};",
                onmousedown: move |evt: Event<MouseData>| { let c = evt.client_coordinates(); down_pos.set((c.x, c.y)); },
                onclick: move |evt: Event<MouseData>| { let c = evt.client_coordinates(); let (dx, dy) = *down_pos.read(); if (c.x - dx).abs() < 5.0 && (c.y - dy).abs() < 5.0 { on_click.call(()); } },
                div { style: "display: flex; align-items: flex-start; justify-content: space-between; gap: 10px; margin-bottom: 8px;",
                    div { style: "min-width: 0; flex: 1;",
                        div { style: "display: flex; align-items: center; gap: 6px; margin-bottom: 4px;",
                            if is_official {
                                span { style: "display: inline-flex; align-items: center; gap: 3px; font-size: 10px; padding: 2px 7px; background: rgba(90, 158, 63, 0.10); color: {Theme::ACCENT_STRONG}; border-radius: 999px;",
                                    crate::icons::LucideIcon { icon: crate::icons::Icon::Lock, size: 10 }
                                    "{t.selectors_official_badge}"
                                }
                            }
                            h3 { style: "font-size: 15px; font-weight: 700; color: {Theme::TEXT};", "{selector.name}" }
                        }
                        if !selector.description.is_empty() {
                            p { style: "font-size: 12px; color: {Theme::MUTED}; line-height: 1.5; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                                "{selector.description}"
                            }
                        }
                    }
                    div { style: "display: flex; gap: 4px; flex-shrink: 0;",
                        onclick: move |e: Event<MouseData>| e.stop_propagation(),
                        SmallButton { label: toggle_label, disabled: form_is_open, accent: if is_enabled { Theme::MUTED } else { Theme::ACCENT_STRONG }, onclick: move |_| on_toggle.call(()) }
                        if is_official {
                            SmallButton { label: t.selectors_clone_template, disabled: form_is_open, onclick: move |_| on_template.call(()) }
                        } else {
                            SmallButton { label: t.selectors_use_template, disabled: form_is_open, onclick: move |_| on_template.call(()) }
                            SmallButton { label: t.selectors_edit, disabled: form_is_open, onclick: move |_| on_edit.call(()) }
                            SmallButton { label: t.selectors_delete, disabled: form_is_open, accent: Theme::DANGER, onclick: move |_| on_delete.call(()) }
                        }
                    }
                }
                div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                    span { style: "font-size: 11px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px;", "{selector.folder_scope}" }
                    span { style: "font-size: 11px; color: {Theme::MUTED};", "{rules_count} rules \u{00B7} {skills_count} skills \u{00B7} {flocks_count} flocks \u{00B7} {repos_count} repos" }
                    if selector.priority != 0 {
                        span { style: "font-size: 11px; color: {Theme::MUTED};", "P{selector.priority}" }
                    }
                    if match_count > 0 {
                        span { style: "font-size: 11px; padding: 1px 7px; background: rgba(90, 158, 63, 0.12); color: {Theme::ACCENT_STRONG}; border-radius: 999px; display: inline-flex; align-items: center; gap: 2px;",
                            crate::icons::LucideIcon { icon: crate::icons::Icon::Check, size: 10 }
                            "{match_count}"
                        }
                    }
                    // Tags (official only, list view)
                    for tag in tags.iter() {
                        span { style: "font-size: 10px; padding: 1px 6px; background: rgba(90, 158, 63, 0.08); color: {Theme::MUTED}; border-radius: 999px;",
                            "{tag}"
                        }
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
    let Some(d) = guard.as_ref() else {
        return rsx! {};
    };

    let name = d.name.clone();
    let desc = d.description.clone();
    let scope = d.folder_scope.clone();
    let expr = d.display_expression();
    let mode = match d.match_mode {
        MatchMode::AllMatch => t.selectors_match_all,
        MatchMode::AnyMatch => t.selectors_match_any,
        MatchMode::Custom => t.selectors_match_custom,
    };
    let rules = d.rules.clone();
    let skills: Vec<String> = d.skills.iter().map(|s| s.to_string()).collect();
    let flocks: Vec<String> = d.flocks.iter().map(|s| s.to_string()).collect();
    let repos: Vec<String> = d.repos.iter().map(|r| r.git_url.clone()).collect();
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
                        onclick: move |_| selector.set(None),
                        crate::icons::LucideIcon { icon: crate::icons::Icon::X, size: 14 }
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
                // Skills + Flocks + Repos
                div { style: "display: flex; flex-direction: column; gap: 10px;",
                    if !repos.is_empty() {
                        TagGroup { label: t.selectors_add_repos_label, items: repos, bg: "rgba(180, 120, 60, 0.10)", color: "rgba(140, 80, 20, 0.9)", border: "rgba(180, 120, 60, 0.16)" }
                    }
                    if !flocks.is_empty() {
                        TagGroup { label: t.selectors_add_flocks_label, items: flocks, bg: "rgba(90, 120, 200, 0.10)", color: "rgba(50, 80, 160, 0.9)", border: "rgba(90, 120, 200, 0.16)" }
                    }
                    if !skills.is_empty() {
                        TagGroup { label: t.selectors_add_skills_label, items: skills, bg: "rgba(46, 139, 87, 0.10)", color: Theme::SUCCESS, border: "rgba(46, 139, 87, 0.16)" }
                    }
                }
            }
        }
    }
}

#[component]
fn SmallButton(
    label: &'static str,
    disabled: bool,
    #[props(default = Theme::MUTED)] accent: &'static str,
    onclick: EventHandler<()>,
) -> Element {
    let opacity = if disabled { "0.5" } else { "1" };
    let cursor = if disabled { "not-allowed" } else { "pointer" };
    rsx! { button { disabled, style: "padding: 5px 10px; background: transparent; color: {accent}; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 11px; font-weight: 600; cursor: {cursor}; opacity: {opacity};", onclick: move |_| onclick.call(()), "{label}" } }
}

#[component]
fn InfoBadge(label: String, color: &'static str) -> Element {
    rsx! { span { style: "display: inline-flex; align-items: center; padding: 4px 10px; background: rgba(255, 255, 255, 0.78); color: {color}; border: 1px solid {Theme::LINE}; border-radius: 999px; font-size: 11px; font-weight: 600;", "{label}" } }
}

#[component]
fn TagGroup(
    label: &'static str,
    items: Vec<String>,
    bg: &'static str,
    color: &'static str,
    border: &'static str,
) -> Element {
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
    let mut repo_input = use_signal(String::new);
    let mut repo_error = use_signal(String::new);

    let is_editing = form.read().as_ref().is_some_and(|f| f.editing_id.is_some());
    let title = if is_editing {
        t.selectors_edit_title
    } else {
        t.selectors_new_title
    };

    let mut set_field = move |mutator: Box<dyn FnOnce(&mut SelectorForm)>| {
        form.with_mut(|opt| {
            if let Some(f) = opt.as_mut() {
                mutator(f);
            }
        });
    };

    // Collect available skills (off UI thread)
    let mut installed_skills_sig = use_signal(Vec::<String>::new);
    use_effect(move || {
        let workdir = state.workdir.read().clone();
        spawn(async move {
            let slugs = tokio::task::spawn_blocking(move || {
                savhub_local::skills::read_fetched_skill_versions(&workdir)
                    .into_keys()
                    .collect::<Vec<_>>()
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
            set_field(Box::new(|f| {
                f.error = "At least one rule is required".to_string()
            }));
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
        let result = if is_edit {
            update_selector(def)
        } else {
            create_selector(def)
        };
        if let Err(e) = result {
            let msg = format!("{e}");
            set_field(Box::new(move |f| f.error = msg));
            return;
        }
        form.set(None);
        version.with_mut(|v| *v += 1);
        // Push to server
        let api_base = state.api_base.read().clone();
        let token = state.token.read().clone();
        if let Some(token) = token {
            spawn(async move {
                let _ = tokio::task::spawn_blocking(move || {
                    savhub_local::selectors::push_custom_selectors(&api_base, &token)
                })
                .await;
            });
        }
    };

    // Read form snapshot
    let guard = form.read();
    let Some(f) = guard.as_ref() else {
        return rsx! {};
    };
    let name_val = f.name.clone();
    let desc_val = f.description.clone();
    let scope_val = f.folder_scope.clone();
    let rules_snapshot = f.rules.clone();
    let rules_count = rules_snapshot.len();
    let match_mode = f.match_mode;
    let custom_expr_val = f.custom_expr.clone();
    let selected_skills = f.skills.clone();
    let selected_flocks = f.flocks.clone();
    let selected_repos = f.repos.clone();
    let priority_val = f.priority;
    let error_val = f.error.clone();
    drop(guard);

    let mut flock_suggestions_sig = use_signal(Vec::<String>::new);
    let selected_flocks_for_effect = selected_flocks.clone();
    use_effect(move || {
        let query = flock_search.read().trim().to_string();
        let selected = selected_flocks_for_effect.clone();
        if query.is_empty() {
            flock_suggestions_sig.set(Vec::new());
            return;
        }
        let client = state.api_client();
        spawn(async move {
            let mut slugs = crate::api::fetch_remote_flock_slug_suggestions(&client, &query, 20)
                .await
                .unwrap_or_default();
            slugs.retain(|slug| !selected.contains(slug));
            flock_suggestions_sig.set(slugs);
        });
    });

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

    let flock_suggestions = flock_suggestions_sig.read().clone();

    let input_style = format!(
        "width: 100%; padding: 8px 12px; border: 1px solid {}; border-radius: 8px; font-size: 13px; background: white; color: {};",
        Theme::LINE,
        Theme::TEXT
    );
    let label_style = format!(
        "font-size: 12px; font-weight: 700; color: {}; margin-bottom: 4px;",
        Theme::MUTED
    );

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
                        crate::icons::LucideIcon { icon: crate::icons::Icon::X, size: 14 }
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
                    // Repos — free-text input with URL normalization and registry validation
                    div {
                        p { style: "{label_style}", "{t.selectors_add_repos_label}" }
                        // Selected repos as removable tags
                        if !selected_repos.is_empty() {
                            div { style: "display: flex; flex-wrap: wrap; gap: 6px; margin-bottom: 8px;",
                                for repo in selected_repos.iter() {
                                    { let sign = repo.clone();
                                      rsx! {
                                        span { style: "display: inline-flex; align-items: center; gap: 4px; padding: 4px 10px; background: rgba(180, 120, 60, 0.10); color: rgba(140, 80, 20, 0.9); border: 1px solid rgba(180, 120, 60, 0.18); border-radius: 999px; font-size: 12px; font-weight: 600;",
                                            "{sign}"
                                            button {
                                                style: "background: none; border: none; color: {Theme::DANGER}; font-size: 13px; cursor: pointer; padding: 0 2px; line-height: 1;",
                                                onclick: { let sign = sign.clone(); move |_| { let s = sign.clone(); set_field(Box::new(move |f| { f.repos.remove(&s); })); } },
                                                crate::icons::LucideIcon { icon: crate::icons::Icon::X, size: 14 }
                                            }
                                        }
                                    }}
                                }
                            }
                        }
                        // Input + Add button
                        div { style: "display: flex; gap: 6px;",
                            input {
                                r#type: "text", value: "{repo_input}", placeholder: t.selectors_repos_placeholder,
                                style: "flex: 1; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                                oninput: move |evt: Event<FormData>| { repo_input.set(evt.value().to_string()); repo_error.set(String::new()); },
                                onkeypress: move |evt: Event<KeyboardData>| {
                                    if evt.key() == Key::Enter {
                                        let raw = repo_input.read().trim().to_string();
                                        if !raw.is_empty() {
                                            let sign = normalize_repo_url_to_sign(&raw);
                                            set_field(Box::new(move |f| { f.repos.insert(sign); }));
                                            repo_input.set(String::new());
                                            repo_error.set(String::new());
                                        }
                                    }
                                },
                            }
                            button {
                                style: "padding: 6px 14px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: 1px solid rgba(90, 158, 63, 0.18); border-radius: 8px; font-size: 12px; font-weight: 600; cursor: pointer;",
                                onclick: move |_| {
                                    let raw = repo_input.read().trim().to_string();
                                    if !raw.is_empty() {
                                        let sign = normalize_repo_url_to_sign(&raw);
                                        set_field(Box::new(move |f| { f.repos.insert(sign); }));
                                        repo_input.set(String::new());
                                        repo_error.set(String::new());
                                    }
                                },
                                "{t.selectors_add_tag}"
                            }
                        }
                        if !repo_error.read().is_empty() {
                            p { style: "font-size: 12px; color: {Theme::DANGER}; margin-top: 3px;", "{repo_error}" }
                        }
                        p { style: "font-size: 11px; color: {Theme::MUTED}; margin-top: 3px;", "{t.selectors_repos_hint}" }
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
                                                crate::icons::LucideIcon { icon: crate::icons::Icon::X, size: 14 }
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
                                    { let s = slug.clone();
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
                    }
                    // Skills — search-to-add from installed skills
                    div {
                        p { style: "{label_style}", "{t.selectors_add_skills_label}" }
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
                                                crate::icons::LucideIcon { icon: crate::icons::Icon::X, size: 14 }
                                            }
                                        }
                                    }}
                                }
                            }
                        }
                        input {
                            r#type: "text", value: "{skill_search}", placeholder: t.selectors_search_skills,
                            style: "width: 100%; padding: 6px 10px; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; background: white; color: {Theme::TEXT};",
                            oninput: move |evt: Event<FormData>| skill_search.set(evt.value().to_string()),
                        }
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
                            p { style: "font-size: 12px; color: {Theme::MUTED}; margin-top: 4px;", "No fetched skills." }
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
