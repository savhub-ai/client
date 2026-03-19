use std::collections::BTreeMap;

use dioxus::prelude::*;
use savhub_local::registry::{CachedSkillSummary, RegistryFlock, SecuritySummary};

use crate::components::pagination::{self, PaginationControls};
use crate::components::view_toggle::{ViewMode, ViewToggleButton};
use crate::state::AppState;
use crate::theme::Theme;
use crate::i18n;

const EXPLORE_PAGE_SIZE: usize = 24;

#[derive(Debug, Clone, PartialEq)]
struct DisplaySkill {
    sign: String,
    slug: String,
    name: String,
    summary: Option<String>,
    version: Option<String>,
    owner: Option<String>,
}

impl From<CachedSkillSummary> for DisplaySkill {
    fn from(item: CachedSkillSummary) -> Self {
        Self {
            sign: item.sign,
            slug: item.slug,
            name: item.name,
            summary: item.summary,
            version: item.version,
            owner: item.owner,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct DisplayFlock {
    flock: RegistryFlock,
    skill_slugs: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillFilter {
    All,
    Installed,
}

fn load_skills_page(
    query: String,
    filter: SkillFilter,
    page_index: usize,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut skill_list: Signal<Vec<DisplaySkill>>,
    mut total_skills: Signal<usize>,
    mut installed_skill_total: Signal<usize>,
) {
    spawn(async move {
        loading.set(true);
        let query = query.trim().to_string();
        let result = tokio::task::spawn_blocking(move || {
            let query_ref = if query.is_empty() {
                None
            } else {
                Some(query.as_str())
            };
            let installed_only = matches!(filter, SkillFilter::Installed);
            let (items, filtered_total) = savhub_local::registry::list_cached_skill_summaries(
                query_ref,
                Some("active"),
                installed_only,
                page_index,
                EXPLORE_PAGE_SIZE,
            )
            .map_err(|e| e.to_string())?;
            let total = savhub_local::registry::count_cached_skills(query_ref, Some("active"), false)
                .map_err(|e| e.to_string())?;
            let installed_total =
                savhub_local::registry::count_cached_skills(query_ref, Some("active"), true)
                    .map_err(|e| e.to_string())?;
            Ok::<_, String>((
                items.into_iter().map(DisplaySkill::from).collect::<Vec<_>>(),
                filtered_total,
                total,
                installed_total,
            ))
        })
        .await;

        match result {
            Ok(Ok((items, filtered_total, total, installed_total))) => {
                skill_list.set(items);
                total_skills.set(if matches!(filter, SkillFilter::Installed) {
                    filtered_total
                } else {
                    total
                });
                installed_skill_total.set(installed_total);
                error.set(None);
            }
            Ok(Err(e)) => {
                skill_list.set(Vec::new());
                total_skills.set(0);
                installed_skill_total.set(0);
                error.set(Some(e.to_string()));
            }
            Err(e) => {
                skill_list.set(Vec::new());
                total_skills.set(0);
                installed_skill_total.set(0);
                error.set(Some(e.to_string()));
            }
        }
        loading.set(false);
    });
}

fn load_flocks(
    mut flocks_data: Signal<Vec<DisplayFlock>>,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
) {
    spawn(async move {
        loading.set(true);
        let result = tokio::task::spawn_blocking(move || {
            let flocks = savhub_local::registry::list_flocks().map_err(|e| e.to_string())?;
            let mut items = Vec::with_capacity(flocks.len());
            for flock in flocks {
                let skill_slugs =
                    savhub_local::registry::list_flock_skill_slugs(&flock.slug).unwrap_or_default();
                items.push(DisplayFlock { flock, skill_slugs });
            }
            Ok::<_, String>(items)
        })
        .await;

        match result {
            Ok(Ok(items)) => {
                flocks_data.set(items);
                error.set(None);
            }
            Ok(Err(e)) => {
                flocks_data.set(Vec::new());
                error.set(Some(e.to_string()));
            }
            Err(e) => {
                flocks_data.set(Vec::new());
                error.set(Some(e.to_string()));
            }
        }
        loading.set(false);
    });
}

#[component]
pub fn ExplorePage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut query = use_signal(String::new);
    let mut applied_query = use_signal(String::new);
    let skill_list: Signal<Vec<DisplaySkill>> = use_signal(Vec::new);
    let total_skills = use_signal(|| 0usize);
    let installed_skill_total = use_signal(|| 0usize);
    let mut installed_versions: Signal<BTreeMap<String, String>> = use_signal(BTreeMap::new);
    let mut active_filter = use_signal(|| SkillFilter::All);
    let mut active_view = use_signal(|| ViewMode::Cards);
    let mut current_page = use_signal(|| 0usize);
    let loading = use_signal(|| false);
    let error = use_signal(|| Option::<String>::None);
    let mut grouped = use_signal(|| true);
    let flocks_data: Signal<Vec<DisplayFlock>> = use_signal(Vec::new);
    let mut reload_version = use_signal(|| 0u32);
    let mut flocks_version = use_signal(|| 0u32);

    use_effect(move || {
        let _ = *state.config_version.read();
        spawn(async move {
            let installed = tokio::task::spawn_blocking(|| {
                savhub_local::registry::read_installed_skills_file()
                    .unwrap_or_default()
                    .into_iter()
                    .map(|e| (e.slug, "installed".to_string()))
                    .collect::<BTreeMap<String, String>>()
            })
            .await
            .unwrap_or_default();
            installed_versions.set(installed);
        });
    });

    use_effect(move || {
        let query = applied_query.read().clone();
        let filter = *active_filter.read();
        let page = *current_page.read();
        let _ = *reload_version.read();
        let _ = *state.config_version.read();
        load_skills_page(
            query,
            filter,
            page,
            loading,
            error,
            skill_list,
            total_skills,
            installed_skill_total,
        );
    });

    use_effect(move || {
        let _ = *flocks_version.read();
        let _ = *state.config_version.read();
        if *grouped.read() {
            load_flocks(flocks_data, loading, error);
        }
    });

    let mut run_search = move || {
        current_page.set(0);
        applied_query.set(query.read().clone());
    };

    let do_search = move |_: Event<MouseData>| {
        run_search();
    };

    let on_enter = move |e: Event<KeyboardData>| {
        if e.key() == Key::Enter {
            run_search();
        }
    };

    let title = t.explore_title;
    let placeholder = t.search_placeholder;
    let search_label = t.search;
    let all_label = t.filter_all;
    let installed_label = t.installed;
    let loading_text = t.loading;
    let no_found = t.no_skills_found;
    let flock_skills_label = t.flock_skills_count;
    let is_grouped = *grouped.read();

    let active_query_value = applied_query.read().clone();
    let filtered_flocks: Vec<DisplayFlock> = if is_grouped {
        let search_lower = active_query_value.to_lowercase();
        flocks_data
            .read()
            .iter()
            .filter(|f| {
                search_lower.is_empty()
                    || f.flock.name.to_lowercase().contains(&search_lower)
                    || f.flock.slug.to_lowercase().contains(&search_lower)
                    || f.flock.description.to_lowercase().contains(&search_lower)
            })
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    let current_filter = *active_filter.read();
    let current_view = *active_view.read();
    let visible_skills = skill_list.read().clone();
    let skill_total = *total_skills.read();
    let current_page_index = pagination::clamp_page(
        *current_page.read(),
        skill_total,
        EXPLORE_PAGE_SIZE,
    );
    let total_pages = pagination::total_pages(skill_total, EXPLORE_PAGE_SIZE);
    let filter_items = [
        (SkillFilter::All, all_label, *total_skills.read()),
        (SkillFilter::Installed, installed_label, *installed_skill_total.read()),
    ];

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            // ── Sticky header ──
            div { style: "flex-shrink: 0; padding: 12px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10;",
                // Row 1: title | search | refresh | view-toggle
                div { style: "display: flex; align-items: center; gap: 10px; margin-bottom: 8px;",
                    h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT}; white-space: nowrap;",
                        "{title}"
                    }
                    div { style: "flex: 1; display: flex; gap: 6px; max-width: 420px; margin-left: auto;",
                        input {
                            style: "flex: 1; padding: 6px 12px; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 13px; background: {Theme::PANEL}; color: {Theme::TEXT}; outline: none;",
                            placeholder: "{placeholder}",
                            value: "{query}",
                            oninput: move |e| query.set(e.value()),
                            onkeypress: on_enter,
                        }
                        button {
                            style: "padding: 6px 14px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 13px; font-weight: 500; cursor: pointer;",
                            onclick: do_search,
                            "{search_label}"
                        }
                    }
                    // Refresh
                    button {
                        title: "Refresh",
                        style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer; font-size: 16px;",
                        onclick: move |_| {
                            current_page.set(0);
                            reload_version += 1;
                            flocks_version += 1;
                        },
                        "\u{21BB}"
                    }
                    // Grouped toggle
                    button {
                        style: format!(
                            "padding: 6px 14px; border-radius: 999px; font-size: 12px; font-weight: 600; cursor: pointer; border: 1px solid {}; {}",
                            if is_grouped { Theme::ACCENT } else { Theme::LINE },
                            if is_grouped { format!("background: {}; color: white;", Theme::ACCENT) } else { format!("background: transparent; color: {};", Theme::MUTED) }
                        ),
                        onclick: move |_| {
                            let current = *grouped.read();
                            grouped.set(!current);
                        },
                        "{t.grouped_label}"
                    }
                    // View toggle
                    ViewToggleButton {
                        mode: current_view,
                        on_toggle: move |_| {
                            active_view.set(if current_view == ViewMode::Cards {
                                ViewMode::List
                            } else {
                                ViewMode::Cards
                            });
                        },
                    }
                }
                // Row 2: filter chips
                div { style: "display: flex; flex-wrap: wrap; gap: 6px;",
                    for (filter, label, count) in filter_items {
                        {
                            let is_active = current_filter == filter;
                            let bg = if is_active { Theme::ACCENT_LIGHT } else { Theme::PANEL };
                            let color = if is_active { Theme::ACCENT_STRONG } else { Theme::MUTED };
                            let border = if is_active { Theme::ACCENT } else { Theme::LINE };
                            rsx! {
                                button {
                                    style: "display: inline-flex; align-items: center; gap: 5px; padding: 5px 10px; background: {bg}; color: {color}; border: 1px solid {border}; border-radius: 999px; font-size: 11px; font-weight: 600; cursor: pointer;",
                                    onclick: move |_| {
                                        current_page.set(0);
                                        active_filter.set(filter);
                                    },
                                    span { "{label}" }
                                    span { style: "font-size: 10px; opacity: 0.8;", "{count}" }
                                }
                            }
                        }
                    }
                }
            }

            // ── Scrollable content ──
            div { style: "flex: 1; overflow-y: auto; padding: 20px 32px 32px;",

            if *loading.read() {
                p { style: "color: {Theme::MUTED}; padding: 20px 0;", "{loading_text}" }
            }

            if let Some(err) = error.read().as_ref() {
                div { style: "padding: 12px 16px; background: rgba(139, 30, 30, 0.08); border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 6px; color: {Theme::DANGER}; margin-bottom: 16px;",
                    "{err}"
                }
            }

            if is_grouped {
                // ── Flocks (grouped) view ──
                if current_view == ViewMode::List {
                    div { style: "display: flex; flex-direction: column; gap: 10px;",
                        for flock in filtered_flocks.iter() {
                            FlockListRow {
                                flock: flock.clone(),
                                flock_skills_label: flock_skills_label,
                                installed_versions: installed_versions,
                            }
                        }
                    }
                } else {
                    div { style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px;",
                        for flock in filtered_flocks.iter() {
                            FlockCard {
                                flock: flock.clone(),
                                flock_skills_label: flock_skills_label,
                                installed_versions: installed_versions,
                            }
                        }
                    }
                }
                if filtered_flocks.is_empty() && !*loading.read() {
                    p { style: "color: {Theme::MUTED}; text-align: center; padding: 40px 0;",
                        "{no_found}"
                    }
                }
            } else {
                // ── Skills (ungrouped) view ──
                if current_view == ViewMode::List {
                    div { style: "display: flex; flex-direction: column; gap: 10px;",
                        for skill in visible_skills.iter() {
                            SkillListRow {
                                skill: skill.clone(),
                                installed_versions: installed_versions,
                            }
                        }
                    }
                } else {
                    div { style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px;",
                        for skill in visible_skills.iter() {
                            SkillCard {
                                skill: skill.clone(),
                                installed_versions: installed_versions,
                            }
                        }
                    }
                }

                if skill_total > 0 {
                    PaginationControls {
                        current_page: current_page_index,
                        total_pages: Some(total_pages),
                        has_prev: current_page_index > 0,
                        has_next: current_page_index + 1 < total_pages,
                        on_prev: move |_| current_page.set(current_page_index.saturating_sub(1)),
                        on_next: move |_| current_page.set(current_page_index + 1),
                    }
                }

                if !*loading.read() && visible_skills.is_empty() && error.read().is_none() {
                    p { style: "color: {Theme::MUTED}; text-align: center; padding: 40px 0;",
                        "{no_found}"
                    }
                }
            }
            } // scrollable content
        }
    }
}

