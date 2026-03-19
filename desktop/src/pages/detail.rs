use dioxus::prelude::*;
use savhub_shared::{SkillDetailResponse, ToggleStarResponse};

use crate::components::pagination::{self, PaginationControls};
use crate::state::AppState;
use crate::theme::Theme;
use crate::{i18n, skills};

const DETAIL_VERSIONS_PAGE_SIZE: usize = 8;
const DETAIL_FILES_PAGE_SIZE: usize = 14;

#[component]
pub fn DetailPage(slug: String) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut detail = use_signal(|| Option::<SkillDetailResponse>::None);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loaded = use_signal(|| false);

    let slug_clone = slug.clone();
    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);
        let client = state.api_client();
        let slug = slug_clone.clone();
        spawn(async move {
            match client
                .get_json::<SkillDetailResponse>(&format!("/skills/{slug}"))
                .await
            {
                Ok(resp) => detail.set(Some(resp)),
                Err(e) => error.set(Some(e)),
            }
        });
    });

    let back_text = t.back_to_explore;
    let loading_text = t.loading;

    rsx! {
        div { style: "padding: 32px;",
            // Back link
            Link {
                to: crate::Route::Explore {},
                span { style: "font-size: 13px; color: {Theme::ACCENT}; cursor: pointer;",
                    "{back_text}"
                }
            }

            if let Some(err) = error.read().as_ref() {
                div { style: "padding: 16px; background: rgba(139,30,30,0.08); border: 1px solid rgba(139,30,30,0.2); border-radius: 8px; color: {Theme::DANGER}; margin-top: 16px;",
                    "{err}"
                }
            }

            if let Some(d) = detail.read().as_ref() {
                DetailContent { detail: d.clone(), slug: slug.clone() }
            } else if error.read().is_none() {
                p { style: "color: {Theme::MUTED}; padding: 40px 0;", "{loading_text}" }
            }
        }
    }
}

