use std::io::{BufRead, BufReader, Write as IoWrite};
use std::net::TcpListener;
use std::time::{Duration, Instant};

use dioxus::prelude::*;
use savhub_shared::{UserSummary, WhoAmIResponse};

use crate::i18n::{self, Language};
use crate::state::AppState;
use crate::theme::Theme;
use crate::updater;

#[derive(Clone, Copy, PartialEq)]
enum SettingsTab {
    General,
    Account,
    About,
}

#[derive(Clone, Copy, PartialEq)]
enum SettingsMenuIcon {
    General,
    Account,
    About,
}

#[component]
pub fn SettingsPage() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let active_tab = use_signal(|| SettingsTab::General);

    let title = t.settings_title;

    rsx! {
        div { style: "padding: 32px; height: 100%; display: flex; flex-direction: column;",
            h1 { style: "font-size: 24px; font-weight: 700; margin-bottom: 20px; color: {Theme::TEXT};",
                "{title}"
            }
            div { style: "display: flex; flex: 1; gap: 24px; min-height: 0;",
                // Left menu
                SettingsMenu { active: active_tab }
                // Right content
                div { style: "flex: 1; overflow-y: auto;",
                    match *active_tab.read() {
                        SettingsTab::General => rsx! { GeneralPane {} },
                        SettingsTab::Account => rsx! { AccountPane {} },
                        SettingsTab::About => rsx! { AboutPane {} },
                    }
                }
            }
        }
    }
}

// --- Left menu ---

