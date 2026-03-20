/// Bamboo green (竹叶青) color palette — matching savhub-server frontend.
pub struct Theme;

impl Theme {
    // Core palette
    pub const BG: &str = "#ecf2e8";
    pub const BG_ELEVATED: &str = "rgba(248, 253, 245, 0.85)";
    pub const PANEL: &str = "rgba(255, 255, 255, 0.9)";
    pub const LINE: &str = "rgba(30, 66, 28, 0.12)";
    pub const TEXT: &str = "#1a2e18";
    pub const MUTED: &str = "#4a6b46";
    pub const ACCENT: &str = "#5a9e3f";
    pub const ACCENT_STRONG: &str = "#2d6b1e";

    // Derived
    pub const ACCENT_LIGHT: &str = "rgba(90, 158, 63, 0.12)";
    #[allow(dead_code)]
    pub const ACCENT_HOVER: &str = "rgba(90, 158, 63, 0.16)";
    pub const DANGER: &str = "#8b1e1e";
    pub const SUCCESS: &str = "#2e8b57";
    pub const SIDEBAR_WIDTH: &str = "220px";
}

/// Global CSS injected into the app.
pub fn global_css() -> &'static str {
    r#"
    * { margin: 0; padding: 0; box-sizing: border-box; }
    body {
        font-family: 'Segoe UI', -apple-system, BlinkMacSystemFont, sans-serif;
        background: #ecf2e8;
        color: #1a2e18;
        line-height: 1.5;
    }
    a, a:visited, a:hover, a:active {
        text-decoration: none;
        color: inherit;
    }
    ::-webkit-scrollbar { width: 6px; }
    ::-webkit-scrollbar-track { background: transparent; }
    ::-webkit-scrollbar-thumb { background: rgba(30, 66, 28, 0.18); border-radius: 3px; }
    @keyframes spin { to { transform: rotate(360deg); } }
    "#
}
