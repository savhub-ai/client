//! Centralized Lucide icon components for the desktop app.
//!
//! All UI icons come from [Lucide](https://lucide.dev) (MIT license).
//! Add new variants here instead of scattering inline SVGs across pages.

use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    // Sidebar / navigation
    LayoutDashboard,
    Compass,
    Package,
    ScanSearch,
    FolderOpen,
    FolderPlus,
    BookOpen,
    Settings,
    PanelLeftClose,
    PanelLeftOpen,

    // Settings menu
    SlidersHorizontal,
    CircleUser,
    Info,

    // View toggle
    List,
    LayoutGrid,

    // Actions
    X,
    RefreshCw,
    Check,
    Star,
    ArrowLeft,
    ChevronLeft,
    ChevronRight,
    Clipboard,
    ClipboardCheck,
    Lock,

    // Security
    ShieldCheck,
    Shield,
}

/// Render a Lucide icon as an inline SVG.
///
/// The icon inherits `currentColor` for its stroke, so wrap it in a container
/// that sets `color` to control the icon colour.
#[component]
pub fn LucideIcon(icon: Icon, #[props(default = 20)] size: u32) -> Element {
    let s = size.to_string();
    match icon {
        // ── Sidebar / navigation ────────────────────────────────────────
        Icon::LayoutDashboard => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { x: "3", y: "3", width: "7", height: "9", rx: "1" }
                rect { x: "14", y: "3", width: "7", height: "5", rx: "1" }
                rect { x: "14", y: "12", width: "7", height: "9", rx: "1" }
                rect { x: "3", y: "16", width: "7", height: "5", rx: "1" }
            }
        },
        Icon::Compass => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                circle { cx: "12", cy: "12", r: "10" }
                polygon { points: "16.24 7.76 14.12 14.12 7.76 16.24 9.88 9.88 16.24 7.76" }
            }
        },
        Icon::Package => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M16.5 9.4 7.55 4.24" }
                path { d: "M21 16V8a2 2 0 0 0-1-1.73l-7-4a2 2 0 0 0-2 0l-7 4A2 2 0 0 0 3 8v8a2 2 0 0 0 1 1.73l7 4a2 2 0 0 0 2 0l7-4A2 2 0 0 0 21 16z" }
                polyline { points: "3.29 7 12 12 20.71 7" }
                line { x1: "12", y1: "22", x2: "12", y2: "12" }
            }
        },
        Icon::ScanSearch => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M3 7V5a2 2 0 0 1 2-2h2" }
                path { d: "M17 3h2a2 2 0 0 1 2 2v2" }
                path { d: "M21 17v2a2 2 0 0 1-2 2h-2" }
                path { d: "M7 21H5a2 2 0 0 1-2-2v-2" }
                circle { cx: "12", cy: "12", r: "3" }
                path { d: "m16 16-1.9-1.9" }
            }
        },
        Icon::FolderOpen => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" }
            }
        },
        Icon::FolderPlus => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M12 10v6" }
                path { d: "M9 13h6" }
                path { d: "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z" }
            }
        },
        Icon::BookOpen => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M2 3h6a4 4 0 0 1 4 4v14a3 3 0 0 0-3-3H2z" }
                path { d: "M22 3h-6a4 4 0 0 0-4 4v14a3 3 0 0 1 3-3h7z" }
            }
        },
        Icon::Settings => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M12.22 2h-.44a2 2 0 0 0-2 2v.18a2 2 0 0 1-1 1.73l-.43.25a2 2 0 0 1-2 0l-.15-.08a2 2 0 0 0-2.73.73l-.22.38a2 2 0 0 0 .73 2.73l.15.1a2 2 0 0 1 1 1.72v.51a2 2 0 0 1-1 1.74l-.15.09a2 2 0 0 0-.73 2.73l.22.38a2 2 0 0 0 2.73.73l.15-.08a2 2 0 0 1 2 0l.43.25a2 2 0 0 1 1 1.73V20a2 2 0 0 0 2 2h.44a2 2 0 0 0 2-2v-.18a2 2 0 0 1 1-1.73l.43-.25a2 2 0 0 1 2 0l.15.08a2 2 0 0 0 2.73-.73l.22-.39a2 2 0 0 0-.73-2.73l-.15-.08a2 2 0 0 1-1-1.74v-.5a2 2 0 0 1 1-1.74l.15-.09a2 2 0 0 0 .73-2.73l-.22-.38a2 2 0 0 0-2.73-.73l-.15.08a2 2 0 0 1-2 0l-.43-.25a2 2 0 0 1-1-1.73V4a2 2 0 0 0-2-2z" }
                circle { cx: "12", cy: "12", r: "3" }
            }
        },
        Icon::PanelLeftClose => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { width: "18", height: "18", x: "3", y: "3", rx: "2" }
                path { d: "M9 3v18" }
                path { d: "m16 15-3-3 3-3" }
            }
        },
        Icon::PanelLeftOpen => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { width: "18", height: "18", x: "3", y: "3", rx: "2" }
                path { d: "M9 3v18" }
                path { d: "m14 9 3 3-3 3" }
            }
        },

        // ── Settings menu ───────────────────────────────────────────────
        Icon::SlidersHorizontal => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                line { x1: "21", x2: "14", y1: "4", y2: "4" }
                line { x1: "10", x2: "3", y1: "4", y2: "4" }
                line { x1: "21", x2: "12", y1: "12", y2: "12" }
                line { x1: "8", x2: "3", y1: "12", y2: "12" }
                line { x1: "21", x2: "16", y1: "20", y2: "20" }
                line { x1: "12", x2: "3", y1: "20", y2: "20" }
                line { x1: "14", x2: "14", y1: "2", y2: "6" }
                line { x1: "8", x2: "8", y1: "10", y2: "14" }
                line { x1: "16", x2: "16", y1: "18", y2: "22" }
            }
        },
        Icon::CircleUser => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                circle { cx: "12", cy: "12", r: "10" }
                circle { cx: "12", cy: "10", r: "3" }
                path { d: "M7 20.662V19a2 2 0 0 1 2-2h6a2 2 0 0 1 2 2v1.662" }
            }
        },
        Icon::Info => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                circle { cx: "12", cy: "12", r: "10" }
                path { d: "M12 16v-4" }
                path { d: "M12 8h.01" }
            }
        },

        // ── View toggle ─────────────────────────────────────────────────
        Icon::List => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                line { x1: "8", x2: "21", y1: "6", y2: "6" }
                line { x1: "8", x2: "21", y1: "12", y2: "12" }
                line { x1: "8", x2: "21", y1: "18", y2: "18" }
                line { x1: "3", x2: "3.01", y1: "6", y2: "6" }
                line { x1: "3", x2: "3.01", y1: "12", y2: "12" }
                line { x1: "3", x2: "3.01", y1: "18", y2: "18" }
            }
        },
        Icon::LayoutGrid => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { width: "7", height: "7", x: "3", y: "3", rx: "1" }
                rect { width: "7", height: "7", x: "14", y: "3", rx: "1" }
                rect { width: "7", height: "7", x: "14", y: "14", rx: "1" }
                rect { width: "7", height: "7", x: "3", y: "14", rx: "1" }
            }
        },

        // ── Actions ─────────────────────────────────────────────────────
        Icon::X => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M18 6 6 18" }
                path { d: "m6 6 12 12" }
            }
        },
        Icon::RefreshCw => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" }
                path { d: "M21 3v5h-5" }
                path { d: "M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" }
                path { d: "M8 16H3v5" }
            }
        },
        Icon::Check => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M20 6 9 17l-5-5" }
            }
        },
        Icon::Star => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                polygon { points: "12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" }
            }
        },
        Icon::ArrowLeft => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "m12 19-7-7 7-7" }
                path { d: "M19 12H5" }
            }
        },
        Icon::ChevronLeft => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "m15 18-6-6 6-6" }
            }
        },
        Icon::ChevronRight => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "m9 18 6-6-6-6" }
            }
        },
        Icon::Clipboard => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { width: "8", height: "4", x: "8", y: "2", rx: "1", ry: "1" }
                path { d: "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" }
            }
        },
        Icon::ClipboardCheck => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { width: "8", height: "4", x: "8", y: "2", rx: "1", ry: "1" }
                path { d: "M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2" }
                path { d: "m9 14 2 2 4-4" }
            }
        },
        Icon::Lock => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                rect { width: "18", height: "11", x: "3", y: "11", rx: "2", ry: "2" }
                path { d: "M7 11V7a5 5 0 0 1 10 0v4" }
            }
        },

        // ── Security ────────────────────────────────────────────────────
        Icon::ShieldCheck => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" }
                path { d: "m9 12 2 2 4-4" }
            }
        },
        Icon::Shield => rsx! {
            svg { width: "{s}", height: "{s}", view_box: "0 0 24 24", fill: "none", stroke: "currentColor", stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
                path { d: "M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" }
            }
        },
    }
}