#[component]
fn DetailContent(detail: SkillDetailResponse, slug: String) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut starred = use_signal(|| detail.starred);
    let mut star_count = use_signal(|| detail.skill.stats.stars);
    let mut installing = use_signal(|| false);
    let mut install_result = use_signal(|| Option::<Result<(), String>>::None);
    let mut versions_page = use_signal(|| 0usize);
    let mut files_page = use_signal(|| 0usize);

    let version_display = detail
        .latest_version
        .as_ref()
        .map(|v| v.version.as_str())
        .unwrap_or("\u{2014}");
    let owner = &detail.skill.owner;
    let summary = detail.skill.summary.as_deref().unwrap_or("");
    let changelog = detail
        .latest_version
        .as_ref()
        .map(|v| v.changelog.as_str())
        .unwrap_or("");

    let slug_star = slug.clone();
    let toggle_star = move |_: Event<MouseData>| {
        let client = state.api_client();
        let slug = slug_star.clone();
        let is_starred = *starred.read();
        spawn(async move {
            let path = if is_starred {
                format!("/skills/{slug}/unstar")
            } else {
                format!("/skills/{slug}/star")
            };
            if let Ok(resp) = client.post_empty::<ToggleStarResponse>(&path).await {
                starred.set(resp.starred);
                star_count.set(resp.stars);
            }
        });
    };

    let slug_install = slug.clone();
    let do_install = move |_: Event<MouseData>| {
        let client = state.api_client();
        let slug = slug_install.clone();
        let workdir = state.workdir.read().clone();
        spawn(async move {
            installing.set(true);
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
                        match client.get_bytes(&download_path).await {
                            Ok(bytes) => {
                                let skill_dir = workdir.join(&slug);
                                if let Err(e) = skills::extract_zip(&bytes, &skill_dir) {
                                    install_result.set(Some(Err(e)));
                                } else {
                                    skills::update_lockfile(&workdir, &slug, &ver);
                                    install_result.set(Some(Ok(())));
                                    // Fire-and-forget install tracking
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
                            Err(e) => install_result.set(Some(Err(e))),
                        }
                    } else {
                        let t = i18n::texts(*state.lang.read());
                        install_result.set(Some(Err(t.no_version.to_string())));
                    }
                }
                Err(e) => install_result.set(Some(Err(e))),
            }
            installing.set(false);
        });
    };

    let installing_text = t.installing;
    let installed_text = t.installed;
    let install_text = t.install;
    let latest_label = t.latest_version;
    let stats_label = t.statistics;
    let downloads_label = t.downloads;
    let stars_label = t.stars;
    let installs_label = t.installs;
    let unique_users_label = t.unique_users;
    let versions_label = t.versions;
    let comments_label = t.comments;
    let changelog_label = t.changelog;
    let history_label = t.version_history;
    let versions_current_page = pagination::clamp_page(
        *versions_page.read(),
        detail.versions.len(),
        DETAIL_VERSIONS_PAGE_SIZE,
    );
    let visible_versions = pagination::slice_for_page(
        &detail.versions,
        versions_current_page,
        DETAIL_VERSIONS_PAGE_SIZE,
    );
    let versions_total_pages =
        pagination::total_pages(detail.versions.len(), DETAIL_VERSIONS_PAGE_SIZE);

    rsx! {
        div { style: "margin-top: 20px;",
            // Header
            div { style: "display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 24px;",
                div {
                    h1 { style: "font-size: 28px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 4px;",
                        "{detail.skill.display_name}"
                        // TODO: add security badge once savhub-shared is published
                        // with the security_status field on SkillListItem.
                    }
                    p { style: "font-size: 14px; color: {Theme::MUTED}; margin-bottom: 8px; display: flex; align-items: center; gap: 4px;",
                        crate::components::copy_sign::CopySign { value: slug.clone() }
                        " \u{00B7} by {owner.handle}"
                    }
                    if !summary.is_empty() {
                        p { style: "font-size: 15px; color: {Theme::TEXT}; max-width: 600px;",
                            "{summary}"
                        }
                    }
                }
                div { style: "display: flex; gap: 8px; align-items: center;",
                    button {
                        style: "padding: 8px 14px; font-size: 13px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 6px; cursor: pointer; font-weight: 500;",
                        onclick: toggle_star,
                        if *starred.read() { "\u{2605} {star_count}" } else { "\u{2606} {star_count}" }
                    }
                    if *installing.read() {
                        span { style: "font-size: 13px; color: {Theme::ACCENT}; padding: 8px 14px;", "{installing_text}" }
                    } else if let Some(Ok(())) = install_result.read().as_ref() {
                        span { style: "font-size: 13px; color: {Theme::SUCCESS}; padding: 8px 14px;", "{installed_text}" }
                    } else {
                        button {
                            style: "padding: 8px 16px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 13px; font-weight: 500; cursor: pointer;",
                            onclick: do_install,
                            "{install_text}"
                        }
                    }
                }
            }
            if let Some(Err(e)) = install_result.read().as_ref() {
                div { style: "padding: 10px 14px; background: rgba(139,30,30,0.08); border-radius: 6px; margin-bottom: 16px; font-size: 13px; color: {Theme::DANGER};",
                    "{e}"
                }
            }

            // Info cards
            div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 16px; margin-bottom: 24px;",
                // Version info
                div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px;",
                    h3 { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 8px;",
                        "{latest_label}"
                    }
                    p { style: "font-size: 18px; font-weight: 600; color: {Theme::ACCENT_STRONG};",
                        "v{version_display}"
                    }
                    if let Some(v) = &detail.latest_version {
                        {
                            let ts = v.created_at.format("%Y-%m-%d %H:%M").to_string();
                            rsx! {
                                p { style: "font-size: 12px; color: {Theme::MUTED}; margin-top: 4px;",
                                    "{ts}"
                                }
                            }
                        }
                        if !v.tags.is_empty() {
                            div { style: "display: flex; gap: 4px; margin-top: 8px; flex-wrap: wrap;",
                                for tag in v.tags.iter() {
                                    span { style: "font-size: 11px; padding: 1px 6px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 8px;",
                                        "{tag}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Stats
                div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px;",
                    h3 { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 8px;",
                        "{stats_label}"
                    }
                    div { style: "display: grid; grid-template-columns: 1fr 1fr; gap: 8px;",
                        StatItem { label: downloads_label, value: format!("{}", detail.skill.stats.downloads) }
                        StatItem { label: stars_label, value: format!("{}", detail.skill.stats.stars) }
                        StatItem { label: installs_label, value: format!("{}", detail.skill.stats.installs) }
                        StatItem { label: unique_users_label, value: format!("{}", detail.skill.stats.unique_users) }
                        StatItem { label: versions_label, value: format!("{}", detail.skill.stats.versions) }
                        StatItem { label: comments_label, value: format!("{}", detail.skill.stats.comments) }
                    }
                }
            }

            // Changelog
            if !changelog.is_empty() {
                div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; margin-bottom: 16px;",
                    h3 { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 8px;",
                        "{changelog_label}"
                    }
                    p { style: "font-size: 14px; color: {Theme::TEXT}; white-space: pre-wrap;",
                        "{changelog}"
                    }
                }
            }

            // Version history
            if !detail.versions.is_empty() {
                div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; margin-bottom: 16px;",
                    h3 { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 12px;",
                        "{history_label}"
                    }
                    div { style: "display: flex; flex-direction: column; gap: 6px;",
                        for ver in visible_versions.iter() {
                            {
                                let ver_ts = ver.created_at.format("%Y-%m-%d %H:%M").to_string();
                                let ver_version = ver.version.clone();
                                let ver_changelog = ver.changelog.clone();
                                rsx! {
                                    div { style: "display: flex; align-items: center; gap: 12px; padding: 6px 0; border-bottom: 1px solid {Theme::LINE};",
                                        span { style: "font-size: 14px; font-weight: 600; color: {Theme::ACCENT_STRONG}; min-width: 80px;",
                                            "v{ver_version}"
                                        }
                                        span { style: "font-size: 12px; color: {Theme::MUTED}; min-width: 120px;",
                                            "{ver_ts}"
                                        }
                                        span { style: "font-size: 13px; color: {Theme::TEXT}; flex: 1;",
                                            "{ver_changelog}"
                                        }
                                    }
                                }
                            }
                        }
                    }
                    PaginationControls {
                        current_page: versions_current_page,
                        total_pages: Some(versions_total_pages),
                        has_prev: versions_current_page > 0,
                        has_next: versions_current_page + 1 < versions_total_pages,
                        on_prev: move |_| versions_page.set(versions_current_page.saturating_sub(1)),
                        on_next: move |_| versions_page.set(versions_current_page + 1),
                    }
                }
            }

            // Files
            if let Some(v) = &detail.latest_version {
                if !v.files.is_empty() {
                    {
                        let file_count = v.files.len();
                        let files_title = t.fmt_files_count(file_count);
                        let file_entries: Vec<(String, String)> = v.files.iter().map(|f| (f.path.clone(), format_bytes(f.size))).collect();
                        let files_current_page =
                            pagination::clamp_page(*files_page.read(), file_entries.len(), DETAIL_FILES_PAGE_SIZE);
                        let visible_file_entries =
                            pagination::slice_for_page(&file_entries, files_current_page, DETAIL_FILES_PAGE_SIZE);
                        let files_total_pages =
                            pagination::total_pages(file_entries.len(), DETAIL_FILES_PAGE_SIZE);
                        rsx! {
                            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px;",
                                h3 { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 12px;",
                                    "{files_title}"
                                }
                                div { style: "font-family: monospace; font-size: 13px;",
                                    for (path, size) in visible_file_entries.iter() {
                                        div { style: "display: flex; justify-content: space-between; padding: 3px 0; border-bottom: 1px solid {Theme::LINE};",
                                            span { style: "color: {Theme::TEXT};", "{path}" }
                                            span { style: "color: {Theme::MUTED};", "{size}" }
                                        }
                                    }
                                }
                                PaginationControls {
                                    current_page: files_current_page,
                                    total_pages: Some(files_total_pages),
                                    has_prev: files_current_page > 0,
                                    has_next: files_current_page + 1 < files_total_pages,
                                    on_prev: move |_| files_page.set(files_current_page.saturating_sub(1)),
                                    on_next: move |_| files_page.set(files_current_page + 1),
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn StatItem(label: &'static str, value: String) -> Element {
    rsx! {
        div {
            p { style: "font-size: 11px; color: {Theme::MUTED};", "{label}" }
            p { style: "font-size: 16px; font-weight: 600; color: {Theme::TEXT};", "{value}" }
        }
    }
}

fn format_bytes(size: i32) -> String {
    if size < 1024 {
        format!("{size} B")
    } else if size < 1024 * 1024 {
        format!("{:.1} KB", size as f64 / 1024.0)
    } else {
        format!("{:.1} MB", size as f64 / 1024.0 / 1024.0)
    }
}
