#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod api;
mod components;
mod i18n;
mod pages;
mod skills;
mod state;
mod theme;
mod updater;
mod watcher;

use api::{ApiCompatibility, CLIENT_API_VERSION};
use dioxus::prelude::*;
use pages::dashboard::DashboardPage;
use pages::detail::DetailPage;
use pages::explore::ExplorePage;
use pages::fetched::FetchedPage;
use pages::profiles::ProjectsPage;
use pages::selectors::SelectorsPage;
use pages::settings::SettingsPage;
use savhub_shared::{UserSummary, WhoAmIResponse};
use state::AppState;
use theme::Theme;

const SAVHUB_WEBSITE_URL: &str = "https://savhub.ai";

/// Build a data URI from the embedded SVG at startup.
fn savhub_logo_data_uri() -> String {
    use std::sync::OnceLock;
    static URI: OnceLock<String> = OnceLock::new();
    URI.get_or_init(|| {
        let svg = include_str!("../assets/savhub.svg");
        let encoded: String = svg
            .chars()
            .map(|c| match c {
                '<' => "%3C".to_string(),
                '>' => "%3E".to_string(),
                '#' => "%23".to_string(),
                '"' => "%22".to_string(),
                '\'' => "%27".to_string(),
                _ => c.to_string(),
            })
            .collect();
        format!("data:image/svg+xml,{encoded}")
    })
    .clone()
}

fn window_icon() -> dioxus::desktop::tao::window::Icon {
    use dioxus::desktop::tao::window::Icon;

    let icon_bytes: &[u8] = if cfg!(target_os = "windows") {
        // Windows title bars downscale this icon aggressively.
        // Use the dedicated app-icon raster instead of the 1024px marketing asset.
        include_bytes!("../assets/savhub_icon_64.png")
    } else {
        include_bytes!("../assets/savhub.png")
    };

    let img = image::load_from_memory(icon_bytes)
        .expect("failed to load icon")
        .to_rgba8();
    let (w, h) = img.dimensions();
    Icon::from_rgba(img.into_raw(), w, h).expect("failed to create icon")
}

fn webview_data_dir() -> std::path::PathBuf {
    let path = savhub_local::config::get_config_dir()
        .map(|dir| dir.join("webview"))
        .unwrap_or_else(|_| std::env::temp_dir().join("savhub").join("webview"));

    if let Err(err) = std::fs::create_dir_all(&path) {
        eprintln!(
            "failed to create desktop webview data directory {}: {err}",
            path.display()
        );
    }

    path
}

#[derive(Debug, Clone, Routable, PartialEq)]
pub enum Route {
    #[layout(Shell)]
    #[route("/")]
    Dashboard {},
    #[route("/explore")]
    Explore {},
    #[route("/detail/:slug")]
    Detail { slug: String },
    #[route("/flock/:slug")]
    FlockDetail { slug: String },
    #[route("/installed")]
    Installed {},
    #[route("/selectors")]
    Selectors {},
    #[route("/projects")]
    Projects {},
    #[route("/settings")]
    Settings {},
}

#[derive(Clone, Copy, PartialEq)]
enum SidebarIconKind {
    Dashboard,
    Explore,
    #[allow(dead_code)]
    Installed,
    Selectors,
    Projects,
    Docs,
    Settings,
}

fn main() {
    use dioxus::desktop::{Config, WindowBuilder};

    // Clean up backup binary from a previous update
    updater::cleanup_old_binary();

    // Read language from config to set the window title
    let lang = state::read_language();
    let t = i18n::texts(lang);
    let title = t.app_window_title;
    let webview_data_dir = webview_data_dir();
    let icon = window_icon();

    dioxus::LaunchBuilder::desktop()
        .with_cfg(
            Config::new()
                // WebView2 defaults to storing its profile next to the executable.
                // That breaks bundled Windows installs under Program Files.
                .with_data_directory(webview_data_dir)
                .with_window(
                    WindowBuilder::new()
                        .with_title(title)
                        .with_inner_size(dioxus::desktop::LogicalSize::new(1240.0, 720.0))
                        .with_window_icon(Some(icon)),
                ),
        )
        .launch(App);
}

