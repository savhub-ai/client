use dioxus::prelude::*;

use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Cards,
    List,
}

/// A 34×34 icon button that toggles between card and list views.
#[component]
pub fn ViewToggleButton(mode: ViewMode, on_toggle: EventHandler<()>) -> Element {
    let title = match mode {
        ViewMode::Cards => "List view",
        ViewMode::List => "Card view",
    };
    let icon_mode = match mode {
        ViewMode::Cards => ViewMode::List,
        ViewMode::List => ViewMode::Cards,
    };
    rsx! {
        button {
            title: title,
            style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer;",
            onclick: move |_| on_toggle.call(()),
            ViewToggleIcon { mode: icon_mode, size: 16 }
        }
    }
}

#[component]
fn ViewToggleIcon(mode: ViewMode, size: u32) -> Element {
    let size_attr = size.to_string();

    match mode {
        ViewMode::Cards => rsx! {
            svg {
                width: "{size_attr}",
                height: "{size_attr}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.9",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                path { d: "M8 6H20" }
                path { d: "M8 12H20" }
                path { d: "M8 18H20" }
                path { d: "M4.5 6H4.51" }
                path { d: "M4.5 12H4.51" }
                path { d: "M4.5 18H4.51" }
            }
        },
        ViewMode::List => rsx! {
            svg {
                width: "{size_attr}",
                height: "{size_attr}",
                view_box: "0 0 24 24",
                fill: "none",
                stroke: "currentColor",
                stroke_width: "1.9",
                stroke_linecap: "round",
                stroke_linejoin: "round",
                rect { x: "3.5", y: "3.5", width: "7", height: "7", rx: "1.3" }
                rect { x: "13.5", y: "3.5", width: "7", height: "7", rx: "1.3" }
                rect { x: "3.5", y: "13.5", width: "7", height: "7", rx: "1.3" }
                rect { x: "13.5", y: "13.5", width: "7", height: "7", rx: "1.3" }
            }
        },
    }
}
