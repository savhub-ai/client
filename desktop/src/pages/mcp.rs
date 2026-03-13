use dioxus::prelude::*;

use savhub_local::clients::ClientKind;
use savhub_local::mcp_config;

use crate::components::pagination::{self, PaginationControls};
use crate::i18n;
use crate::state::AppState;
use crate::theme::Theme;

const MCP_CLIENTS_PAGE_SIZE: usize = 6;
const MCP_LOG_PAGE_SIZE: usize = 12;

#[component]
pub fn McpPage() -> Element {
    let mut state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let mut status_log: Signal<Vec<String>> = use_signal(Vec::new);
    let mut reg_version = use_signal(|| 0u32);
    let mut transitioning = use_signal(|| false);
    let mut clients_page = use_signal(|| 0usize);
    let mut log_page = use_signal(|| 0usize);

    let is_running = *state.mcp_running.read();
    let _ = *reg_version.read();

    let registration_status = mcp_config::get_all_registration_status();
    let auto_start = is_auto_start_enabled();
    let current_clients_page = pagination::clamp_page(
        *clients_page.read(),
        registration_status.len(),
        MCP_CLIENTS_PAGE_SIZE,
    );
    let visible_clients = pagination::slice_for_page(
        &registration_status,
        current_clients_page,
        MCP_CLIENTS_PAGE_SIZE,
    );
    let clients_total_pages =
        pagination::total_pages(registration_status.len(), MCP_CLIENTS_PAGE_SIZE);
    let log_lines = status_log.read().clone();
    let current_log_page =
        pagination::clamp_page(*log_page.read(), log_lines.len(), MCP_LOG_PAGE_SIZE);
    let visible_log_lines =
        pagination::slice_for_page(&log_lines, current_log_page, MCP_LOG_PAGE_SIZE);
    let log_total_pages = pagination::total_pages(log_lines.len(), MCP_LOG_PAGE_SIZE);

    let title = t.mcp_title;
    let server_status_label = t.mcp_server_status;
    let running_text = t.mcp_running;
    let stopped_text = t.mcp_stopped;
    let start_label = t.mcp_start;
    let stop_label = t.mcp_stop;
    let starting_text = t.mcp_starting;
    let stopping_text = t.mcp_stopping;
    let reg_label = t.mcp_registration;
    let register_all_label = t.mcp_register_all;
    let unregister_all_label = t.mcp_unregister_all;
    let auto_start_label = t.mcp_auto_start;
    let auto_start_hint = t.mcp_auto_start_hint;
    let auto_enabled = t.mcp_auto_start_enabled;
    let auto_disabled = t.mcp_auto_start_disabled;
    let clear_label = t.clear;
    let is_transitioning = *transitioning.read();

    // Start MCP server
    let do_start = move |_| {
        transitioning.set(true);
        match start_mcp_server() {
            Ok(child) => {
                {
                    let handle = state.mcp_process.read().clone();
                    let mut guard = handle.lock().unwrap();
                    *guard = Some(child);
                }
                state.mcp_running.set(true);

                // Auto-register to all supported clients
                let statuses = mcp_config::get_all_registration_status();
                for s in &statuses {
                    if s.installed && s.supports_prompts {
                        let client = savhub_local::clients::detect_clients()
                            .into_iter()
                            .find(|c| c.kind == s.kind);
                        if let Some(ref c) = client {
                            match mcp_config::register_mcp(c) {
                                Ok(()) => status_log.with_mut(|log| {
                                    log.push(format!("Registered to {}", s.client_name));
                                }),
                                Err(e) => status_log.with_mut(|log| {
                                    log.push(format!("{}: {}", s.client_name, e));
                                }),
                            }
                        }
                    }
                }
                status_log.with_mut(|log| log.push("MCP server started.".to_string()));
                reg_version.with_mut(|v| *v += 1);
            }
            Err(e) => {
                status_log.with_mut(|log| {
                    log.push(format!("Failed to start: {e}"));
                });
            }
        }
        transitioning.set(false);
    };

    // Stop MCP server
    let do_stop = move |_| {
        transitioning.set(true);

        // Auto-unregister from all clients
        let clients = savhub_local::clients::detect_clients();
        for client in &clients {
            if mcp_config::is_registered(client) {
                match mcp_config::unregister_mcp(client) {
                    Ok(()) => status_log.with_mut(|log| {
                        log.push(format!("Unregistered from {}", client.name));
                    }),
                    Err(e) => status_log.with_mut(|log| {
                        log.push(format!("{}: {}", client.name, e));
                    }),
                }
            }
        }

        // Kill the process
        {
            let handle = state.mcp_process.read().clone();
            let mut guard = handle.lock().unwrap();
            if let Some(ref mut child) = *guard {
                let _ = child.kill();
                let _ = child.wait();
            }
            *guard = None;
        }
        state.mcp_running.set(false);
        status_log.with_mut(|log| log.push("MCP server stopped.".to_string()));
        reg_version.with_mut(|v| *v += 1);
        transitioning.set(false);
    };

    // Register all
    let do_register_all = move |_| {
        let clients = savhub_local::clients::detect_clients();
        for client in &clients {
            if client.kind.supports_mcp_prompts() && client.installed {
                match mcp_config::register_mcp(client) {
                    Ok(()) => status_log.with_mut(|log| {
                        log.push(format!("Registered to {}", client.name));
                    }),
                    Err(e) => status_log.with_mut(|log| {
                        log.push(format!("{}: {}", client.name, e));
                    }),
                }
            }
        }
        reg_version.with_mut(|v| *v += 1);
    };

    // Unregister all
    let do_unregister_all = move |_| {
        let clients = savhub_local::clients::detect_clients();
        for client in &clients {
            if mcp_config::is_registered(client) {
                match mcp_config::unregister_mcp(client) {
                    Ok(()) => status_log.with_mut(|log| {
                        log.push(format!("Unregistered from {}", client.name));
                    }),
                    Err(e) => status_log.with_mut(|log| {
                        log.push(format!("{}: {}", client.name, e));
                    }),
                }
            }
        }
        reg_version.with_mut(|v| *v += 1);
    };

    // Auto-start toggle
    let toggle_auto_start = move |_| {
        if auto_start {
            disable_auto_start();
        } else {
            enable_auto_start();
        }
        reg_version.with_mut(|v| *v += 1);
    };

    let (auto_bg, auto_color) = if auto_start {
        (Theme::ACCENT_LIGHT, Theme::ACCENT_STRONG)
    } else {
        ("rgba(0,0,0,0.04)", Theme::MUTED)
    };
    let auto_text = if auto_start {
        auto_enabled
    } else {
        auto_disabled
    };

    rsx! {
        div { style: "padding: 32px;",
            h1 { style: "font-size: 24px; font-weight: 700; color: {Theme::TEXT}; margin-bottom: 24px;",
                "{title}"
            }

            // Server status card
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 20px; margin-bottom: 20px;",
                h3 { style: "font-size: 13px; color: {Theme::MUTED}; margin-bottom: 12px;",
                    "{server_status_label}"
                }
                div { style: "display: flex; align-items: center; justify-content: space-between;",
                    div { style: "display: flex; align-items: center; gap: 10px;",
                        {
                            let (dot_color, status_text) = if is_running {
                                (Theme::SUCCESS, running_text)
                            } else {
                                (Theme::MUTED, stopped_text)
                            };
                            rsx! {
                                span { style: "font-size: 18px; color: {dot_color};", "\u{25CF}" }
                                span { style: "font-size: 16px; font-weight: 600; color: {Theme::TEXT};", "{status_text}" }
                            }
                        }
                    }
                    {
                        if is_running {
                            rsx! {
                                button {
                                    style: "padding: 8px 20px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 6px; font-size: 13px; font-weight: 500; cursor: pointer;",
                                    disabled: is_transitioning,
                                    onclick: do_stop,
                                    if is_transitioning { "{stopping_text}" } else { "{stop_label}" }
                                }
                            }
                        } else {
                            rsx! {
                                button {
                                    style: "padding: 8px 20px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 13px; font-weight: 500; cursor: pointer;",
                                    disabled: is_transitioning,
                                    onclick: do_start,
                                    if is_transitioning { "{starting_text}" } else { "{start_label}" }
                                }
                            }
                        }
                    }
                }
            }

            // Auto-start on boot
            div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px; margin-bottom: 20px;",
                div { style: "display: flex; align-items: center; justify-content: space-between;",
                    div {
                        h3 { style: "font-size: 14px; font-weight: 600; color: {Theme::TEXT}; margin-bottom: 2px;",
                            "{auto_start_label}"
                        }
                        p { style: "font-size: 12px; color: {Theme::MUTED};", "{auto_start_hint}" }
                    }
                    button {
                        style: "padding: 6px 16px; background: {auto_bg}; color: {auto_color}; border: 1px solid {Theme::LINE}; border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer;",
                        onclick: toggle_auto_start,
                        "{auto_text}"
                    }
                }
            }

            // Registration status
            div { style: "margin-bottom: 20px;",
                div { style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px;",
                    h2 { style: "font-size: 16px; font-weight: 600; color: {Theme::TEXT};",
                        "{reg_label}"
                    }
                    div { style: "display: flex; gap: 8px;",
                        button {
                            style: "padding: 6px 14px; background: linear-gradient(135deg, {Theme::ACCENT} 0%, #7bc25a 100%); color: white; border: none; border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer;",
                            onclick: do_register_all,
                            "{register_all_label}"
                        }
                        button {
                            style: "padding: 6px 14px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 6px; font-size: 12px; font-weight: 500; cursor: pointer;",
                            onclick: do_unregister_all,
                            "{unregister_all_label}"
                        }
                    }
                }
                div { style: "display: flex; flex-direction: column; gap: 8px;",
                    for status in visible_clients.iter() {
                        McpClientRow {
                            client_name: status.client_name.clone(),
                            kind: status.kind,
                            installed: status.installed,
                            supports_prompts: status.supports_prompts,
                            registered: status.registered,
                            config_path: status.config_path.as_ref().map(|p| p.display().to_string()),
                            version: reg_version,
                        }
                    }
                }
                PaginationControls {
                    current_page: current_clients_page,
                    total_pages: Some(clients_total_pages),
                    has_prev: current_clients_page > 0,
                    has_next: current_clients_page + 1 < clients_total_pages,
                    on_prev: move |_| clients_page.set(current_clients_page.saturating_sub(1)),
                    on_next: move |_| clients_page.set(current_clients_page + 1),
                }
            }

            // Status log
            if !log_lines.is_empty() {
                div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 16px;",
                    div { style: "display: flex; justify-content: space-between; align-items: center; margin-bottom: 8px;",
                        h3 { style: "font-size: 12px; color: {Theme::MUTED};",
                            "Log"
                        }
                        button {
                            style: "font-size: 11px; color: {Theme::MUTED}; background: none; border: none; cursor: pointer;",
                            onclick: move |_| status_log.set(Vec::new()),
                            "{clear_label}"
                        }
                    }
                    div { style: "font-family: monospace; font-size: 12px; max-height: 200px; overflow-y: auto;",
                        for (i, line) in visible_log_lines.iter().enumerate() {
                            p { key: "{current_log_page}-{i}", style: "padding: 2px 0; color: {Theme::TEXT};", "{line}" }
                        }
                    }
                    PaginationControls {
                        current_page: current_log_page,
                        total_pages: Some(log_total_pages),
                        has_prev: current_log_page > 0,
                        has_next: current_log_page + 1 < log_total_pages,
                        on_prev: move |_| log_page.set(current_log_page.saturating_sub(1)),
                        on_next: move |_| log_page.set(current_log_page + 1),
                    }
                }
            }
        }
    }
}

