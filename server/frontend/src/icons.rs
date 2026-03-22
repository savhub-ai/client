//! Centralized Lucide icon components for the web frontend.
//!
//! All UI icons come from [Lucide](https://lucide.dev) (MIT license).

use dioxus::prelude::*;

/// Lucide "check" — used for copy-success feedback.
#[component]
pub fn IconCheck(
    #[props(default = 14)] size: u32,
    #[props(default = "currentColor")] color: &'static str,
) -> Element {
    let s = size.to_string();
    rsx! {
        svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "{color}", stroke_width: "2.5", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M20 6 9 17l-5-5" }
        }
    }
}

/// Lucide "clipboard" — default copy button icon.
#[component]
pub fn IconClipboard(#[props(default = 14)] size: u32) -> Element {
    let s = size.to_string();
    rsx! {
        svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            rect { width: "8", height: "4", x: "8", y: "2", rx: "1", ry: "1" }
            path { d: "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" }
        }
    }
}

/// Lucide "x" — close / dismiss buttons.
#[component]
pub fn IconX(#[props(default = 14)] size: u32) -> Element {
    let s = size.to_string();
    rsx! {
        svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M18 6 6 18" }
            path { d: "m6 6 12 12" }
        }
    }
}

/// Lucide "lock" — authentication prompt icon.
#[component]
pub fn IconLock(#[props(default = 20)] size: u32) -> Element {
    let s = size.to_string();
    rsx! {
        svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            rect { width: "18", height: "11", x: "3", y: "11", rx: "2", ry: "2" }
            path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
        }
    }
}

/// Lucide "star" — ratings / star counts.
#[component]
pub fn IconStar(
    #[props(default = 14)] size: u32,
    #[props(default = false)] filled: bool,
) -> Element {
    let s = size.to_string();
    let fill_val = if filled { "currentColor" } else { "none" };
    rsx! {
        svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "{fill_val}", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
        }
    }
}

/// Lucide "shield-check" — security verified badge.
#[component]
pub fn IconShieldCheck(
    #[props(default = 16)] size: u32,
    #[props(default = "currentColor")] color: &'static str,
) -> Element {
    let s = size.to_string();
    rsx! {
        svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "{color}", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" }
            path { d: "m9 12 2 2 4-4" }
        }
    }
}

/// Lucide "shield" — security status badge (non-verified).
#[component]
pub fn IconShield(
    #[props(default = 16)] size: u32,
    #[props(default = "currentColor")] color: &'static str,
) -> Element {
    let s = size.to_string();
    rsx! {
        svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "{color}", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" }
        }
    }
}

/// Lucide "settings" — admin toggle icon.
#[component]
pub fn IconSettings(#[props(default = 14)] size: u32) -> Element {
    let s = size.to_string();
    rsx! {
        svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" }
            circle { cx: "12", cy: "12", r: "3" }
        }
    }
}
