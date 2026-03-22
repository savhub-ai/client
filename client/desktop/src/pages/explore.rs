use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use dioxus::prelude::*;
use savhub_shared::{FlockSummary, SecurityStatus, SkillListItem};

use crate::components::click_guard;
use crate::components::pagination::PaginationControls;
use crate::components::view_toggle::{ViewMode, ViewToggleButton};
use crate::state::AppState;
use crate::theme::Theme;
use crate::{api, i18n};

const EXPLORE_PAGE_SIZE: usize = 24;
const FLOCKS_PAGE_SIZE: usize = 12;

#[derive(Debug, Clone, PartialEq)]
struct DisplaySkill {
    id: String,
    local_slug: String,
    repo_url: String,
    slug: String,
    path: String,
    name: String,
    summary: Option<String>,
    version: Option<String>,
    owner: Option<String>,
    security_status: savhub_shared::SecurityStatus,
}

impl From<SkillListItem> for DisplaySkill {
    fn from(item: SkillListItem) -> Self {
        Self::from_remote(item, None)
    }
}

impl DisplaySkill {
    fn from_remote(item: SkillListItem, local_slug: Option<String>) -> Self {
        Self {
            id: item.id.to_string(),
            local_slug: local_slug.unwrap_or_else(|| item.slug.clone()),
            repo_url: item.repo_url,
            slug: item.slug,
            path: item.path,
            name: item.display_name,
            summary: item.summary,
            version: item.latest_version.map(|v| v.version),
            owner: Some(item.owner.handle),
            security_status: item.security_status,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct DisplayFlock {
    id: String,
    repo_sign: String,
    slug: String,
    name: String,
    description: String,
    version: Option<String>,
    skill_count: usize,
    security_status: SecurityStatus,
}

impl From<FlockSummary> for DisplayFlock {
    fn from(item: FlockSummary) -> Self {
        Self {
            id: item.id.to_string(),
            repo_sign: item.repo_url,
            slug: item.slug,
            name: item.name,
            description: item.description,
            version: item.version,
            skill_count: item.skill_count.max(0) as usize,
            security_status: item.security_status,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkillFilter {
    All,
    Fetched,
}

fn load_skills_page(
    client: api::ApiClient,
    workdir: PathBuf,
    query: String,
    filter: SkillFilter,
    page_index: usize,
    mut loading: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut skill_list: Signal<Vec<DisplaySkill>>,
    mut skills_has_next: Signal<bool>,
    mut fetched_skill_total: Signal<usize>,
) {
    spawn(async move {
        loading.set(true);
        error.set(None);

        let fetched_versions = tokio::task::spawn_blocking(move || {
            savhub_local::skills::read_fetched_skill_versions(&workdir)
        })
        .await
        .unwrap_or_default();
        fetched_skill_total.set(fetched_versions.len());

        let result: Result<(Vec<DisplaySkill>, bool), String> = match filter {
            SkillFilter::All => {
                match api::fetch_remote_skill_page(
                    &client,
                    Some(query.trim()),
                    EXPLORE_PAGE_SIZE,
                    page_index,
                )
                .await
                {
                    Ok((items, has_next)) => Ok((
                        items.into_iter().map(DisplaySkill::from).collect(),
                        has_next,
                    )),
                    Err(err) => Err(err),
                }
            }
            SkillFilter::Fetched => {
                let mut fetched_items = Vec::new();
                let mut fetched_slugs: Vec<String> = fetched_versions.into_keys().collect();
                fetched_slugs.sort_unstable();

                for slug in fetched_slugs {
                    match api::resolve_remote_skill(
                        &client,
                        api::RemoteSkillLookup::from_local_slug(slug.clone()),
                    )
                    .await
                    {
                        Ok(skill) => {
                            fetched_items.push(DisplaySkill::from_remote(skill, Some(slug)))
                        }
                        Err(err) => eprintln!("failed to load fetched skill '{slug}': {err}"),
                    }
                }

                let needle = query.trim().to_lowercase();
                if !needle.is_empty() {
                    fetched_items.retain(|item| {
                        item.slug.to_lowercase().contains(&needle)
                            || item.name.to_lowercase().contains(&needle)
                            || item
                                .summary
                                .as_deref()
                                .unwrap_or_default()
                                .to_lowercase()
                                .contains(&needle)
                            || item
                                .owner
                                .as_deref()
                                .unwrap_or_default()
                                .to_lowercase()
                                .contains(&needle)
                    });
                }

                let start = page_index.saturating_mul(EXPLORE_PAGE_SIZE);
                let total_filtered = fetched_items.len();
                let page_items = fetched_items
                    .into_iter()
                    .skip(start)
                    .take(EXPLORE_PAGE_SIZE)
                    .collect::<Vec<_>>();
                let has_next = start + page_items.len() < total_filtered;
                Ok((page_items, has_next))
            }
        };

        match result {
            Ok((items, has_next)) => {
                skill_list.set(items);
                skills_has_next.set(has_next);
            }
            Err(err) => {
                skill_list.set(Vec::new());
                skills_has_next.set(false);
                error.set(Some(err));
            }
        }
        loading.set(false);
    });
}

fn load_flocks_page(
    client: api::ApiClient,
    query: String,
    page_index: usize,
    append: bool,
    mut flocks_data: Signal<Vec<DisplayFlock>>,
    mut loading: Signal<bool>,
    mut loaded: Signal<bool>,
    mut error: Signal<Option<String>>,
    mut flocks_has_next: Signal<bool>,
) {
    spawn(async move {
        loading.set(true);
        loaded.set(false);
        error.set(None);

        match api::fetch_remote_flock_page(
            &client,
            Some(query.trim()),
            FLOCKS_PAGE_SIZE,
            page_index,
        )
        .await
        {
            Ok((items, has_next)) => {
                let items = items
                    .into_iter()
                    .map(DisplayFlock::from)
                    .collect::<Vec<_>>();
                if append {
                    flocks_data.with_mut(|existing| existing.extend(items));
                } else {
                    flocks_data.set(items);
                }
                flocks_has_next.set(has_next);
            }
            Err(err) => {
                flocks_data.set(Vec::new());
                flocks_has_next.set(false);
                error.set(Some(err));
            }
        }

        loading.set(false);
        loaded.set(true);
    });
}

fn fetch_flock_from_explore(
    state: AppState,
    flock: DisplayFlock,
    mut fetched_versions: Signal<BTreeMap<String, String>>,
    mut fetched_flock_slugs: Signal<HashSet<String>>,
    mut working: Signal<bool>,
    mut action_error: Signal<Option<String>>,
) {
    let client = state.api_client();
    let workdir = state.workdir.read().clone();
    let flock_sign = format!("{}/{}", flock.repo_sign, flock.slug);

    spawn(async move {
        working.set(true);
        action_error.set(None);

        let detail = match api::fetch_remote_flock_detail(&client, &flock.id).await {
            Ok(detail) => detail,
            Err(err) => {
                action_error.set(Some(err));
                working.set(false);
                return;
            }
        };
        let repo_sign = detail.flock.repo_url.clone();
        let flock_detail_slug = detail.flock.slug.clone();
        let mut all_ok = true;

        for skill in detail.skills {
            match api::fetch_remote_skill_with_lookup(
                &client,
                &workdir,
                api::RemoteSkillLookup {
                    local_slug: skill.slug.clone(),
                    id: skill.id.as_ref().map(|id| id.to_string()),
                    slug: Some(skill.slug.clone()),
                    repo_url: Some(repo_sign.clone()),
                    path: Some(skill.path.clone()),
                    flock_slug: Some(flock_detail_slug.clone()),
                },
            )
            .await
            {
                Ok(result) => {
                    let local_slug = result.local_slug.clone();
                    let version = result.version.clone();
                    let track_slug = result.remote_slug;
                    fetched_versions.with_mut(|entries| {
                        entries.insert(local_slug.clone(), version.clone());
                    });

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
                Err(err) => {
                    action_error.set(Some(err));
                    all_ok = false;
                    break;
                }
            }
        }

        if all_ok {
            fetched_flock_slugs.with_mut(|signs| {
                signs.insert(flock_sign);
            });
        }
        working.set(false);
    });
}

fn prune_flock_from_explore(
    state: AppState,
    flock: DisplayFlock,
    mut fetched_versions: Signal<BTreeMap<String, String>>,
    mut fetched_flock_slugs: Signal<HashSet<String>>,
    mut working: Signal<bool>,
    mut action_error: Signal<Option<String>>,
) {
    let workdir = state.workdir.read().clone();
    let flock_sign = format!("{}/{}", flock.repo_sign, flock.slug);

    spawn(async move {
        working.set(true);
        action_error.set(None);

        let fs_clone = flock_sign.clone();
        let wd = workdir.clone();
        let slugs = tokio::task::spawn_blocking(move || {
            savhub_local::skills::fetched_slugs_by_flock_slug(&wd, &fs_clone)
        })
        .await
        .unwrap_or_default();

        let mut all_ok = true;
        for slug in &slugs {
            let wd = workdir.clone();
            let s = slug.clone();
            let result =
                tokio::task::spawn_blocking(move || savhub_local::skills::prune_skill(&wd, &s))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r.map_err(|e| e.to_string()));

            match result {
                Ok(()) => {
                    fetched_versions.with_mut(|entries| {
                        entries.remove(slug);
                    });
                }
                Err(err) => {
                    action_error.set(Some(err));
                    all_ok = false;
                    break;
                }
            }
        }

        if all_ok {
            fetched_flock_slugs.with_mut(|signs| {
                signs.remove(&flock_sign);
            });
        }
        working.set(false);
    });
}

#[component]
pub fn ExplorePage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut query = use_signal(String::new);
    let mut applied_query = use_signal(String::new);
    let skill_list: Signal<Vec<DisplaySkill>> = use_signal(Vec::new);
    let fetched_skill_total = use_signal(|| 0usize);
    let mut fetched_versions: Signal<BTreeMap<String, String>> = use_signal(BTreeMap::new);
    let mut fetched_flock_slugs: Signal<HashSet<String>> = use_signal(HashSet::new);
    let mut active_filter = use_signal(|| SkillFilter::All);
    let mut active_view = use_signal(|| ViewMode::Cards);
    let mut current_page = use_signal(|| 0usize);
    let mut flocks_page = use_signal(|| 0usize);
    let skills_loading = use_signal(|| false);
    let skills_error = use_signal(|| Option::<String>::None);
    let skills_has_next = use_signal(|| false);
    let flocks_loading = use_signal(|| false);
    let flocks_error = use_signal(|| Option::<String>::None);
    let flocks_loaded = use_signal(|| false);
    let flocks_has_next = use_signal(|| false);
    let mut grouped = use_signal(|| true);
    let flocks_data: Signal<Vec<DisplayFlock>> = use_signal(Vec::new);
    let mut reload_version = use_signal(|| 0u32);
    let mut flocks_version = use_signal(|| 0u32);

    use_effect(move || {
        let _ = *state.config_version.read();
        let workdir = state.workdir.read().clone();
        spawn(async move {
            let wd = workdir.clone();
            let (versions, flock_signs) = tokio::task::spawn_blocking(move || {
                let v = savhub_local::skills::read_fetched_skill_versions(&wd);
                let f = savhub_local::skills::fetched_flock_slugs(&wd);
                (v, f)
            })
            .await
            .unwrap_or_default();
            fetched_versions.set(versions);
            fetched_flock_slugs.set(flock_signs);
        });
    });

    use_effect(move || {
        let query = applied_query.read().clone();
        let filter = *active_filter.read();
        let page = *current_page.read();
        let _ = *reload_version.read();
        let _ = *state.config_version.read();
        if *grouped.read() {
            return;
        }
        load_skills_page(
            state.api_client(),
            state.workdir.read().clone(),
            query,
            filter,
            page,
            skills_loading,
            skills_error,
            skill_list,
            skills_has_next,
            fetched_skill_total,
        );
    });

    use_effect(move || {
        let query = applied_query.read().clone();
        let page = *flocks_page.read();
        let _ = *flocks_version.read();
        let _ = *reload_version.read();
        let _ = *state.config_version.read();
        if *grouped.read() {
            load_flocks_page(
                state.api_client(),
                query,
                page,
                page > 0,
                flocks_data,
                flocks_loading,
                flocks_loaded,
                flocks_error,
                flocks_has_next,
            );
        }
    });

    let mut run_search = move || {
        current_page.set(0);
        flocks_page.set(0);
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
    let fetched_label = t.fetched;
    let loading_text = t.loading;
    let no_found = t.no_skills_found;
    let no_flocks_found = t.no_flocks_found;
    let flock_skills_label = t.flock_skills_count;
    let is_grouped = *grouped.read();

    let filtered_flocks = flocks_data.read().clone();

    let current_filter = *active_filter.read();
    let current_view = *active_view.read();
    let visible_skills = skill_list.read().clone();
    let current_page_index = *current_page.read();
    let current_flocks_page = *flocks_page.read();
    let has_more_flocks = *flocks_has_next.read();
    let has_more_skills = *skills_has_next.read();
    let filter_items = [
        (SkillFilter::All, all_label, Option::<usize>::None),
        (
            SkillFilter::Fetched,
            fetched_label,
            Some(*fetched_skill_total.read()),
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
                    if !is_grouped {
                        PaginationControls {
                            current_page: current_page_index,
                            total_pages: None,
                            has_prev: current_page_index > 0,
                            has_next: has_more_skills,
                            on_prev: move |_| current_page.set(current_page_index.saturating_sub(1)),
                            on_next: move |_| current_page.set(current_page_index + 1),
                        }
                    }
                    button {
                        title: "Refresh",
                        style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer; font-size: 16px;",
                        onclick: move |_| {
                            current_page.set(0);
                            flocks_page.set(0);
                            reload_version += 1;
                            flocks_version += 1;
                        },
                        crate::icons::LucideIcon { icon: crate::icons::Icon::RefreshCw, size: 14 }
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
                            current_page.set(0);
                            flocks_page.set(0);
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
                                    if let Some(count) = count {
                                        span { style: "font-size: 10px; opacity: 0.8;", "{count}" }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // ── Scrollable content ──
            div {
                style: "flex: 1; overflow-y: auto; padding: 20px 32px 32px;",
                onscroll: move |evt: Event<ScrollData>| {
                    if !is_grouped || *flocks_loading.read() || !has_more_flocks {
                        return;
                    }
                    let remaining = evt.scroll_height() as f64
                        - evt.client_height() as f64
                        - evt.scroll_top();
                    if remaining <= 180.0 {
                        flocks_page.set(current_flocks_page + 1);
                    }
                },

            {
                let is_loading = if is_grouped { *flocks_loading.read() } else { *skills_loading.read() };
                let current_error = if is_grouped { flocks_error.read().clone() } else { skills_error.read().clone() };
                let show_primary_loading = if is_grouped {
                    is_loading && filtered_flocks.is_empty()
                } else {
                    is_loading
                };
                rsx! {
                    if show_primary_loading {
                        p { style: "color: {Theme::MUTED}; padding: 20px 0; text-align: center; width: 100%;", "{loading_text}" }
                    }
                    if let Some(err) = current_error.as_ref() {
                        div { style: "padding: 12px 16px; background: rgba(139, 30, 30, 0.08); border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 6px; color: {Theme::DANGER}; margin-bottom: 16px;",
                            "{err}"
                        }
                    }
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
                                fetched_versions: fetched_versions,
                                fetched_flock_slugs: fetched_flock_slugs,
                            }
                        }
                    }
                } else {
                    div { style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px;",
                        for flock in filtered_flocks.iter() {
                            FlockCard {
                                flock: flock.clone(),
                                flock_skills_label: flock_skills_label,
                                fetched_versions: fetched_versions,
                                fetched_flock_slugs: fetched_flock_slugs,
                            }
                        }
                    }
                }
                if filtered_flocks.is_empty() && !*flocks_loading.read() {
                    if *flocks_loaded.read() && flocks_error.read().is_none() {
                        p { style: "color: {Theme::MUTED}; text-align: center; padding: 40px 0;",
                            "{no_flocks_found}"
                        }
                    }
                }
                if !filtered_flocks.is_empty() && *flocks_loading.read() {
                    div { style: "display: flex; justify-content: center; padding: 14px 0 6px;",
                        span { style: "display: inline-flex; align-items: center; gap: 8px; font-size: 12px; color: {Theme::MUTED};",
                            span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                            "{loading_text}"
                        }
                    }
                }
            } else {
                // ── Skills (ungrouped) view ──
                if current_view == ViewMode::List {
                    div { style: "display: flex; flex-direction: column; gap: 10px;",
                        for skill in visible_skills.iter() {
                            SkillListRow {
                                skill: skill.clone(),
                                fetched_versions: fetched_versions,
                            }
                        }
                    }
                } else {
                    div { style: "display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px;",
                        for skill in visible_skills.iter() {
                            SkillCard {
                                skill: skill.clone(),
                                fetched_versions: fetched_versions,
                            }
                        }
                    }
                }

                if !*skills_loading.read() && visible_skills.is_empty() && skills_error.read().is_none() {
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
    mut fetched_versions: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let down_pos = use_signal(|| (0.0, 0.0));
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let local_slug = skill.local_slug.clone();
    let skill_id = skill.id.clone();
    let remote_slug = skill.slug.clone();
    let skill_repo_url = skill.repo_url.clone();
    let skill_path = skill.path.clone();
    let is_fetched = fetched_versions.read().contains_key(&skill.local_slug);

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let lookup = api::RemoteSkillLookup {
            local_slug: local_slug.clone(),
            id: Some(skill_id.clone()),
            slug: Some(remote_slug.clone()),
            repo_url: Some(skill_repo_url.clone()),
            path: Some(skill_path.clone()),
            flock_slug: None,
        };
        let local_slug = local_slug.clone();
        let remote_slug = remote_slug.clone();
        let should_prune = is_fetched;
        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        spawn(async move {
            working.set(true);
            action_error.set(None);
            let result = if should_prune {
                let workdir = workdir.clone();
                let local_slug = local_slug.clone();
                tokio::task::spawn_blocking(move || {
                    savhub_local::skills::prune_skill(&workdir, &local_slug)
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map(|_| String::new()).map_err(|e| e.to_string()))
            } else {
                api::fetch_remote_skill_with_lookup(&client, &workdir, lookup)
                    .await
                    .map(|result| result.version)
            };
            match result {
                Ok(version) => {
                    fetched_versions.with_mut(|entries| {
                        if should_prune {
                            entries.remove(&local_slug);
                        } else {
                            entries.insert(local_slug.clone(), version);
                        }
                    });
                    if !should_prune {
                        let track_slug = remote_slug;
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
                Err(err) => action_error.set(Some(err)),
            }
            working.set(false);
        });
    };

    let version_display = skill.version.as_deref().unwrap_or("\u{2014}");

    let nav = use_navigator();
    let slug_nav = skill.id.clone();

    rsx! {
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 10px; padding: 14px 16px; display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; cursor: pointer;",
                onmousedown: move |evt| click_guard::capture_mouse_down(down_pos, evt),
                onclick: move |evt| {
                    if click_guard::is_click_without_drag(down_pos, &evt) {
                        nav.push(crate::Route::Detail { slug: slug_nav.clone() });
                    }
                },
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
                        crate::components::copy_sign::CopySign { repo_url: skill.repo_url.clone(), path: skill.path.clone() }
                    }
                    if let Some(desc) = &skill.summary {
                        p { style: "font-size: 13px; color: {Theme::MUTED}; line-height: 1.5; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                            "{desc}"
                        }
                    }
                }
                div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 8px; flex-shrink: 0;",
                    if *working.read() {
                        span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                            span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                            "{t.fetching}"
                        }
                    } else if is_fetched {
                        button {
                            style: "padding: 5px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 999px; cursor: pointer; font-weight: 600; white-space: nowrap;",
                            onclick: do_action,
                            "{t.prune}"
                        }
                    } else {
                        button {
                            style: "padding: 5px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 999px; cursor: pointer; font-weight: 600; white-space: nowrap;",
                            onclick: do_action,
                            "{t.fetch}"
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
    mut fetched_versions: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let down_pos = use_signal(|| (0.0, 0.0));
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let local_slug = skill.local_slug.clone();
    let skill_id = skill.id.clone();
    let remote_slug = skill.slug.clone();
    let skill_repo_url = skill.repo_url.clone();
    let skill_path = skill.path.clone();
    let is_fetched = fetched_versions.read().contains_key(&skill.local_slug);

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let lookup = api::RemoteSkillLookup {
            local_slug: local_slug.clone(),
            id: Some(skill_id.clone()),
            slug: Some(remote_slug.clone()),
            repo_url: Some(skill_repo_url.clone()),
            path: Some(skill_path.clone()),
            flock_slug: None,
        };
        let local_slug = local_slug.clone();
        let remote_slug = remote_slug.clone();
        let should_prune = is_fetched;
        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        spawn(async move {
            working.set(true);
            action_error.set(None);
            let result = if should_prune {
                let workdir = workdir.clone();
                let local_slug = local_slug.clone();
                tokio::task::spawn_blocking(move || {
                    savhub_local::skills::prune_skill(&workdir, &local_slug)
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map(|_| String::new()).map_err(|e| e.to_string()))
            } else {
                api::fetch_remote_skill_with_lookup(&client, &workdir, lookup)
                    .await
                    .map(|result| result.version)
            };
            match result {
                Ok(version) => {
                    fetched_versions.with_mut(|entries| {
                        if should_prune {
                            entries.remove(&local_slug);
                        } else {
                            entries.insert(local_slug.clone(), version);
                        }
                    });
                    if !should_prune {
                        let track_slug = remote_slug;
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
                Err(err) => action_error.set(Some(err)),
            }
            working.set(false);
        });
    };

    let version_display = skill.version.as_deref().unwrap_or("\u{2014}");

    let nav = use_navigator();
    let slug_nav = skill.id.clone();

    rsx! {
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; display: flex; flex-direction: column; gap: 8px; cursor: pointer; transition: box-shadow 0.15s;",
                onmousedown: move |evt| click_guard::capture_mouse_down(down_pos, evt),
                onclick: move |evt| {
                    if click_guard::is_click_without_drag(down_pos, &evt) {
                        nav.push(crate::Route::Detail { slug: slug_nav.clone() });
                    }
                },
                div { style: "display: flex; justify-content: space-between; align-items: flex-start;",
                    div {
                        h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 2px;",
                            "{skill.name}"
                        }
                        crate::components::copy_sign::CopySign { repo_url: skill.repo_url.clone(), path: skill.path.clone() }
                    }
                    div { style: "display: flex; gap: 4px; align-items: center;",
                        {
                            let (sec_label, sec_color) = match skill.security_status {
                                savhub_shared::SecurityStatus::Verified => ("V", Theme::SUCCESS),
                                savhub_shared::SecurityStatus::Flagged => ("!", "#d4a017"),
                                savhub_shared::SecurityStatus::Rejected => ("X", Theme::DANGER),
                                _ => ("", ""),
                            };
                            if !sec_label.is_empty() {
                                rsx! {
                                    span { style: "font-size: 10px; width: 16px; height: 16px; display: inline-flex; align-items: center; justify-content: center; border-radius: 50%; background: {sec_color}20; color: {sec_color}; font-weight: 700;",
                                        "{sec_label}"
                                    }
                                }
                            } else {
                                rsx! {}
                            }
                        }
                        span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 10px; white-space: nowrap;",
                            "v{version_display}"
                        }
                    }
                }
                if let Some(desc) = &skill.summary {
                    p { style: "font-size: 13px; color: {Theme::MUTED}; flex: 1; display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden;",
                        "{desc}"
                    }
                }
                div { style: "display: flex; justify-content: flex-end; align-items: center; margin-top: 4px;",
                    if *working.read() {
                        span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                            span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                            "{t.fetching}"
                        }
                    } else if is_fetched {
                        button {
                            style: "padding: 4px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 4px; cursor: pointer; font-weight: 500;",
                            onclick: do_action,
                            "{t.prune}"
                        }
                    } else {
                        button {
                            style: "padding: 4px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; cursor: pointer; font-weight: 500;",
                            onclick: do_action,
                            "{t.fetch}"
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
    fetched_versions: Signal<BTreeMap<String, String>>,
    fetched_flock_slugs: Signal<HashSet<String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let down_pos = use_signal(|| (0.0, 0.0));
    let working = use_signal(|| false);
    let action_error = use_signal(|| Option::<String>::None);
    let version_display = flock.version.as_deref().unwrap_or("\u{2014}");
    let flock_sign = format!("{}/{}", flock.repo_sign, flock.slug);
    let is_fetched = fetched_flock_slugs.read().contains(&flock_sign);
    let nav = use_navigator();
    let nav_slug = flock.id.clone();
    let flock_action = {
        let flock = flock.clone();
        move |e: Event<MouseData>| {
            e.stop_propagation();
            if is_fetched {
                prune_flock_from_explore(
                    state,
                    flock.clone(),
                    fetched_versions,
                    fetched_flock_slugs,
                    working,
                    action_error,
                );
            } else {
                fetch_flock_from_explore(
                    state,
                    flock.clone(),
                    fetched_versions,
                    fetched_flock_slugs,
                    working,
                    action_error,
                );
            }
        }
    };

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 10px; padding: 14px 16px; display: flex; align-items: flex-start; justify-content: space-between; gap: 16px; cursor: pointer;",
            onmousedown: move |evt| click_guard::capture_mouse_down(down_pos, evt),
            onclick: move |evt| {
                if click_guard::is_click_without_drag(down_pos, &evt) {
                    nav.push(crate::Route::FlockDetail { slug: nav_slug.clone() });
                }
            },
            div { style: "min-width: 0; flex: 1;",
                div { style: "display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin-bottom: 4px;",
                    h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                        "{flock.name}"
                    }
                    crate::components::security_badge::SecurityBadge { status: flock.security_status }
                    span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px; white-space: nowrap;",
                        "v{version_display}"
                    }
                }
                div { style: "margin-bottom: 6px;",
                    crate::components::copy_sign::CopySign { repo_url: flock.repo_sign.clone(), path: flock.slug.clone() }
                }
                if !flock.description.is_empty() {
                    p { style: "font-size: 13px; color: {Theme::MUTED}; line-height: 1.5; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; overflow: hidden;",
                        "{flock.description}"
                    }
                }
            }
            div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 8px; flex-shrink: 0;",
                span { style: "font-size: 12px; color: {Theme::MUTED}; white-space: nowrap;",
                    "{flock.skill_count} {flock_skills_label}"
                }
                if *working.read() {
                    span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                        span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                        "{t.fetching}"
                    }
                } else if is_fetched {
                    button {
                        style: "padding: 5px 12px; font-size: 12px; background: rgba(200,50,50,0.1); color: #c03030; border: none; border-radius: 999px; cursor: pointer; font-weight: 600; white-space: nowrap;",
                        onclick: flock_action,
                        "{t.flock_prune_all}"
                    }
                } else {
                    button {
                        style: "padding: 5px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 999px; cursor: pointer; font-weight: 600; white-space: nowrap;",
                        onclick: flock_action,
                        "{t.flock_fetch_all}"
                    }
                }
                if let Some(err) = action_error.read().as_ref() {
                    p { style: "font-size: 11px; color: {Theme::DANGER}; max-width: 220px; text-align: right;",
                        "{err}"
                    }
                }
            }
        }
    }
}

#[component]
fn FlockCard(
    flock: DisplayFlock,
    flock_skills_label: &'static str,
    fetched_versions: Signal<BTreeMap<String, String>>,
    fetched_flock_slugs: Signal<HashSet<String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let down_pos = use_signal(|| (0.0, 0.0));
    let working = use_signal(|| false);
    let action_error = use_signal(|| Option::<String>::None);
    let version_display = flock.version.as_deref().unwrap_or("\u{2014}");
    let slug_display = format!("{}/{}", flock.repo_sign, flock.slug);
    let flock_sign = format!("{}/{}", flock.repo_sign, flock.slug);
    let is_fetched = fetched_flock_slugs.read().contains(&flock_sign);
    let nav = use_navigator();
    let nav_slug = flock.id.clone();
    let flock_action = {
        let flock = flock.clone();
        move |e: Event<MouseData>| {
            e.stop_propagation();
            if is_fetched {
                prune_flock_from_explore(
                    state,
                    flock.clone(),
                    fetched_versions,
                    fetched_flock_slugs,
                    working,
                    action_error,
                );
            } else {
                fetch_flock_from_explore(
                    state,
                    flock.clone(),
                    fetched_versions,
                    fetched_flock_slugs,
                    working,
                    action_error,
                );
            }
        }
    };

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; display: flex; flex-direction: column; gap: 8px; cursor: pointer; transition: box-shadow 0.15s;",
            onmousedown: move |evt| click_guard::capture_mouse_down(down_pos, evt),
            onclick: move |evt| {
                if click_guard::is_click_without_drag(down_pos, &evt) {
                    nav.push(crate::Route::FlockDetail { slug: nav_slug.clone() });
                }
            },
            div { style: "display: flex; justify-content: space-between; align-items: flex-start;",
                div {
                    div { style: "display: flex; align-items: center; gap: 6px; margin-bottom: 2px;",
                        h3 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                            "{flock.name}"
                        }
                        crate::components::security_badge::SecurityBadge { status: flock.security_status }
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
                    "{flock.skill_count} {flock_skills_label}"
                }
                if *working.read() {
                    span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                        span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                        "{t.fetching}"
                    }
                } else if is_fetched {
                    button {
                        style: "padding: 4px 12px; font-size: 12px; background: rgba(200,50,50,0.1); color: #c03030; border: none; border-radius: 4px; cursor: pointer; font-weight: 500;",
                        onclick: flock_action,
                        "{t.flock_prune_all}"
                    }
                } else {
                    button {
                        style: "padding: 4px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; cursor: pointer; font-weight: 500;",
                        onclick: flock_action,
                        "{t.flock_fetch_all}"
                    }
                }
            }
            if let Some(err) = action_error.read().as_ref() {
                p { style: "font-size: 11px; color: {Theme::DANGER};",
                    "{err}"
                }
            }
        }
    }
}