#[component]
fn App() -> Element {
    use_context_provider(AppState::init);

    rsx! {
        style { "{theme::global_css()}" }
        Router::<Route> {}
    }
}

// --- Route page wrappers ---

#[component]
fn Dashboard() -> Element {
    rsx! { DashboardPage {} }
}

#[component]
fn Explore() -> Element {
    rsx! { ExplorePage {} }
}

#[component]
fn Detail(slug: String) -> Element {
    rsx! { DetailPage { slug } }
}

#[component]
fn FlockDetail(slug: String) -> Element {
    rsx! { pages::flock_detail::FlockDetailPage { slug } }
}

#[component]
fn Installed() -> Element {
    rsx! { FetchedPage {} }
}

#[component]
fn Selectors() -> Element {
    rsx! { SelectorsPage {} }
}

#[component]
fn Projects() -> Element {
    rsx! { ProjectsPage {} }
}

#[component]
fn Settings() -> Element {
    rsx! { SettingsPage {} }
}

// --- Shell layout with sidebar + update banner ---

#[component]
fn Shell() -> Element {
    let mut state = use_context::<AppState>();
    let mut update_status = use_signal(|| updater::UpdateStatus::Checking);
    let mut user_loaded = use_signal(|| false);

    // Poll for external config changes (selectors.json, config.toml, etc.)
    let config_version = watcher::use_config_watcher();
    use_effect(move || {
        let v = *config_version.read();
        if v > 0 {
            state.config_version.set(v);
        }
    });

    // Check for updates and registry API compatibility on mount
    use_effect(move || {
        spawn(async move {
            match updater::check_for_update().await {
                Ok(Some((version, download_url, asset_name))) => {
                    update_status.set(updater::UpdateStatus::Available {
                        version,
                        download_url,
                        asset_name,
                    });
                }
                Ok(None) => update_status.set(updater::UpdateStatus::UpToDate),
                Err(_) => update_status.set(updater::UpdateStatus::UpToDate),
            }
        });

        let compat_client = state.api_client();
        spawn(async move {
            let result = compat_client.check_api_compatibility().await;
            state.registry_compat.set(result);
        });
    });

    use_effect(move || {
        if *user_loaded.read()
            || state.token.read().is_none()
            || state.current_user.read().is_some()
        {
            return;
        }

        user_loaded.set(true);
        let client = state.api_client();
        spawn(async move {
            match client.get_json::<WhoAmIResponse>("/whoami").await {
                Ok(resp) => {
                    state.current_user.set(resp.user);
                }
                Err(e) => {
                    let msg = format!("{e}");
                    // Token expired or invalid — clear it so UI shows logged-out
                    if msg.starts_with("401") {
                        state.token.set(None);
                        // Remove stale token from config
                        if let Ok(Some(mut cfg)) = savhub_local::config::read_global_config() {
                            cfg.token = None;
                            let _ = savhub_local::config::write_global_config(&cfg);
                        }
                    }
                }
            }
        });
    });

    rsx! {
        div { style: "display: flex; height: 100vh; background: {Theme::BG};",
            Sidebar {}
            div { style: "flex: 1; display: flex; flex-direction: column; overflow: hidden;",
                CompatBanner {}
                UpdateBanner { status: update_status }
                ConfigChangedBanner {}
                div { style: "flex: 1; overflow-y: auto;",
                    Outlet::<Route> {}
                }
            }
        }
    }
}