#[component]
fn SkillListRow(
    skill: DisplaySkill,
    mut installed_versions: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let sign = skill.sign.clone();
    let slug = skill.slug.clone();
    let is_installed = installed_versions.read().contains_key(&skill.slug);

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let sign = sign.clone();
        let slug = slug.clone();
        let uninstall = is_installed;
        spawn(async move {
            working.set(true);
            action_error.set(None);
            let result = {
                let sign = sign.clone();
                let slug = slug.clone();
                tokio::task::spawn_blocking(move || {
                    if uninstall {
                        savhub_local::registry::uninstall_skill_from_registry(&slug).map(|_| ())
                    } else {
                        savhub_local::registry::install_skill_from_registry(&sign).map(|_| ())
                    }
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()))
            };
            match result {
                Ok(()) => {
                    installed_versions.with_mut(|entries| {
                        if uninstall {
                            entries.remove(&slug);
                        } else {
                            entries.insert(slug.clone(), "installed".to_string());
                        }
                    });
                    if !uninstall {
                        let track_sign = sign.clone();
                        let track_client = state.api_client();
                        tokio::spawn(async move {
                            let _ = track_client
                                .post_json::<serde_json::Value, serde_json::Value>(
                                    &format!("/collect?skill={track_sign}"),
                                    &serde_json::json!({ "client_type": "desktop" }),
                                )
                                .await;
                        });
                    }
                }
                Err(e) => action_error.set(Some(e.to_string())),
            }
            working.set(false);
        });
    };

    let version_display = skill.version.as_deref().unwrap_or("\u{2014}");
    let owner_display = skill.owner.as_deref().unwrap_or("unknown");

    let nav = use_navigator();
    let slug_nav = skill.slug.clone();

    rsx! {
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 10px; padding: 14px 16px; display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; cursor: pointer;",
                onclick: move |_| { nav.push(crate::Route::Detail { slug: slug_nav.clone() }); },
                div { style: "min-width: 0; flex: 1;",
                    div { style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: 4px;",
                        h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                            "{skill.name}"
                        }
                        span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px; white-space: nowrap;",
                            "v{version_display}"
                        }
                    }
                    div { style: "margin-bottom: 6px;",
                        crate::components::copy_sign::CopySign { value: skill.sign.clone() }
                    }
                    if let Some(desc) = &skill.summary {
                        p { style: "font-size: 13px; color: {Theme::MUTED}; line-height: 1.5; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                            "{desc}"
                        }
                    }
                }
                div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 8px; flex-shrink: 0;",
                    span { style: "font-size: 12px; color: {Theme::MUTED}; white-space: nowrap;",
                        "{t.by} {owner_display}"
                    }
                    if *working.read() {
                        span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                            span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                            "{t.installing}"
                        }
                    } else if is_installed {
                        button {
                            style: "padding: 5px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 999px; cursor: pointer; font-weight: 600; white-space: nowrap;",
                            onclick: do_action,
                            "{t.uninstall}"
                        }
                    } else {
                        button {
                            style: "padding: 5px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 999px; cursor: pointer; font-weight: 600; white-space: nowrap;",
                            onclick: do_action,
                            "{t.install}"
                        }
                    }
                    if let Some(err) = action_error.read().as_ref() {
                        p { style: "font-size: 11px; color: {Theme::DANGER}; max-width: 220px; text-align: right;", "{err}" }
                    }
                }
            }
    }
}

