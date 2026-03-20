use dioxus::prelude::*;

use crate::icons::{Icon, LucideIcon};
use crate::theme::Theme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Cards,
    List,
}

/// A 34x34 icon button that toggles between card and list views.
#[component]
pub fn ViewToggleButton(mode: ViewMode, on_toggle: EventHandler<()>) -> Element {
    let title = match mode {
        ViewMode::Cards => "List view",
        ViewMode::List => "Card view",
    };
    let icon = match mode {
        ViewMode::Cards => Icon::List,
        ViewMode::List => Icon::LayoutGrid,
    };
    rsx! {
        button {
            title: title,
            style: "display: inline-flex; align-items: center; justify-content: center; width: 34px; height: 34px; flex-shrink: 0; background: {Theme::PANEL}; color: {Theme::ACCENT_STRONG}; border: 1px solid {Theme::LINE}; border-radius: 8px; cursor: pointer;",
            onclick: move |_| on_toggle.call(()),
            LucideIcon { icon, size: 16 }
        }
    }
}