#[component]
fn UpdateBanner(mut status: Signal<updater::UpdateStatus>) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());

    match &*status.read() {
        updater::UpdateStatus::Available { version, .. } => {
            let msg = t.fmt_update_available(version);
            let download_label = t.update_download;
            let dismiss_label = t.update_dismiss;

            rsx! {
                div { style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 20px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; font-size: 13px;",
                    span { "{msg}" }
                    div { style: "display: flex; gap: 8px;",
                        button {
                            style: "padding: 4px 14px; background: rgba(255,255,255,0.25); color: white; border: 1px solid rgba(255,255,255,0.4); border-radius: 4px; font-size: 12px; font-weight: 500; cursor: pointer;",
                            onclick: move |_| {
                                let vals = status.read().clone();
                                if let updater::UpdateStatus::Available { download_url, asset_name, .. } = vals {
                                    spawn(async move {
                                        status.set(updater::UpdateStatus::Downloading);
                                        match updater::download_and_install(&download_url, &asset_name).await {
                                            Ok(()) => status.set(updater::UpdateStatus::ReadyToRestart),
                                            Err(e) => status.set(updater::UpdateStatus::Failed(e)),
                                        }
                                    });
                                }
                            },
                            "{download_label}"
                        }
                        button {
                            style: "padding: 4px 10px; background: none; color: rgba(255,255,255,0.8); border: none; font-size: 12px; cursor: pointer;",
                            onclick: move |_| status.set(updater::UpdateStatus::UpToDate),
                            "{dismiss_label}"
                        }
                    }
                }
            }
        }
        updater::UpdateStatus::Downloading => {
            let downloading_text = t.update_downloading;
            rsx! {
                div { style: "display: flex; align-items: center; gap: 10px; padding: 8px 20px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; font-size: 13px;",
                    span { style: "display: inline-block; width: 14px; height: 14px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.8s linear infinite;" }
                    span { "{downloading_text}" }
                }
            }
        }
        updater::UpdateStatus::ReadyToRestart => {
            let ready_text = t.update_ready;
            let restart_label = t.update_restart;
            rsx! {
                div { style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 20px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; font-size: 13px;",
                    span { "{ready_text}" }
                    button {
                        style: "padding: 4px 14px; background: white; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; font-size: 12px; font-weight: 600; cursor: pointer;",
                        onclick: move |_| updater::restart(),
                        "{restart_label}"
                    }
                }
            }
        }
        updater::UpdateStatus::Failed(err) => {
            let msg = t.fmt_update_failed(err);
            let dismiss_label = t.update_dismiss;
            rsx! {
                div { style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 20px; background: rgba(139, 30, 30, 0.9); color: white; font-size: 13px;",
                    span { "{msg}" }
                    button {
                        style: "padding: 4px 10px; background: none; color: rgba(255,255,255,0.8); border: none; font-size: 12px; cursor: pointer;",
                        onclick: move |_| status.set(updater::UpdateStatus::UpToDate),
                        "{dismiss_label}"
                    }
                }
            }
        }
        _ => rsx! {},
    }
}

/// Non-dismissible banner shown when the registry API version is incompatible.
#[component]
fn CompatBanner() -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let compat = state.registry_compat.read().clone();

    match compat {
        ApiCompatibility::Incompatible { registry_version } => {
            let detail = t.fmt_compat_detail(CLIENT_API_VERSION, registry_version);
            rsx! {
                div { style: "display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 10px 20px; background: rgba(139, 30, 30, 0.92); color: white; font-size: 13px;",
                    div { style: "display: flex; flex-direction: column; gap: 2px; min-width: 0;",
                        span { style: "font-weight: 700;", "{t.compat_incompatible}" }
                        span { style: "font-size: 11px; opacity: 0.85;", "{detail}" }
                    }
                    Link {
                        to: Route::Settings {},
                        span { style: "padding: 5px 14px; background: white; color: rgba(139, 30, 30, 0.92); border-radius: 6px; font-size: 12px; font-weight: 700; white-space: nowrap; cursor: pointer;",
                            "{t.compat_update_now}"
                        }
                    }
                }
            }
        }
        _ => rsx! {},
    }
}

