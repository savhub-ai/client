use dioxus::prelude::*;

use crate::icons::{Icon, LucideIcon};
use crate::theme::Theme;

/// Inline sign display: value text + copy icon. Copies JSON `{"repo":..,"path":..}`.
#[component]
pub fn CopySign(repo_url: String, path: String) -> Element {
    let mut copied = use_signal(|| false);
    let display = format!("{}/{}", strip_scheme(&repo_url), path);
    let copy_json = format!("{{\"repo\":\"{}\",\"path\":\"{}\"}}", repo_url, path);

    rsx! {
        span {
            style: "display: inline-flex; align-items: center; gap: 5px; cursor: pointer; \
                    max-width: 100%; font-size: 12px; line-height: 1;",
            title: "Click to copy",
            onclick: move |evt: Event<MouseData>| {
                evt.stop_propagation();
                let v = copy_json.clone();
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
                style: "color: {Theme::MUTED}; overflow: hidden; text-overflow: ellipsis; \
                        white-space: nowrap; min-width: 0;",
                "{display}"
            }
            if *copied.read() {
                span { style: "flex-shrink: 0; color: #2e8b57;",
                    LucideIcon { icon: Icon::Check, size: 13 }
                }
            } else {
                span { style: "flex-shrink: 0; color: {Theme::MUTED}; opacity: 0.4;",
                    LucideIcon { icon: Icon::Clipboard, size: 13 }
                }
            }
        }
    }
}

fn strip_scheme(url: &str) -> &str {
    let url = url.trim().trim_end_matches('/').trim_end_matches(".git");
    url.strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url)
}
