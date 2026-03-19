use std::collections::BTreeMap;

use dioxus::prelude::*;
use savhub_local::registry::{RegistryFlock, SecuritySummary};
use savhub_shared::{PagedResponse, SearchResponse, SearchResult, SkillListItem};

use crate::components::pagination::{self, PaginationControls};
use crate::components::view_toggle::{ViewMode, ViewToggleButton};
use crate::state::AppState;
use crate::theme::Theme;
use crate::{i18n, skills};

const EXPLORE_PAGE_SIZE: usize = 24;
const EXPLORE_SEARCH_FETCH_LIMIT: usize = 120;

/// Unified display item for skills from either browse or search APIs.
#[derive(Debug, Clone, PartialEq)]
struct DisplaySkill {
    sign: String,
    slug: String,
    name: String,
    summary: Option<String>,
    version: Option<String>,
    owner: Option<String>,
}

impl From<&SkillListItem> for DisplaySkill {
    fn from(item: &SkillListItem) -> Self {
        Self {
            sign: item.sign.clone(),
            slug: item.slug.clone(),
            name: item.display_name.clone(),
            summary: item.summary.clone(),
            version: item.latest_version.as_ref().map(|v| v.version.clone()),
            owner: Some(item.owner.handle.clone()),
        }
    }
}

