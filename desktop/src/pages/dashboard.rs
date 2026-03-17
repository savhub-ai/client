use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use chrono::{Local, TimeZone};
use dioxus::prelude::*;
use savhub_local::clients::DetectedClient;
use savhub_local::config::{ProjectEntry, read_projects_list};
use savhub_shared::{UserSummary, WhoAmIResponse};

use crate::api::ApiCompatibility;
use crate::components::pagination::{self, PaginationControls};
use crate::i18n;
use crate::state::AppState;
use crate::theme::Theme;

const RECENT_PROJECTS_PAGE_SIZE: usize = 6;
const DETECTED_AGENTS_PAGE_SIZE: usize = 6;

#[derive(Clone, Copy, PartialEq, Eq)]
enum RecentProjectKind {
    Added,
    Updated,
}

#[derive(Clone, PartialEq)]
struct RecentProject {
    name: String,
    path: String,
    activity_at: i64,
    kind: RecentProjectKind,
}

#[component]
pub fn DashboardPage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut health = use_signal(|| t.checking.to_string());
    let mut health_error = use_signal(|| Option::<String>::None);
    let mut show_health_error = use_signal(|| false);
    let mut user_info = use_signal(|| "\u{2014}".to_string());
    let mut skill_count = use_signal(|| 0usize);
    let mut detected_agents = use_signal(Vec::<DetectedClient>::new);
    let mut recent_projects = use_signal(Vec::<RecentProject>::new);
    let mut recent_projects_page = use_signal(|| 0usize);
    let mut detected_agents_page = use_signal(|| 0usize);
    let mut loaded = use_signal(|| false);
    let mut registry_syncing = use_signal(|| false);
    let mut registry_sync_result = use_signal(|| Option::<Result<bool, String>>::None);

    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);

        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        let mut current_user = state.current_user;
        detected_agents.set(
            savhub_local::clients::detect_clients()
                .into_iter()
                .filter(|client| client.installed)
                .collect(),
        );
        recent_projects.set(load_recent_projects());

        let lock_path = workdir.join(".savhub").join("lock.json");
        if let Ok(raw) = std::fs::read_to_string(&lock_path) {
            if let Ok(lock) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(skills) = lock.get("skills").and_then(|value| value.as_object()) {
                    skill_count.set(skills.len());
                }
            }
        }

        spawn(async move {
            let t = i18n::texts(*state.lang.read());

            match client.get_json::<serde_json::Value>("/health").await {
                Ok(value) => {
                    let status = value
                        .get("status")
                        .and_then(|field| field.as_str())
                        .unwrap_or("unknown");
                    let api_ver = value
                        .get("apiVersion")
                        .and_then(|v| v.as_u64())
                        .map(|v| format!(" (API v{v})"))
                        .unwrap_or_default();
                    health.set(format!("{status}{api_ver}"));
                    health_error.set(None);
                    show_health_error.set(false);
                }
                Err(error) => {
                    health.set(t.offline.to_string());
                    health_error.set(Some(error));
                }
            }

            match client.get_json::<WhoAmIResponse>("/whoami").await {
                Ok(resp) => {
                    if let Some(user) = resp.user {
                        user_info.set(format!("@{}", user.handle));
                        current_user.set(Some(user));
                    } else {
                        current_user.set(None);
                        user_info.set(t.anonymous.to_string());
                    }
                }
                Err(_) => {
                    current_user.set(None);
                    user_info.set(t.not_logged_in.to_string());
                }
            }
        });
    });

    let title = t.dashboard_title;
    let registry_label = t.registry_status;
    let account_label = t.dashboard_account;
    let skills_label = t.installed_skills;
    let recent_projects_label = t.dashboard_recent_projects;
    let no_recent_projects_label = t.dashboard_no_recent_projects;
    let recent_added_label = t.dashboard_recent_added;
    let recent_updated_label = t.dashboard_recent_updated;
    let agents_label = t.detected_ai_agents;
    let no_agents_label = t.no_ai_agents_detected;
    let registry_value = health.read().clone();
    let registry_error = health_error.read().clone();
    let registry_error_dialog = if *show_health_error.read() {
        registry_error.clone()
    } else {
        None
    };
    let agents = detected_agents.read().clone();
    let recent_projects_list = recent_projects.read().clone();
    let recent_projects_current_page = pagination::clamp_page(
        *recent_projects_page.read(),
        recent_projects_list.len(),
        RECENT_PROJECTS_PAGE_SIZE,
    );
    let visible_recent_projects = pagination::slice_for_page(
        &recent_projects_list,
        recent_projects_current_page,
        RECENT_PROJECTS_PAGE_SIZE,
    );
    let recent_projects_total_pages =
        pagination::total_pages(recent_projects_list.len(), RECENT_PROJECTS_PAGE_SIZE);
    let agents_current_page = pagination::clamp_page(
        *detected_agents_page.read(),
        agents.len(),
        DETECTED_AGENTS_PAGE_SIZE,
    );
    let visible_agents =
        pagination::slice_for_page(&agents, agents_current_page, DETECTED_AGENTS_PAGE_SIZE);
    let agents_total_pages = pagination::total_pages(agents.len(), DETECTED_AGENTS_PAGE_SIZE);
    let compat = state.registry_compat.read().clone();
    let registry_accent = if registry_error.is_some() {
        Theme::DANGER
    } else if matches!(compat, ApiCompatibility::Incompatible { .. }) {
        Theme::DANGER
    } else if registry_value.starts_with("ok") {
        Theme::SUCCESS
    } else {
        Theme::DANGER
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            // ── Sticky header ──
            div { style: "flex-shrink: 0; padding: 12px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10; display: flex; align-items: center; gap: 10px;",
                h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT};",
                    "{title}"
                }
                div { style: "flex: 1;" }
                button {
                    title: "Refresh",
                    style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer; font-size: 16px;",
                    onclick: move |_| loaded.set(false),
                    "\u{21BB}"
                }
            }
            // ── Scrollable content ──
            div { style: "flex: 1; overflow-y: auto; padding: 20px 32px 32px;",

            div { style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 16px;",
                div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
                    div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px;",
                        p { style: "font-size: 12px; color: {Theme::MUTED};",
                            "{registry_label}"
                        }
                        if *registry_syncing.read() {
                            span { style: "display: inline-flex; align-items: center; gap: 5px; font-size: 11px; color: {Theme::ACCENT}; font-weight: 600;",
                                span { style: "display: inline-block; width: 12px; height: 12px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                                "{t.registry_syncing}"
                            }
                        } else {
                            button {
                                style: "padding: 3px 10px; font-size: 11px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: 1px solid rgba(90, 158, 63, 0.18); border-radius: 6px; cursor: pointer; font-weight: 600;",
                                onclick: move |_| {
                                    spawn(async move {
                                        registry_syncing.set(true);
                                        registry_sync_result.set(None);
                                        let result = tokio::task::spawn_blocking(|| {
                                            // Force sync by clearing registry.json first
                                            let _ = savhub_local::registry::write_registry_state(
                                                &savhub_local::registry::RegistryState::default(),
                                            );
                                            savhub_local::registry::ensure_registry_synced()
                                        })
                                        .await
                                        .map_err(|e| e.to_string())
                                        .and_then(|r| r.map_err(|e| e.to_string()));
                                        registry_sync_result.set(Some(result));
                                        registry_syncing.set(false);

                                        // Re-check API health after sync so status updates if back online
                                        let client = state.api_client();
                                        let t = i18n::texts(*state.lang.read());
                                        match client.get_json::<serde_json::Value>("/health").await {
                                            Ok(value) => {
                                                let status = value
                                                    .get("status")
                                                    .and_then(|field| field.as_str())
                                                    .unwrap_or("unknown");
                                                let api_ver = value
                                                    .get("apiVersion")
                                                    .and_then(|v| v.as_u64())
                                                    .map(|v| format!(" (API v{v})"))
                                                    .unwrap_or_default();
                                                health.set(format!("{status}{api_ver}"));
                                                health_error.set(None);
                                                show_health_error.set(false);
                                            }
                                            Err(error) => {
                                                health.set(t.offline.to_string());
                                                health_error.set(Some(error));
                                            }
                                        }
                                    });
                                },
                                "{t.registry_sync}"
                            }
                        }
                    }
                    if registry_error.is_some() {
                        button {
                            style: "padding: 0; background: none; border: none; font-size: 20px; font-weight: 600; color: {registry_accent}; cursor: pointer; text-align: left; text-decoration: underline; text-decoration-color: rgba(139, 30, 30, 0.35); text-underline-offset: 3px;",
                            onclick: move |_| show_health_error.set(true),
                            "{registry_value}"
                        }
                    } else {
                        p { style: "font-size: 20px; font-weight: 600; color: {registry_accent};",
                            "{registry_value}"
                        }
                    }
                    if let Some(result) = registry_sync_result.read().as_ref() {
                        match result {
                            Ok(true) => rsx! {
                                p { style: "font-size: 11px; color: {Theme::SUCCESS}; margin-top: 6px;", "{t.registry_synced}" }
                            },
                            Ok(false) => rsx! {
                                p { style: "font-size: 11px; color: {Theme::MUTED}; margin-top: 6px;", "{t.registry_synced}" }
                            },
                            Err(e) => rsx! {
                                p { style: "font-size: 11px; color: {Theme::DANGER}; margin-top: 6px;", "{e}" }
                            },
                        }
                    }
                }
                UserCard {
                    label: account_label,
                }
                StatCard {
                    label: skills_label,
                    value: format!("{}", *skill_count.read()),
                    accent: Theme::ACCENT_STRONG,
                }
            }

            div { style: "margin-top: 24px; background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
                div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px;",
                    h2 { style: "font-size: 16px; font-weight: 600; color: {Theme::TEXT};",
                        "{recent_projects_label}"
                    }
                    span { style: "font-size: 12px; padding: 2px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px; font-weight: 600;",
                        "{recent_projects_list.len()}"
                    }
                }

                if recent_projects_list.is_empty() {
                    p { style: "font-size: 13px; color: {Theme::MUTED};",
                        "{no_recent_projects_label}"
                    }
                } else {
                    div { style: "display: flex; flex-direction: column; gap: 10px;",
                        for project in visible_recent_projects.iter() {
                            RecentProjectRow {
                                key: "{project.path}",
                                name: project.name.clone(),
                                path: project.path.clone(),
                                activity_at: project.activity_at,
                                kind: project.kind,
                                added_label: recent_added_label,
                                updated_label: recent_updated_label,
                            }
                        }
                    }
                    PaginationControls {
                        current_page: recent_projects_current_page,
                        total_pages: Some(recent_projects_total_pages),
                        has_prev: recent_projects_current_page > 0,
                        has_next: recent_projects_current_page + 1 < recent_projects_total_pages,
                        on_prev: move |_| recent_projects_page.set(recent_projects_current_page.saturating_sub(1)),
                        on_next: move |_| recent_projects_page.set(recent_projects_current_page + 1),
                    }
                }
            }

            div { style: "margin-top: 24px; background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
                div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 14px;",
                    h2 { style: "font-size: 16px; font-weight: 600; color: {Theme::TEXT};",
                        "{agents_label}"
                    }
                    span { style: "font-size: 12px; padding: 2px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px; font-weight: 600;",
                        "{agents.len()}"
                    }
                }

                if agents.is_empty() {
                    p { style: "font-size: 13px; color: {Theme::MUTED};",
                        "{no_agents_label}"
                    }
                } else {
                    div { style: "display: grid; grid-template-columns: repeat(auto-fit, minmax(220px, 1fr)); gap: 12px;",
                        for agent in visible_agents.iter() {
                            DetectedAgentCard {
                                key: "{agent.kind.as_str()}",
                                name: agent.name.clone(),
                                path: agent.config_dir.display().to_string(),
                            }
                        }
                    }
                    PaginationControls {
                        current_page: agents_current_page,
                        total_pages: Some(agents_total_pages),
                        has_prev: agents_current_page > 0,
                        has_next: agents_current_page + 1 < agents_total_pages,
                        on_prev: move |_| detected_agents_page.set(agents_current_page.saturating_sub(1)),
                        on_next: move |_| detected_agents_page.set(agents_current_page + 1),
                    }
                }
            }

            if let Some(detail) = registry_error_dialog {
                ErrorDialog {
                    title: t.connection_details,
                    detail: detail,
                    close_label: t.close,
                    open: show_health_error,
                }
            }
            } // scrollable content
        }
    }
}

