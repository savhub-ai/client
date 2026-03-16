use std::collections::BTreeMap;
use std::path::PathBuf;

use dioxus::prelude::*;
use serde::{Deserialize, Serialize};

use crate::components::pagination::{self, PaginationControls};
use crate::i18n;
use crate::skills;
use crate::state::AppState;
use crate::theme::Theme;

const INSTALLED_SKILLS_PAGE_SIZE: usize = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LockEntry {
    version: String,
    installed_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Lockfile {
    version: u8,
    skills: BTreeMap<String, LockEntry>,
}

#[derive(Debug, Clone, PartialEq)]
struct InstalledSkill {
    slug: String,
    version: String,
    installed_at: String,
    path: PathBuf,
}

#[component]
pub fn InstalledPage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut skill_list: Signal<Vec<InstalledSkill>> = use_signal(Vec::new);
    let mut skill_page = use_signal(|| 0usize);
    let mut loaded = use_signal(|| false);

    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);
        let workdir = state.workdir.read().clone();
        let lock_path = workdir.join(".savhub").join("lock.json");

        if let Ok(raw) = std::fs::read_to_string(&lock_path) {
            if let Ok(lock) = serde_json::from_str::<Lockfile>(&raw) {
                let mut list = Vec::new();
                for (slug, entry) in &lock.skills {
                    let ts = chrono::DateTime::from_timestamp(entry.installed_at, 0)
                        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                        .unwrap_or_else(|| "\u{2014}".to_string());
                    list.push(InstalledSkill {
                        slug: slug.clone(),
                        version: entry.version.clone(),
                        installed_at: ts,
                        path: workdir.join(slug),
                    });
                }
                skill_list.set(list);
            }
        }
    });

    let update_all = move |_| {
        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        let slugs: Vec<String> = skill_list.read().iter().map(|s| s.slug.clone()).collect();
        let mut status = state.status_message;
        spawn(async move {
            let mut updated = 0usize;
            for slug in &slugs {
                match client
                    .get_json::<savhub_shared::ResolveResponse>(&format!(
                        "/skills/{slug}/resolve?tag=latest"
                    ))
                    .await
                {
                    Ok(resolved) => {
                        let version = resolved
                            .matched
                            .or(resolved.latest_version)
                            .map(|v| v.version);
                        if let Some(ver) = version {
                            let download_path = format!("/skills/{slug}/versions/{ver}/download");
                            if let Ok(bytes) = client.get_bytes(&download_path).await {
                                let skill_dir = workdir.join(slug);
                                let _ = skills::extract_zip(&bytes, &skill_dir);
                                skills::update_lockfile(&workdir, slug, &ver);
                                updated += 1;
                            }
                        }
                    }
                    Err(_) => {}
                }
            }
            let t = i18n::texts(*state.lang.read());
            status.set(t.fmt_updated_skills(updated, slugs.len()));
        });
    };

    let title = t.installed_skills;
    let update_all_label = t.update_all;
    let empty_msg = t.no_skills_installed;
    let empty_hint = t.no_skills_installed_hint;
    let installed_items = skill_list.read().clone();
    let current_page = pagination::clamp_page(
        *skill_page.read(),
        installed_items.len(),
        INSTALLED_SKILLS_PAGE_SIZE,
    );
    let visible_skills =
        pagination::slice_for_page(&installed_items, current_page, INSTALLED_SKILLS_PAGE_SIZE);
    let total_pages = pagination::total_pages(installed_items.len(), INSTALLED_SKILLS_PAGE_SIZE);

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            // ── Sticky header ──
            div { style: "flex-shrink: 0; padding: 12px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10; display: flex; justify-content: space-between; align-items: center; gap: 10px;",
                h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT};",
                    "{title}"
                }
                div { style: "display: flex; gap: 8px; align-items: center; margin-left: auto;",
                    button {
                        title: "Refresh",
                        style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer; font-size: 16px;",
                        onclick: move |_| skill_list.with_mut(|_| {}),
                        "\u{21BB}"
                    }
                    if !installed_items.is_empty() {
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

            if installed_items.is_empty() {
                div { style: "text-align: center; padding: 60px 20px; color: {Theme::MUTED};",
                    p { style: "font-size: 16px; margin-bottom: 8px;", "{empty_msg}" }
                    p { style: "font-size: 13px;", "{empty_hint}" }
                }
            } else {
                div { style: "display: flex; flex-direction: column; gap: 8px;",
                    for skill in visible_skills.iter() {
                        InstalledRow {
                            skill: skill.clone(),
                            skill_list: skill_list,
                        }
                    }
                }
                PaginationControls {
                    current_page: current_page,
                    total_pages: Some(total_pages),
                    has_prev: current_page > 0,
                    has_next: current_page + 1 < total_pages,
                    on_prev: move |_| skill_page.set(current_page.saturating_sub(1)),
                    on_next: move |_| skill_page.set(current_page + 1),
                }
            }
            } // scrollable content
        }
    }
}

#[component]
fn InstalledRow(skill: InstalledSkill, mut skill_list: Signal<Vec<InstalledSkill>>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let slug = skill.slug.clone();

    let do_uninstall = move |_| {
        let workdir = state.workdir.read().clone();
        let slug = slug.clone();
        let skill_dir = workdir.join(&slug);
        let _ = std::fs::remove_dir_all(&skill_dir);

        let lock_path = workdir.join(".savhub").join("lock.json");
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

    let installed_prefix = t.installed_at_prefix;
    let uninstall_label = t.uninstall;

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 14px 16px; display: flex; align-items: center; justify-content: space-between;",
            div { style: "display: flex; align-items: center; gap: 16px; flex: 1; min-width: 0;",
                div {
                    p { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};",
                        "{skill.slug}"
                    }
                    p { style: "font-size: 12px; color: {Theme::MUTED};",
                        "{installed_prefix} {skill.installed_at}"
                    }
                }
                span { style: "font-size: 12px; padding: 2px 8px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 10px;",
                    "v{skill.version}"
                }
            }
            button {
                style: "padding: 4px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 4px; cursor: pointer;",
                onclick: do_uninstall,
                "{uninstall_label}"
            }
        }
    }
}
