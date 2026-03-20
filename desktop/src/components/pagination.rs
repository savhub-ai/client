use dioxus::prelude::*;

use crate::icons::{Icon, LucideIcon};
use crate::theme::Theme;

pub fn total_pages(total_items: usize, page_size: usize) -> usize {
    if total_items == 0 {
        0
    } else {
        total_items.div_ceil(page_size.max(1))
    }
}

pub fn clamp_page(current_page: usize, total_items: usize, page_size: usize) -> usize {
    let total_pages = total_pages(total_items, page_size);
    if total_pages == 0 {
        0
    } else {
        current_page.min(total_pages - 1)
    }
}

pub fn slice_for_page<T>(items: &[T], current_page: usize, page_size: usize) -> &[T] {
    let total_items = items.len();
    if total_items == 0 {
        return &items[0..0];
    }

    let page_size = page_size.max(1);
    let current_page = clamp_page(current_page, total_items, page_size);
    let start = current_page * page_size;
    let end = (start + page_size).min(total_items);
    &items[start..end]
}

#[component]
pub fn PaginationControls(
    current_page: usize,
    total_pages: Option<usize>,
    has_prev: bool,
    has_next: bool,
    on_prev: EventHandler<MouseEvent>,
    on_next: EventHandler<MouseEvent>,
) -> Element {
    if !has_prev && !has_next && total_pages.unwrap_or(0) <= 1 {
        return rsx! {};
    }

    let page_display = current_page + 1;
    let nav_btn = |enabled: bool| -> String {
        if enabled {
            format!(
                "display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; background: {bg}; color: {color}; border: 1px solid {line}; border-radius: 6px; cursor: pointer; line-height: 1; padding: 0;",
                bg = Theme::PANEL,
                color = Theme::ACCENT_STRONG,
                line = Theme::LINE
            )
        } else {
            format!(
                "display: inline-flex; align-items: center; justify-content: center; width: 28px; height: 28px; background: rgba(0,0,0,0.03); color: {color}; border: 1px solid {line}; border-radius: 6px; cursor: not-allowed; opacity: 0.4; line-height: 1; padding: 0;",
                color = Theme::MUTED,
                line = Theme::LINE
            )
        }
    };

    rsx! {
        div { style: "display: inline-flex; align-items: center; gap: 6px;",
            button {
                style: "{nav_btn(has_prev)}",
                disabled: !has_prev,
                onclick: move |evt| on_prev.call(evt),
                LucideIcon { icon: Icon::ChevronLeft, size: 16 }
            }
            span { style: "font-size: 12px; color: {Theme::MUTED}; min-width: 16px; text-align: center;",
                "{page_display}"
            }
            button {
                style: "{nav_btn(has_next)}",
                disabled: !has_next,
                onclick: move |evt| on_next.call(evt),
                LucideIcon { icon: Icon::ChevronRight, size: 16 }
            }
        }
    }
}