fn load_recent_projects() -> Vec<RecentProject> {
    let Ok(list) = read_projects_list() else {
        return Vec::new();
    };

    let mut projects = list
        .projects
        .into_iter()
        .map(build_recent_project)
        .collect::<Vec<_>>();
    projects.sort_by(|left, right| {
        right
            .activity_at
            .cmp(&left.activity_at)
            .then_with(|| left.name.cmp(&right.name))
    });
    projects
}

fn build_recent_project(entry: ProjectEntry) -> RecentProject {
    let project_path = PathBuf::from(&entry.path);
    let name = project_path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .unwrap_or_else(|| entry.path.clone());
    let updated_at = latest_project_update(&project_path);
    let activity_at = updated_at.max(entry.added_at);
    let kind = if updated_at > entry.added_at && updated_at > 0 {
        RecentProjectKind::Updated
    } else {
        RecentProjectKind::Added
    };

    RecentProject {
        name,
        path: entry.path,
        activity_at,
        kind,
    }
}

fn latest_project_update(project_path: &Path) -> i64 {
    [
        project_path.join("savhub.toml"),
        project_path.join("savhub.lock"),
        project_path.join(".savhub").join("lock.json"),
        project_path.join(".savhub").join("profile.json"),
        project_path.to_path_buf(),
    ]
    .into_iter()
    .filter_map(|path| modified_timestamp(&path))
    .max()
    .unwrap_or(0)
}

