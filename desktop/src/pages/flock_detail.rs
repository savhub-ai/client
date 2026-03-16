use std::collections::BTreeMap;

use dioxus::prelude::*;
use savhub_local::registry::{self, RegistryFlock, RegistrySkill};

use crate::i18n;
use crate::state::AppState;
use crate::theme::Theme;

#[component]
pub fn FlockDetailPage(slug: String) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let nav = use_navigator();
    let mut working = use_signal(|| false);
    let mut action_error = use_signal(|| Option::<String>::None);

    // Load flock metadata
    let flock: Option<RegistryFlock> = registry::get_flock_by_slug(&slug).ok().flatten();
    // Load skills in this flock
    let skills: Vec<RegistrySkill> = registry::list_skills_in_flock(&slug)
        .ok()
        .and_then(|list| if list.is_empty() { None } else { Some(list) })
        .unwrap_or_default();

    // Installed state
    let mut installed: Signal<BTreeMap<String, bool>> = use_signal(|| {
        let entries = registry::read_installed_skills_file().unwrap_or_default();
        entries.into_iter().map(|e| (e.slug, true)).collect()
    });

    let all_installed = !skills.is_empty() && {
        let map = installed.read();
        skills.iter().all(|s| map.contains_key(&s.slug))
    };

    let skill_signs: Vec<String> = skills.iter().map(|s| s.signs.clone()).collect();
    let do_all = move |_: MouseEvent| {
        let signs = skill_signs.clone();
        let uninstall = all_installed;
        spawn(async move {
            working.set(true);
            action_error.set(None);
            for sign in &signs {
                let sign = sign.clone();
                let result = tokio::task::spawn_blocking(move || {
                    if uninstall {
                        registry::uninstall_skill_from_registry(&sign).map(|_| ())
                    } else {
                        registry::install_skill_from_registry(&sign).map(|_| ())
                    }
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()));
                if let Err(e) = result {
                    action_error.set(Some(e.to_string()));
                    break;
                }
                installed.with_mut(|map| {
                    if uninstall {
                        map.remove(slug);
                    } else {
                        map.insert(slug.clone(), true);
                    }
                });
                if !uninstall {
                    let track_slug = slug.clone();
                    let track_client = state.api_client();
                    tokio::spawn(async move {
                        let _ = track_client
                            .post_json::<serde_json::Value, serde_json::Value>(
                                &format!("/skills/{track_slug}/install"),
                                &serde_json::json!({ "client_type": "desktop" }),
                            )
                            .await;
                    });
                }
            }
            working.set(false);
        });
    };

    let Some(flock) = flock else {
        return rsx! {
            div { style: "padding: 32px; text-align: center; color: {Theme::MUTED};",
                p { "Flock \"{slug}\" not found." }
                button {
                    style: "margin-top: 16px; padding: 8px 20px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 8px; cursor: pointer; font-weight: 600;",
                    onclick: move |_| { nav.go_back(); },
                    "{t.flock_back}"
                }
            }
        };
    };

    let version_display = flock.version.as_deref().unwrap_or("\u{2014}");
    let slug_display = if flock.repo.is_empty() {
        flock.slug.clone()
    } else {
        format!("{}/{}", flock.repo, flock.slug)
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; height: 100%;",
            // Header
            div { style: "flex-shrink: 0; padding: 16px 32px; background: {Theme::BG}; border-bottom: 1px solid {Theme::LINE}; z-index: 10;",
                div { style: "display: flex; align-items: center; gap: 12px; margin-bottom: 8px;",
                    button {
                        style: "padding: 6px 14px; background: {Theme::PANEL}; color: {Theme::MUTED}; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 13px; cursor: pointer; font-weight: 600;",
                        onclick: move |_| { nav.go_back(); },
                        "\u{2190} {t.flock_back}"
                    }
                    h1 { style: "font-size: 18px; font-weight: 700; color: {Theme::TEXT};",
                        "{flock.name}"
                    }
                    span { style: "font-size: 12px; padding: 2px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border-radius: 999px;",
                        "v{version_display}"
                    }
                }
                div { style: "display: flex; align-items: center; gap: 12px;",
                    p { style: "font-size: 12px; color: {Theme::MUTED};", "{slug_display}" }
                    span { style: "font-size: 12px; color: {Theme::MUTED};",
                        "{skills.len()} {t.flock_skills_count}"
                    }
                    if !flock.description.is_empty() {
                        p { style: "font-size: 13px; color: {Theme::MUTED}; margin-left: 8px;", "{flock.description}" }
                    }
                }
                // Install/Uninstall all button
                div { style: "margin-top: 10px; display: flex; align-items: center; gap: 10px;",
                    if *working.read() {
                        span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 13px; color: {Theme::ACCENT}; font-weight: 600;",
                            span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                            "{t.installing}"
                        }
                    } else if all_installed {
                        button {
                            style: "padding: 7px 18px; font-size: 13px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 8px; cursor: pointer; font-weight: 700;",
                            onclick: do_all,
                            "{t.flock_uninstall_all}"
                        }
                    } else {
                        button {
                            style: "padding: 7px 18px; font-size: 13px; background: {Theme::ACCENT}; color: white; border: none; border-radius: 8px; cursor: pointer; font-weight: 700;",
                            onclick: do_all,
                            "{t.flock_install_all}"
                        }
                    }
                    if let Some(err) = action_error.read().as_ref() {
                        p { style: "font-size: 12px; color: {Theme::DANGER};", "{err}" }
                    }
                }
            }

            // Skills list
            div { style: "flex: 1; overflow-y: auto; padding: 16px 32px 32px;",
                div { style: "display: flex; flex-direction: column; gap: 8px; max-width: 900px;",
                    for skill in skills.iter() {
                        FlockSkillRow {
                            skill: skill.clone(),
                            installed: installed,
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
fn FlockSkillRow(skill: RegistrySkill, mut installed: Signal<BTreeMap<String, bool>>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut working = use_signal(|| false);
    let mut error_msg: Signal<Option<String>> = use_signal(|| Option::<String>::None);
    let sign = skill.sign.clone();
    let is_installed = installed.read().contains_key(&sign);

    let do_action = move |e: Event<MouseData>| {
        e.stop_propagation();
        let sign = sign.clone();
        let uninstall = is_installed;
        spawn(async move {
            working.set(true);
            error_msg.set(None);
            let result = {
                let s = sign.clone();
                tokio::task::spawn_blocking(move || {
                    if uninstall {
                        registry::uninstall_skill_from_registry(&s).map(|_| ())
                    } else {
                        registry::install_skill_from_registry(&sign).map(|_| ())
                    }
                })
                .await
                .map_err(|e| e.to_string())
                .and_then(|r| r.map_err(|e| e.to_string()))
            };
            match result {
                Ok(()) => {
                    installed.with_mut(|map| {
                        if uninstall {
                            map.remove(&slug);
                        } else {
                            map.insert(slug.clone(), true);
                        }
                    });
                    if !uninstall {
                        let track_slug = slug.clone();
                        let track_client = state.api_client();
                        tokio::spawn(async move {
                            let _ = track_client
                                .post_json::<serde_json::Value, serde_json::Value>(
                                    &format!("/skills/{track_slug}/install"),
                                    &serde_json::json!({ "client_type": "desktop" }),
                                )
                                .await;
                        });
                    }
                }
                Err(e) => error_msg.set(Some(e.to_string())),
            }
            working.set(false);
        });
    };

    let version_display = skill.version.as_deref().unwrap_or("\u{2014}");
    let desc = skill.description.as_deref().unwrap_or("");

    let nav = use_navigator();
    let slug_nav = slug.clone();

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
                p { style: "font-size: 12px; color: {Theme::MUTED};", "{skill.slug}" }
                if !desc.is_empty() {
                    p { style: "font-size: 12px; color: {Theme::MUTED}; margin-top: 2px; display: -webkit-box; -webkit-line-clamp: 1; -webkit-box-orient: vertical; overflow: hidden;",
                        "{desc}"
                    }
                }
            }
            div { style: "flex-shrink: 0;",
                if *working.read() {
                    span { style: "display: inline-flex; align-items: center; gap: 6px; font-size: 12px; color: {Theme::ACCENT}; font-weight: 600;",
                        span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(90, 158, 63, 0.3); border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                        "{t.installing}"
                    }
                } else if is_installed {
                    button {
                        style: "padding: 5px 12px; font-size: 12px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 999px; cursor: pointer; font-weight: 600;",
                        onclick: do_action,
                        "{t.uninstall}"
                    }
                } else {
                    button {
                        style: "padding: 5px 12px; font-size: 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 999px; cursor: pointer; font-weight: 600;",
                        onclick: do_action,
                        "{t.install}"
                    }
                }
            }
            if let Some(err) = error_msg.read().as_ref() {
                p { style: "font-size: 11px; color: {Theme::DANGER};", "{err}" }
            }
        }
    }
}
