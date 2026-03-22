use std::path::PathBuf;

use dioxus::prelude::*;
use savhub_shared::Lockfile;

use crate::components::pagination::{self, PaginationControls};
use crate::i18n;
use crate::state::AppState;
use crate::theme::Theme;

const FETCHED_SKILLS_PAGE_SIZE: usize = 10;

#[derive(Debug, Clone, PartialEq)]
struct FetchedSkill {
    slug: String,
    version: String,
    fetched_at: String,
    path: PathBuf,
    remote_id: Option<String>,
    remote_slug: Option<String>,
    repo_url: Option<String>,
    remote_path: Option<String>,
}

#[component]
pub fn FetchedPage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut skill_list: Signal<Vec<FetchedSkill>> = use_signal(Vec::new);
    let mut skill_page = use_signal(|| 0usize);
    let mut loaded = use_signal(|| false);

    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);
        let workdir = state.workdir.read().clone();

        spawn(async move {
            let workdir_bg = workdir.clone();
            let list = tokio::task::spawn_blocking(move || {
                let lock_path = workdir_bg.join("skills.lock.json");
                let raw = std::fs::read_to_string(&lock_path).ok()?;
                let lock: Lockfile = serde_json::from_str(&raw).ok()?;
                let flat = savhub_local::skills::flatten_lockfile(&lock);
                let list: Vec<FetchedSkill> = flat
                    .into_iter()
                    .map(|e| {
                        let slug = e.slug.clone();
                        let ts = chrono::DateTime::from_timestamp(e.entry.fetched_at, 0)
                            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                            .unwrap_or_else(|| "\u{2014}".to_string());
                        FetchedSkill {
                            slug: slug.clone(),
                            version: e.entry.version,
                            fetched_at: ts,
                            path: workdir_bg.join(&slug),
                            remote_id: e.entry.remote_id,
                            remote_slug: e.entry.remote_slug,
                            repo_url: Some(e.repo_url),
                            remote_path: Some(e.path),
                        }
                    })
                    .collect();
                Some(list)
            })
            .await
            .ok()
            .flatten()
            .unwrap_or_default();
            skill_list.set(list);
        });
    });

    let update_all = move |_| {
        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        let entries = skill_list.read().clone();
        let mut status = state.status_message;
        let mut skill_list_signal = skill_list;
        spawn(async move {
            let mut updated = 0usize;
            let mut failures = Vec::new();
            for entry in &entries {
                let lookup = crate::api::RemoteSkillLookup {
                    local_slug: entry.slug.clone(),
                    id: entry.remote_id.clone(),
                    slug: entry
                        .remote_slug
                        .clone()
                        .or_else(|| Some(entry.slug.clone())),
                    repo_url: entry.repo_url.clone(),
                    path: entry.remote_path.clone(),
                    flock_slug: None,
                };
                match crate::api::fetch_remote_skill_with_lookup(&client, &workdir, lookup).await {
                    Ok(result) => {
                        updated += 1;
                        let fetched_at = chrono::Local::now().format("%Y-%m-%d %H:%M").to_string();
                        skill_list_signal.with_mut(|items| {
                            if let Some(item) =
                                items.iter_mut().find(|item| item.slug == result.local_slug)
                            {
                                item.version = result.version.clone();
                                item.fetched_at = fetched_at.clone();
                                item.remote_id = Some(result.remote_id.clone());
                                item.remote_slug = Some(result.remote_slug.clone());
                                item.repo_url = Some(result.repo_url.clone());
                                item.remote_path = Some(result.path.clone());
                            }
                        });
                    }
                    Err(err) => failures.push(format!("{}: {err}", entry.slug)),
                }
            }
            let total = entries.len();
            let t = i18n::texts(*state.lang.read());
            let base_message = t.fmt_updated_skills(updated, total);
            if let Some(first_error) = failures.first() {
                status.set(format!("{base_message} | {}", first_error));
            } else {
                status.set(base_message);
            }
        });
    };

    let title = t.fetched_skills;
    let update_all_label = t.update_all;
    let empty_msg = t.no_skills_fetched;
    let empty_hint = t.no_skills_fetched_hint;
    let fetched_items = skill_list.read().clone();
    let current_page = pagination::clamp_page(
        *skill_page.read(),
        fetched_items.len(),
        FETCHED_SKILLS_PAGE_SIZE,
    );
    let visible_skills =
        pagination::slice_for_page(&fetched_items, current_page, FETCHED_SKILLS_PAGE_SIZE);
    let total_pages = pagination::total_pages(fetched_items.len(), FETCHED_SKILLS_PAGE_SIZE);

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            // ── Sticky header ──
            div { style: "flex-shrink: 0; padding: 12px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10; display: flex; justify-content: space-between; align-items: center; gap: 10px;",
                h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT};",
                    "{title}"
                }
                div { style: "display: flex; gap: 8px; align-items: center; margin-left: auto;",
                    PaginationControls {
                        current_page: current_page,
                        total_pages: Some(total_pages),
                        has_prev: current_page > 0,
                        has_next: current_page + 1 < total_pages,
                        on_prev: move |_| skill_page.set(current_page.saturating_sub(1)),
                        on_next: move |_| skill_page.set(current_page + 1),
                    }
                    button {
                        title: "Refresh",
                        style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer; font-size: 16px;",
                        onclick: move |_| skill_list.with_mut(|_| {}),
                        crate::icons::LucideIcon { icon: crate::icons::Icon::RefreshCw, size: 14 }
                    }
                    if !fetched_items.is_empty() {
                        button {
                            style: "padding: 7px 16px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 13px; font-weight: 500; cursor: pointer;",
                            onclick: update_all,
                            "{update_all_label}"
                        }
                    }
                }
            }

            // ── Scrollable content ──
            div { style: "flex: 1; overflow-y: auto; padding: 20px 32px 32px;",

            if !state.status_message.read().is_empty() {
                div { style: "padding: 10px 14px; background: {Theme::ACCENT_LIGHT}; border-radius: 6px; margin-bottom: 16px; font-size: 13px; color: {Theme::ACCENT_STRONG};",
                    "{state.status_message}"
                }
            }

            if fetched_items.is_empty() {
                div { style: "text-align: center; padding: 60px 20px; color: {Theme::MUTED};",
                    p { style: "font-size: 16px; margin-bottom: 8px;", "{empty_msg}" }
                    p { style: "font-size: 13px;", "{empty_hint}" }
                }
            } else {
                div { style: "display: flex; flex-direction: column; gap: 8px;",
                    for skill in visible_skills.iter() {
                        FetchedRow {
                            skill: skill.clone(),
                            skill_list: skill_list,
                        }
                    }
                }
            }
            } // scrollable content
        }
    }
}