fn modified_timestamp(path: &Path) -> Option<i64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs() as i64)
}

fn format_activity_timestamp(timestamp: i64) -> Option<String> {
    if timestamp < 1_000_000_000 {
        return None;
    }

    Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|value| value.format("%Y-%m-%d %H:%M").to_string())
}

fn avatar_initial(value: &str) -> String {
    value
        .chars()
        .find(|ch| ch.is_alphanumeric())
        .unwrap_or('?')
        .to_uppercase()
        .to_string()
}

#[component]
fn UserCard(label: &'static str) -> Element {
    let mut state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    // Read current_user directly from state so the component re-renders on change
    let user = state.current_user.read().clone();
    let mut login_status = use_signal(|| Option::<String>::None);
    let mut logging_in = use_signal(|| false);

    let do_login = move |_| {
        logging_in.set(true);
        login_status.set(None);
        let api_base = state.api_base.read().clone();
        spawn(async move {
            match crate::pages::settings::perform_github_login(&api_base).await {
                Ok(token) => {
                    let base = state.api_base.read().clone();
                    let lang_code = state.lang.read().code();
                    let workdir = state.workdir.read().clone();
                    crate::pages::settings::save_config(&base, &token, lang_code, &workdir, &[]);
                    state.token.set(Some(token));

                    let client = state.api_client();
                    let t = i18n::texts(*state.lang.read());
                    match client.get_json::<WhoAmIResponse>("/whoami").await {
                        Ok(resp) => {
                            if let Some(u) = resp.user {
                                state.current_user.set(Some(u));
                                // No need to set login_status — the card will re-render as logged
                                // in
                            } else {
                                login_status.set(Some(t.login_succeeded_no_user.to_string()));
                            }
                        }
                        Err(e) => login_status.set(Some(t.fmt_login_verify_failed(&e))),
                    }
                }
                Err(e) => {
                    let t = i18n::texts(*state.lang.read());
                    login_status.set(Some(t.fmt_login_failed(&e)));
                }
            }
            logging_in.set(false);
        });
    };

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
            p { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 12px;",
                "{label}"
            }
            if let Some(ref u) = user {
                {
                    let display = format!("@{}", u.handle);
                    rsx! {
                        div { style: "display: flex; align-items: center; gap: 12px;",
                            DashboardAvatar { user: user.clone(), fallback_text: display.clone(), size: 42 }
                            p { style: "font-size: 20px; font-weight: 600; color: {Theme::ACCENT}; min-width: 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                "{display}"
                            }
                        }
                    }
                }
            } else {
                div { style: "display: flex; flex-direction: column; gap: 10px;",
                    if *logging_in.read() {
                        div { style: "display: flex; align-items: center; gap: 8px;",
                            span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                            span { style: "font-size: 13px; color: {Theme::MUTED};",
                                "{t.opening_browser}"
                            }
                        }
                    } else if let Some(msg) = login_status.read().as_ref() {
                        p { style: "font-size: 12px; color: {Theme::DANGER};",
                            "{msg}"
                        }
                        button {
                            style: "padding: 8px 16px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 8px; font-size: 13px; font-weight: 600; cursor: pointer; align-self: flex-start;",
                            onclick: do_login,
                            "{t.login_with_github}"
                        }
                    } else {
                        button {
                            style: "padding: 8px 16px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 8px; font-size: 13px; font-weight: 600; cursor: pointer; align-self: flex-start;",
                            onclick: do_login,
                            "{t.login_with_github}"
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn DashboardAvatar(user: Option<UserSummary>, fallback_text: String, size: u32) -> Element {
    let dimension = format!("{size}px");

    if let Some(user) = user {
        if let Some(url) = user.avatar_url.as_deref().filter(|url| !url.is_empty()) {
            return rsx! {
                img {
                    src: "{url}",
                    alt: "@{user.handle}",
                    style: "width: {dimension}; height: {dimension}; border-radius: 50%; object-fit: cover; border: 1px solid {Theme::LINE}; background: {Theme::ACCENT_LIGHT}; flex-shrink: 0;",
                }
            };
        }

        let initial = avatar_initial(&user.handle);
        return rsx! {
            div { style: "width: {dimension}; height: {dimension}; border-radius: 50%; background: {Theme::ACCENT_LIGHT}; display: flex; align-items: center; justify-content: center; font-size: 18px; color: {Theme::ACCENT_STRONG}; font-weight: 700; flex-shrink: 0;",
                "{initial}"
            }
        };
    }

    let initial = avatar_initial(&fallback_text);
    rsx! {
        div { style: "width: {dimension}; height: {dimension}; border-radius: 50%; background: {Theme::ACCENT_LIGHT}; display: flex; align-items: center; justify-content: center; font-size: 18px; color: {Theme::ACCENT_STRONG}; font-weight: 700; flex-shrink: 0;",
            "{initial}"
        }
    }
}

#[component]
fn RecentProjectRow(
    name: String,
    path: String,
    activity_at: i64,
    kind: RecentProjectKind,
    added_label: &'static str,
    updated_label: &'static str,
) -> Element {
    let badge_label = match kind {
        RecentProjectKind::Added => added_label,
        RecentProjectKind::Updated => updated_label,
    };
    let timestamp = format_activity_timestamp(activity_at);

    rsx! {
        div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 14px; background: {Theme::BG_ELEVATED}; border: 1px solid {Theme::LINE}; border-radius: 8px;",
            div { style: "min-width: 0; flex: 1;",
                p { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 4px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                    "{name}"
                }
                p { style: "font-size: 11px; color: {Theme::MUTED}; font-family: Consolas, 'SFMono-Regular', monospace; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                    "{path}"
                }
            }
            div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 6px; flex-shrink: 0;",
                span { style: "font-size: 11px; padding: 4px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px; font-weight: 600;",
                    "{badge_label}"
                }
                if let Some(timestamp) = timestamp {
                    p { style: "font-size: 11px; color: {Theme::MUTED};",
                        "{timestamp}"
                    }
                }
            }
        }
    }
}

#[component]
fn DetectedAgentCard(name: String, path: String) -> Element {
    rsx! {
        div { style: "background: {Theme::BG_ELEVATED}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 14px;",
            p { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 6px;",
                "{name}"
            }
            p { style: "font-size: 11px; color: {Theme::MUTED}; font-family: Consolas, 'SFMono-Regular', monospace; word-break: break-word;",
                "{path}"
            }
        }
    }
}

#[component]
fn StatCard(label: &'static str, value: String, accent: &'static str) -> Element {
    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
            p { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 8px;",
                "{label}"
            }
            p { style: "font-size: 20px; font-weight: 600; color: {accent};",
                "{value}"
            }
        }
    }
}

#[component]
fn ErrorDialog(
    title: &'static str,
    detail: String,
    close_label: &'static str,
    mut open: Signal<bool>,
) -> Element {
    let mut backdrop_pressed = use_signal(|| false);
    rsx! {
        div {
            style: "position: fixed; inset: 0; background: rgba(26, 46, 24, 0.38); display: flex; align-items: center; justify-content: center; padding: 24px; z-index: 1000;",
            onmousedown: move |_| backdrop_pressed.set(true),
            onmouseup: move |_| { if *backdrop_pressed.read() { open.set(false); } backdrop_pressed.set(false); },
            div {
                style: "width: 100%; max-width: 640px; background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 12px; box-shadow: 0 24px 64px rgba(26, 46, 24, 0.18); padding: 20px;",
                onmousedown: move |evt| evt.stop_propagation(),
                onmouseup: move |evt| evt.stop_propagation(),
                div { style: "display: flex; align-items: center; justify-content: space-between; gap: 16px; margin-bottom: 14px;",
                    h2 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT};",
                        "{title}"
                    }
                    button {
                        style: "width: 32px; height: 32px; display: flex; align-items: center; justify-content: center; background: transparent; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 16px; color: {Theme::MUTED}; cursor: pointer;",
                        onclick: move |_| open.set(false),
                        "\u{00D7}"
                    }
                }
                pre { style: "margin: 0; max-height: 480px; overflow: auto; white-space: pre-wrap; word-break: break-word; padding: 14px; background: {Theme::BG}; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 12px; line-height: 1.6; color: {Theme::TEXT}; font-family: Consolas, 'SFMono-Regular', monospace;",
                    "{detail}"
                }
            }
        }
    }
}