/// Thin banner shown when external config changes are detected.
#[component]
fn ConfigChangedBanner() -> Element {
    let state = use_context::<AppState>();
    let version = *state.config_version.read();
    let mut dismissed = use_signal(|| 0u64);

    if version == 0 || version <= *dismissed.read() {
        return rsx! {};
    }

    rsx! {
        div { style: "display: flex; align-items: center; justify-content: space-between; padding: 6px 20px; background: linear-gradient(135deg, #3b82f6 0%, #6366f1 100%); color: white; font-size: 13px;",
            span { "Config updated externally. Pages will reflect the latest changes." }
            button {
                style: "padding: 3px 10px; background: rgba(255,255,255,0.2); color: white; border: 1px solid rgba(255,255,255,0.3); border-radius: 4px; font-size: 11px; cursor: pointer;",
                onclick: move |_| dismissed.set(version),
                "Dismiss"
            }
        }
    }
}

#[component]
fn Sidebar() -> Element {
    let route: Route = use_route();
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut collapsed = use_signal(|| false);
    let current_user = state.current_user.read().clone();
    let app_name = t.app_name;
    let is_collapsed = *collapsed.read();
    let sidebar_width = if is_collapsed {
        "76px"
    } else {
        Theme::SIDEBAR_WIDTH
    };
    let header_padding = if is_collapsed {
        "8px 0 20px"
    } else {
        "8px 16px 24px 20px"
    };
    let header_justify = if is_collapsed {
        "center"
    } else {
        "space-between"
    };
    let header_direction = if is_collapsed { "column" } else { "row" };
    let header_gap = if is_collapsed { "8px" } else { "10px" };

    rsx! {
        nav { style: "width: {sidebar_width}; background: rgba(238, 246, 232, 0.92); border-right: 1px solid {Theme::LINE}; display: flex; flex-direction: column; padding: 16px 0; user-select: none; transition: width 0.18s ease;",
            div { style: "display: flex; flex-direction: {header_direction}; align-items: center; justify-content: {header_justify}; gap: {header_gap}; padding: {header_padding};",
                if !is_collapsed {
                    button {
                        style: "display: flex; align-items: center; gap: 10px; min-width: 0; flex: 1; background: transparent; border: none; color: inherit; cursor: pointer; text-align: left;",
                        onclick: move |_| {
                            let _ = crate::pages::settings::open_browser(SAVHUB_WEBSITE_URL);
                        },
                        {
                            let logo_src = savhub_logo_data_uri();
                            rsx! {
                                img { src: "{logo_src}", alt: "{app_name}", style: "width: 28px; height: 28px; flex-shrink: 0;" }
                                span { style: "font-size: 17px; font-weight: 700; color: {Theme::TEXT}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "{app_name}" }
                            }
                        }
                    }
                } else {
                    button {
                        style: "display: flex; align-items: center; justify-content: center; width: 36px; height: 36px; background: transparent; border: none; cursor: pointer; flex-shrink: 0;",
                        onclick: move |_| {
                            let _ = crate::pages::settings::open_browser(SAVHUB_WEBSITE_URL);
                        },
                        {
                            let logo_src = savhub_logo_data_uri();
                            rsx! {
                                img { src: "{logo_src}", alt: "{app_name}", style: "width: 28px; height: 28px; flex-shrink: 0;" }
                            }
                        }
                    }
                }
                button {
                    title: if is_collapsed { "Expand menu" } else { "Collapse menu" },
                    style: "display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; font-size: 14px; cursor: pointer; flex-shrink: 0; line-height: 1;",
                    onclick: move |_| collapsed.set(!is_collapsed),
                    SidebarToggleIcon { collapsed: is_collapsed, size: 18 }
                }
            }

            // Nav items
            NavItem { to: Route::Dashboard {}, label: t.nav_dashboard, icon: SidebarIconKind::Dashboard, current_route: route.clone(), collapsed: is_collapsed }
            NavItem { to: Route::Projects {}, label: t.nav_profiles, icon: SidebarIconKind::Projects, current_route: route.clone(), collapsed: is_collapsed }
            NavItem { to: Route::Explore {}, label: t.nav_explore, icon: SidebarIconKind::Explore, current_route: route.clone(), collapsed: is_collapsed }
            NavItem { to: Route::Selectors {}, label: t.nav_selectors, icon: SidebarIconKind::Selectors, current_route: route.clone(), collapsed: is_collapsed }

            // Docs (external link — styled like NavItem)
            {
                let lang_code = (*state.lang.read()).code();
                let docs_url = format!("https://savhub.ai/{lang_code}/docs/client");
                let docs_label = t.nav_docs;
                let justify = if is_collapsed { "center" } else { "flex-start" };
                let gap = if is_collapsed { "0" } else { "12px" };
                let padding = if is_collapsed { "12px 0" } else { "12px 20px" };
                let icon_size: u32 = if is_collapsed { 24 } else { 22 };
                let icon_width = if is_collapsed { "32px" } else { "32px" };
                rsx! {
                    button {
                        title: "{docs_label}",
                        style: "display: flex; align-items: center; justify-content: {justify}; gap: {gap}; padding: {padding}; min-height: 48px; width: 100%; background: transparent; border: none; color: {Theme::MUTED}; font-weight: 400; font-size: 14px; cursor: pointer; text-align: left;",
                        onclick: move |_| {
                            let _ = crate::pages::settings::open_browser(&docs_url);
                        },
                        span { style: "display: inline-flex; align-items: center; justify-content: center; width: {icon_width}; height: 28px; line-height: 1; text-align: center; flex-shrink: 0;",
                            SidebarIcon { kind: SidebarIconKind::Docs, size: icon_size }
                        }
                        if !is_collapsed {
                            span { "{docs_label}" }
                        }
                    }
                }
            }

            // Spacer
            div { style: "flex: 1;" }

            // User info
            if let Some(user) = current_user.clone() {
                if is_collapsed {
                    div { style: "display: flex; justify-content: center; padding: 8px 0 10px;",
                        SidebarUserAvatar { user: user, size: 32 }
                    }
                } else {
                    div { style: "padding: 8px 20px 10px; margin-bottom: 4px; display: flex; align-items: center; gap: 10px;",
                        SidebarUserAvatar { user: user.clone(), size: 32 }
                        p { style: "font-size: 13px; font-weight: 600; color: {Theme::ACCENT_STRONG}; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;", "@{user.handle}" }
                    }
                }
            }

            NavItem { to: Route::Settings {}, label: t.nav_settings, icon: SidebarIconKind::Settings, current_route: route, collapsed: is_collapsed }
        }
    }
}

// Lucide: panel-left-close / panel-left-open
#[component]
fn SidebarToggleIcon(collapsed: bool, size: u32) -> Element {
    let size_attr = size.to_string();

    if collapsed {
        // lucide: panel-left-open
        rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { width: "18", height: "18", x: "3", y: "3", rx: "2" }
                path { d: "M9 3v18" }
                path { d: "m14 9 3 3-3 3" }
            }
        }
    } else {
        // lucide: panel-left-close
        rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { width: "18", height: "18", x: "3", y: "3", rx: "2" }
                path { d: "M9 3v18" }
                path { d: "m16 15-3-3 3-3" }
            }
        }
    }
}

