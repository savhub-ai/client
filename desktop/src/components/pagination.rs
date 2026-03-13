use dioxus::prelude::*;

use crate::i18n;
use crate::state::AppState;
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
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());

    if !has_prev && !has_next && total_pages.unwrap_or(0) <= 1 {
        return rsx! {};
    }

    let prev_label = t.pagination_previous;
    let next_label = t.pagination_next;
    let indicator = t.fmt_page_indicator(current_page + 1, total_pages);
    let prev_style = if has_prev {
        format!(
            "padding: 6px 12px; background: {bg}; color: {color}; border: 1px solid {line}; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: pointer;",
            bg = Theme::PANEL,
            color = Theme::ACCENT_STRONG,
            line = Theme::LINE
        )
    } else {
        format!(
            "padding: 6px 12px; background: rgba(0,0,0,0.03); color: {color}; border: 1px solid {line}; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: not-allowed; opacity: 0.55;",
            color = Theme::MUTED,
            line = Theme::LINE
        )
    };
    let next_style = if has_next {
        format!(
            "padding: 6px 12px; background: {bg}; color: {color}; border: 1px solid {line}; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: pointer;",
            bg = Theme::PANEL,
            color = Theme::ACCENT_STRONG,
            line = Theme::LINE
        )
    } else {
        format!(
            "padding: 6px 12px; background: rgba(0,0,0,0.03); color: {color}; border: 1px solid {line}; border-radius: 6px; font-size: 12px; font-weight: 600; cursor: not-allowed; opacity: 0.55;",
            color = Theme::MUTED,
            line = Theme::LINE
        )
    };

    rsx! {
        div { style: "display: flex; align-items: center; justify-content: flex-end; gap: 10px; margin-top: 12px;",
            span { style: "font-size: 12px; color: {Theme::MUTED};",
                "{indicator}"
            }
            button {
                style: "{prev_style}",
                disabled: !has_prev,
                onclick: move |evt| on_prev.call(evt),
                "{prev_label}"
            }
            button {
                style: "{next_style}",
                disabled: !has_next,
                onclick: move |evt| on_next.call(evt),
                "{next_label}"
            }
        }
    }
}