#[component]
fn McpClientRow(
    client_name: String,
    kind: ClientKind,
    installed: bool,
    supports_prompts: bool,
    registered: bool,
    config_path: Option<String>,
    mut version: Signal<u32>,
) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());

    let (status_text, status_color, badge_bg) = if !installed {
        (t.not_found, Theme::MUTED, "rgba(0,0,0,0.04)")
    } else if !supports_prompts {
        (t.mcp_not_supported, Theme::MUTED, "rgba(0,0,0,0.04)")
    } else if registered {
        (t.mcp_registered, Theme::SUCCESS, Theme::ACCENT_LIGHT)
    } else {
        (t.mcp_not_registered, Theme::MUTED, "rgba(0,0,0,0.04)")
    };

    let can_toggle = installed && supports_prompts;
    let path_display = config_path.unwrap_or_default();

    rsx! {
        div { style: "background: {Theme::PANEL}; border: 1px solid {Theme::LINE}; border-radius: 8px; padding: 14px 16px; display: flex; align-items: center; justify-content: space-between;",
            div { style: "display: flex; align-items: center; gap: 12px; flex: 1;",
                {
                    let indicator_color = if registered { Theme::SUCCESS } else { Theme::MUTED };
                    let indicator = if registered { "\u{25CF}" } else { "\u{25CB}" };
                    rsx! {
                        span { style: "font-size: 14px; color: {indicator_color};", "{indicator}" }
                    }
                }
                div {
                    p { style: "font-size: 15px; font-weight: 600; color: {Theme::TEXT};", "{client_name}" }
                    if !path_display.is_empty() {
                        p { style: "font-size: 11px; color: {Theme::MUTED}; font-family: monospace;", "{path_display}" }
                    }
                }
            }
            div { style: "display: flex; align-items: center; gap: 8px;",
                span { style: "font-size: 12px; padding: 2px 10px; background: {badge_bg}; color: {status_color}; border-radius: 10px; font-weight: 500;",
                    "{status_text}"
                }
                if can_toggle {
                    {
                        let kind_for_click = kind;
                        if registered {
                            rsx! {
                                button {
                                    style: "padding: 4px 10px; background: rgba(139, 30, 30, 0.08); color: {Theme::DANGER}; border: 1px solid rgba(139, 30, 30, 0.2); border-radius: 4px; font-size: 11px; cursor: pointer;",
                                    onclick: move |_| {
                                        let clients = savhub_local::clients::detect_clients();
                                        if let Some(c) = clients.iter().find(|c| c.kind == kind_for_click) {
                                            let _ = mcp_config::unregister_mcp(c);
                                        }
                                        version.with_mut(|v| *v += 1);
                                    },
                                    "Unregister"
                                }
                            }
                        } else {
                            rsx! {
                                button {
                                    style: "padding: 4px 10px; background: {Theme::ACCENT_LIGHT}; color: {Theme::ACCENT_STRONG}; border: none; border-radius: 4px; font-size: 11px; cursor: pointer;",
                                    onclick: move |_| {
                                        let clients = savhub_local::clients::detect_clients();
                                        if let Some(c) = clients.iter().find(|c| c.kind == kind_for_click) {
                                            let _ = mcp_config::register_mcp(c);
                                        }
                                        version.with_mut(|v| *v += 1);
                                    },
                                    "Register"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// MCP server process management
// ---------------------------------------------------------------------------

/// Start the savhub-mcp binary as a child process (detached, stdio piped).
fn start_mcp_server() -> Result<std::process::Child, String> {
    let mcp_bin = mcp_config::mcp_binary_path().map_err(|e| e.to_string())?;
    if !mcp_bin.exists() {
        return Err(format!("MCP binary not found at {}", mcp_bin.display()));
    }

    std::process::Command::new(&mcp_bin)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|e| format!("Failed to spawn MCP server: {e}"))
}

// ---------------------------------------------------------------------------
// Auto-start on boot (OS-level)
// ---------------------------------------------------------------------------

/// Check if auto-start is enabled for this app.
fn is_auto_start_enabled() -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let output = Command::new("reg")
            .args([
                "query",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "SavhubDesktop",
            ])
            .output();
        matches!(output, Ok(o) if o.status.success())
    }
    #[cfg(target_os = "macos")]
    {
        let plist = dirs_home().join("Library/LaunchAgents/com.savhub.desktop.plist");
        plist.exists()
    }
    #[cfg(target_os = "linux")]
    {
        let desktop_file = dirs_home().join(".config/autostart/savhub-desktop.desktop");
        desktop_file.exists()
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        false
    }
}

/// Enable auto-start on boot.
fn enable_auto_start() {
    let exe = std::env::current_exe().unwrap_or_default();
    let exe_str = exe.to_string_lossy();

    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let _ = Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "SavhubDesktop",
                "/t",
                "REG_SZ",
                "/d",
                &exe_str,
                "/f",
            ])
            .output();
    }
    #[cfg(target_os = "macos")]
    {
        let plist_dir = dirs_home().join("Library/LaunchAgents");
        let _ = std::fs::create_dir_all(&plist_dir);
        let plist = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.savhub.desktop</string>
    <key>ProgramArguments</key>
    <array>
        <string>{exe_str}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
</dict>
</plist>
"#
        );
        let _ = std::fs::write(plist_dir.join("com.savhub.desktop.plist"), plist);
    }
    #[cfg(target_os = "linux")]
    {
        let autostart_dir = dirs_home().join(".config/autostart");
        let _ = std::fs::create_dir_all(&autostart_dir);
        let entry = format!(
            "[Desktop Entry]\nType=Application\nName=Savhub Desktop\nExec={exe_str}\nX-GNOME-Autostart-enabled=true\n"
        );
        let _ = std::fs::write(autostart_dir.join("savhub-desktop.desktop"), entry);
    }
}

/// Disable auto-start on boot.
fn disable_auto_start() {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        let _ = Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "SavhubDesktop",
                "/f",
            ])
            .output();
    }
    #[cfg(target_os = "macos")]
    {
        let plist = dirs_home().join("Library/LaunchAgents/com.savhub.desktop.plist");
        let _ = std::fs::remove_file(plist);
    }
    #[cfg(target_os = "linux")]
    {
        let desktop_file = dirs_home().join(".config/autostart/savhub-desktop.desktop");
        let _ = std::fs::remove_file(desktop_file);
    }
}

#[cfg(any(target_os = "macos", target_os = "linux"))]
fn dirs_home() -> std::path::PathBuf {
    directories::UserDirs::new()
        .map(|d| d.home_dir().to_path_buf())
        .unwrap_or_else(|| {
            std::env::var("HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| std::path::PathBuf::from("."))
        })
}
