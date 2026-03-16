use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::config::get_config_dir;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single rule condition for a selector.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SelectorRule {
    /// Check that a file exists relative to the folder scope.
    FileExists { path: String },
    /// Check that a sub-folder exists relative to the folder scope.
    FolderExists { path: String },
    /// Check that at least one file matching the glob pattern exists.
    GlobMatch { pattern: String },
    /// Check that a file contains a specific string (case-sensitive substring match).
    FileContains { path: String, contains: String },
    /// Check that a file's content matches a regular expression.
    FileRegex { path: String, pattern: String },
    /// Check that an environment variable is set (non-empty).
    EnvVarSet { name: String },
    /// Run a shell command and check that it exits with code 0.
    CommandExits { command: String },
}

/// Mode for how rules are combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchMode {
    AllMatch,
    AnyMatch,
    Custom,
}

/// A composable boolean expression tree over selector rules.
///
/// Rules are referenced by 0-based index into `SelectorDefinition.rules`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum RuleExpression {
    /// Reference to a rule by index.
    Check { index: usize },
    /// All operands must evaluate to true.
    And { operands: Vec<RuleExpression> },
    /// At least one operand must evaluate to true.
    Or { operands: Vec<RuleExpression> },
    /// Negation.
    Not { operand: Box<RuleExpression> },
}

/// A complete selector definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SelectorDefinition {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_folder_scope")]
    pub folder_scope: String,
    pub rules: Vec<SelectorRule>,
    pub match_mode: MatchMode,
    /// Custom expression string (only used when match_mode is Custom).
    #[serde(default)]
    pub custom_expression: String,
    #[serde(default)]
    pub presets: Vec<String>,
    #[serde(default)]
    pub add_skills: Vec<String>,
    #[serde(default)]
    pub add_flocks: Vec<String>,
    /// Priority (higher value = higher priority). When multiple selectors
    /// contribute conflicting skills, the selector with the higher priority wins.
    #[serde(default)]
    pub priority: i32,
}

fn default_folder_scope() -> String {
    ".".to_string()
}

/// Persistent store for all selector definitions.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct SelectorsStore {
    pub version: u8,
    pub selectors: Vec<SelectorDefinition>,
}

// ---------------------------------------------------------------------------
// Expression builder & evaluation
// ---------------------------------------------------------------------------

impl SelectorDefinition {
    /// Build the effective rule expression from the match mode.
    pub fn build_expression(&self) -> Result<RuleExpression> {
        match self.match_mode {
            MatchMode::AllMatch => {
                let operands: Vec<RuleExpression> = (0..self.rules.len())
                    .map(|i| RuleExpression::Check { index: i })
                    .collect();
                if operands.is_empty() {
                    bail!("no rules defined");
                }
                Ok(if operands.len() == 1 {
                    operands.into_iter().next().unwrap()
                } else {
                    RuleExpression::And { operands }
                })
            }
            MatchMode::AnyMatch => {
                let operands: Vec<RuleExpression> = (0..self.rules.len())
                    .map(|i| RuleExpression::Check { index: i })
                    .collect();
                if operands.is_empty() {
                    bail!("no rules defined");
                }
                Ok(if operands.len() == 1 {
                    operands.into_iter().next().unwrap()
                } else {
                    RuleExpression::Or { operands }
                })
            }
            MatchMode::Custom => parse_expression(&self.custom_expression, self.rules.len()),
        }
    }

    /// Evaluate this selector against a project root directory.
    pub fn evaluate(&self, project_root: &Path) -> bool {
        let Ok(expr) = self.build_expression() else {
            return false;
        };
        let base = if self.folder_scope == "." || self.folder_scope.is_empty() {
            project_root.to_path_buf()
        } else {
            project_root.join(&self.folder_scope)
        };
        expr.evaluate(&base, &self.rules)
    }

    /// Generate a human-readable expression string.
    pub fn display_expression(&self) -> String {
        match self.match_mode {
            MatchMode::AllMatch => (1..=self.rules.len())
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(" && "),
            MatchMode::AnyMatch => (1..=self.rules.len())
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(" || "),
            MatchMode::Custom => self.custom_expression.clone(),
        }
    }
}