#[component]
fn SkillCard(
    skill: DisplaySkill,
    mut installed_versions: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let sign = skill.sign.clone();
    let slug = skill.slug.clone();
    let is_installed = installed_versions.read().contains_key(&skill.slug);

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let sign = sign.clone();
        let slug = slug.clone();
        let uninstall = is_installed;
        spawn(async move {
            working.set(true);
            action_error.set(None);
            let result = {
                let s = sign.clone();
                let slug = slug.clone();
                tokio::task::spawn_blocking(move || {
                    if uninstall {
                        savhub_local::registry::uninstall_skill_from_registry(&slug).map(|_| ())
                    } else {
                        savhub_local::registry::install_skill_from_registry(&s).map(|_| ())
                    }
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()))
            };
            match result {
                Ok(()) => {
                    installed_versions.with_mut(|entries| {
                        if uninstall {
                            entries.remove(&slug);
                        } else {
                            entries.insert(slug.clone(), "installed".to_string());
                        }
                    });
                    if !uninstall {
                        let track_sign = sign.clone();
                        let track_client = state.api_client();
                        tokio::spawn(async move {
                            let _ = track_client
                                .post_json::<serde_json::Value, serde_json::Value>(
                                    &format!("/collect?skill={track_sign}"),
                                    &serde_json::json!({ "client_type": "desktop" }),
                                )
                                .await;
                        });
                    }
                }
                Err(e) => action_error.set(Some(e.to_string())),
            }
            working.set(false);
        });
    };

    let version_display = skill.version.as_deref().unwrap_or("\u{2014}");
    let owner_display = skill.owner.as_deref().unwrap_or("unknown");

    let nav = use_navigator();
    let slug_nav = skill.slug.clone();

    rsx! {
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; display: flex; flex-direction: column; gap: 8px; cursor: pointer; transition: box-shadow 0.15s;",
                onclick: move |_| { nav.push(crate::Route::Detail { slug: slug_nav.clone() }); },
                div { style: "display: flex; justify-content: space-between; align-items: flex-start;",
                    div {
                        h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 2px;",
                            "{skill.name}"
                        }
                        crate::components::copy_sign::CopySign { value: skill.sign.clone() }
                    }
                    span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 10px; white-space: nowrap;",
                        "v{version_display}"
                    }
                }
                if let Some(desc) = &skill.summary {
                    p { style: "font-size: 13px; color: {Theme::MUTED}; flex: 1; display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden;",
                        "{desc}"
                    }
                }
                div { style: "display: flex; justify-content: space-between; align-items: center; margin-top: 4px;",
                    span { style: "font-size: 12px; color: {Theme::MUTED};",
                        "{t.by} {owner_display}"
                    }
                    if *working.read() {
                        span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                            span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                            "{t.installing}"
                        }
                    } else if is_installed {
                        button {
                            style: "padding: 4px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 4px; cursor: pointer; font-weight: 500;",
                            onclick: do_action,
                            "{t.uninstall}"
                        }
                    } else {
                        button {
                            style: "padding: 4px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; cursor: pointer; font-weight: 500;",
                            onclick: do_action,
                            "{t.install}"
                        }
                    }
                }
                if let Some(err) = action_error.read().as_ref() {
                    p { style: "font-size: 11px; color: {Theme::DANGER};", "{err}" }
                }
            }
    }
}

