use dioxus::prelude::*;

use crate::theme::Theme;

/// Inline sign display with click-to-copy.  Shows a subtle copy icon on hover.
#[component]
pub fn CopySign(value: String) -> Element {
    let mut copied = use_signal(|| false);
    let display = value.clone();

    rsx! {
        span {
            style: "display: inline-flex; align-items: center; gap: 3px; cursor: pointer; \
                    padding: 0 3px; border-radius: 4px; font-size: 12px; color: {Theme::MUTED}; \
                    transition: background 0.15s;",
            title: "Copy: {display}",
            onclick: move |evt: Event<MouseData>| {
                evt.stop_propagation();
                let v = value.clone();
                if let Ok(mut clip) = arboard::Clipboard::new() {
                    let _ = clip.set_text(&v);
                    copied.set(true);
                    spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                        copied.set(false);
                    });
                }
            },
            if *copied.read() {
                span { style: "font-size: 11px; color: {Theme::SUCCESS};", "\u{2713}" }
            }
            "{display}"
            if !*copied.read() {
                span { style: "opacity: 0.4; font-size: 10px;", "\u{2398}" }
            }
        }
    }
}