impl RuleExpression {
    /// Evaluate the expression tree against a base directory.
    pub fn evaluate(&self, base: &Path, rules: &[SelectorRule]) -> bool {
        match self {
            RuleExpression::Check { index } => {
                rules.get(*index).is_some_and(|rule| rule.evaluate(base))
            }
            RuleExpression::And { operands } => operands.iter().all(|op| op.evaluate(base, rules)),
            RuleExpression::Or { operands } => operands.iter().any(|op| op.evaluate(base, rules)),
            RuleExpression::Not { operand } => !operand.evaluate(base, rules),
        }
    }

    /// Convert the expression tree to a human-readable string with 1-based rule numbers.
    pub fn to_display_string(&self) -> String {
        self.fmt_inner(false)
    }

    fn fmt_inner(&self, needs_parens: bool) -> String {
        match self {
            RuleExpression::Check { index } => format!("{}", index + 1),
            RuleExpression::And { operands } => {
                let inner = operands
                    .iter()
                    .map(|op| op.fmt_inner(matches!(op, RuleExpression::Or { .. })))
                    .collect::<Vec<_>>()
                    .join(" && ");
                if needs_parens {
                    format!("({inner})")
                } else {
                    inner
                }
            }
            RuleExpression::Or { operands } => {
                let inner = operands
                    .iter()
                    .map(|op| op.fmt_inner(false))
                    .collect::<Vec<_>>()
                    .join(" || ");
                if needs_parens {
                    format!("({inner})")
                } else {
                    inner
                }
            }
            RuleExpression::Not { operand } => {
                let wrap = matches!(
                    **operand,
                    RuleExpression::And { .. } | RuleExpression::Or { .. }
                );
                format!("!{}", operand.fmt_inner(wrap))
            }
        }
    }
}

impl SelectorRule {
    /// Evaluate a single rule against a base directory.
    pub fn evaluate(&self, base: &Path) -> bool {
        match self {
            SelectorRule::FileExists { path } => base.join(path).is_file(),
            SelectorRule::FolderExists { path } => base.join(path).is_dir(),
            SelectorRule::GlobMatch { pattern } => glob_any_match(base, pattern),
            SelectorRule::FileContains { path, contains } => {
                std::fs::read_to_string(base.join(path))
                    .map(|content| content.contains(contains.as_str()))
                    .unwrap_or(false)
            }
            SelectorRule::FileRegex { path, pattern } => {
                let Ok(re) = regex::Regex::new(pattern) else {
                    return false;
                };
                std::fs::read_to_string(base.join(path))
                    .map(|content| re.is_match(&content))
                    .unwrap_or(false)
            }
            SelectorRule::EnvVarSet { name } => {
                std::env::var(name).map(|v| !v.is_empty()).unwrap_or(false)
            }
            SelectorRule::CommandExits { command } => {
                #[cfg(target_os = "windows")]
                {
                    std::process::Command::new("cmd")
                        .args(["/C", command])
                        .current_dir(base)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                }
                #[cfg(not(target_os = "windows"))]
                {
                    std::process::Command::new("sh")
                        .args(["-c", command])
                        .current_dir(base)
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .map(|s| s.success())
                        .unwrap_or(false)
                }
            }
        }
    }

    /// Human-readable display string.
    pub fn display(&self) -> String {
        match self {
            SelectorRule::FileExists { path } => format!("File: {path}"),
            SelectorRule::FolderExists { path } => format!("Folder: {path}"),
            SelectorRule::GlobMatch { pattern } => format!("Glob: {pattern}"),
            SelectorRule::FileContains { path, contains } => {
                format!("Contains: {path} → \"{contains}\"")
            }
            SelectorRule::FileRegex { path, pattern } => {
                format!("Regex: {path} → /{pattern}/")
            }
            SelectorRule::EnvVarSet { name } => format!("Env: ${name}"),
            SelectorRule::CommandExits { command } => format!("Cmd: {command}"),
        }
    }

    /// Short kind string for form selectors.
    pub fn kind_str(&self) -> &'static str {
        match self {
            SelectorRule::FileExists { .. } => "file_exists",
            SelectorRule::FolderExists { .. } => "folder_exists",
            SelectorRule::GlobMatch { .. } => "glob_match",
            SelectorRule::FileContains { .. } => "file_contains",
            SelectorRule::FileRegex { .. } => "file_regex",
            SelectorRule::EnvVarSet { .. } => "env_var_set",
            SelectorRule::CommandExits { .. } => "command_exits",
        }
    }
}