impl From<&SearchResult> for DisplaySkill {
    fn from(item: &SearchResult) -> Self {
        Self {
            sign: String::new(), // SearchResult doesn't have sign
            slug: item.slug.clone(),
            name: item.display_name.clone(),
            summary: item.summary.clone(),
            version: item.latest_version.clone(),
            owner: item.owner_handle.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillFilter {
    All,
    Installed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillInstallState {
    NotInstalled,
    Installed,
    Latest,
    Outdated,
}

fn skill_install_state(
    skill: &DisplaySkill,
    installed_versions: &BTreeMap<String, String>,
) -> SkillInstallState {
    let Some(local_version) = installed_versions.get(&skill.slug) else {
        return SkillInstallState::NotInstalled;
    };

    match skill.version.as_deref() {
        Some(remote_version) if remote_version == local_version => SkillInstallState::Latest,
        Some(_) => SkillInstallState::Outdated,
        None => SkillInstallState::Installed,
    }
}

fn matches_filter(state: SkillInstallState, filter: SkillFilter) -> bool {
    match filter {
        SkillFilter::All => true,
        SkillFilter::Installed => !matches!(state, SkillInstallState::NotInstalled),
    }
}

fn browse_path(cursor: Option<&str>) -> String {
    let mut path = format!("/skills?limit={EXPLORE_PAGE_SIZE}");
    if let Some(cursor) = cursor {
        path.push_str("&cursor=");
        path.push_str(&skills::urlencoding(cursor));
    }
    path
}

fn load_browse_page(
    state: AppState,
    cursor: Option<String>,
    previous_cursors: Vec<Option<String>>,
    page_index: usize,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut skill_list: Signal<Vec<DisplaySkill>>,
    mut showing_search_results: Signal<bool>,
    mut browse_page: Signal<usize>,
    mut browse_current_cursor: Signal<Option<String>>,
    mut browse_previous_cursors: Signal<Vec<Option<String>>>,
    mut browse_next_cursor: Signal<Option<String>>,
    mut search_page: Signal<usize>,
) {
    let client = state.api_client();
    spawn(async move {
        loading.set(true);
        match client
            .get_json::<PagedResponse<SkillListItem>>(&browse_path(cursor.as_deref()))
            .await
        {
            Ok(resp) => {
                let items = resp.items.iter().map(DisplaySkill::from).collect();
                skill_list.set(items);
                error.set(None);
                showing_search_results.set(false);
                browse_page.set(page_index);
                browse_current_cursor.set(cursor);
                browse_previous_cursors.set(previous_cursors);
                browse_next_cursor.set(resp.next_cursor);
                search_page.set(0);
            }
            Err(e) => error.set(Some(e)),
        }
        loading.set(false);
    });
}

#[component]
pub fn ExplorePage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut query = use_signal(String::new);
    let mut skill_list: Signal<Vec<DisplaySkill>> = use_signal(Vec::new);
    let mut installed_versions: Signal<BTreeMap<String, String>> = use_signal(BTreeMap::new);
    let mut active_filter = use_signal(|| SkillFilter::All);
    let mut active_view = use_signal(|| ViewMode::Cards);
    let mut showing_search_results = use_signal(|| false);
    let mut browse_page = use_signal(|| 0usize);
    let mut browse_current_cursor = use_signal(|| Option::<String>::None);
    let mut browse_previous_cursors = use_signal(Vec::<Option<String>>::new);
    let mut browse_next_cursor = use_signal(|| Option::<String>::None);
    let mut search_page = use_signal(|| 0usize);
    let mut loading = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);
    let mut initial_loaded = use_signal(|| false);
    let mut grouped = use_signal(|| true);
    let mut flocks_data: Signal<Vec<RegistryFlock>> = use_signal(Vec::new);
    let mut flocks_version = use_signal(|| 0u32);

    use_effect(move || {
        // Load installed state from installed_skills.json
        let entries = savhub_local::registry::read_installed_skills_file().unwrap_or_default();
        let installed: BTreeMap<String, String> = entries
            .into_iter()
            .map(|e| (e.slug, "installed".to_string()))
            .collect();
        installed_versions.set(installed);
    });

    // Load initial browse on mount
    use_effect(move || {
        if *initial_loaded.read() {
            return;
        }
        initial_loaded.set(true);
        load_browse_page(
            state,
            None,
            Vec::new(),
            0,
            loading,
            error,
            skill_list,
            showing_search_results,
            browse_page,
            browse_current_cursor,
            browse_previous_cursors,
            browse_next_cursor,
            search_page,
        );
    });

    // Load flocks when grouped is on (re-triggers on version bump from Refresh)
    use_effect(move || {
        let _ = *flocks_version.read();
        if *grouped.read() {
            let flocks = savhub_local::registry::list_flocks().unwrap_or_default();
            flocks_data.set(flocks);
        }
    });

    let run_search = move || {
        let q = query.read().clone();
        if q.trim().is_empty() {
            load_browse_page(
                state,
                None,
                Vec::new(),
                0,
                loading,
                error,
                skill_list,
                showing_search_results,
                browse_page,
                browse_current_cursor,
                browse_previous_cursors,
                browse_next_cursor,
                search_page,
            );
        } else {
            let client = state.api_client();
            spawn(async move {
                loading.set(true);
                let path = format!(
                    "/search?q={}&kind=skill&limit={EXPLORE_SEARCH_FETCH_LIMIT}",
                    skills::urlencoding(&q)
                );
                match client.get_json::<SearchResponse>(&path).await {
                    Ok(resp) => {
                        let items = resp.results.iter().map(DisplaySkill::from).collect();
                        skill_list.set(items);
                        error.set(None);
                        showing_search_results.set(true);
                        browse_page.set(0);
                        browse_current_cursor.set(None);
                        browse_previous_cursors.set(Vec::new());
                        browse_next_cursor.set(None);
                        search_page.set(0);
                    }
                    Err(e) => error.set(Some(e)),
                }
                loading.set(false);
            });
        }
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

    // Filter flocks by search query when grouped
    let filtered_flocks: Vec<RegistryFlock> = if is_grouped {
        let search_lower = query.read().to_lowercase();
        flocks_data
            .read()
            .iter()
            .filter(|f| {
                search_lower.is_empty()
                    || f.name.to_lowercase().contains(&search_lower)
                    || f.slug.to_lowercase().contains(&search_lower)
                    || f.description.to_lowercase().contains(&search_lower)
            })
            .cloned()
            .collect()
    } else {
        Vec::new()
    };

    let all_skills = skill_list.read().clone();
    let installed_map = installed_versions.read().clone();
    let current_filter = *active_filter.read();
    let current_view = *active_view.read();
    let is_search_mode = *showing_search_results.read();
    let filtered_skills: Vec<DisplaySkill> = all_skills
        .iter()
        .filter(|skill| matches_filter(skill_install_state(skill, &installed_map), current_filter))
        .cloned()
        .collect();
    let search_current_page = pagination::clamp_page(
        *search_page.read(),
        filtered_skills.len(),
        EXPLORE_PAGE_SIZE,
    );
    let search_visible_skills =
        pagination::slice_for_page(&filtered_skills, search_current_page, EXPLORE_PAGE_SIZE);
    let search_total_pages = pagination::total_pages(filtered_skills.len(), EXPLORE_PAGE_SIZE);
    let current_browse_page = *browse_page.read();
    let browse_has_prev = !browse_previous_cursors.read().is_empty();
    let browse_has_next = browse_next_cursor.read().is_some();
    let visible_skills: &[DisplaySkill] = if is_search_mode {
        search_visible_skills
    } else {
        filtered_skills.as_slice()
    };
    let filter_items = [
        (SkillFilter::All, all_label, all_skills.len()),
        (
            SkillFilter::Installed,
            installed_label,
            all_skills
                .iter()
                .filter(|skill| {
                    matches_filter(
                        skill_install_state(skill, &installed_map),
                        SkillFilter::Installed,
                    )
                })
                .count(),
        ),
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
                            initial_loaded.set(false);
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
                                    onclick: move |_| active_filter.set(filter),
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

                if is_search_mode {
                    PaginationControls {
                        current_page: search_current_page,
                        total_pages: Some(search_total_pages),
                        has_prev: search_current_page > 0,
                        has_next: search_current_page + 1 < search_total_pages,
                        on_prev: move |_| search_page.set(search_current_page.saturating_sub(1)),
                        on_next: move |_| search_page.set(search_current_page + 1),
                    }
                } else {
                    PaginationControls {
                        current_page: current_browse_page,
                        total_pages: None,
                        has_prev: browse_has_prev,
                        has_next: browse_has_next,
                        on_prev: move |_| {
                            let mut history = browse_previous_cursors.read().clone();
                            if let Some(previous_cursor) = history.pop() {
                                load_browse_page(
                                    state,
                                    previous_cursor,
                                    history,
                                    current_browse_page.saturating_sub(1),
                                    loading,
                                    error,
                                    skill_list,
                                    showing_search_results,
                                    browse_page,
                                    browse_current_cursor,
                                    browse_previous_cursors,
                                    browse_next_cursor,
                                    search_page,
                                );
                            }
                        },
                        on_next: move |_| {
                            if let Some(next_cursor) = browse_next_cursor.read().clone() {
                                let mut history = browse_previous_cursors.read().clone();
                                history.push(browse_current_cursor.read().clone());
                                load_browse_page(
                                    state,
                                    Some(next_cursor),
                                    history,
                                    current_browse_page + 1,
                                    loading,
                                    error,
                                    skill_list,
                                    showing_search_results,
                                    browse_page,
                                    browse_current_cursor,
                                    browse_previous_cursors,
                                    browse_next_cursor,
                                    search_page,
                                );
                            }
                        },
                    }
                }

                if !*loading.read() && filtered_skills.is_empty() && error.read().is_none() && *initial_loaded.read() {
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
    let is_installed = installed_versions.read().contains_key(&skill.sign);

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let sign = sign.clone();
        let uninstall = is_installed;
        spawn(async move {
            working.set(true);
            action_error.set(None);
            let result = {
                let sign = sign.clone();
                tokio::task::spawn_blocking(move || {
                    if uninstall {
                        savhub_local::registry::uninstall_skill_from_registry(&sign).map(|_| ())
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
                            entries.remove(&sign);
                        } else {
                            entries.insert(sign.clone(), "installed".to_string());
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
    let is_installed = installed_versions.read().contains_key(&skill.sign);

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let sign = sign.clone();
        let uninstall = is_installed;
        spawn(async move {
            working.set(true);
            action_error.set(None);
            let result = {
                let s = sign.clone();
                tokio::task::spawn_blocking(move || {
                    if uninstall {
                        savhub_local::registry::uninstall_skill_from_registry(&s).map(|_| ())
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
                            entries.remove(&sign);
                        } else {
                            entries.insert(sign.clone(), "installed".to_string());
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
    flock: RegistryFlock,
    flock_skills_label: &'static str,
    mut installed_versions: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let flock_slug = flock.slug.clone();
    let skill_slugs =
        savhub_local::registry::list_flock_skill_slugs(&flock_slug).unwrap_or_default();
    let skill_count = skill_slugs.len();
    let all_installed = skill_count > 0 && {
        let map = installed_versions.read();
        skill_slugs.iter().all(|s| map.contains_key(s))
    };
    let version_display = flock.version.as_deref().unwrap_or("\u{2014}");
    let slug_display = if flock.repo.is_empty() {
        flock.slug.clone()
    } else {
        format!("{}/{}", flock.repo, flock.slug)
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
    let nav_slug = flock.slug.clone();

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 10px; padding: 14px 16px; display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; cursor: pointer;",
            onclick: move |_| { nav.push(crate::Route::FlockDetail { slug: nav_slug.clone() }); },
            div { style: "min-width: 0; flex: 1;",
                div { style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: 4px;",
                    h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                        "{flock.name}"
                    }
                    SecurityBadge { security: flock.security.clone() }
                    span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px; white-space: nowrap;",
                        "v{version_display}"
                    }
                }
                div { style: "margin-bottom: 6px;",
                    crate::components::copy_sign::CopySign { value: slug_display.clone() }
                }
                if !flock.description.is_empty() {
                    p { style: "font-size: 13px; color: {Theme::MUTED}; line-height: 1.5; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                        "{flock.description}"
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
    flock: RegistryFlock,
    flock_skills_label: &'static str,
    mut installed_versions: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let flock_slug = flock.slug.clone();
    let skill_slugs =
        savhub_local::registry::list_flock_skill_slugs(&flock_slug).unwrap_or_default();
    let skill_count = skill_slugs.len();
    let all_installed = skill_count > 0 && {
        let map = installed_versions.read();
        skill_slugs.iter().all(|s| map.contains_key(s))
    };
    let version_display = flock.version.as_deref().unwrap_or("\u{2014}");
    let slug_display = if flock.repo.is_empty() {
        flock.slug.clone()
    } else {
        format!("{}/{}", flock.repo, flock.slug)
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
    let nav_slug = flock.slug.clone();

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; display: flex; flex-direction: column; gap: 8px; cursor: pointer; transition: box-shadow 0.15s;",
            onclick: move |_| { nav.push(crate::Route::FlockDetail { slug: nav_slug.clone() }); },
            div { style: "display: flex; justify-content: space-between; align-items: flex-start;",
                div {
                    div { style: "display: flex; align-items: center; gap: 6px; margin-bottom: 2px;",
                        h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                            "{flock.name}"
                        }
                        SecurityBadge { security: flock.security.clone() }
                    }
                    p { style: "font-size: 12px; color: {Theme::MUTED};",
                        "{slug_display}"
                    }
                }
                span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 10px; white-space: nowrap;",
                    "v{version_display}"
                }
            }
            if !flock.description.is_empty() {
                p { style: "font-size: 13px; color: {Theme::MUTED}; flex: 1; display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden;",
                    "{flock.description}"
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