#[component]
fn FetchedRow(skill: FetchedSkill, mut skill_list: Signal<Vec<FetchedSkill>>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let slug = skill.slug.clone();

    let do_uninstall = move |_| {
        let workdir = state.workdir.read().clone();
        let slug = slug.clone();
        let skill_dir = workdir.join(&slug);
        let _ = std::fs::remove_dir_all(&skill_dir);

        let lock_path = workdir.join("skills.lock.json");
        if let Ok(raw) = std::fs::read_to_string(&lock_path) {
            if let Ok(mut lock) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(map) = lock.get_mut("skills").and_then(|v| v.as_object_mut()) {
                    map.remove(&slug);
                }
                let _ = std::fs::write(
                    &lock_path,
                    serde_json::to_string_pretty(&lock).unwrap_or_default(),
                );
            }
        }
        skill_list.with_mut(|items| items.retain(|entry| entry.slug != slug));
    };

    let fetched_prefix = t.fetched_at_prefix;
    let prune_label = t.prune;

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 14px 16px; display: flex; align-items: center; justify-content: space-between;",
            div { style: "display: flex; align-items: center; gap: 16px; flex: 1; min-width: 0;",
                div {
                    p { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                        "{skill.slug}"
                    }
                    p { style: "font-size: 12px; color: {Theme::MUTED};",
                        "{fetched_prefix} {skill.fetched_at}"
                    }
                }
                span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 10px;",
                    "v{skill.version}"
                }
            }
            button {
                style: "padding: 4px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 4px; cursor: pointer;",
                onclick: do_uninstall,
                "{prune_label}"
            }
        }
    }
}
