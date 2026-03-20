use std::collections::BTreeMap;

use dioxus::prelude::*;
use savhub_shared::{FlockDetailResponse, ImportedSkillRecord, SecurityStatus};

use crate::state::AppState;
use crate::theme::Theme;
use crate::{api, i18n};

#[component]
pub fn FlockDetailPage(slug: String) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let nav = use_navigator();
    let mut detail = use_signal(|| Option::<FlockDetailResponse>::None);
    let mut error = use_signal(|| Option::<String>::None);
    let mut loaded = use_signal(|| false);
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);
    let mut fetched: Signal<BTreeMap<String, String>> = use_signal(BTreeMap::new);

    let flock_id = slug.clone();
    use_effect(move || {
        if *loaded.read() {
            return;
        }
        loaded.set(true);
        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        let flock_id = flock_id.clone();
        spawn(async move {
            let fetched_map = tokio::task::spawn_blocking(move || {
                crate::skills::read_fetched_skill_versions(&workdir)
            })
            .await
            .unwrap_or_default();
            fetched.set(fetched_map);

            match api::fetch_remote_flock_detail(&client, &flock_id).await {
                Ok(resp) => detail.set(Some(resp)),
                Err(err) => error.set(Some(err)),
            }
        });
    });

    let Some(payload) = detail.read().as_ref().cloned() else {
        if let Some(err) = error.read().as_ref() {
            return rsx! {
                div { style: "padding: 32px; text-align: center; color: {Theme::MUTED};",
                    p { style: "color: {Theme::DANGER};", "{err}" }
                    button {
                        style: "margin-top: 16px; padding: 8px 20px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 8px; cursor: pointer; font-weight: 600;",
                        onclick: move |_| { nav.go_back(); },
                        "{t.flock_back}"
                    }
                }
            };
        }

        return rsx! {
            div { style: "padding: 32px; color: {Theme::MUTED};",
                "{t.loading}"
            }
        };
    };

    let skill_slugs: Vec<String> = payload
        .skills
        .iter()
        .map(|skill| skill.slug.clone())
        .collect();
    let all_fetched = !skill_slugs.is_empty() && {
        let fetched_map = fetched.read();
        skill_slugs
            .iter()
            .all(|skill_slug| fetched_map.contains_key(skill_slug))
    };

    let all_skills = payload.skills.clone();
    let do_all = move |_: MouseEvent| {
        let skills = all_skills.clone();
        let should_prune = all_fetched;
        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        spawn(async move {
            working.set(true);
            action_error.set(None);
            for skill in &skills {
                let result = if should_prune {
                    let slug = skill.slug.clone();
                    let workdir = workdir.clone();
                    tokio::task::spawn_blocking(move || {
                        crate::skills::prune_skill(&workdir, &slug)
                    })
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
                } else {
                    api::fetch_remote_skill(&client, &workdir, &skill.slug)
                        .await
                        .map(|_| ())
                };

                match result {
                    Ok(()) => {
                        fetched.with_mut(|map| {
                            if should_prune {
                                map.remove(skill.slug.as_str());
                            } else {
                                map.insert(skill.slug.clone(), "fetched".to_string());
                            }
                        });
                        if !should_prune {
                            let track_slug = skill.slug.clone();
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
                    Err(err) => {
                        action_error.set(Some(err));
                        break;
                    }
                }
            }
            working.set(false);
        });
    };

    let version_display = payload.flock.version.as_deref().unwrap_or("\u{2014}");
    let slug_display = format!("{}/{}", payload.flock.repo_sign, payload.flock.slug);
    let skills = payload.skills.clone();

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            div { style: "flex-shrink: 0; padding: 16px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10;",
                div { style: "display: flex; align-items: center; gap: 12px; margin-bottom: 8px;",
                    button {
                        style: "padding: 6px 14px; background: {Theme::PANEL}; color: {Theme::MUTED}; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 13px; cursor: pointer; font-weight: 600;",
                        onclick: move |_| { nav.go_back(); },
                        "\u{2190} {t.flock_back}"
                    }
                    h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT};",
                        "{payload.flock.name}"
                    }
                    SecurityBadge { status: payload.flock.security_status }
                    span { style: "font-size: 12px; padding: 2px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px;",
                        "v{version_display}"
                    }
                }
                div { style: "display: flex; align-items: center; gap: 12px; flex-wrap: wrap;",
                    crate::components::copy_sign::CopySign { value: slug_display.clone() }
                    span { style: "font-size: 12px; color: {Theme::MUTED};",
                        "{payload.flock.skill_count} {t.flock_skills_count}"
                    }
                    if !payload.flock.description.is_empty() {
                        p { style: "font-size: 13px; color: {Theme::MUTED}; margin-left: 8px;",
                            "{payload.flock.description}"
                        }
                    }
                }
                div { style: "margin-top: 10px; display: flex; align-items: center; gap: 10px;",
                    if *working.read() {
                        span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: {Theme::ACCENT}; font-weight: 600;",
                            span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                            "{t.fetching}"
                        }
                    } else if all_fetched {
                        button {
                            style: "padding: 7px 18px; font-size: 13px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 8px; cursor: pointer; font-weight: 700;",
                            onclick: do_all,
                            "{t.flock_prune_all}"
                        }
                    } else {
                        button {
                            style: "padding: 7px 18px; font-size: 13px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 8px; cursor: pointer; font-weight: 700;",
                            onclick: do_all,
                            "{t.flock_fetch_all}"
                        }
                    }
                    if let Some(err) = action_error.read().as_ref() {
                        p { style: "font-size: 12px; color: {Theme::DANGER};", "{err}" }
                    }
                }
            }

            div { style: "flex: 1; overflow-y: auto; padding: 16px 32px 32px;",
                div { style: "display: flex; flex-direction: column; gap: 8px; max-width: 900px;",
                    for skill in skills.iter() {
                        FlockSkillRow {
                            skill: skill.clone(),
                            fetched: fetched,
                        }
                    }
                }
                if skills.is_empty() {
                    p { style: "color: {Theme::MUTED}; text-align: center; padding: 40px 0;",
                        "{t.no_skills_found}"
                    }
                }
            }
        }
    }
}

