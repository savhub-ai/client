use dioxus::prelude::*;
use savhub_shared::SecurityStatus;

use crate::i18n;
use crate::state::AppState;

/// Colors matching the frontend CSS (.security-badge .sb-* classes).
fn badge_colors(status: &SecurityStatus) -> (&'static str, &'static str) {
    // Returns (icon_bg, value_bg)
    match status {
        SecurityStatus::Verified => ("#555", "#2e8b57"),
        SecurityStatus::Validated => ("#555", "#6a9f5b"),
        SecurityStatus::Suspicious => ("#555", "#b8860b"),
        SecurityStatus::Malicious => ("#555", "#e0413a"),
        SecurityStatus::Unscanned => ("#555", "#999"),
    }
}

fn badge_label<'a>(status: &SecurityStatus, t: &'a i18n::Texts) -> &'a str {
    match status {
        SecurityStatus::Verified => t.security_verified,
        SecurityStatus::Validated => t.security_validated,
        SecurityStatus::Suspicious => t.security_suspicious,
        SecurityStatus::Malicious => t.security_malicious,
        SecurityStatus::Unscanned => t.security_unscanned,
    }
}

#[component]
pub fn SecurityBadge(status: SecurityStatus) -> Element {
    let state = use_context::<AppState>();
    let t = i18n::texts(*state.lang.read());
    let (icon_bg, value_bg) = badge_colors(&status);
    let label = badge_label(&status, t);
    let badge_height = 18;

    rsx! {
        span { style: "display: inline-flex; align-items: stretch; font-size: 11px; line-height: 1; border-radius: 3px; overflow: hidden; vertical-align: middle; white-space: nowrap; position: relative; top: -0.1em;",
            span { style: "display: inline-flex; align-items: center; justify-content: center; box-sizing: border-box; min-height: {badge_height}px; padding: 0 5px; background: {icon_bg}; color: #fff;",
                if matches!(status, SecurityStatus::Verified | SecurityStatus::Validated) {
                    crate::icons::LucideIcon { icon: crate::icons::Icon::ShieldCheck, size: 12 }
                } else {
                    crate::icons::LucideIcon { icon: crate::icons::Icon::Shield, size: 12 }
                }
            }
            span { style: "display: inline-flex; align-items: center; box-sizing: border-box; min-height: {badge_height}px; padding: 0 6px; color: #fff; font-weight: 600; background: {value_bg};",
                "{label}"
            }
        }
    }
}
