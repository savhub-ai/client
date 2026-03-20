use dioxus::prelude::*;

use crate::theme::Theme;

/// Inline sign display: shields.io-style "SIGN" badge + value text + copy icon.
#[component]
pub fn CopySign(value: String) -> Element {
    let mut copied = use_signal(|| false);
    let display = value.clone();

    rsx! {
        span {
            style: "display: inline-flex; align-items: center; gap: 5px; cursor: pointer; \
                    max-width: 100%; font-size: 12px; line-height: 1;",
            title: "Click to copy",
            onclick: move |evt: Event<MouseData>| {
                evt.stop_propagation();
                let v = value.clone();
                if let Ok(mut clip) = arboard::Clipboard::new() {
                    let _ = clip.set_text(&v);
                    copied.set(true);
                    spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                        copied.set(false);
                    });
                }
            },
            span {
                style: "font-size: 10px; font-weight: 700; color: #fff; background: #5a9e3f; \
                        padding: 3px 7px; border-radius: 4px; letter-spacing: 0.02em; \
                        flex-shrink: 0;",
                "sign"
            }
            span {
                style: "color: {Theme::MUTED}; overflow: hidden; text-overflow: ellipsis; \
                        white-space: nowrap; min-width: 0;",
                "{display}"
            }
            if *copied.read() {
                span { style: "flex-shrink: 0; color: #2e8b57; font-size: 13px;", "\u{2713}" }
            } else {
                span { style: "flex-shrink: 0; color: {Theme::MUTED}; opacity: 0.4; font-size: 13px;", "\u{2398}" }
            }
        }
    }
}