#[component]
fn FlockSkillRow(
    skill: ImportedSkillRecord,
    mut fetched: Signal<BTreeMap<String, String>>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut working = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| Option::<String>::None);
    let skill_slug = skill.slug.clone();
    let is_fetched = fetched.read().contains_key(&skill_slug);

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let should_prune = is_fetched;
        let skill_slug = skill_slug.clone();
        let client = state.api_client();
        let workdir = state.workdir.read().clone();
        spawn(async move {
            working.set(true);
            error_msg.set(None);
            let result = if should_prune {
                let workdir = workdir.clone();
                let slug = skill_slug.clone();
                tokio::task::spawn_blocking(move || crate::skills::prune_skill(&workdir, &slug))
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|r| r)
            } else {
                api::fetch_remote_skill(&client, &workdir, &skill_slug)
                    .await
                    .map(|_| ())
            };
            match result {
                Ok(()) => {
                    fetched.with_mut(|map| {
                        if should_prune {
                            map.remove(&skill_slug);
                        } else {
                            map.insert(skill_slug.clone(), "fetched".to_string());
                        }
                    });
                    if !should_prune {
                        let track_slug = skill_slug.clone();
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
                Err(err) => error_msg.set(Some(err)),
            }
            working.set(false);
        });
    };

    let version_display = skill.version.as_deref().unwrap_or("\u{2014}");
    let desc = skill.description.as_deref().unwrap_or("");
    let nav = use_navigator();
    let slug_nav = skill.slug.clone();

    rsx! {
        div {
            style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 10px; padding: 12px 16px; display: flex; align-items: center; justify-content: space-between; gap: 12px; cursor: pointer;",
            onclick: move |_| { nav.push(crate::Route::Detail { slug: slug_nav.clone() }); },
            div { style: "min-width: 0; flex: 1;",
                div { style: "display: flex; align-items: center; gap: 8px; margin-bottom: 2px;",
                    h3 { style: "font-size: 14px; font-weight: 600; color: {Theme::TEXT};",
                        "{skill.name}"
                    }
                    span { style: "font-size: 11px; padding: 1px 7px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px;",
                        "v{version_display}"
                    }
                }
                crate::components::copy_sign::CopySign { value: skill.slug.clone() }
                if !desc.is_empty() {
                    p { style: "font-size: 13px; color: {Theme::MUTED}; margin-top: 4px;",
                        "{desc}"
                    }
                }
            }
            div { style: "display: flex; flex-direction: column; align-items: flex-end; gap: 8px;",
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
                if let Some(err) = error_msg.read().as_ref() {
                    p { style: "font-size: 11px; color: {Theme::DANGER}; max-width: 220px; text-align: right;",
                        "{err}"
                    }
                }
            }
        }
    }
}

#[component]
fn SecurityBadge(status: SecurityStatus) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let (value_bg, value_label) = match status {
        SecurityStatus::Verified => ("#2e8b57", t.security_verified),
        SecurityStatus::Scanning => ("#1e82d2", t.security_scanning),
        SecurityStatus::Flagged => ("#b8860b", t.security_flagged),
        SecurityStatus::Rejected => ("#9f2b2b", t.security_rejected),
        SecurityStatus::Unverified => ("#999", t.security_unverified),
    };

    rsx! {
        span { style: "display: inline-flex; align-items: center; font-size: 11px; line-height: 1; border-radius: 3px; overflow: hidden; vertical-align: middle; white-space: nowrap; position: relative; top: -1px;",
            span { style: "padding: 3px 6px; background: #555; color: #fff;", "security" }
            span { style: "padding: 3px 6px; background: {value_bg}; color: #fff; font-weight: 600;",
                "{value_label}"
            }
        }
    }
}