#[component]
fn NavItem(
    to: Route,
    label: &'static str,
    icon: SidebarIconKind,
    current_route: Route,
    collapsed: bool,
) -> Element {
    let active = std::mem::discriminant(&current_route) == std::mem::discriminant(&to);
    let bg = if active {
        Theme::ACCENT_LIGHT
    } else {
        "transparent"
    };
    let color = if active {
        Theme::ACCENT_STRONG
    } else {
        Theme::MUTED
    };
    let weight = if active { "600" } else { "400" };
    let justify = if collapsed { "center" } else { "flex-start" };
    let gap = if collapsed { "0" } else { "12px" };
    let padding = if collapsed { "12px 0" } else { "12px 20px" };
    let icon_size = if collapsed { 24 } else { 22 };
    let icon_width = if collapsed { "32px" } else { "32px" };

    rsx! {
        Link {
            to,
            div { title: "{label}", style: "display: flex; align-items: center; justify-content: {justify}; gap: {gap}; padding: {padding}; min-height: 48px; background: {bg}; color: {color}; font-weight: {weight}; font-size: 14px; text-decoration: none; transition: background 0.15s;",
                span { style: "display: inline-flex; align-items: center; justify-content: center; width: {icon_width}; height: 28px; line-height: 1; text-align: center; flex-shrink: 0;",
                    SidebarIcon { kind: icon, size: icon_size }
                }
                if !collapsed {
                    span { style: "font-size: 15px;", "{label}" }
                }
            }
        }
    }
}