#[component]
fn FlockListRow(
    flock: DisplayFlock,
    flock_skills_label: &'static str,
    mut installed_versions: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let flock_info = flock.flock.clone();
    let skill_slugs = flock.skill_slugs.clone();
    let skill_count = skill_slugs.len();
    let all_installed = skill_count > 0 && {
        let map = installed_versions.read();
        skill_slugs.iter().all(|s| map.contains_key(s))
    };
    let version_display = flock_info.version.as_deref().unwrap_or("\u{2014}");
    let slug_display = if flock_info.repo.is_empty() {
        flock_info.slug.clone()
    } else {
        format!("{}/{}", flock_info.repo, flock_info.slug)
    };

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let slugs = skill_slugs.clone();
        let uninstall = all_installed;
        spawn(async move {
            working.set(true);
            action_error.set(None);
            for slug in &slugs {
                let s = slug.clone();
                let result = tokio::task::spawn_blocking(move || {
                    if uninstall {
                        savhub_local::registry::uninstall_skill_from_registry(&s).map(|_| ())
                    } else {
                        let sign = savhub_local::registry::get_skill_db_info(&s)
                            .map(|(repo_id, path)| format!("{repo_id}/{path}"))
                            .unwrap_or(s.clone());
                        savhub_local::registry::install_skill_from_registry(&sign).map(|_| ())
                    }
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
                if let Err(e) = result {
                    action_error.set(Some(e.to_string()));
                    break;
                }
                installed_versions.with_mut(|map| {
                    if uninstall {
                        map.remove(slug);
                    } else {
                        map.insert(slug.clone(), "installed".to_string());
                    }
                });
                if !uninstall {
                    let track_slug = slug.clone();
                    let track_client = state.api_client();
                    tokio::spawn(async move {
                        let _ = track_client
                            .post_json::<serde_json::Value, serde_json::Value>(
                                &format!("/collect?skill={track_slug}"),
                                &serde_json::json!({ "client_type": "desktop" }),
                            )
                            .await;
                    });
                }
            }
            working.set(false);
        });
    };

    let nav = use_navigator();
    let nav_slug = flock_info.slug.clone();

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 10px; padding: 14px 16px; display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; cursor: pointer;",
            onclick: move |_| { nav.push(crate::Route::FlockDetail { slug: nav_slug.clone() }); },
            div { style: "min-width: 0; flex: 1;",
                div { style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: 4px;",
                    h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                        "{flock_info.name}"
                    }
                    SecurityBadge { security: flock_info.security.clone() }
                    span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px; white-space: nowrap;",
                        "v{version_display}"
                    }
                }
                div { style: "margin-bottom: 6px;",
                    crate::components::copy_sign::CopySign { value: slug_display.clone() }
                }
                if !flock_info.description.is_empty() {
                    p { style: "font-size: 13px; color: {Theme::MUTED}; line-height: 1.5; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                        "{flock_info.description}"
                    }
                }
            }
            div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 8px; flex-shrink: 0;",
                span { style: "font-size: 12px; color: {Theme::MUTED}; white-space: nowrap;",
                    "{skill_count} {flock_skills_label}"
                }
                if *working.read() {
                    span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                        span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                        "{t.installing}"
                    }
                } else if all_installed {
                    button {
                        style: "padding: 5px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 999px; cursor: pointer; font-weight: 600; white-space: nowrap;",
                        onclick: do_action,
                        "{t.uninstall}"
                    }
                } else {
                    button {
                        style: "padding: 5px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 999px; cursor: pointer; font-weight: 600; white-space: nowrap;",
                        onclick: do_action,
                        "{t.install}"
                    }
                }
                if let Some(err) = action_error.read().as_ref() {
                    p { style: "font-size: 11px; color: {Theme::DANGER}; max-width: 220px; text-align: right;", "{err}" }
                }
            }
        }
    }
}

