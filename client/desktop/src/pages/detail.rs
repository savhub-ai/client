use dioxus::prelude::*;
use savhub_shared::{ScanVerdict, SkillDetailResponse, ToggleStarResponse, VersionScanSummary};

use crate::components::pagination::{self, PaginationControls};
use crate::state::AppState;
use crate::theme::Theme;
use crate::{api, i18n};

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
            let lookup = api::RemoteSkillLookup {
                local_slug: slug.clone(),
                id: Some(slug.clone()),
                slug: Some(slug.clone()),
                ..api::RemoteSkillLookup::default()
            };
            match api::resolve_remote_skill(&client, lookup).await {
                Ok(skill) => {
                    match api::fetch_remote_skill_detail(&client, &skill.id.to_string()).await {
                        Ok(resp) => detail.set(Some(resp)),
                        Err(e) => error.set(Some(e)),
                    }
                }
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
                DetailContent { detail: d.clone() }
            } else if error.read().is_none() {
                p { style: "color: {Theme::MUTED}; padding: 40px 0; text-align: center; width: 100%;", "{loading_text}" }
            }
        }
    }
}

#[component]
fn DetailContent(detail: SkillDetailResponse) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut starred = use_signal(|| detail.starred);
    let mut star_count = use_signal(|| detail.skill.stats.stars);
    let mut fetching = use_signal(|| false);
    let mut fetch_result = use_signal(|| Option::<Result<(), String>>::None);
    let mut versions_page = use_signal(|| 0usize);
    let mut files_page = use_signal(|| 0usize);

    let version_display = detail
        .latest_version
        .as_ref()
        .map(|v| v.version.as_str())
        .unwrap_or("\u{2014}");
    let summary = detail.skill.summary.as_deref().unwrap_or("");
    let changelog = detail
        .latest_version
        .as_ref()
        .map(|v| v.changelog.as_str())
        .unwrap_or("");
    let skill_id = detail.skill.id.to_string();
    let skill_slug = detail.skill.slug.clone();
    let skill_repo_url = detail.skill.repo_url.clone();
    let skill_path = detail.skill.path.clone();
    let star_skill_id = skill_id.clone();
    let fetch_skill_id = skill_id.clone();
    let fetch_skill_slug = skill_slug.clone();
    let fetch_skill_repo_url = skill_repo_url.clone();
    let fetch_skill_path = skill_path.clone();

    let toggle_star = move |_: Event<MouseData>| {
        let client = state.api_client();
        let skill_id = star_skill_id.clone();
        spawn(async move {
            if let Ok(resp) = client
                .post_empty::<ToggleStarResponse>(&format!("/skills/{skill_id}/star"))
                .await
            {
                starred.set(resp.starred);
                star_count.set(resp.stars);
            }
        });
    };

    let do_fetch = move |_: Event<MouseData>| {
        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        let lookup = api::RemoteSkillLookup {
            local_slug: fetch_skill_slug.clone(),
            id: Some(fetch_skill_id.clone()),
            slug: Some(fetch_skill_slug.clone()),
            repo_url: Some(fetch_skill_repo_url.clone()),
            path: Some(fetch_skill_path.clone()),
            flock_slug: None,
        };
        spawn(async move {
            fetching.set(true);
            match api::fetch_remote_skill_with_lookup(&client, &workdir, lookup).await {
                Ok(result) => {
                    fetch_result.set(Some(Ok(())));
                    let track_slug = result.remote_slug;
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
                Err(e) => fetch_result.set(Some(Err(e))),
            }
            fetching.set(false);
        });
    };

    let fetching_text = t.fetching;
    let fetched_text = t.fetched;
    let fetch_text = t.fetch;
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
                    h1 { style: "font-size: 28px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 4px; display: flex; align-items: center; gap: 8px;",
                        "{detail.skill.display_name}"
                        crate::components::security_badge::SecurityBadge { status: detail.skill.security_status }
                    }
                    div { style: "font-size: 14px; color: {Theme::MUTED}; margin-bottom: 8px; display: flex; align-items: center; gap: 4px;",
                        crate::components::copy_sign::CopySign { repo_url: detail.skill.repo_url.clone(), path: detail.skill.path.clone() }
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
                        span { style: "display: inline-flex; align-items: center; gap: 3px;",
                            crate::icons::LucideIcon { icon: crate::icons::Icon::Star, size: 14 }
                            "{star_count}"
                        }
                    }
                    if *fetching.read() {
                        span { style: "font-size: 13px; color: {Theme::ACCENT}; padding: 8px 14px;", "{fetching_text}" }
                    } else if let Some(Ok(())) = fetch_result.read().as_ref() {
                        span { style: "font-size: 13px; color: {Theme::SUCCESS}; padding: 8px 14px;", "{fetched_text}" }
                    } else {
                        button {
                            style: "padding: 8px 16px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 13px; font-weight: 500; cursor: pointer;",
                            onclick: do_fetch,
                            "{fetch_text}"
                        }
                    }
                }
            }
            if let Some(Err(e)) = fetch_result.read().as_ref() {
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

            // Security scan summary
            if let Some(scan) = detail.latest_version.as_ref().and_then(|v| v.scan_summary.as_ref()) {
                SecurityScanPanel { scan: scan.clone() }
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
                    div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 12px;",
                        h3 { style: "font-size: 12px; color: {Theme::MUTED};",
                            "{history_label}"
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
                                div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 12px;",
                                    h3 { style: "font-size: 12px; color: {Theme::MUTED};",
                                        "{files_title}"
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
                                div { style: "font-family: monospace; font-size: 13px;",
                                    for (path, size) in visible_file_entries.iter() {
                                        div { style: "display: flex; justify-content: space-between; padding: 3px 0; border-bottom: 1px solid {Theme::LINE};",
                                            span { style: "color: {Theme::TEXT};", "{path}" }
                                            span { style: "color: {Theme::MUTED};", "{size}" }
                                        }
                                    }
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

fn verdict_color(verdict: ScanVerdict) -> &'static str {
    match verdict {
        ScanVerdict::Benign => Theme::SUCCESS,
        ScanVerdict::Suspicious => "#d4a017",
        ScanVerdict::Malicious => Theme::DANGER,
        ScanVerdict::Pending => Theme::MUTED,
    }
}

fn verdict_label(verdict: ScanVerdict, t: &i18n::Texts) -> &'static str {
    match verdict {
        ScanVerdict::Benign => t.scan_benign,
        ScanVerdict::Suspicious => t.scan_suspicious,
        ScanVerdict::Malicious => t.scan_malicious,
        ScanVerdict::Pending => t.scan_pending,
    }
}

#[component]
fn SecurityScanPanel(scan: VersionScanSummary) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let overall = scan.overall_verdict();
    let overall_color = verdict_color(overall);
    let overall_label = verdict_label(overall, &t);

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; margin-bottom: 16px;",
            h3 { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 12px; display: flex; align-items: center; gap: 8px;",
                "{t.security_scan}"
                span { style: "font-size: 11px; padding: 1px 8px; border-radius: 10px; background: {overall_color}20; color: {overall_color}; font-weight: 600;",
                    "{overall_label}"
                }
            }
            div { style: "display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 12px;",
                // VirusTotal
                div { style: "padding: 10px; border: 1px solid {Theme::LINE}; border-radius: 6px;",
                    p { style: "font-size: 11px; color: {Theme::MUTED}; margin-bottom: 4px;", "{t.scan_virustotal}" }
                    if let Some(vt) = &scan.virustotal {
                        {
                            let color = verdict_color(vt.verdict);
                            let label = verdict_label(vt.verdict, &t);
                            rsx! {
                                p { style: "font-size: 14px; font-weight: 600; color: {color};", "{label}" }
                                if let Some(url) = &vt.report_url {
                                    a { style: "font-size: 11px; color: {Theme::ACCENT}; text-decoration: none;",
                                        href: "{url}",
                                        "View Report"
                                    }
                                }
                            }
                        }
                    } else {
                        p { style: "font-size: 13px; color: {Theme::MUTED};", "\u{2014}" }
                    }
                }

                // LLM Analysis
                div { style: "padding: 10px; border: 1px solid {Theme::LINE}; border-radius: 6px;",
                    p { style: "font-size: 11px; color: {Theme::MUTED}; margin-bottom: 4px;", "{t.scan_llm_analysis}" }
                    if let Some(llm) = &scan.llm_analysis {
                        {
                            let color = verdict_color(llm.verdict);
                            let label = verdict_label(llm.verdict, &t);
                            rsx! {
                                p { style: "font-size: 14px; font-weight: 600; color: {color};", "{label}" }
                                if let Some(conf) = &llm.confidence {
                                    p { style: "font-size: 11px; color: {Theme::MUTED};", "Confidence: {conf}" }
                                }
                                if let Some(summary) = &llm.summary {
                                    p { style: "font-size: 11px; color: {Theme::TEXT}; margin-top: 4px; line-height: 1.4;",
                                        "{summary}"
                                    }
                                }
                            }
                        }
                    } else {
                        p { style: "font-size: 13px; color: {Theme::MUTED};", "\u{2014}" }
                    }
                }

                // Static Scan
                div { style: "padding: 10px; border: 1px solid {Theme::LINE}; border-radius: 6px;",
                    p { style: "font-size: 11px; color: {Theme::MUTED}; margin-bottom: 4px;", "{t.scan_static}" }
                    if let Some(st) = &scan.static_scan {
                        {
                            let color = match st.status.as_str() {
                                "clean" => Theme::SUCCESS,
                                "suspicious" => "#d4a017",
                                "malicious" => Theme::DANGER,
                                _ => Theme::MUTED,
                            };
                            let label = match st.status.as_str() {
                                "clean" => t.scan_benign,
                                "suspicious" => t.scan_suspicious,
                                "malicious" => t.scan_malicious,
                                _ => t.scan_pending,
                            };
                            rsx! {
                                p { style: "font-size: 14px; font-weight: 600; color: {color};", "{label}" }
                                if let Some(engine) = &st.engine_version {
                                    p { style: "font-size: 11px; color: {Theme::MUTED};", "Engine: {engine}" }
                                }
                                if let Some(summary) = &st.summary {
                                    p { style: "font-size: 11px; color: {Theme::TEXT}; margin-top: 4px;",
                                        "{summary}"
                                    }
                                }
                                if !st.findings.is_empty() {
                                    p { style: "font-size: 11px; color: #d4a017; margin-top: 4px;",
                                        "{st.findings.len()} finding(s)"
                                    }
                                }
                            }
                        }
                    } else {
                        p { style: "font-size: 13px; color: {Theme::MUTED};", "\u{2014}" }
                    }
                }
            }
        }
    }
}