/// Check if any file under `base` matches the given glob pattern.
///
/// Supports `*` (any chars in filename), `?` (single char), and `**` (recursive dirs).
fn glob_any_match(base: &Path, pattern: &str) -> bool {
    use walkdir::WalkDir;

    // Normalise pattern separators to /
    let pat = pattern.replace('\\', "/");

    for entry in WalkDir::new(base).max_depth(10).into_iter().flatten() {
        let Ok(rel) = entry.path().strip_prefix(base) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        if glob_pattern_matches(&pat, &rel_str) {
            return true;
        }
    }
    false
}

/// Simple glob matching: `*` matches non-`/` chars, `**` matches anything, `?` matches one char.
fn glob_pattern_matches(pattern: &str, text: &str) -> bool {
    glob_match_recursive(pattern.as_bytes(), text.as_bytes())
}

fn glob_match_recursive(pat: &[u8], txt: &[u8]) -> bool {
    if pat.is_empty() {
        return txt.is_empty();
    }

    // Handle ** (matches any path segments)
    if pat.starts_with(b"**") {
        let rest = if pat.len() > 2 && pat[2] == b'/' {
            &pat[3..]
        } else {
            &pat[2..]
        };
        // Try matching rest against every suffix of txt
        for i in 0..=txt.len() {
            if glob_match_recursive(rest, &txt[i..]) {
                return true;
            }
        }
        return false;
    }

    if txt.is_empty() {
        return false;
    }

    match pat[0] {
        b'*' => {
            // * matches zero or more non-/ characters
            for i in 0..=txt.len() {
                if i > 0 && txt[i - 1] == b'/' {
                    break;
                }
                if glob_match_recursive(&pat[1..], &txt[i..]) {
                    return true;
                }
            }
            false
        }
        b'?' => {
            if txt[0] != b'/' {
                glob_match_recursive(&pat[1..], &txt[1..])
            } else {
                false
            }
        }
        c => {
            if c == txt[0] {
                glob_match_recursive(&pat[1..], &txt[1..])
            } else {
                false
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Expression parser
// ---------------------------------------------------------------------------

/// Supported expression tokens.
#[derive(Debug, Clone, PartialEq)]
enum Token {
    Number(usize),
    And,
    Or,
    Not,
    LParen,
    RParen,
}

fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();
    while let Some(&ch) = chars.peek() {
        match ch {
            ' ' | '\t' | '\n' | '\r' => {
                chars.next();
            }
            '(' => {
                tokens.push(Token::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(Token::RParen);
                chars.next();
            }
            '!' => {
                tokens.push(Token::Not);
                chars.next();
            }
            '&' => {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    tokens.push(Token::And);
                } else {
                    bail!("expected '&&', got single '&'");
                }
            }
            '|' => {
                chars.next();
                if chars.peek() == Some(&'|') {
                    chars.next();
                    tokens.push(Token::Or);
                } else {
                    bail!("expected '||', got single '|'");
                }
            }
            '0'..='9' => {
                let mut num = String::new();
                while let Some(&d) = chars.peek() {
                    if d.is_ascii_digit() {
                        num.push(d);
                        chars.next();
                    } else {
                        break;
                    }
                }
                let n: usize = num.parse().context("invalid number")?;
                tokens.push(Token::Number(n));
            }
            other => bail!("unexpected character: '{other}'"),
        }
    }
    Ok(tokens)
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    max_index: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>, max_index: usize) -> Self {
        Self {
            tokens,
            pos: 0,
            max_index,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos)?.clone();
        self.pos += 1;
        Some(token)
    }

    fn expect(&mut self, expected: &Token) -> Result<()> {
        let token = self.advance().context("unexpected end of expression")?;
        if &token != expected {
            bail!("expected {expected:?}, got {token:?}");
        }
        Ok(())
    }

    /// expr = or_expr
    fn parse_expr(&mut self) -> Result<RuleExpression> {
        self.parse_or()
    }

    /// or_expr = and_expr ("||" and_expr)*
    fn parse_or(&mut self) -> Result<RuleExpression> {
        let mut left = self.parse_and()?;
        while self.peek() == Some(&Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = match left {
                RuleExpression::Or { mut operands } => {
                    operands.push(right);
                    RuleExpression::Or { operands }
                }
                _ => RuleExpression::Or {
                    operands: vec![left, right],
                },
            };
        }
        Ok(left)
    }

    /// and_expr = unary ("&&" unary)*
    fn parse_and(&mut self) -> Result<RuleExpression> {
        let mut left = self.parse_unary()?;
        while self.peek() == Some(&Token::And) {
            self.advance();
            let right = self.parse_unary()?;
            left = match left {
                RuleExpression::And { mut operands } => {
                    operands.push(right);
                    RuleExpression::And { operands }
                }
                _ => RuleExpression::And {
                    operands: vec![left, right],
                },
            };
        }
        Ok(left)
    }

    /// unary = "!" unary | primary
    fn parse_unary(&mut self) -> Result<RuleExpression> {
        if self.peek() == Some(&Token::Not) {
            self.advance();
            let operand = self.parse_unary()?;
            Ok(RuleExpression::Not {
                operand: Box::new(operand),
            })
        } else {
            self.parse_primary()
        }
    }

    /// primary = NUMBER | "(" expr ")"
    fn parse_primary(&mut self) -> Result<RuleExpression> {
        match self.advance() {
            Some(Token::Number(n)) => {
                if n == 0 || n > self.max_index {
                    bail!("rule number {n} out of range (1..={})", self.max_index);
                }
                Ok(RuleExpression::Check { index: n - 1 })
            }
            Some(Token::LParen) => {
                let expr = self.parse_expr()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Some(other) => bail!("unexpected token: {other:?}"),
            None => bail!("unexpected end of expression"),
        }
    }
}

/// Parse an expression string like `(1 && 2) || !3` into a `RuleExpression` tree.
///
/// Rule numbers are 1-based. `max_rules` is the total number of available rules.
pub fn parse_expression(input: &str, max_rules: usize) -> Result<RuleExpression> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        bail!("expression is empty");
    }
    let tokens = tokenize(trimmed)?;
    if tokens.is_empty() {
        bail!("expression is empty");
    }
    let mut parser = Parser::new(tokens, max_rules);
    let expr = parser.parse_expr()?;
    if parser.pos < parser.tokens.len() {
        bail!("unexpected tokens after expression");
    }
    Ok(expr)
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

fn selectors_path() -> Result<PathBuf> {
    Ok(get_config_dir()?.join("selectors.json"))
}

pub fn read_selectors_store() -> Result<SelectorsStore> {
    let path = selectors_path()?;
    if let Ok(raw) = fs::read_to_string(&path) {
        let store: SelectorsStore = serde_json::from_str(&raw)
            .with_context(|| format!("invalid selectors at {}", path.display()))?;
        return Ok(store);
    }
    let mut store = SelectorsStore {
        version: 1,
        selectors: Vec::new(),
    };
    seed_default_selectors(&mut store);
    let _ = write_selectors_store(&store);
    Ok(store)
}

pub fn write_selectors_store(store: &SelectorsStore) -> Result<()> {
    let path = selectors_path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_string_pretty(store)?;
    fs::write(&path, format!("{payload}\n"))?;
    Ok(())
}

/// Generate a unique ID for a new selector.
pub fn generate_selector_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("det-{ts:x}")
}

/// Deduplicate skills and presets in a selector before saving.
fn dedup_selector(mut d: SelectorDefinition) -> SelectorDefinition {
    let mut seen = std::collections::BTreeSet::new();
    d.add_skills.retain(|s| seen.insert(s.clone()));
    let mut seen = std::collections::BTreeSet::new();
    d.presets.retain(|s| seen.insert(s.clone()));
    d
}

pub fn create_selector(selector: SelectorDefinition) -> Result<()> {
    let mut store = read_selectors_store()?;
    if store.selectors.iter().any(|d| d.id == selector.id) {
        bail!("selector with id '{}' already exists", selector.id);
    }
    store.selectors.push(dedup_selector(selector));
    write_selectors_store(&store)
}

pub fn update_selector(selector: SelectorDefinition) -> Result<()> {
    let mut store = read_selectors_store()?;
    if let Some(existing) = store.selectors.iter_mut().find(|d| d.id == selector.id) {
        *existing = dedup_selector(selector);
    } else {
        bail!("selector '{}' not found", selector.id);
    }
    write_selectors_store(&store)
}

pub fn delete_selector(id: &str) -> Result<()> {
    let mut store = read_selectors_store()?;
    let before = store.selectors.len();
    store.selectors.retain(|d| d.id != id);
    if store.selectors.len() == before {
        bail!("selector '{id}' not found");
    }
    write_selectors_store(&store)
}

// ---------------------------------------------------------------------------
// Default selectors (seeded on first use)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Selector execution engine
// ---------------------------------------------------------------------------

/// A matched selector with its collected skills.
#[derive(Debug, Clone)]
pub struct SelectorMatch {
    pub selector: SelectorDefinition,
    pub presets: Vec<String>,
    pub skills: Vec<String>,
    pub flocks: Vec<String>,
}

/// Result of running all selectors against a project.
#[derive(Debug, Clone)]
pub struct SelectorRunResult {
    /// Selectors that matched, sorted by priority (highest first).
    pub matched: Vec<SelectorMatch>,
    /// Merged presets from all matched selectors.
    pub presets: Vec<String>,
    /// Merged skills with priority-based conflict resolution.
    /// Higher-priority selectors' skills take precedence.
    pub skills: Vec<String>,
    /// Merged flocks from all matched selectors.
    pub flocks: Vec<String>,
}

/// Run all selectors against a project directory.
///
/// Selectors are evaluated in priority order (highest first).
/// When multiple selectors contribute a skill with the same slug,
/// the higher-priority selector wins.
pub fn run_selectors(project_root: &Path) -> Result<SelectorRunResult> {
    let store = read_selectors_store()?;
    let mut matched: Vec<SelectorMatch> = Vec::new();

    for selector in &store.selectors {
        if selector.evaluate(project_root) {
            matched.push(SelectorMatch {
                selector: selector.clone(),
                presets: selector.presets.clone(),
                skills: selector.add_skills.clone(),
                flocks: selector.add_flocks.clone(),
            });
        }
    }

    // Sort by priority descending (higher priority first)
    matched.sort_by(|a, b| b.selector.priority.cmp(&a.selector.priority));

    // Merge presets (order by priority, deduplicate)
    let mut seen_presets = std::collections::BTreeSet::new();
    let mut presets = Vec::new();
    for m in &matched {
        for preset in &m.presets {
            if seen_presets.insert(preset.clone()) {
                presets.push(preset.clone());
            }
        }
    }

    // Merge skills with priority-based conflict resolution
    // Higher-priority selector's skills come first and take precedence
    let mut seen_skills = std::collections::BTreeSet::new();
    let mut skills = Vec::new();
    for m in &matched {
        for skill in &m.skills {
            if seen_skills.insert(skill.clone()) {
                skills.push(skill.clone());
            }
        }
    }

    // Merge flocks (order by priority, deduplicate)
    let mut seen_flocks = std::collections::BTreeSet::new();
    let mut flocks = Vec::new();
    for m in &matched {
        for flock in &m.flocks {
            if seen_flocks.insert(flock.clone()) {
                flocks.push(flock.clone());
            }
        }
    }

    Ok(SelectorRunResult {
        matched,
        presets,
        skills,
        flocks,
    })
}

fn seed_default_selectors(store: &mut SelectorsStore) {
    let defaults = vec![
        // ── Language-level selectors ─────────────────────────
        SelectorDefinition {
            id: "builtin-rust-project".to_string(),
            name: "Rust Project".to_string(),
            description: "Detects Rust projects by the presence of Cargo.toml.".to_string(),
            folder_scope: ".".to_string(),
            rules: vec![SelectorRule::FileExists {
                path: "Cargo.toml".to_string(),
            }],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 10,
        },
        SelectorDefinition {
            id: "builtin-python-project".to_string(),
            name: "Python Project".to_string(),
            description: "Detects Python projects by pyproject.toml or requirements.txt."
                .to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "pyproject.toml".to_string(),
                },
                SelectorRule::FileExists {
                    path: "requirements.txt".to_string(),
                },
                SelectorRule::FileExists {
                    path: "setup.py".to_string(),
                },
            ],
            match_mode: MatchMode::AnyMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 10,
        },
        SelectorDefinition {
            id: "builtin-go-project".to_string(),
            name: "Go Project".to_string(),
            description: "Detects Go projects by the presence of go.mod.".to_string(),
            folder_scope: ".".to_string(),
            rules: vec![SelectorRule::FileExists {
                path: "go.mod".to_string(),
            }],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 10,
        },
        SelectorDefinition {
            id: "builtin-java-project".to_string(),
            name: "Java / Kotlin Project".to_string(),
            description: "Detects JVM projects via pom.xml or build.gradle.".to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "pom.xml".to_string(),
                },
                SelectorRule::FileExists {
                    path: "build.gradle".to_string(),
                },
                SelectorRule::FileExists {
                    path: "build.gradle.kts".to_string(),
                },
            ],
            match_mode: MatchMode::AnyMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 10,
        },
        // ── Rust framework selectors ─────────────────────────
        SelectorDefinition {
            id: "builtin-salvo-project".to_string(),
            name: "Salvo Web Framework".to_string(),
            description: "Detects Rust projects using the Salvo web framework.".to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "Cargo.toml".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "Cargo.toml".to_string(),
                    pattern: r#"salvo\s*="#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        SelectorDefinition {
            id: "builtin-actix-project".to_string(),
            name: "Actix Web Framework".to_string(),
            description: "Detects Rust projects using the Actix-web framework.".to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "Cargo.toml".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "Cargo.toml".to_string(),
                    pattern: r#"actix-web\s*="#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        SelectorDefinition {
            id: "builtin-axum-project".to_string(),
            name: "Axum Web Framework".to_string(),
            description: "Detects Rust projects using the Axum web framework.".to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "Cargo.toml".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "Cargo.toml".to_string(),
                    pattern: r#"axum\s*="#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        SelectorDefinition {
            id: "builtin-dioxus-project".to_string(),
            name: "Dioxus Framework".to_string(),
            description: "Detects Rust projects using the Dioxus UI framework.".to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "Cargo.toml".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "Cargo.toml".to_string(),
                    pattern: r#"dioxus\s*="#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        // ── JS/TS framework selectors ────────────────────────
        SelectorDefinition {
            id: "builtin-web-frontend".to_string(),
            name: "Web Frontend (Node/TS)".to_string(),
            description: "Detects Node.js or TypeScript frontend projects.".to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "package.json".to_string(),
                },
                SelectorRule::FileExists {
                    path: "tsconfig.json".to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 10,
        },
        SelectorDefinition {
            id: "builtin-react-project".to_string(),
            name: "React".to_string(),
            description: "Detects React projects by checking package.json for react dependency."
                .to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "package.json".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "package.json".to_string(),
                    pattern: r#""react"\s*:"#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        SelectorDefinition {
            id: "builtin-vue-project".to_string(),
            name: "Vue".to_string(),
            description: "Detects Vue.js projects by checking package.json for vue dependency."
                .to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "package.json".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "package.json".to_string(),
                    pattern: r#""vue"\s*:"#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        SelectorDefinition {
            id: "builtin-angular-project".to_string(),
            name: "Angular".to_string(),
            description: "Detects Angular projects by checking package.json for @angular/core."
                .to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "package.json".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "package.json".to_string(),
                    pattern: r#""@angular/core"\s*:"#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        SelectorDefinition {
            id: "builtin-svelte-project".to_string(),
            name: "Svelte".to_string(),
            description: "Detects Svelte projects by checking package.json for svelte dependency."
                .to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "package.json".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "package.json".to_string(),
                    pattern: r#""svelte"\s*:"#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        SelectorDefinition {
            id: "builtin-nextjs-project".to_string(),
            name: "Next.js".to_string(),
            description: "Detects Next.js projects by checking package.json for next dependency."
                .to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "package.json".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "package.json".to_string(),
                    pattern: r#""next"\s*:"#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        SelectorDefinition {
            id: "builtin-nuxt-project".to_string(),
            name: "Nuxt".to_string(),
            description: "Detects Nuxt projects by checking package.json for nuxt dependency."
                .to_string(),
            folder_scope: ".".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "package.json".to_string(),
                },
                SelectorRule::FileRegex {
                    path: "package.json".to_string(),
                    pattern: r#""nuxt"\s*:"#.to_string(),
                },
            ],
            match_mode: MatchMode::AllMatch,
            custom_expression: String::new(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
        // ── Monorepo ─────────────────────────────────────────
        SelectorDefinition {
            id: "builtin-monorepo-web".to_string(),
            name: "Monorepo Web App".to_string(),
            description: "Scopes detection to a workspace folder inside a monorepo.".to_string(),
            folder_scope: "apps/web".to_string(),
            rules: vec![
                SelectorRule::FileExists {
                    path: "package.json".to_string(),
                },
                SelectorRule::FileExists {
                    path: "vite.config.ts".to_string(),
                },
                SelectorRule::FileExists {
                    path: "../pnpm-workspace.yaml".to_string(),
                },
            ],
            match_mode: MatchMode::Custom,
            custom_expression: "(1 && 2) || 3".to_string(),
            presets: vec![],
            add_skills: vec![],
            add_flocks: vec![],
            priority: 20,
        },
    ];
    for selector in defaults {
        if !store.selectors.iter().any(|d| d.id == selector.id) {
            store.selectors.push(selector);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_all_match() {
        let expr = parse_expression("1 && 2 && 3", 3).unwrap();
        assert_eq!(
            expr,
            RuleExpression::And {
                operands: vec![
                    RuleExpression::Check { index: 0 },
                    RuleExpression::Check { index: 1 },
                    RuleExpression::Check { index: 2 },
                ]
            }
        );
    }

    #[test]
    fn parse_any_match() {
        let expr = parse_expression("1 || 2 || 3", 3).unwrap();
        assert_eq!(
            expr,
            RuleExpression::Or {
                operands: vec![
                    RuleExpression::Check { index: 0 },
                    RuleExpression::Check { index: 1 },
                    RuleExpression::Check { index: 2 },
                ]
            }
        );
    }

    #[test]
    fn parse_mixed_with_parens() {
        let expr = parse_expression("(1 && 2) || !3", 3).unwrap();
        assert_eq!(
            expr,
            RuleExpression::Or {
                operands: vec![
                    RuleExpression::And {
                        operands: vec![
                            RuleExpression::Check { index: 0 },
                            RuleExpression::Check { index: 1 },
                        ]
                    },
                    RuleExpression::Not {
                        operand: Box::new(RuleExpression::Check { index: 2 }),
                    },
                ]
            }
        );
    }

    #[test]
    fn parse_nested() {
        let expr = parse_expression("(1 || 2) && (3 || !4)", 4).unwrap();
        assert_eq!(
            expr,
            RuleExpression::And {
                operands: vec![
                    RuleExpression::Or {
                        operands: vec![
                            RuleExpression::Check { index: 0 },
                            RuleExpression::Check { index: 1 },
                        ]
                    },
                    RuleExpression::Or {
                        operands: vec![
                            RuleExpression::Check { index: 2 },
                            RuleExpression::Not {
                                operand: Box::new(RuleExpression::Check { index: 3 }),
                            },
                        ]
                    },
                ]
            }
        );
    }

    #[test]
    fn parse_single_rule() {
        let expr = parse_expression("1", 1).unwrap();
        assert_eq!(expr, RuleExpression::Check { index: 0 });
    }

    #[test]
    fn parse_out_of_range() {
        assert!(parse_expression("5", 3).is_err());
        assert!(parse_expression("0", 3).is_err());
    }

    #[test]
    fn display_round_trip() {
        // AND binds tighter than OR, so (1 && 2) || !3 displays without redundant parens.
        let expr = parse_expression("(1 && 2) || !3", 3).unwrap();
        let display = expr.to_display_string();
        assert_eq!(display, "1 && 2 || !3");

        // Re-parsing the display string should produce the same tree.
        let expr2 = parse_expression(&display, 3).unwrap();
        assert_eq!(expr, expr2);
    }

    #[test]
    fn display_preserves_needed_parens() {
        // OR inside AND needs parens: 1 && (2 || 3)
        let expr = parse_expression("1 && (2 || 3)", 3).unwrap();
        let display = expr.to_display_string();
        assert_eq!(display, "1 && (2 || 3)");

        let expr2 = parse_expression(&display, 3).unwrap();
        assert_eq!(expr, expr2);
    }

    #[test]
    fn display_simple_and() {
        let expr = RuleExpression::And {
            operands: vec![
                RuleExpression::Check { index: 0 },
                RuleExpression::Check { index: 1 },
                RuleExpression::Check { index: 2 },
            ],
        };
        assert_eq!(expr.to_display_string(), "1 && 2 && 3");
    }
}