#[component]
fn FlockCard(
    flock: DisplayFlock,
    flock_skills_label: &'static str,
    mut installed_versions: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let flock_info = flock.flock.clone();
    let skill_slugs = flock.skill_slugs.clone();
    let skill_count = skill_slugs.len();
    let all_installed = skill_count > 0 && {
        let map = installed_versions.read();
        skill_slugs.iter().all(|s| map.contains_key(s))
    };
    let version_display = flock_info.version.as_deref().unwrap_or("\u{2014}");
    let slug_display = if flock_info.repo.is_empty() {
        flock_info.slug.clone()
    } else {
        format!("{}/{}", flock_info.repo, flock_info.slug)
    };

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let slugs = skill_slugs.clone();
        let uninstall = all_installed;
        spawn(async move {
            working.set(true);
            action_error.set(None);
            for slug in &slugs {
                let s = slug.clone();
                let result = tokio::task::spawn_blocking(move || {
                    if uninstall {
                        savhub_local::registry::uninstall_skill_from_registry(&s).map(|_| ())
                    } else {
                        let sign = savhub_local::registry::get_skill_db_info(&s)
                            .map(|(repo_id, path)| format!("{repo_id}/{path}"))
                            .unwrap_or(s.clone());
                        savhub_local::registry::install_skill_from_registry(&sign).map(|_| ())
                    }
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
                if let Err(e) = result {
                    action_error.set(Some(e.to_string()));
                    break;
                }
                installed_versions.with_mut(|map| {
                    if uninstall {
                        map.remove(slug);
                    } else {
                        map.insert(slug.clone(), "installed".to_string());
                    }
                });
                if !uninstall {
                    let track_slug = slug.clone();
                    let track_client = state.api_client();
                    tokio::spawn(async move {
                        let _ = track_client
                            .post_json::<serde_json::Value, serde_json::Value>(
                                &format!("/collect?skill={track_slug}"),
                                &serde_json::json!({ "client_type": "desktop" }),
                            )
                            .await;
                    });
                }
            }
            working.set(false);
        });
    };

    let nav = use_navigator();
    let nav_slug = flock_info.slug.clone();

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; display: flex; flex-direction: column; gap: 8px; cursor: pointer; transition: box-shadow 0.15s;",
            onclick: move |_| { nav.push(crate::Route::FlockDetail { slug: nav_slug.clone() }); },
            div { style: "display: flex; justify-content: space-between; align-items: flex-start;",
                div {
                    div { style: "display: flex; align-items: center; gap: 6px; margin-bottom: 2px;",
                        h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                            "{flock_info.name}"
                        }
                        SecurityBadge { security: flock_info.security.clone() }
                    }
                    p { style: "font-size: 12px; color: {Theme::MUTED};",
                        "{slug_display}"
                    }
                }
                span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 10px; white-space: nowrap;",
                    "v{version_display}"
                }
            }
            if !flock_info.description.is_empty() {
                p { style: "font-size: 13px; color: {Theme::MUTED}; flex: 1; display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden;",
                    "{flock_info.description}"
                }
            }
            div { style: "display: flex; justify-content: space-between; align-items: center; margin-top: 4px;",
                span { style: "font-size: 12px; color: {Theme::MUTED};",
                    "{skill_count} {flock_skills_label}"
                }
                if *working.read() {
                    span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                        span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                        "{t.installing}"
                    }
                } else if all_installed {
                    button {
                        style: "padding: 4px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 4px; cursor: pointer; font-weight: 500;",
                        onclick: do_action,
                        "{t.uninstall}"
                    }
                } else {
                    button {
                        style: "padding: 4px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; cursor: pointer; font-weight: 500;",
                        onclick: do_action,
                        "{t.install}"
                    }
                }
            }
            if let Some(err) = action_error.read().as_ref() {
                p { style: "font-size: 11px; color: {Theme::DANGER};", "{err}" }
            }
        }
    }
}

#[component]
fn SecurityBadge(security: SecuritySummary) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let status = security.status.as_deref().unwrap_or("unverified");
    let value_bg = match status {
        "verified" => "#2e8b57",
        "scanning" => "#1e82d2",
        "flagged" => "#b8860b",
        "rejected" => "#9f2b2b",
        _ => "#999",
    };
    let value_label = match status {
        "verified" => t.security_verified,
        "scanning" => t.security_scanning,
        "flagged" => t.security_flagged,
        "rejected" => t.security_rejected,
        _ => t.security_unverified,
    };
    rsx! {
        span { style: "display: inline-flex; align-items: center; font-size: 11px; line-height: 1; border-radius: 3px; overflow: hidden; vertical-align: middle; white-space: nowrap; position: relative; top: -1px;",
            span { style: "padding: 3px 6px; background: #555; color: #fff;", "security" }
            span { style: "padding: 3px 6px; background: {value_bg}; color: #fff; font-weight: 600;", "{value_label}" }
        }
    }
}