#[component]
fn SidebarIcon(kind: SidebarIconKind, size: u32) -> Element {
    let size_attr = size.to_string();

    // Icons from Lucide (https://lucide.dev) — MIT license
    let svg_attrs = format!(
        "width=\"{size_attr}\" height=\"{size_attr}\" viewBox=\"0 0 24 24\" fill=\"none\" stroke=\"currentColor\" stroke-width=\"2\" stroke-linecap=\"round\" stroke-linejoin=\"round\""
    );
    let _ = &svg_attrs; // used only in the doc comment above

    match kind {
        // lucide: layout-dashboard
        SidebarIconKind::Dashboard => rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { x: "3", y: "3", width: "7", height: "9", rx: "1" }
                rect { x: "14", y: "3", width: "7", height: "5", rx: "1" }
                rect { x: "14", y: "12", width: "7", height: "9", rx: "1" }
                rect { x: "3", y: "16", width: "7", height: "5", rx: "1" }
            }
        },
        // lucide: compass
        SidebarIconKind::Explore => rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                circle { cx: "12", cy: "12", r: "10" }
                polygon { points: "16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76" }
            }
        },
        // lucide: package
        SidebarIconKind::Installed => rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M16.5 9.4 7.55 4.24" }
                path { d: "M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" }
                polyline { points: "3.29 7 12 12 20.71 7" }
                line { x1: "12", y1: "22", x2: "12", y2: "12" }
            }
        },
        // lucide: scan-search
        SidebarIconKind::Selectors => rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M3 7V5a2 2 0 0 1 2-2h2" }
                path { d: "M17 3h2a2 2 0 0 1 2 2v2" }
                path { d: "M21 17v2a2 2 0 0 1-2 2h-2" }
                path { d: "M7 21H5a2 2 0 0 1-2-2v-2" }
                circle { cx: "12", cy: "12", r: "3" }
                path { d: "m16 16-1.9-1.9" }
            }
        },
        // lucide: folder-open
        SidebarIconKind::Projects => rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" }
            }
        },
        // lucide: book-open
        SidebarIconKind::Docs => rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" }
                path { d: "M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" }
            }
        },
        // lucide: settings
        SidebarIconKind::Settings => rsx! {
            svg { width: "{size_attr}", height: "{size_attr}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" }
                circle { cx: "12", cy: "12", r: "3" }
            }
        },
    }
}

#[component]
fn SidebarUserAvatar(user: UserSummary, size: u32) -> Element {
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
            div { style: "width: {dimension}; height: {dimension}; border-radius: 50%; background: {Theme::ACCENT_LIGHT}; display: flex; align-items: center; justify-content: center; font-size: 14px; color: {Theme::ACCENT_STRONG}; font-weight: 600; flex-shrink: 0;",
                "{initial}"
            }
        }
    }
}