#[component]
fn SettingsMenu(mut active: Signal<SettingsTab>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());

    let tabs: [(SettingsTab, SettingsMenuIcon, &str); 3] = [
        (
            SettingsTab::General,
            SettingsMenuIcon::General,
            t.settings_general,
        ),
        (
            SettingsTab::Account,
            SettingsMenuIcon::Account,
            t.settings_account,
        ),
        (
            SettingsTab::About,
            SettingsMenuIcon::About,
            t.settings_about,
        ),
    ];

    rsx! {
        div { style: "width: 176px; display: flex; flex-direction: column; gap: 6px;",
            for (tab, icon, label) in tabs {
                {
                    let is_active = *active.read() == tab;
                    let bg = if is_active { Theme::ACCENT_LIGHT } else { "transparent" };
                    let color = if is_active { Theme::ACCENT_STRONG } else { Theme::MUTED };
                    let weight = if is_active { "600" } else { "400" };
                    rsx! {
                        div {
                            style: "display: flex; align-items: center; gap: 12px; padding: 10px 14px; border-radius: 8px; background: {bg}; color: {color}; font-weight: {weight}; font-size: 14px; cursor: pointer; user-select: none;",
                            onclick: move |_| active.set(tab),
                            span { style: "display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; flex-shrink: 0;",
                                SettingsMenuGlyph { kind: icon, size: 20 }
                            }
                            span { "{label}" }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn SettingsMenuGlyph(kind: SettingsMenuIcon, size: u32) -> Element {
    use crate::icons::{Icon, LucideIcon};
    let icon = match kind {
        SettingsMenuIcon::General => Icon::SlidersHorizontal,
        SettingsMenuIcon::Account => Icon::CircleUser,
        SettingsMenuIcon::About => Icon::Info,
    };
    rsx! { LucideIcon { icon, size } }
}

// --- General pane ---

#[component]
fn GeneralPane() -> Element {
    let mut state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut api_input = use_signal(|| state.api_base.read().clone());
    let mut token_input = use_signal(|| state.token.read().clone().unwrap_or_default());
    let mut agents_mode = use_signal(|| {
        if state.agents.read().is_empty() {
            0u8
        } else {
            1u8
        } // 0=auto, 1=manual
    });
    let mut agents_input = use_signal(|| state.agents.read().join(", "));
    let mut save_status = use_signal(|| Option::<String>::None);

    let save = move |_| {
        let t = i18n::texts(*state.lang.read());
        let base = api_input.read().clone();
        let token = token_input.read().clone();

        state.api_base.set(base.clone());
        if token.trim().is_empty() {
            state.token.set(None);
        } else {
            state.token.set(Some(token.clone()));
        }

        let agents: Vec<String> = if *agents_mode.read() == 0 {
            Vec::new()
        } else {
            agents_input
                .read()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        };
        state.agents.set(agents.clone());

        let lang_code = state.lang.read().code();
        let sec_level = *state.security_level.read();
        let workdir = state.workdir.read().clone();
        save_config(&base, &token, lang_code, &workdir, &agents, sec_level);
        save_status.set(Some(t.settings_saved.to_string()));
    };

    // Language styles
    let current_lang = *state.lang.read();
    let (en_bg, en_color, en_weight) = if current_lang == Language::English {
        (Theme::ACCENT_LIGHT, Theme::ACCENT_STRONG, "600")
    } else {
        ("transparent", Theme::MUTED, "400")
    };
    let (zh_bg, zh_color, zh_weight) = if current_lang == Language::Chinese {
        (Theme::ACCENT_LIGHT, Theme::ACCENT_STRONG, "600")
    } else {
        ("transparent", Theme::MUTED, "400")
    };

    let lang_label = t.language_label;
    let registry_label = t.registry_url;
    let token_label = t.bearer_token;
    let token_hint = t.token_hint;
    let save_label = t.save_settings;

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 20px; max-width: 520px;",
            // Language
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
                h2 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 12px;",
                    "{lang_label}"
                }
                div { style: "display: flex; gap: 8px;",
                    button {
                        style: "padding: 6px 16px; font-size: 13px; background: {en_bg}; color: {en_color}; font-weight: {en_weight}; border: 1px solid {Theme::LINE}; border-radius: 6px; cursor: pointer;",
                        onclick: move |_| {
                            state.lang.set(Language::English);
                            let base = state.api_base.read().clone();
                            let token = state.token.read().clone().unwrap_or_default();
                            let workdir = state.workdir.read().clone();
                            save_config(&base, &token, "en", &workdir, &[], *state.security_level.read());
                        },
                        "English"
                    }
                    button {
                        style: "padding: 6px 16px; font-size: 13px; background: {zh_bg}; color: {zh_color}; font-weight: {zh_weight}; border: 1px solid {Theme::LINE}; border-radius: 6px; cursor: pointer;",
                        onclick: move |_| {
                            state.lang.set(Language::Chinese);
                            let base = state.api_base.read().clone();
                            let token = state.token.read().clone().unwrap_or_default();
                            let workdir = state.workdir.read().clone();
                            save_config(&base, &token, "zh", &workdir, &[], *state.security_level.read());
                        },
                        "\u{4e2d}\u{6587}"
                    }
                }
            }

            // Registry URL
            div {
                label { style: "display: block; font-size: 13px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 6px;",
                    "{registry_label}"
                }
                input {
                    style: "width: 100%; padding: 10px 14px; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 14px; background: {Theme::PANEL}; color: {Theme::TEXT}; outline: none;",
                    value: "{api_input}",
                    oninput: move |e| api_input.set(e.value()),
                }
            }

            // Token
            div {
                label { style: "display: block; font-size: 13px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 6px;",
                    "{token_label}"
                }
                input {
                    style: "width: 100%; padding: 10px 14px; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 14px; background: {Theme::PANEL}; color: {Theme::TEXT}; outline: none; font-family: monospace;",
                    r#type: "password",
                    value: "{token_input}",
                    oninput: move |e| token_input.set(e.value()),
                }
                p { style: "font-size: 11px; color: {Theme::MUTED}; margin-top: 4px;",
                    "{token_hint}"
                }
            }

            // AI Agents
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
                h2 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 4px;",
                    "{t.settings_agents}"
                }
                p { style: "font-size: 11px; color: {Theme::MUTED}; margin-bottom: 12px;",
                    "{t.settings_agents_hint}"
                }
                div { style: "display: flex; gap: 8px; margin-bottom: 10px;",
                    {
                        let auto_mode = *agents_mode.read();
                        let (auto_bg, auto_color, auto_weight) = if auto_mode == 0 {
                            (Theme::ACCENT_LIGHT, Theme::ACCENT_STRONG, "600")
                        } else {
                            ("transparent", Theme::MUTED, "400")
                        };
                        let (manual_bg, manual_color, manual_weight) = if auto_mode == 1 {
                            (Theme::ACCENT_LIGHT, Theme::ACCENT_STRONG, "600")
                        } else {
                            ("transparent", Theme::MUTED, "400")
                        };
                        rsx! {
                            button {
                                style: "padding: 6px 16px; font-size: 13px; background: {auto_bg}; color: {auto_color}; font-weight: {auto_weight}; border: 1px solid {Theme::LINE}; border-radius: 6px; cursor: pointer;",
                                onclick: move |_| agents_mode.set(0),
                                "{t.settings_agents_auto}"
                            }
                            button {
                                style: "padding: 6px 16px; font-size: 13px; background: {manual_bg}; color: {manual_color}; font-weight: {manual_weight}; border: 1px solid {Theme::LINE}; border-radius: 6px; cursor: pointer;",
                                onclick: move |_| agents_mode.set(1),
                                "{t.settings_agents_manual}"
                            }
                        }
                    }
                }
                if *agents_mode.read() == 1 {
                    input {
                        style: "width: 100%; padding: 10px 14px; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 13px; background: {Theme::BG_ELEVATED}; color: {Theme::TEXT}; outline: none; font-family: monospace;",
                        placeholder: "claude-code, codex, cursor, windsurf",
                        value: "{agents_input}",
                        oninput: move |e| agents_input.set(e.value()),
                    }
                } else {
                    {
                        let clients = savhub_local::clients::detect_clients();
                        let installed: Vec<String> = clients.iter().filter(|c| c.installed).map(|c| c.name.clone()).collect();
                        let label = if installed.is_empty() { "\u{2014}".to_string() } else { installed.join(", ") };
                        rsx! {
                            p { style: "font-size: 13px; color: {Theme::TEXT}; padding: 6px 0;",
                                "{label}"
                            }
                        }
                    }
                }
            }

            // Security Level
            div {
                label { style: "display: block; font-size: 13px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 6px;",
                    "Security Level"
                }
                p { style: "font-size: 12px; color: {Theme::MUTED}; margin-bottom: 8px;",
                    "Minimum security level required when fetching skills and flocks."
                }
                {
                    use savhub_local::config::SecurityLevel;
                    let current_sec = *state.security_level.read();
                    let levels = [
                        (SecurityLevel::Verified, "Verified Only", "Only fetch skills that passed security scans (recommended)"),
                        (SecurityLevel::Suspicious, "Allow Suspicious", "Also allow skills with suspicious patterns detected"),
                        (SecurityLevel::Any, "Allow All", "Fetch any skill regardless of security status"),
                    ];
                    rsx! {
                        div { style: "display: flex; flex-direction: column; gap: 6px;",
                            for (level, label, desc) in levels {
                                {
                                    let is_active = current_sec == level;
                                    let (bg, color, weight) = if is_active {
                                        (Theme::ACCENT_LIGHT, Theme::ACCENT_STRONG, "600")
                                    } else {
                                        ("transparent", Theme::MUTED, "400")
                                    };
                                    rsx! {
                                        button {
                                            style: "display: flex; flex-direction: column; align-items: flex-start; gap: 2px; padding: 8px 14px; background: {bg}; color: {color}; font-weight: {weight}; border: 1px solid {Theme::LINE}; border-radius: 6px; cursor: pointer; text-align: left; font-size: 13px;",
                                            onclick: move |_| {
                                                state.security_level.set(level);
                                            },
                                            span { "{label}" }
                                            span { style: "font-size: 11px; font-weight: 400; opacity: 0.7;", "{desc}" }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            // Buttons
            div { style: "display: flex; gap: 8px; flex-wrap: wrap;",
                button {
                    style: "padding: 10px 20px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 14px; font-weight: 500; cursor: pointer;",
                    onclick: save,
                    "{save_label}"
                }
            }

            if let Some(msg) = save_status.read().as_ref() {
                div { style: "padding: 10px 14px; background: {Theme::ACCENT_LIGHT}; border-radius: 6px; font-size: 13px; color: {Theme::ACCENT_STRONG};",
                    "{msg}"
                }
            }
        }
    }
}

// --- Account pane ---

#[component]
fn AccountPane() -> Element {
    let mut state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut token_input = use_signal(|| state.token.read().clone().unwrap_or_default());
    let mut login_status = use_signal(|| Option::<String>::None);
    let mut logging_in = use_signal(|| false);

    use_effect(move || {
        if state.token.read().is_none() || state.current_user.read().is_some() {
            return;
        }

        let client = state.api_client();
        spawn(async move {
            if let Ok(resp) = client.get_json::<WhoAmIResponse>("/whoami").await {
                state.current_user.set(resp.user);
            }
        });
    });

    let do_login = move |_| {
        let t = i18n::texts(*state.lang.read());
        logging_in.set(true);
        login_status.set(Some(t.opening_browser.to_string()));
        let api_base = state.api_base.read().clone();
        spawn(async move {
            match perform_github_login(&api_base).await {
                Ok(token) => {
                    let base = state.api_base.read().clone();
                    let lang_code = state.lang.read().code();
                    let workdir = state.workdir.read().clone();
                    save_config(
                        &base,
                        &token,
                        lang_code,
                        &workdir,
                        &[],
                        *state.security_level.read(),
                    );
                    state.token.set(Some(token.clone()));
                    token_input.set(token);

                    let client = state.api_client();
                    let t = i18n::texts(*state.lang.read());
                    match client.get_json::<WhoAmIResponse>("/whoami").await {
                        Ok(resp) => {
                            if let Some(u) = resp.user {
                                state.current_user.set(Some(u.clone()));
                                login_status.set(Some(t.fmt_logged_in_via_github(&u.handle)));
                            } else {
                                login_status.set(Some(t.login_succeeded_no_user.to_string()));
                            }
                        }
                        Err(e) => {
                            login_status.set(Some(t.fmt_login_verify_failed(&e.to_string())));
                        }
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

    let do_logout = move |_| {
        let t = i18n::texts(*state.lang.read());
        state.token.set(None);
        state.current_user.set(None);
        token_input.set(String::new());
        let base = state.api_base.read().clone();
        let lang_code = state.lang.read().code();
        let workdir = state.workdir.read().clone();
        save_config(
            &base,
            "",
            lang_code,
            &workdir,
            &[],
            *state.security_level.read(),
        );
        login_status.set(Some(t.logged_out.to_string()));
    };

    let is_logged_in = state.token.read().is_some();
    let auth_label = t.authentication;
    let logout_label = t.logout;
    let not_logged_hint = t.not_logged_in_hint;
    let logging_in_text = t.logging_in;
    let login_github_text = t.login_with_github;
    let auth_token_text = t.authenticated_token_set;

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 20px; max-width: 520px;",
            // Authentication
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
                h2 { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 12px;",
                    "{auth_label}"
                }
                if is_logged_in {
                    div { style: "display: flex; flex-direction: column; gap: 12px;",
                        div {
                            if let Some(user) = state.current_user.read().as_ref() {
                                div { style: "display: flex; align-items: center; gap: 12px;",
                                    UserAvatar { user: user.clone(), size: 48 }
                                    div {
                                        p { style: "font-size: 16px; font-weight: 600; color: {Theme::TEXT};", "@{user.handle}" }
                                    }
                                }
                            } else {
                                p { style: "font-size: 14px; color: {Theme::TEXT};",
                                    "{auth_token_text}"
                                }
                            }
                        }
                        button {
                            style: "align-self: flex-start; padding: 8px 16px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 6px; font-size: 13px; cursor: pointer;",
                            onclick: do_logout,
                            "{logout_label}"
                        }
                    }
                } else {
                    div { style: "display: flex; flex-direction: column; gap: 12px;",
                        p { style: "font-size: 14px; color: {Theme::MUTED};",
                            "{not_logged_hint}"
                        }
                        button {
                            style: "align-self: flex-start; padding: 10px 20px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 14px; font-weight: 500; cursor: pointer;",
                            disabled: *logging_in.read(),
                            onclick: do_login,
                            if *logging_in.read() { "{logging_in_text}" } else { "{login_github_text}" }
                        }
                    }
                }
            }

            if let Some(msg) = login_status.read().as_ref() {
                div { style: "padding: 10px 14px; background: {Theme::ACCENT_LIGHT}; border-radius: 6px; font-size: 13px; color: {Theme::ACCENT_STRONG};",
                    "{msg}"
                }
            }
        }
    }
}

#[component]
fn UserAvatar(user: UserSummary, size: u32) -> Element {
    let dimension = format!("{size}px");

    if let Some(url) = user.avatar_url.as_deref().filter(|url| !url.is_empty()) {
        rsx! {
            img {
                src: "{url}",
                alt: "@{user.handle}",
                style: "width: {dimension}; height: {dimension}; border-radius: 50%; object-fit: cover; border: 1px solid {Theme::LINE}; background: {Theme::ACCENT_LIGHT}; flex-shrink: 0;",
            }
        }
    } else {
        let initial = user
            .handle
            .chars()
            .next()
            .unwrap_or('?')
            .to_uppercase()
            .to_string();
        rsx! {
            div { style: "width: {dimension}; height: {dimension}; border-radius: 50%; background: {Theme::ACCENT_LIGHT}; display: flex; align-items: center; justify-content: center; font-size: 20px; color: {Theme::ACCENT_STRONG}; font-weight: 600; flex-shrink: 0;",
                "{initial}"
            }
        }
    }
}

// --- About pane ---

#[component]
fn AboutPane() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut update_status = use_signal(|| Option::<updater::UpdateStatus>::None);

    let current_ver = savhub_local::build_info::version_string();
    let app_name = t.app_name;
    let ver_label = t.about_version;
    let check_label = t.about_check_update;
    let checking_text = t.about_checking;
    let up_to_date_text = t.about_up_to_date;
    let copyright = t.about_copyright;
    let license_label = t.about_license;
    let github_label = t.about_github;
    let open_label = t.about_open;

    let check_update = move |_| {
        update_status.set(Some(updater::UpdateStatus::Checking));
        spawn(async move {
            match updater::check_for_update().await {
                Ok(Some((version, download_url, asset_name))) => {
                    update_status.set(Some(updater::UpdateStatus::Available {
                        version,
                        download_url,
                        asset_name,
                    }));
                }
                Ok(None) => update_status.set(Some(updater::UpdateStatus::UpToDate)),
                Err(e) => update_status.set(Some(updater::UpdateStatus::Failed(e))),
            }
        });
    };

    rsx! {
        div { style: "display: flex; flex-direction: column; gap: 20px; max-width: 520px;",
            // App info
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 24px; text-align: center;",
                {
                    let logo_src = crate::savhub_logo_data_uri();
                    rsx! {
                        img { src: "{logo_src}", alt: "{app_name}", style: "width: 56px; height: 56px; margin-bottom: 12px;" }
                    }
                }
                h2 { style: "font-size: 22px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 4px;",
                    "{app_name}"
                }
                p { style: "font-size: 14px; color: {Theme::MUTED};",
                    "{ver_label} {current_ver}"
                }
            }

            // Check for updates
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px;",
                div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px;",
                    h3 { style: "font-size: 14px; font-weight: 600; color: {Theme::TEXT};",
                        "{check_label}"
                    }
                    {
                        let is_checking = matches!(&*update_status.read(), Some(updater::UpdateStatus::Checking));
                        let is_downloading = matches!(&*update_status.read(), Some(updater::UpdateStatus::Downloading));
                        let disabled = is_checking || is_downloading;
                        rsx! {
                            button {
                                style: "padding: 6px 14px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer;",
                                disabled: disabled,
                                onclick: check_update,
                                if is_checking { "{checking_text}" } else { "{check_label}" }
                            }
                        }
                    }
                }

                // Update status
                match &*update_status.read() {
                    Some(updater::UpdateStatus::UpToDate) => rsx! {
                        p { style: "display: flex; align-items: center; gap: 4px; font-size: 13px; color: {Theme::SUCCESS};",
                            crate::icons::LucideIcon { icon: crate::icons::Icon::Check, size: 14 }
                            "{up_to_date_text}"
                        }
                    },
                    Some(updater::UpdateStatus::Available { version, .. }) => {
                        let msg = t.fmt_update_available(version);
                        let download_label = t.update_download;
                        rsx! {
                            div { style: "display: flex; align-items: center; justify-content: space-between;",
                                p { style: "font-size: 13px; color: {Theme::ACCENT_STRONG};", "{msg}" }
                                button {
                                    style: "padding: 6px 14px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer;",
                                    onclick: move |_| {
                                        let vals = update_status.read().clone();
                                        if let Some(updater::UpdateStatus::Available { download_url, asset_name, .. }) = vals {
                                            spawn(async move {
                                                update_status.set(Some(updater::UpdateStatus::Downloading));
                                                match updater::download_and_install(&download_url, &asset_name).await {
                                                    Ok(()) => update_status.set(Some(updater::UpdateStatus::ReadyToRestart)),
                                                    Err(e) => update_status.set(Some(updater::UpdateStatus::Failed(e))),
                                                }
                                            });
                                        }
                                    },
                                    "{download_label}"
                                }
                            }
                        }
                    },
                    Some(updater::UpdateStatus::Downloading) => {
                        let downloading_text = t.update_downloading;
                        rsx! {
                            div { style: "display: flex; align-items: center; gap: 8px;",
                                span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid {Theme::LINE}; border-top-color: {Theme::ACCENT}; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                                p { style: "font-size: 13px; color: {Theme::ACCENT};", "{downloading_text}" }
                            }
                        }
                    },
                    Some(updater::UpdateStatus::ReadyToRestart) => {
                        let ready_text = t.update_ready;
                        let restart_label = t.update_restart;
                        rsx! {
                            div { style: "display: flex; align-items: center; justify-content: space-between;",
                                p { style: "font-size: 13px; color: {Theme::SUCCESS};", "{ready_text}" }
                                button {
                                    style: "padding: 6px 14px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: pointer;",
                                    onclick: move |_| updater::restart(),
                                    "{restart_label}"
                                }
                            }
                        }
                    },
                    Some(updater::UpdateStatus::Failed(err)) => {
                        let msg = t.fmt_update_failed(err);
                        rsx! {
                            p { style: "font-size: 13px; color: {Theme::DANGER};", "{msg}" }
                        }
                    },
                    _ => rsx! {},
                }
            }

            // Info rows
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; overflow: hidden;",
                // License
                div { style: "display: flex; align-items: center; justify-content: space-between; padding: 14px 20px; border-bottom: 1px solid {Theme::LINE};",
                    span { style: "font-size: 13px; color: {Theme::TEXT}; font-weight: 500;", "{license_label}" }
                    span { style: "font-size: 13px; color: {Theme::MUTED};", "Apache-2.0" }
                }
                // GitHub
                div { style: "display: flex; align-items: center; justify-content: space-between; padding: 14px 20px;",
                    span { style: "font-size: 13px; color: {Theme::TEXT}; font-weight: 500;", "{github_label}" }
                    button {
                        style: "padding: 4px 12px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; font-size: 12px; font-weight: 500; cursor: pointer;",
                        onclick: move |_| { let _ = open_browser("https://github.com/savhub-ai/client"); },
                        "{open_label}"
                    }
                }
            }

            // Copyright
            p { style: "font-size: 12px; color: {Theme::MUTED}; text-align: center; padding: 8px 0;",
                "{copyright}"
            }
        }
    }
}

// --- Helpers ---

pub fn save_config(
    base: &str,
    token: &str,
    lang: &str,
    workdir: &std::path::Path,
    agents: &[String],
    security_level: savhub_local::config::SecurityLevel,
) {
    let default_workdir = savhub_local::clients::home_dir().join(".savhub");

    let config = savhub_local::config::GlobalConfig {
        api_base: Some(base.to_string()),
        token: if token.trim().is_empty() {
            None
        } else {
            Some(token.to_string())
        },
        language: if lang == "en" {
            None
        } else {
            Some(lang.to_string())
        },
        workdir: {
            let w = workdir.display().to_string();
            if workdir == default_workdir {
                None
            } else {
                Some(w)
            }
        },
        agents: agents.to_vec(),
        security_level,
    };
    let _ = savhub_local::config::write_global_config(&config);
    crate::watcher::mark_self_written();
}

/// Perform GitHub OAuth login by opening a browser and waiting for the callback.
pub async fn perform_github_login(api_base: &str) -> Result<String, String> {
    let listener =
        TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Failed to bind port: {e}"))?;
    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to get port: {e}"))?
        .port();
    let return_to = format!("http://127.0.0.1:{port}/callback");

    let client = crate::api::ApiClient::new(api_base, None::<String>);
    let mut url = client
        .v1_url("/auth/github/start")
        .map_err(|e| format!("Failed to build login URL: {e}"))?;
    url.query_pairs_mut().append_pair("return_to", &return_to);
    let login_url = url.to_string();

    open_browser(&login_url).map_err(|e| format!("Failed to open browser: {e}"))?;

    listener
        .set_nonblocking(true)
        .map_err(|e| format!("Failed to configure listener: {e}"))?;
    let deadline = Instant::now() + Duration::from_secs(240);

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut reader = BufReader::new(
                    stream
                        .try_clone()
                        .map_err(|e| format!("Stream error: {e}"))?,
                );
                let mut request_line = String::new();
                reader
                    .read_line(&mut request_line)
                    .map_err(|e| format!("Read error: {e}"))?;

                let path = request_line.split_whitespace().nth(1).unwrap_or("/");
                if !path.starts_with("/callback") {
                    write_response(
                        &mut stream,
                        "Savhub login: unexpected path. You can close this window.",
                    );
                    continue;
                }

                let url = reqwest::Url::parse(&format!("http://127.0.0.1{path}"))
                    .map_err(|e| format!("URL parse error: {e}"))?;
                let mut auth_token = None;
                let mut auth_error = None;
                for (key, value) in url.query_pairs() {
                    match key.as_ref() {
                        "auth_token" => auth_token = Some(value.into_owned()),
                        "auth_error" => auth_error = Some(value.into_owned()),
                        _ => {}
                    }
                }

                if let Some(error) = auth_error {
                    write_response(&mut stream, "Login failed. Return to the app for details.");
                    return Err(format!("GitHub login failed: {error}"));
                }

                if let Some(token) = auth_token {
                    write_response(&mut stream, "Login complete! You can close this window.");
                    return Ok(token);
                }

                write_response(&mut stream, "Waiting for authentication...");
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("Timed out waiting for GitHub login.".to_string());
                }
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            Err(e) => return Err(format!("Listener error: {e}")),
        }
    }
}

fn write_response(stream: &mut std::net::TcpStream, message: &str) {
    let body = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Savhub Login</title>\
         <style>body{{font-family:'Segoe UI',sans-serif;padding:40px;background:#ecf2e8;color:#1a2e18;text-align:center}}\
         h1{{color:#2d6b1e}}</style></head><body><h1>Savhub</h1><p>{message}</p></body></html>"
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

pub(crate) fn open_browser(url: &str) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::process::Command::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err("Browser launch not supported on this platform".to_string())
}
