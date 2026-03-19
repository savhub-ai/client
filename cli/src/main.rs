mod api;
mod clients;
mod config;
mod skills;
mod tui;

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command as ProcessCommand;
use std::time::{Duration, Instant};
use std::{fs, thread};

use anyhow::{Context, Result, anyhow, bail};
use api::ApiClient;
use chrono::{DateTime, Utc};
use clap::{ArgAction, Args, Parser, Subcommand};
use config::{read_global_config, write_global_config};
use dialoguer::{Confirm, Select};
use savhub_local::presets::{
    EnableProjectRepoSkillResult, ProjectSkillConflictChoice, ResolvedSkillSources,
    disable_project_skill, enable_repo_skill_in_project, read_project_added_skills,
    write_project_added_skills,
};
use savhub_local::skills::LockEntry as ProjectLockEntry;
use savhub_shared::{
    BanUserRequest, BanUserResponse, DeleteResponse, FileContentResponse, IndexRequest,
    MAX_BUNDLE_BYTES, ModerationStatus, ModerationUpdateRequest, PagedResponse, PublishBundleFile,
    PublishResponse, ResolveResponse, RoleUpdateResponse, SearchResponse, SetUserRoleRequest,
    SkillDetailResponse, SkillListItem, ToggleStarResponse, UserListResponse, UserRole,
    UserSummary, WhoAmIResponse, is_slug, normalize_bundle_files, normalize_tags,
    total_bundle_bytes,
};
use semver::Version;
use serde_json::json;
use skills::{
    LockEntry, SkillFolder, SkillOrigin, compute_fingerprint, ensure_skill_marker,
    extract_zip_to_dir, find_skill_folders, inspect_zip, list_publishable_files,
    load_local_skill_metadata, read_lockfile, read_skill_origin, sanitize_slug, write_lockfile,
    write_skill_origin,
};

const DEFAULT_SITE: &str = "https://savhub.ai";

// Transfer types (removed from savhub-shared, kept locally for CLI transfer commands)
#[derive(Debug, serde::Serialize)]
struct TransferRequest {
    to_handle: String,
    message: Option<String>,
    expires_in_hours: Option<u64>,
}

#[derive(Debug, serde::Deserialize)]
struct TransferSummary {
    skill_slug: String,
    status: String,
    to_user: UserSummary,
}

#[derive(Debug, serde::Deserialize)]
struct TransferEntry {
    id: i64,
    skill_slug: String,
    status: String,
    from_user: UserSummary,
    to_user: UserSummary,
    expires_at: DateTime<Utc>,
}

#[derive(Debug, serde::Deserialize)]
struct TransferListResponse {
    transfers: Vec<TransferEntry>,
}

#[derive(Debug, serde::Deserialize)]
struct TransferDecisionResponse {
    skill_slug: String,
}

#[derive(Debug, Parser)]
#[command(
    name = "savhub",
    version = savhub_local::build_info::VERSION_LONG,
    about = "Savhub CLI\n\nDocumentation: https://savhub.ai/docs/en/client"
)]
struct Cli {
    #[arg(long, global = true)]
    workdir: Option<PathBuf>,
    #[arg(long, global = true)]
    dir: Option<PathBuf>,
    #[arg(long, global = true)]
    site: Option<String>,
    #[arg(long, global = true)]
    registry: Option<String>,
    #[arg(long = "no-input", global = true, action = ArgAction::SetTrue)]
    no_input: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Login via GitHub OAuth
    Login(LoginArgs),
    /// Clear local auth token
    Logout,
    /// Show current authenticated user
    Whoami,
    /// Search skills in the registry
    Search(SearchArgs),
    /// Enable a skill from a local repo into a project
    Enable(EnableArgs),
    /// Disable a skill in the current project
    Disable(DisableArgs),
    /// Install a skill by cloning its source repo
    Install(InstallArgs),
    /// Update installed skill(s)
    Update(UpdateArgs),
    /// Uninstall a skill
    Uninstall(UninstallArgs),
    /// List installed skills in the current project
    List,
    /// Browse skills from the registry API
    Explore(ExploreArgs),
    /// View detailed info about a skill
    Inspect(InspectArgs),
    /// Delete a skill from the registry (admin)
    Delete(DeleteArgs),
    /// Transfer skill ownership
    Transfer {
        #[command(subcommand)]
        command: TransferCommand,
    },
    /// Star a skill
    Star(DeleteArgs),
    /// Unstar a skill
    Unstar(DeleteArgs),
    /// Manage MCP server integration with AI clients
    Mcp {
        #[command(subcommand)]
        command: McpCommand,
    },
    /// Manage the local registry cache
    Registry {
        #[command(subcommand)]
        command: RegistryCommand,
    },
    /// Manage selectors (project type detection rules)
    Selector {
        #[command(subcommand)]
        command: SelectorCommand,
    },
    /// Detect project type via selectors and apply skills to AI clients
    Apply(ApplyArgs),
    /// Manage flocks (skill collections)
    Flock {
        #[command(subcommand)]
        command: FlockCommand,
    },
    /// Manage the savhub-pilot AI skill (install/uninstall/status)
    Pilot {
        #[command(subcommand)]
        command: PilotCommand,
    },
    /// Open documentation in the browser
    Docs,
}

#[derive(Debug, Subcommand)]
enum TransferCommand {
    Request(TransferRequestArgs),
    List(TransferListArgs),
    Accept(DeleteArgs),
    Reject(DeleteArgs),
    Cancel(DeleteArgs),
}

#[derive(Debug, Subcommand)]
enum McpCommand {
    Register(McpClientArgs),
    Unregister(McpClientArgs),
    /// Generate a project-level .mcp.json for the current working directory
    Init,
    Status,
    Serve,
}

#[derive(Debug, Args)]
struct McpClientArgs {
    #[arg(long)]
    client: Option<String>,
}

#[derive(Debug, Subcommand)]
enum SelectorCommand {
    /// List all configured selectors
    List,
    /// Show details of a selector by name
    Show(SelectorShowArgs),
    /// Run all selectors against a directory and show matches (no changes)
    Test,
}

#[derive(Debug, Args)]
struct SelectorShowArgs {
    /// Selector name (partial match)
    name: String,
}

#[derive(Debug, Subcommand)]
enum RegistryCommand {
    /// Force sync the local registry cache from GitHub
    Sync,
    /// Show registry sync status
    Info,
    /// Search registry skills
    Search(RegistrySearchArgs),
    /// List registry skills with pagination
    List(RegistryListArgs),
}

#[derive(Debug, Args)]
struct RegistrySearchArgs {
    query: Vec<String>,
    #[arg(long, default_value_t = 25)]
    limit: usize,
}

#[derive(Debug, Args)]
struct RegistryListArgs {
    #[arg(long, default_value_t = 1)]
    page: usize,
    #[arg(long, default_value_t = 25)]
    page_size: usize,
    #[arg(long)]
    query: Option<String>,
    #[arg(long)]
    status: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, Subcommand)]
enum FlockCommand {
    /// List all flocks from the registry cache
    List,
    /// Show details of a flock and its skills
    Show(FlockShowArgs),
    /// Install all skills from a flock
    Install(FlockInstallArgs),
}

#[derive(Debug, Args)]
struct FlockShowArgs {
    /// Flock slug
    slug: String,
}

#[derive(Debug, Args)]
struct FlockInstallArgs {
    /// Flock slug
    slug: String,
    /// Skip confirmation prompt
    #[arg(long, action = ArgAction::SetTrue)]
    yes: bool,
}

#[derive(Debug, Subcommand)]
enum PilotCommand {
    /// Install the savhub-pilot skill into AI agent skill directories
    Install(PilotAgentArgs),
    /// Uninstall the savhub-pilot skill from AI agent skill directories
    Uninstall(PilotAgentArgs),
    /// Show installation status for each configured agent
    Status(PilotAgentArgs),
    /// Touch the config-changed signal file (useful for external tools)
    Notify,
}

#[derive(Debug, Args)]
struct PilotAgentArgs {
    /// Override which agents to target (defaults to agents from config)
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    agents: Vec<String>,
}

#[derive(Debug, Default, Args)]
struct ApplyArgs {
    /// Show what would be done without making changes
    #[arg(long = "dry-run", action = ArgAction::SetTrue)]
    dry_run: bool,
    /// Skip confirmation prompt
    #[arg(long, action = ArgAction::SetTrue)]
    yes: bool,
    /// Only sync skills to these AI agents (e.g. --agents claude-code codex)
    #[arg(long, num_args = 1.., value_delimiter = ',')]
    agents: Vec<String>,
    /// Skip syncing skills to these AI agents (e.g. --skip-agents cursor windsurf)
    #[arg(long = "skip-agents", num_args = 1.., value_delimiter = ',')]
    skip_agents: Vec<String>,
    /// Manually add skills by slug or sign (saved to savhub.toml skills.manual_added)
    #[arg(long = "skills", num_args = 1.., value_delimiter = ',')]
    add_skills: Vec<String>,
    /// Manually skip skills by slug or sign (saved to savhub.toml skills.manual_skipped)
    #[arg(long = "skip-skills", num_args = 1.., value_delimiter = ',')]
    skip_skills: Vec<String>,
    /// Manually add flocks (saved to savhub.toml flocks.manual_added)
    #[arg(long = "flocks", num_args = 1.., value_delimiter = ',')]
    add_flocks: Vec<String>,
    /// Manually skip flocks (saved to savhub.toml flocks.manual_skipped)
    #[arg(long = "skip-flocks", num_args = 1.., value_delimiter = ',')]
    skip_flocks: Vec<String>,
}

#[derive(Debug, Args)]
struct LoginArgs {
    #[arg(long, hide = true)]
    token: Option<String>,
    #[arg(long, hide = true)]
    label: Option<String>,
    #[arg(long = "no-browser", action = ArgAction::SetTrue)]
    no_browser: bool,
}

#[derive(Debug, Args)]
struct SearchArgs {
    query: Vec<String>,
    #[arg(long)]
    limit: Option<i64>,
}

#[derive(Debug, Args)]
struct EnableArgs {
    slug: String,
    #[arg(long)]
    repo: String,
    #[arg(long = "selector", alias = "detector")]
    selectors: Vec<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    use_repo: bool,
    #[arg(long, action = ArgAction::SetTrue)]
    keep_existing: bool,
}

#[derive(Debug, Args)]
struct DisableArgs {
    slug: String,
    #[arg(long, action = ArgAction::SetTrue)]
    yes: bool,
}

#[derive(Debug, Args)]
struct InstallArgs {
    slug: String,
    #[arg(long)]
    version: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    force: bool,
}

#[derive(Debug, Args)]
struct UpdateArgs {
    slug: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    all: bool,
    #[arg(short = 'g', long = "global", action = ArgAction::SetTrue)]
    global: bool,
    #[arg(long)]
    version: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    force: bool,
}

#[derive(Debug, Args)]
struct UninstallArgs {
    slug: String,
    #[arg(long, action = ArgAction::SetTrue)]
    yes: bool,
}

#[derive(Debug, Args)]
struct ExploreArgs {
    #[arg(long, default_value_t = 25)]
    limit: i64,
    #[arg(long, default_value = "newest")]
    sort: String,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, Args)]
struct InspectArgs {
    slug: String,
    #[arg(long)]
    version: Option<String>,
    #[arg(long)]
    tag: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    versions: bool,
    #[arg(long)]
    limit: Option<usize>,
    #[arg(long, action = ArgAction::SetTrue)]
    files: bool,
    #[arg(long)]
    file: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    json: bool,
}

#[derive(Debug, Args)]
struct DeleteArgs {
    slug: String,
    #[arg(long, action = ArgAction::SetTrue)]
    yes: bool,
}

#[derive(Debug, Args)]
struct TransferRequestArgs {
    slug: String,
    handle: String,
    #[arg(long)]
    message: Option<String>,
    #[arg(long, action = ArgAction::SetTrue)]
    yes: bool,
}

#[derive(Debug, Args)]
struct TransferListArgs {
    #[arg(long, action = ArgAction::SetTrue)]
    outgoing: bool,
}

// Retained for internal handler code (commands removed from CLI surface)
#[derive(Debug, Args)]
struct PublishArgs {
    path: PathBuf,
    #[arg(long)]
    slug: Option<String>,
    #[arg(long = "name")]
    display_name: Option<String>,
    #[arg(long)]
    version: Option<String>,
    #[arg(long)]
    changelog: Option<String>,
    #[arg(long, default_value = "latest")]
    tags: String,
}
#[derive(Debug, Args)]
struct SyncArgs {
    #[arg(long = "root")]
    roots: Vec<PathBuf>,
    #[arg(long, action=ArgAction::SetTrue)]
    all: bool,
    #[arg(long="dry-run", action=ArgAction::SetTrue)]
    dry_run: bool,
    #[arg(long, default_value = "patch")]
    bump: String,
    #[arg(long)]
    changelog: Option<String>,
    #[arg(long, default_value = "latest")]
    tags: String,
    #[arg(long, default_value_t = 4)]
    concurrency: usize,
}
#[derive(Debug, Args)]
struct BanUserArgs {
    handle_or_id: String,
    #[arg(long, action=ArgAction::SetTrue)]
    id: bool,
    #[arg(long, action=ArgAction::SetTrue)]
    fuzzy: bool,
    #[arg(long)]
    reason: Option<String>,
    #[arg(long, action=ArgAction::SetTrue)]
    yes: bool,
}
#[derive(Debug, Args)]
struct SetRoleArgs {
    handle_or_id: String,
    role: String,
    #[arg(long, action=ArgAction::SetTrue)]
    id: bool,
    #[arg(long, action=ArgAction::SetTrue)]
    fuzzy: bool,
    #[arg(long, action=ArgAction::SetTrue)]
    yes: bool,
}
#[allow(dead_code)]
#[derive(Debug, Clone)]
struct SyncCandidate {
    skill: SkillFolder,
    local_version: String,
    latest_version: Option<String>,
    matched_version: Option<String>,
    file_count: usize,
    status: SyncStatus,
    issue: Option<String>,
}
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncStatus {
    New,
    Update,
    Synced,
    Blocked,
}

#[derive(Debug, Clone)]
struct GlobalOpts {
    workdir: PathBuf,
    dir: PathBuf,
    registry: String,
    input_allowed: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let opts = resolve_global_opts(&cli)?;

    match cli.command {
        Some(Command::Login(args)) => cmd_login(&opts, args).await?,
        Some(Command::Logout) => cmd_logout(&opts)?,
        Some(Command::Whoami) => cmd_whoami(&opts).await?,
        Some(Command::Search(args)) => cmd_search(&opts, args).await?,
        Some(Command::Enable(args)) => cmd_enable(&opts, args)?,
        Some(Command::Disable(args)) => cmd_disable(&opts, args)?,
        Some(Command::Install(args)) => cmd_install(&opts, args).await?,
        Some(Command::Update(args)) => cmd_update(&opts, args).await?,
        Some(Command::Uninstall(args)) => cmd_uninstall(&opts, args)?,
        Some(Command::List) => cmd_list(&opts)?,
        Some(Command::Explore(args)) => cmd_explore(&opts, args).await?,
        Some(Command::Inspect(args)) => cmd_inspect(&opts, args).await?,
        Some(Command::Delete(args)) => cmd_delete(&opts, args).await?,
        Some(Command::Transfer { command }) => match command {
            TransferCommand::Request(args) => cmd_transfer_request(&opts, args).await?,
            TransferCommand::List(args) => cmd_transfer_list(&opts, args).await?,
            TransferCommand::Accept(args) => cmd_transfer_decision(&opts, args, "accept").await?,
            TransferCommand::Reject(args) => cmd_transfer_decision(&opts, args, "reject").await?,
            TransferCommand::Cancel(args) => cmd_transfer_decision(&opts, args, "cancel").await?,
        },
        Some(Command::Star(args)) => cmd_set_starred(&opts, args, true).await?,
        Some(Command::Unstar(args)) => cmd_set_starred(&opts, args, false).await?,
        Some(Command::Mcp { command }) => cmd_mcp(&opts, command)?,
        Some(Command::Registry { command }) => cmd_registry(&opts, command).await?,
        Some(Command::Selector { command }) => cmd_selector(&opts, command)?,
        Some(Command::Apply(args)) => cmd_apply(&opts, args)?,
        Some(Command::Flock { command }) => cmd_flock(&opts, command)?,
        Some(Command::Pilot { command }) => cmd_pilot(command)?,
        Some(Command::Docs) => {
            let url = "https://savhub.ai/docs/en/client";
            println!("Documentation: {url}");
            #[cfg(target_os = "windows")]
            {
                let _ = std::process::Command::new("cmd")
                    .args(["/C", "start", "", url])
                    .spawn();
            }
            #[cfg(target_os = "macos")]
            {
                let _ = std::process::Command::new("open").arg(url).spawn();
            }
            #[cfg(target_os = "linux")]
            {
                let _ = std::process::Command::new("xdg-open").arg(url).spawn();
            }
        }
        None => cmd_apply(&opts, ApplyArgs::default())?,
    }

    Ok(())
}

fn resolve_global_opts(cli: &Cli) -> Result<GlobalOpts> {
    let workdir = resolve_workdir(cli)?;
    let dir = workdir.join(
        cli.dir
            .clone()
            .or_else(|| std::env::var_os("SAVHUB_DIR").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("skills")),
    );
    let site = cli
        .site
        .clone()
        .or_else(|| std::env::var("SAVHUB_SITE").ok())
        .unwrap_or_else(|| DEFAULT_SITE.to_string());
    let cached = read_global_config()?;
    // Priority: --registry flag > env > config.toml [rest_api] override > config.toml registry >
    // site default
    let api_override = savhub_local::registry::read_api_base_url();
    let registry = cli
        .registry
        .clone()
        .or_else(|| std::env::var("SAVHUB_REGISTRY").ok())
        .or(api_override)
        .or_else(|| cached.as_ref().and_then(|config| config.registry.clone()))
        .unwrap_or_else(|| site.clone());
    Ok(GlobalOpts {
        workdir,
        dir,
        registry,
        input_allowed: !cli.no_input,
    })
}

fn resolve_workdir(cli: &Cli) -> Result<PathBuf> {
    if let Some(path) = cli.workdir.clone() {
        return Ok(path.canonicalize().unwrap_or_else(|_| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }));
    }
    if let Some(path) = std::env::var_os("SAVHUB_WORKDIR") {
        let path = PathBuf::from(path);
        return Ok(path.canonicalize().unwrap_or_else(|_| {
            std::env::current_dir()
                .unwrap_or_else(|_| PathBuf::from("."))
                .join(path)
        }));
    }
    std::env::current_dir().context("failed to resolve current directory")
}

async fn cmd_login(opts: &GlobalOpts, args: LoginArgs) -> Result<()> {
    if args.token.is_some() {
        bail!(
            "manual token login is no longer supported; run `savhub login` and complete GitHub auth in the browser"
        );
    }
    if args.label.is_some() {
        eprintln!("Ignoring --label; savhub login now uses GitHub OAuth.");
    }

    let client = ApiClient::new(&opts.registry, None);
    let listener = TcpListener::bind("127.0.0.1:0")
        .context("failed to bind a local callback port for GitHub login")?;
    let return_to = format!(
        "http://127.0.0.1:{}/callback",
        listener
            .local_addr()
            .context("failed to resolve local callback address")?
            .port()
    );
    let mut login_url = client.v1_url("/auth/github/start")?;
    login_url
        .query_pairs_mut()
        .append_pair("return_to", &return_to);

    if args.no_browser {
        println!("Open this URL in your browser to finish GitHub login:\n{login_url}");
    } else if let Err(error) = open_browser(login_url.as_str()) {
        eprintln!(
            "Failed to open a browser automatically: {error}\nOpen this URL manually:\n{login_url}"
        );
    }

    let token = wait_for_login_callback(listener)?;
    let client = ApiClient::new(&opts.registry, Some(token.clone()));
    let whoami = client.get_json::<WhoAmIResponse>("/whoami").await?;
    let Some(user) = whoami.user else {
        bail!("login failed: token is not valid");
    };
    let mut existing = read_global_config()?.unwrap_or_default();
    existing.registry = Some(opts.registry.clone());
    existing.token = Some(token);
    write_global_config(&existing)?;
    println!("Logged in as @{} via GitHub", user.handle);
    Ok(())
}

fn cmd_logout(opts: &GlobalOpts) -> Result<()> {
    let mut existing = read_global_config()?.unwrap_or_default();
    if existing.registry.is_none() {
        existing.registry = Some(opts.registry.clone());
    }
    existing.token = None;
    write_global_config(&existing)?;
    println!("Logged out locally.");
    Ok(())
}

async fn cmd_whoami(opts: &GlobalOpts) -> Result<()> {
    let client = authed_client(opts)?;
    let whoami = client.get_json::<WhoAmIResponse>("/whoami").await?;
    let Some(user) = whoami.user else {
        bail!("token is valid but no user is associated with it");
    };
    let token_name = whoami
        .token_name
        .as_deref()
        .map(|value| format!(" via {}", value))
        .unwrap_or_default();
    println!("{}{}", user.handle, token_name);
    Ok(())
}

fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        ProcessCommand::new("cmd")
            .args(["/C", "start", "", url])
            .spawn()
            .map(|_| ())
            .map_err(|error| anyhow!("failed to launch browser: {error}"))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        ProcessCommand::new("open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| anyhow!("failed to launch browser: {error}"))?;
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        ProcessCommand::new("xdg-open")
            .arg(url)
            .spawn()
            .map(|_| ())
            .map_err(|error| anyhow!("failed to launch browser: {error}"))?;
        return Ok(());
    }

    #[allow(unreachable_code)]
    Err(anyhow!(
        "automatic browser launch is not supported on this platform"
    ))
}

fn wait_for_login_callback(listener: TcpListener) -> Result<String> {
    listener
        .set_nonblocking(true)
        .context("failed to configure local callback listener")?;
    let deadline = Instant::now() + Duration::from_secs(240);

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                if let Some(token) = handle_login_callback(&mut stream)? {
                    return Ok(token);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    bail!("timed out waiting for the GitHub login callback");
                }
                thread::sleep(Duration::from_millis(150));
            }
            Err(error) => return Err(anyhow!("failed to accept login callback: {error}")),
        }
    }
}

fn handle_login_callback(stream: &mut TcpStream) -> Result<Option<String>> {
    let mut reader = BufReader::new(
        stream
            .try_clone()
            .context("failed to read the login callback stream")?,
    );
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .context("failed to read the login callback request line")?;

    let path = request_line.split_whitespace().nth(1).unwrap_or("/");
    if !path.starts_with("/callback") {
        write_callback_page(
            stream,
            "Savhub login did not recognize the callback path. You can close this window.",
            true,
        )?;
        return Ok(None);
    }

    let url = reqwest::Url::parse(&format!("http://127.0.0.1{path}"))
        .context("failed to parse the login callback URL")?;
    let mut auth_token = None;
    let mut auth_error = None;
    for (key, value) in url.query_pairs() {
        match key.as_ref() {
            "auth_token" => auth_token = Some(value.into_owned()),
            "auth_error" => auth_error = Some(value.into_owned()),
            _ => {}
        }
    }

    if let Some(error) = auth_error {
        write_callback_page(
            stream,
            "Savhub login failed. Return to the terminal for details.",
            true,
        )?;
        bail!("GitHub login failed: {error}");
    }

    if let Some(token) = auth_token {
        write_callback_page(
            stream,
            "Savhub login is complete. You can close this window.",
            false,
        )?;
        return Ok(Some(token));
    }

    write_callback_page(
        stream,
        "Savhub login is still waiting for an authentication result.",
        true,
    )?;
    Ok(None)
}

fn write_callback_page(stream: &mut TcpStream, message: &str, is_error: bool) -> Result<()> {
    let title = if is_error {
        "Login Failed"
    } else {
        "Login Complete"
    };
    let accent = if is_error { "#c0392b" } else { "#287850" };
    let body = format!(
        r##"<!doctype html><html><head><meta charset="utf-8"><title>Savhub — {title}</title>
<style>
body{{margin:0;min-height:100vh;display:flex;align-items:center;justify-content:center;font-family:'Segoe UI',system-ui,sans-serif;background:#f6efe4;color:#2d2015}}
.card{{text-align:center;background:#fff;border-radius:16px;padding:48px 40px;box-shadow:0 2px 24px rgba(0,0,0,.08);max-width:400px}}
.logo{{width:72px;height:72px;margin:0 auto 20px}}
h1{{font-size:22px;margin:0 0 8px;color:{accent}}}
p{{font-size:15px;color:#5a4e42;margin:0;line-height:1.5}}
</style></head><body><div class="card">
<svg class="logo" viewBox="0 0 1021 1021" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink"><defs><linearGradient id="g0" x1="1496" y1="1345" x2="1547" y2="1022" gradientUnits="userSpaceOnUse" gradientTransform="matrix(.751 0 0 .781 -298 -272)"><stop offset="0" stop-color="#1D1E1F"/><stop offset="1" stop-color="#4C5154"/></linearGradient><linearGradient id="g1" x1="757" y1="719" x2="756" y2="438" gradientUnits="userSpaceOnUse" gradientTransform="matrix(.751 0 0 .781 -306 -290)"><stop offset="0" stop-color="#202122"/><stop offset="1" stop-color="#4B4F53"/></linearGradient></defs><path id="a" d="m1020 262c0 153 0 83 1 337-15-34-30-57-57-83C912 471 859 452 725 442 624 338 636 342 474 289c3-77 12-147 68-205 48-50 114-79 184-81 76-3 150 24 206 74 54 48 86 115 87 185z" style="stroke-width:.766"/><use href="#a" fill="#287850"/><use href="#a" transform="rotate(90 511 511)" fill="#0a0a0a"/><use href="#a" transform="rotate(180 511 511)" fill="#287850"/><use href="#a" transform="rotate(-90 510 512)" fill="#0a0a0a"/><path fill="url(#g0)" d="m773 544c18-18 44-29 69-28 30 1 58 17 78 40 19 21 32 47 36 75 4 10 3 36 1 46-5 33-22 63-48 82-51 39-118 26-155-27-21-31-29-69-23-106 5-28 18-65 42-82z" style="stroke-width:.766"/><path fill="url(#g1)" d="m116 163c0-4-1-8-1-13C121 21 298-9 375 70c17 17 24 32 31 55 6 31 1 57-16 84-48 76-160 83-228 34-23-17-41-43-46-72 0-2-1-5-1-7z" style="stroke-width:.766"/><circle cx="510" cy="510" r="232" fill="#fff"/></svg>
<h1>{title}</h1><p>{message}</p></div></body></html>"##
    );
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    stream
        .write_all(response.as_bytes())
        .context("failed to write the login callback response")?;
    stream.flush().ok();
    Ok(())
}

async fn cmd_search(opts: &GlobalOpts, args: SearchArgs) -> Result<()> {
    let query = args.query.join(" ").trim().to_string();
    if query.is_empty() {
        bail!("query required");
    }
    let client = optional_client(opts)?;
    let mut url = client.v1_url("/search")?;
    url.query_pairs_mut()
        .append_pair("q", &query)
        .append_pair("kind", "skill");
    if let Some(limit) = args.limit {
        url.query_pairs_mut()
            .append_pair("limit", &limit.to_string());
    }
    let response = client.get_json_url::<SearchResponse>(url).await?;
    if response.results.is_empty() {
        println!("No results.");
        return Ok(());
    }
    for entry in response.results {
        let version = entry
            .latest_version
            .as_deref()
            .map(|value| format!(" v{value}"))
            .unwrap_or_default();
        let owner = entry
            .owner_handle
            .as_deref()
            .map(|handle| format!("  @{handle}"))
            .unwrap_or_default();
        println!(
            "{}{}  {}{}  ({:.3})",
            entry.slug, version, entry.display_name, owner, entry.score
        );
    }
    Ok(())
}

fn cmd_enable(opts: &GlobalOpts, args: EnableArgs) -> Result<()> {
    if args.use_repo && args.keep_existing {
        bail!("use only one of --use-repo or --keep-existing");
    }

    let sources = ResolvedSkillSources {
        selectors: args.selectors.clone(),
        flocks: Vec::new(),
        manual: true,
    };
    let initial_choice = if args.use_repo {
        ProjectSkillConflictChoice::UseRepo
    } else if args.keep_existing {
        ProjectSkillConflictChoice::KeepExisting
    } else {
        ProjectSkillConflictChoice::Ask
    };

    match enable_repo_skill_in_project(
        &opts.workdir,
        &args.repo,
        &args.slug,
        initial_choice,
        sources.clone(),
    )? {
        EnableProjectRepoSkillResult::Enabled {
            slug,
            overwritten,
            version,
            git_commit,
        } => {
            let revision = version
                .map(|value| format!("v{value}"))
                .or(git_commit.map(|value| value.chars().take(12).collect::<String>()))
                .unwrap_or_else(|| "latest".to_string());
            if overwritten {
                println!(
                    "Enabled {slug} from repo '{}' and replaced the existing project skill ({revision}).",
                    args.repo
                );
            } else {
                println!("Enabled {slug} from repo '{}' ({revision}).", args.repo);
            }
        }
        EnableProjectRepoSkillResult::KeptExisting { slug } => {
            println!("Kept the existing project skill for {slug}.");
        }
        EnableProjectRepoSkillResult::Conflict(conflict) => {
            if !opts.input_allowed {
                bail!(
                    "skill '{}' already exists at {}. Re-run with --use-repo or --keep-existing.",
                    conflict.slug,
                    conflict.existing_skill_path.display()
                );
            }

            let selection = Select::new()
                .with_prompt(format!(
                    "Skill '{}' already exists. Which one should be used?",
                    conflict.slug
                ))
                .items([
                    format!("Use repo skill from {}", conflict.repo_skill_path.display()),
                    format!(
                        "Keep existing project skill at {}",
                        conflict.existing_skill_path.display()
                    ),
                ])
                .default(0)
                .interact()
                .map_err(|error| anyhow!("failed to read selection: {error}"))?;

            let chosen = if selection == 0 {
                ProjectSkillConflictChoice::UseRepo
            } else {
                ProjectSkillConflictChoice::KeepExisting
            };

            match enable_repo_skill_in_project(
                &opts.workdir,
                &args.repo,
                &args.slug,
                chosen,
                sources,
            )? {
                EnableProjectRepoSkillResult::Enabled {
                    slug, overwritten, ..
                } => {
                    if overwritten {
                        println!(
                            "Enabled {slug} from repo '{}' and replaced the existing project skill.",
                            args.repo
                        );
                    } else {
                        println!("Enabled {slug} from repo '{}'.", args.repo);
                    }
                }
                EnableProjectRepoSkillResult::KeptExisting { slug } => {
                    println!("Kept the existing project skill for {slug}.");
                }
                EnableProjectRepoSkillResult::Conflict(_) => {
                    bail!("unexpected conflict state while enabling project skill");
                }
            }
        }
    }

    Ok(())
}

fn cmd_disable(opts: &GlobalOpts, args: DisableArgs) -> Result<()> {
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("Disable {}?", args.slug),
            "pass --yes when input is disabled",
        )?;
    }

    let slug = normalize_slug(&args.slug)?;
    if disable_project_skill(&opts.workdir, &slug)? {
        println!("Disabled {slug}");
        Ok(())
    } else {
        bail!("{slug} is not enabled in this project")
    }
}

async fn cmd_install(opts: &GlobalOpts, args: InstallArgs) -> Result<()> {
    let slug = normalize_slug(&args.slug)?;
    let target = opts.dir.join(&slug);
    if target.exists() {
        if !args.force {
            bail!(
                "{} already exists; use --force to overwrite",
                target.display()
            );
        }
        fs::remove_dir_all(&target)
            .with_context(|| format!("failed to remove {}", target.display()))?;
    }

    let client = optional_client(opts)?;
    let detail = client
        .get_json::<SkillDetailResponse>(&format!("/skills/{slug}"))
        .await?;
    let version = args
        .version
        .or_else(|| {
            detail
                .latest_version
                .as_ref()
                .map(|value| value.version.clone())
        })
        .ok_or_else(|| anyhow!("could not resolve a version for {slug}"))?;
    let bytes = download_skill_bundle(&client, &slug, Some(&version), None).await?;
    extract_zip_to_dir(&bytes, &target)?;

    let now = now_millis();
    write_skill_origin(
        &target,
        &SkillOrigin {
            version: 1,
            registry: opts.registry.clone(),
            slug: slug.clone(),
            installed_version: version.clone(),
            installed_at: now,
        },
    )?;
    let mut lockfile = read_project_added_skills(&opts.workdir)?;
    lockfile.skills.insert(
        slug.clone(),
        ProjectLockEntry {
            version: version.clone(),
            installed_at: now,
        },
    );
    write_project_added_skills(&opts.workdir, &lockfile)?;
    println!("Installed {slug}@{version} -> {}", target.display());
    Ok(())
}

async fn cmd_update(opts: &GlobalOpts, args: UpdateArgs) -> Result<()> {
    // Global mode: update global skills and sync to all AI clients
    if args.global {
        return cmd_update_global(opts, &args).await;
    }
    if args.slug.is_none() && !args.all {
        bail!("provide <slug>, --all, or -g/--global");
    }
    if args.slug.is_some() && args.all {
        bail!("use either <slug> or --all");
    }
    if let Some(version) = &args.version {
        Version::parse(version).with_context(|| format!("invalid semver: {version}"))?;
    }

    let mut lockfile = read_project_added_skills(&opts.workdir)?;
    let slugs = if let Some(slug) = args.slug {
        vec![normalize_slug(&slug)?]
    } else {
        lockfile.skills.keys().cloned().collect::<Vec<_>>()
    };
    if slugs.is_empty() {
        println!("No installed skills.");
        return Ok(());
    }

    let client = optional_client(opts)?;
    let now = now_millis();
    for slug in slugs {
        let target = opts.dir.join(&slug);
        let local_files = if target.is_dir() {
            list_publishable_files(&target)?
        } else {
            Vec::new()
        };
        let local_fingerprint =
            (!local_files.is_empty()).then(|| compute_fingerprint(&local_files));

        let resolved = if let Some(fingerprint) = local_fingerprint.as_deref() {
            resolve_skill_version(&client, &slug, fingerprint).await?
        } else {
            ResolveResponse {
                slug: slug.clone(),
                matched: None,
                latest_version: None,
            }
        };
        let latest = match args.version.clone().or_else(|| {
            resolved
                .latest_version
                .as_ref()
                .map(|entry| entry.version.clone())
        }) {
            Some(version) => version,
            None => {
                let detail = client
                    .get_json::<SkillDetailResponse>(&format!("/skills/{slug}"))
                    .await?;
                detail
                    .latest_version
                    .map(|value| value.version)
                    .ok_or_else(|| anyhow!("could not determine latest version for {slug}"))?
            }
        };

        if resolved
            .matched
            .as_ref()
            .map(|entry| entry.version.as_str() == latest.as_str())
            .unwrap_or(false)
        {
            println!("{slug}: already at {latest}");
            lockfile.skills.insert(
                slug.clone(),
                ProjectLockEntry {
                    version: latest.clone(),
                    installed_at: lockfile
                        .skills
                        .get(&slug)
                        .map(|entry| entry.installed_at)
                        .unwrap_or(now),
                },
            );
            continue;
        }

        if local_fingerprint.is_some() && resolved.matched.is_none() && !args.force {
            if !opts.input_allowed {
                println!("{slug}: local changes detected, skipped (use --force)");
                continue;
            }
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "{slug}: local files do not match a published version. Overwrite with {latest}?"
                ))
                .default(false)
                .interact()
                .map_err(|error| anyhow!("failed to read confirmation: {error}"))?;
            if !confirmed {
                println!("{slug}: skipped");
                continue;
            }
        }

        if target.exists() {
            fs::remove_dir_all(&target)
                .with_context(|| format!("failed to remove {}", target.display()))?;
        }
        let bytes = download_skill_bundle(&client, &slug, Some(&latest), None).await?;
        extract_zip_to_dir(&bytes, &target)?;
        let existing_origin = read_skill_origin(&target)?;
        write_skill_origin(
            &target,
            &SkillOrigin {
                version: 1,
                registry: existing_origin
                    .as_ref()
                    .map(|origin| origin.registry.clone())
                    .unwrap_or_else(|| opts.registry.clone()),
                slug: existing_origin
                    .as_ref()
                    .map(|origin| origin.slug.clone())
                    .unwrap_or_else(|| slug.clone()),
                installed_version: latest.clone(),
                installed_at: now,
            },
        )?;
        lockfile.skills.insert(
            slug.clone(),
            ProjectLockEntry {
                version: latest.clone(),
                installed_at: now,
            },
        );
        println!("{slug}: updated -> {latest}");
    }

    write_project_added_skills(&opts.workdir, &lockfile)?;
    Ok(())
}

async fn cmd_update_global(opts: &GlobalOpts, args: &UpdateArgs) -> Result<()> {
    let global_dir = clients::global_skills_dir();
    fs::create_dir_all(&global_dir).with_context(|| {
        format!(
            "failed to create global skills dir: {}",
            global_dir.display()
        )
    })?;
    println!("Global skills directory: {}", global_dir.display());

    // Update all skills in global dir using same logic as regular update
    let mut lockfile = read_lockfile(&global_dir)?;
    let slugs: Vec<String> = if let Some(slug) = &args.slug {
        vec![normalize_slug(slug)?]
    } else {
        lockfile.skills.keys().cloned().collect()
    };

    if slugs.is_empty() {
        println!("No global skills installed. Install skills first with `savhub install <slug>`.");
    } else {
        let client = optional_client(opts)?;
        let now = now_millis();
        for slug in &slugs {
            let target = global_dir.join(slug);
            let resolved = if target.is_dir() {
                let local_files = list_publishable_files(&target)?;
                let fingerprint = compute_fingerprint(&local_files);
                resolve_skill_version(&client, slug, &fingerprint).await?
            } else {
                ResolveResponse {
                    slug: slug.clone(),
                    matched: None,
                    latest_version: None,
                }
            };
            let latest = match args
                .version
                .clone()
                .or_else(|| resolved.latest_version.as_ref().map(|e| e.version.clone()))
            {
                Some(v) => v,
                None => {
                    let detail = client
                        .get_json::<SkillDetailResponse>(&format!("/skills/{slug}"))
                        .await?;
                    detail
                        .latest_version
                        .map(|v| v.version)
                        .ok_or_else(|| anyhow!("no latest version for {slug}"))?
                }
            };

            if resolved
                .matched
                .as_ref()
                .is_some_and(|e| e.version == latest)
            {
                println!("{slug}: already at {latest}");
                continue;
            }

            if target.exists() {
                fs::remove_dir_all(&target)?;
            }
            let bytes = download_skill_bundle(&client, slug, Some(&latest), None).await?;
            extract_zip_to_dir(&bytes, &target)?;
            lockfile.skills.insert(
                slug.clone(),
                LockEntry {
                    version: latest.clone(),
                    installed_at: now,
                },
            );
            println!("{slug}: updated -> {latest}");
        }
        write_lockfile(&global_dir, &lockfile)?;
    }

    // Sync to all detected AI clients
    println!("\nSyncing to AI clients...");
    let detected = clients::detect_clients();
    let installed: Vec<_> = detected.iter().filter(|c| c.installed).collect();
    if installed.is_empty() {
        println!("No AI clients detected.");
    } else {
        for client in &installed {
            match clients::sync_skills_to_client(client, &global_dir) {
                Ok(count) => println!(
                    "  {}: synced {} skill(s) -> {}",
                    client.name,
                    count,
                    client.skills_dir.display()
                ),
                Err(e) => println!("  {}: error - {e}", client.name),
            }
        }
    }

    Ok(())
}

fn cmd_uninstall(opts: &GlobalOpts, args: UninstallArgs) -> Result<()> {
    let slug = normalize_slug(&args.slug)?;
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("Uninstall {slug}?"),
            "pass --yes when input is disabled",
        )?;
    }
    if !disable_project_skill(&opts.workdir, &slug)? {
        bail!("{slug} is not installed");
    }
    println!("Uninstalled {slug}");
    Ok(())
}

fn cmd_list(opts: &GlobalOpts) -> Result<()> {
    let lockfile = read_project_added_skills(&opts.workdir)?;
    if lockfile.skills.is_empty() {
        println!("No installed skills.");
        return Ok(());
    }
    for (slug, entry) in lockfile.skills {
        println!("{slug}  {}", entry.version);
    }
    Ok(())
}

async fn cmd_explore(opts: &GlobalOpts, args: ExploreArgs) -> Result<()> {
    let client = optional_client(opts)?;
    let mut url = client.v1_url("/skills")?;
    let sort = map_explore_sort(&args.sort);
    url.query_pairs_mut()
        .append_pair("limit", &args.limit.clamp(1, 100).to_string());
    if sort != "updated" {
        url.query_pairs_mut().append_pair("sort", sort);
    }
    let response = client
        .get_json_url::<PagedResponse<SkillListItem>>(url)
        .await?;
    if args.json {
        println!("{}", serde_json::to_string_pretty(&response)?);
        return Ok(());
    }
    if response.items.is_empty() {
        println!("No skills found.");
        return Ok(());
    }
    for item in response.items {
        let version = item
            .latest_version
            .as_ref()
            .map(|value| value.version.as_str())
            .unwrap_or("?");
        let age = relative_time(item.updated_at);
        let summary = item
            .summary
            .as_deref()
            .map(|summary| format!("  {}", truncate(summary, 64)))
            .unwrap_or_default();
        println!("{}  v{}  {}{}", item.slug, version, age, summary);
    }
    Ok(())
}

async fn cmd_inspect(opts: &GlobalOpts, args: InspectArgs) -> Result<()> {
    if args.version.is_some() && args.tag.is_some() {
        bail!("use either --version or --tag");
    }
    let slug = normalize_slug(&args.slug)?;
    let client = optional_client(opts)?;
    let detail = client
        .get_json::<SkillDetailResponse>(&format!("/skills/{slug}"))
        .await?;
    let selected_version =
        resolve_selected_version(&detail, args.version.as_deref(), args.tag.as_deref())?;

    let file_payload = if let Some(path) = args.file.as_deref() {
        let mut url = client.v1_url(&format!("/skills/{slug}/file"))?;
        url.query_pairs_mut().append_pair("path", path);
        if let Some(version) = selected_version.as_deref() {
            url.query_pairs_mut().append_pair("version", version);
        }
        Some(client.get_json_url::<FileContentResponse>(url).await?)
    } else {
        None
    };

    let selected_files = if args.files {
        match (
            selected_version.as_deref(),
            detail
                .latest_version
                .as_ref()
                .map(|value| value.version.as_str()),
        ) {
            (None, _) => Some(latest_files_json(&detail)),
            (Some(requested), Some(current)) if requested == current => {
                Some(latest_files_json(&detail))
            }
            (Some(requested), _) => {
                let bytes = download_skill_bundle(&client, &slug, Some(requested), None).await?;
                Some(
                    inspect_zip(&bytes)?
                        .into_iter()
                        .map(|file| {
                            json!({
                                "path": file.path,
                                "size": file.size,
                                "sha256": file.sha256,
                            })
                        })
                        .collect::<Vec<_>>(),
                )
            }
        }
    } else {
        None
    };

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "detail": detail,
                "selected_version": selected_version,
                "file": file_payload,
                "files": selected_files,
            }))?
        );
        return Ok(());
    }

    println!("{}  {}", detail.skill.slug, detail.skill.display_name);
    if let Some(summary) = detail.skill.summary.as_deref() {
        println!("Summary: {summary}");
    }
    println!("Owner: @{}", detail.skill.owner.handle);
    println!(
        "Latest: {}",
        detail
            .latest_version
            .as_ref()
            .map(|value| value.version.as_str())
            .unwrap_or("?")
    );
    println!(
        "Stats: {} downloads, {} stars, {} installs, {} users, {} versions, {} comments",
        detail.skill.stats.downloads,
        detail.skill.stats.stars,
        detail.skill.stats.installs,
        detail.skill.stats.unique_users,
        detail.skill.stats.versions,
        detail.skill.stats.comments
    );
    println!("Moderation: {:?}", detail.skill.moderation_status);
    if !detail.skill.tags.is_empty() {
        println!(
            "Tags: {}",
            detail
                .skill
                .tags
                .iter()
                .map(|(tag, version)| format!("{tag}={version}"))
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
    if let Some(version) = selected_version.as_deref() {
        println!("Selected: {version}");
    }

    if args.versions {
        let limit = args.limit.unwrap_or(25);
        println!("Versions:");
        for entry in detail.versions.iter().take(limit) {
            println!(
                "  {}  {}  {}",
                entry.version,
                entry.created_at.to_rfc3339(),
                truncate(&entry.changelog, 80)
            );
        }
    }

    if let Some(files) = selected_files {
        if files.is_empty() {
            println!("Files: none");
        } else {
            println!("Files:");
            for file in files {
                println!(
                    "  {}  {}  {}",
                    file.get("path")
                        .and_then(|value| value.as_str())
                        .unwrap_or("?"),
                    file.get("size")
                        .and_then(|value| value.as_i64())
                        .unwrap_or_default(),
                    file.get("sha256")
                        .and_then(|value| value.as_str())
                        .unwrap_or("?")
                );
            }
        }
    }

    if let Some(file) = file_payload {
        println!();
        println!("{}:", file.path);
        print!("{}", file.content);
        if !file.content.ends_with('\n') {
            println!();
        }
    }

    Ok(())
}

#[allow(dead_code)]
async fn cmd_publish(opts: &GlobalOpts, args: PublishArgs) -> Result<()> {
    publish_folder(
        opts,
        &resolve_folder(&opts.workdir, &args.path)?,
        args.slug.as_deref(),
        args.display_name.as_deref(),
        args.version.as_deref(),
        args.changelog
            .as_deref()
            .unwrap_or("Published via savhub CLI."),
        &args.tags,
    )
    .await
}

async fn cmd_delete(opts: &GlobalOpts, args: DeleteArgs) -> Result<()> {
    let slug = normalize_slug(&args.slug)?;
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("Delete {slug}?"),
            "pass --yes when input is disabled",
        )?;
    }
    let client = authed_client(opts)?;
    client
        .delete_json::<DeleteResponse>(&format!("/skills/{slug}"))
        .await?;
    println!("Deleted {slug}");
    Ok(())
}

#[allow(dead_code)]
async fn cmd_hide(opts: &GlobalOpts, args: DeleteArgs) -> Result<()> {
    moderate_skill(opts, &args, ModerationStatus::Hidden, "Hidden").await
}

#[allow(dead_code)]
async fn cmd_undelete(opts: &GlobalOpts, args: DeleteArgs) -> Result<()> {
    let slug = normalize_slug(&args.slug)?;
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("Restore {slug}?"),
            "pass --yes when input is disabled",
        )?;
    }
    let client = authed_client(opts)?;
    client
        .post_empty::<DeleteResponse>(&format!("/skills/{slug}/restore"))
        .await?;
    println!("Restored {slug}");
    Ok(())
}

#[allow(dead_code)]
async fn cmd_unhide(opts: &GlobalOpts, args: DeleteArgs) -> Result<()> {
    moderate_skill(opts, &args, ModerationStatus::Active, "Unhidden").await
}

async fn cmd_transfer_request(opts: &GlobalOpts, args: TransferRequestArgs) -> Result<()> {
    let slug = normalize_slug(&args.slug)?;
    let handle = args.handle.trim().trim_start_matches('@').to_lowercase();
    if handle.is_empty() {
        bail!("recipient handle required");
    }
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("Transfer {slug} to @{handle}?"),
            "pass --yes when input is disabled",
        )?;
    }
    let client = authed_client(opts)?;
    let result = client
        .post_json::<_, TransferSummary>(
            &format!("/skills/{slug}/transfer"),
            &TransferRequest {
                to_handle: handle.clone(),
                message: args.message,
                expires_in_hours: None,
            },
        )
        .await?;
    println!(
        "Transfer requested for {} -> @{} ({:?})",
        result.skill_slug, result.to_user.handle, result.status
    );
    Ok(())
}

async fn cmd_transfer_list(opts: &GlobalOpts, args: TransferListArgs) -> Result<()> {
    let client = authed_client(opts)?;
    let path = if args.outgoing {
        "/transfers/outgoing"
    } else {
        "/transfers/incoming"
    };
    let result = client.get_json::<TransferListResponse>(path).await?;
    if result.transfers.is_empty() {
        println!(
            "{}",
            if args.outgoing {
                "No outgoing transfers."
            } else {
                "No incoming transfers."
            }
        );
        return Ok(());
    }
    for transfer in result.transfers {
        let other = if args.outgoing {
            &transfer.to_user.handle
        } else {
            &transfer.from_user.handle
        };
        println!(
            "{}  {:?}  @{}  expires {}",
            transfer.skill_slug,
            transfer.status,
            other,
            transfer.expires_at.to_rfc3339()
        );
    }
    Ok(())
}

async fn cmd_transfer_decision(opts: &GlobalOpts, args: DeleteArgs, action: &str) -> Result<()> {
    let slug = normalize_slug(&args.slug)?;
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("{} transfer for {slug}?", capitalize(action)),
            "pass --yes when input is disabled",
        )?;
    }
    let client = authed_client(opts)?;
    let transfers = match action {
        "cancel" => {
            client
                .get_json::<TransferListResponse>("/transfers/outgoing")
                .await?
        }
        _ => {
            client
                .get_json::<TransferListResponse>("/transfers/incoming")
                .await?
        }
    };
    let transfer = transfers
        .transfers
        .into_iter()
        .find(|transfer| transfer.skill_slug == slug)
        .ok_or_else(|| anyhow!("no matching transfer found for {slug}"))?;
    let response = client
        .post_empty::<TransferDecisionResponse>(&format!("/transfers/{}/{}", transfer.id, action))
        .await?;
    println!("{} {}", capitalize(action), response.skill_slug);
    Ok(())
}

async fn cmd_set_starred(opts: &GlobalOpts, args: DeleteArgs, desired: bool) -> Result<()> {
    let slug = normalize_slug(&args.slug)?;
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("{} {slug}?", if desired { "Star" } else { "Unstar" }),
            "pass --yes when input is disabled",
        )?;
    }
    let client = authed_client(opts)?;
    let detail = client
        .get_json::<SkillDetailResponse>(&format!("/skills/{slug}"))
        .await?;
    if detail.starred == desired {
        println!(
            "{slug} is already {}",
            if desired { "starred" } else { "not starred" }
        );
        return Ok(());
    }
    let result = client
        .post_empty::<ToggleStarResponse>(&format!("/skills/{slug}/star"))
        .await?;
    println!(
        "{} {} ({} stars)",
        if result.starred {
            "Starred"
        } else {
            "Unstarred"
        },
        slug,
        result.stars
    );
    Ok(())
}

#[allow(dead_code)]
async fn cmd_sync(opts: &GlobalOpts, args: SyncArgs) -> Result<()> {
    let bump = normalize_bump(&args.bump)?;
    let _concurrency = args.concurrency.clamp(1, 32);
    let client = authed_client(opts)?;
    let roots = build_scan_roots(opts, &args.roots);
    let mut by_slug = BTreeMap::<String, SkillFolder>::new();
    for root in roots {
        for skill in find_skill_folders(&root)? {
            by_slug.entry(skill.slug.clone()).or_insert(skill);
        }
    }
    if by_slug.is_empty() {
        println!("No local skills found.");
        return Ok(());
    }

    let mut candidates = Vec::new();
    for skill in by_slug.into_values() {
        let files = list_publishable_files(&skill.folder)?;
        ensure_skill_marker(&files)?;
        let metadata = match load_local_skill_metadata(&files) {
            Ok(Some(metadata)) => metadata,
            Ok(None) => {
                candidates.push(SyncCandidate {
                    skill,
                    local_version: String::new(),
                    latest_version: None,
                    matched_version: None,
                    file_count: files.len(),
                    status: SyncStatus::Blocked,
                    issue: Some("_meta.toml is required for sync".to_string()),
                });
                continue;
            }
            Err(error) => {
                candidates.push(SyncCandidate {
                    skill,
                    local_version: String::new(),
                    latest_version: None,
                    matched_version: None,
                    file_count: files.len(),
                    status: SyncStatus::Blocked,
                    issue: Some(error.to_string()),
                });
                continue;
            }
        };
        let fingerprint = compute_fingerprint(&files);
        let resolved = match resolve_skill_version(&client, &skill.slug, &fingerprint).await {
            Ok(resolved) => resolved,
            Err(error) if error.to_string().contains("404") => ResolveResponse {
                slug: skill.slug.clone(),
                matched: None,
                latest_version: None,
            },
            Err(error) => return Err(error),
        };
        let latest_version = resolved.latest_version.map(|entry| entry.version);
        let matched_version = resolved.matched.map(|entry| entry.version);
        let local_version = metadata.package.version.clone();
        let (status, issue) = if latest_version.is_none() {
            (SyncStatus::New, None)
        } else if matched_version.is_some() {
            (SyncStatus::Synced, None)
        } else if let Some(latest) = latest_version.as_deref() {
            let expected = bump_version(latest, bump)?;
            if local_version == expected {
                (SyncStatus::Update, None)
            } else if local_version == latest {
                (
                    SyncStatus::Blocked,
                    Some(format!(
                        "local files changed but _meta.toml version is still {latest}; expected {expected}"
                    )),
                )
            } else {
                let local = Version::parse(&local_version)
                    .with_context(|| format!("invalid local version: {local_version}"))?;
                let remote = Version::parse(latest)
                    .with_context(|| format!("invalid remote version: {latest}"))?;
                if local <= remote {
                    (
                        SyncStatus::Blocked,
                        Some(format!(
                            "local _meta.toml version {local_version} must be newer than remote {latest}"
                        )),
                    )
                } else {
                    (
                        SyncStatus::Blocked,
                        Some(format!(
                            "local _meta.toml version {local_version} does not match expected {expected} for --bump {bump}"
                        )),
                    )
                }
            }
        } else {
            (
                SyncStatus::Blocked,
                Some("failed to resolve remote version state".to_string()),
            )
        };
        candidates.push(SyncCandidate {
            skill,
            local_version,
            latest_version,
            matched_version,
            file_count: files.len(),
            status,
            issue,
        });
    }

    let blocked = candidates
        .iter()
        .filter(|candidate| candidate.status == SyncStatus::Blocked)
        .cloned()
        .collect::<Vec<_>>();
    if !blocked.is_empty() {
        println!("Blocked:");
        for candidate in &blocked {
            println!(
                "  {}  {}",
                candidate.skill.slug,
                candidate
                    .issue
                    .as_deref()
                    .unwrap_or("invalid local metadata")
            );
        }
    }

    let actionable = candidates
        .iter()
        .filter(|candidate| matches!(candidate.status, SyncStatus::New | SyncStatus::Update))
        .cloned()
        .collect::<Vec<_>>();
    if actionable.is_empty() {
        println!(
            "{}",
            if blocked.is_empty() {
                "Nothing to sync."
            } else {
                "Nothing eligible to sync."
            }
        );
        return Ok(());
    }

    println!("To sync:");
    for candidate in &actionable {
        println!(
            "  {}  {}  (v{} · {} files)",
            candidate.skill.slug,
            sync_status_label(candidate, bump),
            candidate.local_version,
            candidate.file_count
        );
    }

    if args.dry_run {
        println!("Dry run: would upload {} skill(s).", actionable.len());
        return Ok(());
    }

    let selected = if args.all || !opts.input_allowed {
        actionable
    } else {
        let mut selected = Vec::new();
        for candidate in actionable {
            let confirmed = Confirm::new()
                .with_prompt(format!(
                    "Upload {} ({})?",
                    candidate.skill.slug,
                    sync_status_label(&candidate, bump)
                ))
                .default(true)
                .interact()
                .map_err(|error| anyhow!("failed to read confirmation: {error}"))?;
            if confirmed {
                selected.push(candidate);
            }
        }
        selected
    };

    if selected.is_empty() {
        println!("Nothing selected.");
        return Ok(());
    }

    let mut uploaded = 0usize;
    for candidate in selected {
        let changelog = args.changelog.clone().unwrap_or_else(|| {
            if candidate.status == SyncStatus::New {
                "Initial sync import.".to_string()
            } else {
                "Sync update.".to_string()
            }
        });
        publish_folder(
            opts,
            &candidate.skill.folder,
            Some(&candidate.skill.slug),
            Some(&candidate.skill.display_name),
            Some(&candidate.local_version),
            &changelog,
            &args.tags,
        )
        .await?;
        uploaded += 1;
    }

    println!("Uploaded {uploaded} skill(s).");
    Ok(())
}

#[allow(dead_code)]
async fn cmd_ban_user(opts: &GlobalOpts, args: BanUserArgs) -> Result<()> {
    let client = authed_client(opts)?;
    let user_id = resolve_user_id(&client, &args.handle_or_id, args.id, args.fuzzy).await?;
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("Ban user {}?", args.handle_or_id),
            "pass --yes when input is disabled",
        )?;
    }
    let response = client
        .post_json::<_, BanUserResponse>(
            &format!("/management/users/{user_id}/ban"),
            &BanUserRequest {
                reason: args.reason,
            },
        )
        .await?;
    println!(
        "Banned @{} (revoked {}, deleted {} skills, {} souls)",
        response.user.handle,
        response.revoked_tokens,
        response.deleted_skills,
        response.deleted_skills
    );
    Ok(())
}

#[allow(dead_code)]
async fn cmd_set_role(opts: &GlobalOpts, args: SetRoleArgs) -> Result<()> {
    let role = parse_role_arg(&args.role)?;
    let client = authed_client(opts)?;
    let user_id = resolve_user_id(&client, &args.handle_or_id, args.id, args.fuzzy).await?;
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("Set role for {} to {:?}?", args.handle_or_id, role),
            "pass --yes when input is disabled",
        )?;
    }
    let response = client
        .post_json::<_, RoleUpdateResponse>(
            &format!("/management/users/{user_id}/role"),
            &SetUserRoleRequest { role },
        )
        .await?;
    println!(
        "Updated @{} -> {:?}",
        response.user.handle, response.user.role
    );
    Ok(())
}

#[allow(dead_code)]
async fn moderate_skill(
    opts: &GlobalOpts,
    args: &DeleteArgs,
    status: ModerationStatus,
    verb: &str,
) -> Result<()> {
    let slug = normalize_slug(&args.slug)?;
    if !args.yes {
        ensure_confirmed(
            opts.input_allowed,
            &format!("{verb} {slug}?"),
            "pass --yes when input is disabled",
        )?;
    }
    let client = authed_client(opts)?;
    client
        .post_json::<_, SkillDetailResponse>(
            &format!("/skills/{slug}/moderation"),
            &ModerationUpdateRequest {
                status,
                highlighted: None,
                official: None,
                deprecated: None,
                suspicious: None,
                notes: None,
            },
        )
        .await?;
    println!("{verb} {slug}");
    Ok(())
}

#[allow(dead_code)]
async fn publish_folder(
    opts: &GlobalOpts,
    folder: &Path,
    slug_arg: Option<&str>,
    display_name_arg: Option<&str>,
    version_arg: Option<&str>,
    changelog: &str,
    tags: &str,
) -> Result<()> {
    let client = authed_client(opts)?;
    let files = list_publishable_files(folder)?;
    if files.is_empty() {
        bail!("no publishable text files found in {}", folder.display());
    }
    ensure_skill_marker(&files)?;
    let metadata = load_local_skill_metadata(&files)?
        .ok_or_else(|| anyhow!("_meta.toml is required for publishing {}", folder.display()))?;

    let slug = metadata.package.slug.clone();
    if !is_slug(&slug) {
        bail!(
            "invalid package.slug in _meta.toml: {}",
            metadata.package.slug
        );
    }
    if let Some(slug_arg) = slug_arg {
        let requested = sanitize_slug(slug_arg);
        if !requested.is_empty() && requested != slug {
            bail!("--slug does not match _meta.toml package.slug ({slug})");
        }
    }

    let display_name = metadata.package.name.clone();
    if let Some(display_name_arg) = display_name_arg {
        let requested = display_name_arg.trim();
        if !requested.is_empty() && requested != display_name {
            bail!("--name does not match _meta.toml package.name ({display_name})");
        }
    }

    let version = metadata.package.version.clone();
    Version::parse(&version).with_context(|| format!("invalid semver: {version}"))?;
    if let Some(version_arg) = version_arg {
        let requested = version_arg.trim();
        if !requested.is_empty() && requested != version {
            bail!("--version does not match _meta.toml package.version ({version})");
        }
    }

    let tags = normalize_tags(
        &tags
            .split(',')
            .map(|tag| tag.trim().to_string())
            .collect::<Vec<_>>(),
    );
    let files = normalize_bundle_files(
        &files
            .into_iter()
            .map(|file| PublishBundleFile {
                path: file.path,
                content: file.content,
            })
            .collect::<Vec<_>>(),
    )
    .map_err(|error| anyhow!(error))?;
    if total_bundle_bytes(&files) > MAX_BUNDLE_BYTES {
        bail!(
            "bundle exceeds the {}MB upload limit",
            MAX_BUNDLE_BYTES / 1024 / 1024
        );
    }

    let publish_files: Vec<PublishBundleFile> = files
        .into_iter()
        .map(|f| PublishBundleFile {
            path: f.path,
            content: f.content,
        })
        .collect();
    let request = IndexRequest {
        slug: slug.clone(),
        display_name,
        version: version.clone(),
        changelog: if changelog.trim().is_empty() {
            "Published via savhub CLI.".to_string()
        } else {
            changelog.trim().to_string()
        },
        tags,
        files: publish_files,
        summary: Some(metadata.package.description.clone()),
    };
    let response = client
        .post_json::<_, PublishResponse>("/skills", &request)
        .await?;
    println!("Published {}@{} ({})", slug, version, response.version_id);
    Ok(())
}

async fn resolve_skill_version(
    client: &ApiClient,
    slug: &str,
    fingerprint: &str,
) -> Result<ResolveResponse> {
    let mut url = client.v1_url("/resolve")?;
    url.query_pairs_mut()
        .append_pair("slug", slug)
        .append_pair("hash", fingerprint);
    client.get_json_url(url).await
}

async fn download_skill_bundle(
    client: &ApiClient,
    slug: &str,
    version: Option<&str>,
    tag: Option<&str>,
) -> Result<Vec<u8>> {
    let mut url = client.v1_url("/download")?;
    url.query_pairs_mut()
        .append_pair("slug", slug)
        .append_pair("kind", "skill");
    if let Some(version) = version {
        url.query_pairs_mut().append_pair("version", version);
    }
    if let Some(tag) = tag {
        url.query_pairs_mut().append_pair("tag", tag);
    }
    client.get_bytes_url(url).await
}

fn authed_client(opts: &GlobalOpts) -> Result<ApiClient> {
    Ok(ApiClient::new(&opts.registry, Some(require_auth_token()?)))
}

fn optional_client(opts: &GlobalOpts) -> Result<ApiClient> {
    Ok(ApiClient::new(
        &opts.registry,
        read_global_config()?.and_then(|config| config.token),
    ))
}

fn require_auth_token() -> Result<String> {
    read_global_config()?
        .and_then(|config| config.token)
        .ok_or_else(|| anyhow!("not logged in; run `savhub login`"))
}

#[allow(dead_code)]
fn resolve_folder(workdir: &Path, path: &Path) -> Result<PathBuf> {
    let folder = workdir.join(path);
    let metadata = fs::metadata(&folder)
        .with_context(|| format!("path does not exist: {}", folder.display()))?;
    if !metadata.is_dir() {
        bail!("path must be a directory: {}", folder.display());
    }
    Ok(folder)
}

fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

fn normalize_slug(value: &str) -> Result<String> {
    let slug = value.trim().to_lowercase();
    if slug.is_empty() || slug.contains('/') || slug.contains('\\') || slug.contains("..") {
        bail!("invalid slug: {value}");
    }
    Ok(slug)
}

fn ensure_confirmed(input_allowed: bool, prompt: &str, disabled_message: &str) -> Result<()> {
    if !input_allowed {
        bail!(disabled_message.to_string());
    }
    let confirmed = Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .map_err(|error| anyhow!("failed to read confirmation: {error}"))?;
    if confirmed { Ok(()) } else { bail!("canceled") }
}

fn map_explore_sort(value: &str) -> &'static str {
    match value.trim().to_lowercase().as_str() {
        "" | "newest" | "updated" | "trending" => "updated",
        "downloads" | "download" => "downloads",
        "installs" | "install" | "installsalltime" | "installs-all-time" => "installs",
        "users" | "used" | "most-used" => "users",
        "rating" | "stars" | "star" => "stars",
        "name" => "name",
        _ => "updated",
    }
}

fn resolve_selected_version(
    detail: &SkillDetailResponse,
    version: Option<&str>,
    tag: Option<&str>,
) -> Result<Option<String>> {
    if let Some(version) = version {
        return Ok(Some(version.to_string()));
    }
    if let Some(tag) = tag {
        return detail
            .skill
            .tags
            .get(tag)
            .map(|value| Some(value.clone()))
            .ok_or_else(|| anyhow!("unknown tag `{tag}`"));
    }
    Ok(detail
        .latest_version
        .as_ref()
        .map(|value| value.version.clone()))
}

fn latest_files_json(detail: &SkillDetailResponse) -> Vec<serde_json::Value> {
    detail
        .latest_version
        .as_ref()
        .map(|value| {
            value
                .files
                .iter()
                .map(|file| {
                    json!({
                        "path": file.path,
                        "size": file.size,
                        "sha256": file.sha256,
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn relative_time(timestamp: DateTime<Utc>) -> String {
    let now = Utc::now();
    let delta = now - timestamp;
    if delta.num_days() > 30 {
        format!("{}mo ago", delta.num_days() / 30)
    } else if delta.num_days() > 0 {
        format!("{}d ago", delta.num_days())
    } else if delta.num_hours() > 0 {
        format!("{}h ago", delta.num_hours())
    } else if delta.num_minutes() > 0 {
        format!("{}m ago", delta.num_minutes())
    } else {
        "just now".to_string()
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.chars().count() <= max {
        value.to_string()
    } else {
        value
            .chars()
            .take(max.saturating_sub(3))
            .collect::<String>()
            + "..."
    }
}

fn capitalize(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[allow(dead_code)]
fn build_scan_roots(opts: &GlobalOpts, extra_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut roots = Vec::new();
    for root in std::iter::once(opts.workdir.clone())
        .chain(std::iter::once(opts.dir.clone()))
        .chain(extra_roots.iter().map(|path| opts.workdir.join(path)))
    {
        let normalized = root.canonicalize().unwrap_or(root);
        if seen.insert(normalized.clone()) {
            roots.push(normalized);
        }
    }
    roots
}

#[allow(dead_code)]
fn normalize_bump(value: &str) -> Result<&'static str> {
    match value.trim().to_lowercase().as_str() {
        "patch" => Ok("patch"),
        "minor" => Ok("minor"),
        "major" => Ok("major"),
        _ => bail!("--bump must be patch, minor, or major"),
    }
}

#[allow(dead_code)]
fn bump_version(version: &str, bump: &str) -> Result<String> {
    let parsed = Version::parse(version).with_context(|| format!("invalid semver: {version}"))?;
    let mut next = parsed.clone();
    match bump {
        "major" => {
            next.major += 1;
            next.minor = 0;
            next.patch = 0;
        }
        "minor" => {
            next.minor += 1;
            next.patch = 0;
        }
        _ => {
            next.patch += 1;
        }
    }
    next.pre = semver::Prerelease::EMPTY;
    next.build = semver::BuildMetadata::EMPTY;
    Ok(next.to_string())
}

#[allow(dead_code)]
fn sync_status_label(candidate: &SyncCandidate, bump: &str) -> String {
    match candidate.status {
        SyncStatus::New => "NEW".to_string(),
        SyncStatus::Update => candidate
            .latest_version
            .as_deref()
            .map(|version| format!("UPDATE {version} -> {}", candidate.local_version))
            .unwrap_or_else(|| format!("UPDATE -> {}", candidate.local_version)),
        SyncStatus::Synced => candidate
            .matched_version
            .as_deref()
            .map(|version| format!("SYNCED {version}"))
            .unwrap_or_else(|| "SYNCED".to_string()),
        SyncStatus::Blocked => candidate
            .issue
            .clone()
            .unwrap_or_else(|| format!("BLOCKED ({bump})")),
    }
}

#[allow(dead_code)]
async fn resolve_user_id(
    client: &ApiClient,
    value: &str,
    treat_as_id: bool,
    fuzzy: bool,
) -> Result<String> {
    if treat_as_id {
        return Ok(value.trim().to_string());
    }
    let query = value.trim().trim_start_matches('@');
    let mut url = client.v1_url("/users")?;
    url.query_pairs_mut()
        .append_pair("q", query)
        .append_pair("limit", "20");
    let result = client.get_json_url::<UserListResponse>(url).await?;
    let exact = result
        .items
        .iter()
        .find(|item| item.user.handle.eq_ignore_ascii_case(query))
        .map(|item| item.user.id.to_string());
    if let Some(exact) = exact {
        return Ok(exact);
    }
    if fuzzy && result.items.len() == 1 {
        return Ok(result.items[0].user.id.to_string());
    }
    bail!("could not resolve user `{value}`")
}

#[allow(dead_code)]
fn parse_role_arg(value: &str) -> Result<UserRole> {
    match value.trim().to_lowercase().as_str() {
        "admin" => Ok(UserRole::Admin),
        "moderator" => Ok(UserRole::Moderator),
        "user" => Ok(UserRole::User),
        _ => bail!("role must be one of: user, moderator, admin"),
    }
}

// ---------------------------------------------------------------------------
// Profile commands
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// MCP commands
// ---------------------------------------------------------------------------

fn cmd_mcp(opts: &GlobalOpts, command: McpCommand) -> Result<()> {
    use savhub_local::clients::detect_clients;
    use savhub_local::mcp_config;

    match command {
        McpCommand::Register(args) => {
            let clients = detect_clients();
            let mcp_bin = mcp_config::mcp_binary_path()?;
            if !mcp_bin.exists() {
                eprintln!(
                    "Warning: MCP binary not found at {}. \
                     Build it first with `cargo build -p savhub-mcp`.",
                    mcp_bin.display()
                );
            }

            for client in &clients {
                if !client.installed {
                    continue;
                }
                if let Some(ref filter) = args.client {
                    if !client.kind.as_str().eq_ignore_ascii_case(filter)
                        && !client.name.eq_ignore_ascii_case(filter)
                    {
                        continue;
                    }
                }
                if !client.kind.supports_mcp_prompts() {
                    println!("  {} — skipped (MCP prompts not supported)", client.name);
                    continue;
                }
                match mcp_config::register_mcp(client) {
                    Ok(()) => {
                        let path = mcp_config::mcp_config_path(client)
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        println!("  {} — registered ({})", client.name, path);
                    }
                    Err(e) => {
                        println!("  {} — failed: {e}", client.name);
                    }
                }
            }
        }
        McpCommand::Unregister(args) => {
            let clients = detect_clients();
            for client in &clients {
                if !client.installed {
                    continue;
                }
                if let Some(ref filter) = args.client {
                    if !client.kind.as_str().eq_ignore_ascii_case(filter)
                        && !client.name.eq_ignore_ascii_case(filter)
                    {
                        continue;
                    }
                }
                match mcp_config::unregister_mcp(client) {
                    Ok(()) => println!("  {} — unregistered", client.name),
                    Err(e) => println!("  {} — failed: {e}", client.name),
                }
            }
        }
        McpCommand::Init => {
            mcp_config::register_mcp_for_project(&opts.workdir)?;
            let config_path = opts.workdir.join(".mcp.json");
            println!("Created {}", config_path.display());
            println!("AI agents will now pick up the savhub MCP server when opening this project.");
        }
        McpCommand::Status => {
            let statuses = mcp_config::get_all_registration_status();
            for s in &statuses {
                let installed = if s.installed {
                    "installed"
                } else {
                    "not found"
                };
                let prompts = if s.supports_prompts {
                    "prompts ✓"
                } else {
                    "prompts ✗"
                };
                let registered = if s.registered {
                    "registered"
                } else {
                    "not registered"
                };
                println!(
                    "  {:<12} {:<12} {:<12} {}",
                    s.client_name, installed, prompts, registered
                );
            }
        }
        McpCommand::Serve => {
            println!("Starting MCP server for {}...", opts.workdir.display());
            let mcp_bin = mcp_config::mcp_binary_path()?;
            let status = std::process::Command::new(&mcp_bin)
                .arg("--workdir")
                .arg(&opts.workdir)
                .status()
                .with_context(|| format!("failed to start {}", mcp_bin.display()))?;
            if !status.success() {
                bail!("MCP server exited with {status}");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// registry subcommands
// ---------------------------------------------------------------------------

async fn cmd_registry(_opts: &GlobalOpts, command: RegistryCommand) -> Result<()> {
    use savhub_local::registry;

    match command {
        RegistryCommand::Sync => {
            println!("Syncing registry...");
            match registry::ensure_registry_synced()? {
                true => {
                    let state = registry::read_registry_state()?;
                    let sha = &state.synced_commit;
                    println!("Done. Synced to commit {}", &sha[..8.min(sha.len())]);
                }
                false => println!("Already up to date."),
            }
        }
        RegistryCommand::Info => match registry::sync_info()? {
            Some(info) => {
                println!("Registry cache:");
                println!("  Commit:  {}", info.commit_sha);
                println!("  Synced:  {}", info.synced_at);
                println!("  Skills:  {}", info.skill_count);
            }
            None => {
                println!("Registry cache is empty. Run `savhub registry sync` first.");
            }
        },
        RegistryCommand::Search(args) => {
            ensure_registry_cache().await?;
            let query = args.query.join(" ");
            let results = registry::search_skills(&query, args.limit)?;
            if results.is_empty() {
                println!("No skills matching \"{query}\".");
                return Ok(());
            }
            for skill in &results {
                let summary = truncate(skill.description.as_deref().unwrap_or(""), 60);
                println!(
                    "  {:<30} v{:<10} {}",
                    skill.slug,
                    skill.version.as_deref().unwrap_or("-"),
                    summary
                );
            }
            println!("\n{} result(s)", results.len());
        }
        RegistryCommand::List(args) => {
            ensure_registry_cache().await?;
            let page = args.page.saturating_sub(1); // user-facing 1-based
            let (skills, total) = registry::list_skills(
                args.query.as_deref(),
                args.status.as_deref(),
                page,
                args.page_size,
            )?;

            if args.json {
                let out = serde_json::json!({
                    "items": skills,
                    "total": total,
                    "page": args.page,
                    "page_size": args.page_size,
                });
                println!("{}", serde_json::to_string_pretty(&out)?);
                return Ok(());
            }

            if skills.is_empty() {
                println!("No skills found.");
                return Ok(());
            }

            for skill in &skills {
                let summary = truncate(skill.description.as_deref().unwrap_or(""), 55);
                println!(
                    "  {:<30} v{:<10} {}",
                    skill.slug,
                    skill.version.as_deref().unwrap_or("-"),
                    summary
                );
            }

            let total_pages = (total + args.page_size - 1) / args.page_size;
            println!(
                "\nPage {}/{} ({} total skills)",
                args.page, total_pages, total
            );
        }
    }
    Ok(())
}

/// Ensure the local registry cache has been populated at least once.
async fn ensure_registry_cache() -> Result<()> {
    use savhub_local::registry;

    match registry::ensure_registry_synced() {
        Ok(true) => eprintln!("Registry synced."),
        Ok(false) => {} // Already up to date
        Err(e) => {
            // Non-fatal: offline or git unavailable
            if registry::cached_commit_sha()?.is_none() {
                return Err(e);
            }
            eprintln!("Warning: registry sync failed ({e}), using cached data.");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// selector subcommands
// ---------------------------------------------------------------------------

fn cmd_selector(opts: &GlobalOpts, command: SelectorCommand) -> Result<()> {
    use savhub_local::selectors::{read_selectors_store, run_selectors};

    match command {
        SelectorCommand::List => {
            let store = read_selectors_store()?;
            if store.selectors.is_empty() {
                println!("No selectors configured.");
                return Ok(());
            }
            for d in &store.selectors {
                let pri = if d.priority != 0 {
                    format!(" [P{}]", d.priority)
                } else {
                    String::new()
                };
                let status = if d.enabled { "+" } else { "-" };
                let rules = d.rules.len();
                let skills = d.skills.len();
                println!(
                    "  [{status}] {:<24} scope={:<10} {}r {}s{}",
                    d.name, d.folder_scope, rules, skills, pri
                );
                if !d.description.is_empty() {
                    let desc: String = d.description.chars().take(80).collect();
                    println!("      {desc}");
                }
            }
            println!("\n{} selector(s)", store.selectors.len());
        }
        SelectorCommand::Show(args) => {
            let store = read_selectors_store()?;
            let query = args.name.to_lowercase();
            let found: Vec<_> = store
                .selectors
                .iter()
                .filter(|d| {
                    d.name.to_lowercase().contains(&query) || d.sign.to_lowercase().contains(&query)
                })
                .collect();
            if found.is_empty() {
                println!(
                    "No selector matching \"{}\". Use `savhub selector list` to see all.",
                    args.name
                );
                return Ok(());
            }
            for d in &found {
                println!("Name:       {}", d.name);
                println!("ID:         {}", d.sign);
                println!("Enabled:    {}", if d.enabled { "yes" } else { "no" });
                if !d.description.is_empty() {
                    println!("Desc:       {}", d.description);
                }
                println!("Scope:      {}", d.folder_scope);
                println!("Mode:       {:?}", d.match_mode);
                if !d.custom_expression.is_empty() {
                    println!("Expression: {}", d.custom_expression);
                } else {
                    println!("Expression: {}", d.display_expression());
                }
                println!("Priority:   {}", d.priority);
                println!("Rules:");
                for (i, rule) in d.rules.iter().enumerate() {
                    println!("  {}. {}", i + 1, rule.display());
                }
                if !d.skills.is_empty() {
                    println!("Skills:     {}", d.skills.join(", "));
                }
                if !d.flocks.is_empty() {
                    println!("Flocks:     {}", d.flocks.join(", "));
                }
                if !d.repos.is_empty() {
                    println!("Repos:      {}", d.repos.join(", "));
                }
                println!();
            }
        }
        SelectorCommand::Test => {
            let result = run_selectors(&opts.workdir)?;
            if result.matched.is_empty() {
                println!("No selectors matched {}.", opts.workdir.display());
                return Ok(());
            }
            println!("Matched selectors for {}:", opts.workdir.display());
            for m in &result.matched {
                let pri = m.selector.priority;
                println!(
                    "  [+] {} (P{pri}) — {}",
                    m.selector.name, m.selector.description
                );
            }
            if !result.skills.is_empty() {
                println!("Skills:  {}", result.skills.join(", "));
            }
            if !result.flocks.is_empty() {
                println!("Flocks:  {}", result.flocks.join(", "));
            }
            if !result.repos.is_empty() {
                println!("Repos:   {}", result.repos.join(", "));
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// flock commands
// ---------------------------------------------------------------------------

fn cmd_flock(_opts: &GlobalOpts, command: FlockCommand) -> Result<()> {
    use savhub_local::registry;

    match command {
        FlockCommand::List => {
            let flocks = registry::list_flocks()?;
            if flocks.is_empty() {
                println!("No flocks in registry cache. Run `savhub registry sync` first.");
                return Ok(());
            }
            for flock in &flocks {
                let skill_count = registry::list_flock_skill_slugs(&flock.slug)
                    .map(|s| s.len())
                    .unwrap_or(0);
                println!(
                    "  {:<24} {:>3} skill(s)  {}",
                    flock.slug, skill_count, flock.name
                );
                if !flock.description.is_empty() {
                    let desc: String = flock.description.chars().take(72).collect();
                    println!("    {desc}");
                }
            }
            println!("\n{} flock(s)", flocks.len());
        }
        FlockCommand::Show(args) => {
            let flock = registry::get_flock_by_slug(&args.slug)?;
            let Some(flock) = flock else {
                println!(
                    "Flock \"{}\" not found. Run `savhub flock list` to see available flocks.",
                    args.slug
                );
                return Ok(());
            };
            println!("Name:        {}", flock.name);
            println!("Slug:        {}", flock.slug);
            if !flock.description.is_empty() {
                println!("Description: {}", flock.description);
            }
            let skills = registry::list_skills_in_flock(&flock.slug)?;
            if skills.is_empty() {
                println!("Skills:      (none)");
            } else {
                println!("Skills ({}):", skills.len());
                for skill in &skills {
                    println!("  - {}  {}", skill.slug, skill.name);
                }
            }
        }
        FlockCommand::Install(args) => {
            let flock = registry::get_flock_by_slug(&args.slug)?;
            let Some(flock) = flock else {
                println!("Flock \"{}\" not found.", args.slug);
                return Ok(());
            };
            let skill_slugs = registry::list_flock_skill_slugs(&flock.slug)?;
            if skill_slugs.is_empty() {
                println!("Flock \"{}\" has no skills.", flock.slug);
                return Ok(());
            }
            println!("Flock: {} ({})", flock.name, flock.slug);
            println!("Skills to install:");
            for slug in &skill_slugs {
                println!("  [+] {slug}");
            }
            if !args.yes {
                let proceed = Confirm::new()
                    .with_prompt(format!(
                        "Install {} skill(s) from flock \"{}\"?",
                        skill_slugs.len(),
                        flock.slug
                    ))
                    .default(true)
                    .interact()?;
                if !proceed {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            let mut lockfile = read_project_added_skills(&_opts.workdir)?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            let mut added = 0;
            for slug in &skill_slugs {
                if !lockfile.skills.contains_key(slug) {
                    lockfile.skills.insert(
                        slug.clone(),
                        ProjectLockEntry {
                            version: "latest".to_string(),
                            installed_at: now,
                        },
                    );
                    added += 1;
                    println!("  Added: {slug}");
                } else {
                    println!("  Already installed: {slug}");
                }
            }
            if added > 0 {
                write_project_added_skills(&_opts.workdir, &lockfile)?;
            }
            println!(
                "\nDone. {added} skill(s) added from flock \"{}\".",
                flock.slug
            );
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// pilot command
// ---------------------------------------------------------------------------

fn cmd_pilot(command: PilotCommand) -> Result<()> {
    use savhub_local::pilot;

    // Resolve agents: use --agents flag, or fall back to config
    let resolve_agents = |args: &PilotAgentArgs| -> Result<Vec<String>> {
        if !args.agents.is_empty() {
            return Ok(args.agents.clone());
        }
        let cfg = savhub_local::config::read_global_config()?
            .unwrap_or_default();
        if cfg.agents.is_empty() {
            // Default to claude-code if nothing configured
            Ok(vec!["claude-code".to_string()])
        } else {
            Ok(cfg.agents)
        }
    };

    match command {
        PilotCommand::Install(args) => {
            let agents = resolve_agents(&args)?;
            let installed = pilot::install(&agents)?;
            for dir in &installed {
                println!("  Installed: {}", dir.display());
            }
            println!("\n{} agent(s) configured. Run `savhub apply` in your project to activate.", installed.len());
        }
        PilotCommand::Uninstall(args) => {
            let agents = resolve_agents(&args)?;
            let removed = pilot::uninstall(&agents)?;
            if removed.is_empty() {
                println!("Nothing to remove.");
            } else {
                for dir in &removed {
                    println!("  Removed: {}", dir.display());
                }
            }
        }
        PilotCommand::Status(args) => {
            let agents = resolve_agents(&args)?;
            let statuses = pilot::status(&agents)?;
            for (agent, path) in &statuses {
                if let Some(p) = path {
                    println!("  {agent}: installed at {}", p.display());
                } else {
                    println!("  {agent}: not installed");
                }
            }
        }
        PilotCommand::Notify => {
            pilot::notify_config_changed()?;
            println!("Config change signal sent.");
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// apply command
// ---------------------------------------------------------------------------

fn cmd_apply(opts: &GlobalOpts, mut args: ApplyArgs) -> Result<()> {
    use savhub_local::registry;
    use savhub_local::selectors::run_selectors;

    // Trim and deduplicate all list args
    fn clean(v: &mut Vec<String>) {
        *v = v
            .iter()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        v.dedup();
    }
    clean(&mut args.agents);
    clean(&mut args.skip_agents);
    clean(&mut args.add_skills);
    clean(&mut args.skip_skills);
    clean(&mut args.add_flocks);
    clean(&mut args.skip_flocks);

    // Ensure registry is synced before applying
    eprint!("Syncing registry...");
    match registry::ensure_registry_synced() {
        Ok(true) => eprintln!(" done."),
        Ok(false) => eprintln!(" up to date."),
        Err(e) => {
            eprintln!();
            if registry::cached_commit_sha()?.is_none() {
                bail!("Registry not available: {e}\nRun `savhub registry sync` first.");
            }
            eprintln!("Warning: registry sync failed ({e}), using cached data.");
        }
    }

    let workdir = &opts.workdir;
    eprintln!("Scanning project...");
    let result = run_selectors(workdir)?;

    if result.matched.is_empty() {
        println!(
            "No selectors matched this project. All skills previously applied by savhub will be removed."
        );

        // Read savhub.lock for installed skills
        let lockfile = savhub_local::presets::read_project_lockfile(workdir)?;

        if !lockfile.skills.is_empty() {
            println!("\nSkills to remove:");
            for s in &lockfile.skills {
                println!("  \x1b[31m[-]\x1b[0m {}", s.sign);
            }

            if !args.yes && opts.input_allowed {
                let proceed = Confirm::new()
                    .with_prompt(format!(
                        "Remove {} skill(s) from AI client directories?",
                        lockfile.skills.len()
                    ))
                    .default(true)
                    .interact()?;
                if !proceed {
                    println!("Cancelled.");
                    return Ok(());
                }
            }

            // Remove skill folders from AI client project-level dirs
            let all_clients = savhub_local::clients::detect_clients();
            for skill in &lockfile.skills {
                let slug = skill.slug();
                for client in &all_clients {
                    if !client.installed {
                        continue;
                    }
                    let Some(rel_dir) = client.kind.project_skills_dir() else {
                        continue;
                    };
                    let _ = std::fs::remove_dir_all(workdir.join(rel_dir).join(slug));
                }
            }
        }

        // Clear selectors.matched, flocks.matched (leave manual_* untouched)
        let mut config = savhub_local::presets::read_project_config(workdir)?;
        config.selectors.matched.clear();
        config.flocks.matched.clear();
        savhub_local::presets::write_project_config_force(workdir, &config)?;

        // Clear savhub.lock (empty but file still exists)
        savhub_local::presets::write_project_lockfile_force(
            workdir,
            &savhub_local::presets::ProjectLockFile::default(),
        )?;

        if lockfile.skills.is_empty() {
            println!("No installed skills to remove.");
        } else {
            println!(
                "\n\x1b[32mDone.\x1b[0m {} skill(s) removed.",
                lockfile.skills.len()
            );
        }

        return Ok(());
    }

    let existing_config = savhub_local::presets::read_project_config(workdir)?;

    // ── Collect all matched items ──
    let matched_selector_names: Vec<String> = result
        .matched
        .iter()
        .map(|m| m.selector.name.clone())
        .collect();
    let matched_flocks: Vec<String> = result.flocks.clone();

    // ── Collect previously matched selectors that no longer match ──
    let unmatched: Vec<tui::UnmatchedSelector> = existing_config
        .selectors
        .matched
        .iter()
        .filter(|prev| !matched_selector_names.contains(&prev.selector))
        .map(|prev| tui::UnmatchedSelector {
            name: prev.selector.clone(),
            flocks: prev.flocks.clone(),
        })
        .collect();

    // ── Interactive selection of selectors and flocks (unless -y) ──
    let (selected_selectors, skipped_selectors): (Vec<String>, Vec<String>);
    let (selected_flocks, skipped_flocks): (Vec<String>, Vec<String>);

    if args.yes || !opts.input_allowed {
        selected_selectors = matched_selector_names.clone();
        skipped_selectors = Vec::new();
        selected_flocks = matched_flocks.clone();
        skipped_flocks = Vec::new();

        // Print summary
        if !selected_selectors.is_empty() {
            println!("\nSelectors:");
            for s in &selected_selectors {
                println!("  \x1b[32m[+]\x1b[0m {s}");
            }
        }
        if !selected_flocks.is_empty() {
            println!("\nFlocks:");
            for f in &selected_flocks {
                println!("  \x1b[32m[+]\x1b[0m {f}");
            }
        }
        if !unmatched.is_empty() {
            println!("\n\x1b[33mWill be removed (no longer matched):\x1b[0m");
            for u in &unmatched {
                println!("  \x1b[31m✕\x1b[0m {}", u.name);
            }
        }
    } else {
        // Build TUI selectors with their contributed flocks
        let mut tui_selectors: Vec<tui::MatchedSelector> = result
            .matched
            .iter()
            .map(|m| {
                let pri = m.selector.priority;
                let sel_flocks = m.flocks.clone();
                tui::MatchedSelector {
                    name: m.selector.name.clone(),
                    label: format!("{} (P{pri}) — {}", m.selector.name, m.selector.description),
                    checked: !existing_config
                        .selectors
                        .manual_skipped
                        .contains(&m.selector.name),
                    flocks: sel_flocks,
                }
            })
            .collect();

        let flock_skip: BTreeSet<String> = existing_config
            .flocks
            .manual_skipped
            .iter()
            .cloned()
            .collect();

        let sel_result = tui::apply_select(
            &mut tui_selectors,
            &flock_skip,
            &|slug| {
                registry::list_flock_skill_slugs(slug)
                    .map(|v| v.len())
                    .unwrap_or(0)
            },
            &unmatched,
        )?;

        let Some(sel) = sel_result else {
            println!("Cancelled.");
            return Ok(());
        };

        selected_selectors = sel.selected_selectors;
        skipped_selectors = sel.skipped_selectors;
        selected_flocks = sel.selected_flocks;
        skipped_flocks = sel.skipped_flocks;

        // Print summary after TUI
        if !selected_selectors.is_empty() || !skipped_selectors.is_empty() {
            println!("\nSelectors:");
            for s in &selected_selectors {
                println!("  \x1b[32m[+]\x1b[0m {s}");
            }
            for s in &skipped_selectors {
                println!("  \x1b[31m[-]\x1b[0m {s}");
            }
        }
        if !selected_flocks.is_empty() || !skipped_flocks.is_empty() {
            println!("Flocks:");
            for f in &selected_flocks {
                println!("  \x1b[32m[+]\x1b[0m {f}");
            }
            for f in &skipped_flocks {
                println!("  \x1b[31m[-]\x1b[0m {f}");
            }
        }
    }

    // Merge CLI --skip-* args into skipped lists
    let mut skipped_flocks = skipped_flocks;
    for f in &args.skip_flocks {
        if !skipped_flocks.contains(f) {
            skipped_flocks.push(f.clone());
        }
    }

    // ── Expand selected flocks into skill slugs ──
    // Only use selectors that were selected (not skipped)
    let mut all_skills: Vec<String> = Vec::new();
    for m in &result.matched {
        if !selected_selectors.contains(&m.selector.name) {
            continue;
        }
        for skill in &m.skills {
            if !all_skills.contains(skill) {
                all_skills.push(skill.clone());
            }
        }
    }
    for flock_slug in &selected_flocks {
        if let Ok(flock_skills) = registry::list_flock_skill_slugs(flock_slug) {
            if flock_skills.is_empty() {
                eprintln!(
                    "  \x1b[33m!\x1b[0m flock \"{flock_slug}\" has 0 skills in cache. Try: savhub registry sync"
                );
            }
            for skill in flock_skills {
                if !all_skills.contains(&skill) {
                    all_skills.push(skill);
                }
            }
        }
    }

    // ── Include CLI --flocks skills ──
    for flock_slug in &args.add_flocks {
        if let Ok(flock_skills) = registry::list_flock_skill_slugs(flock_slug) {
            for skill in flock_skills {
                if !all_skills.contains(&skill) {
                    all_skills.push(skill);
                }
            }
        }
    }

    // ── Include CLI --skills directly ──
    for s in &args.add_skills {
        let slug = s.rsplit('/').next().unwrap_or(s).to_string();
        if !slug.is_empty() && !all_skills.contains(&slug) {
            all_skills.push(slug);
        }
    }

    // ── Filter out skipped skills (existing config + CLI --skip-skills) ──
    let config = savhub_local::presets::read_project_config(workdir)?;
    let mut skipped = config.skills.manual_skipped.clone();
    for s in &args.skip_skills {
        if !s.is_empty() && !skipped.contains(s) {
            skipped.push(s.clone());
        }
    }
    let skipped = &skipped;
    let desired_skills: BTreeSet<String> = all_skills
        .into_iter()
        .filter(|s| !registry::skill_matches_skipped(s, skipped))
        .collect();

    // ── Compute diff against current lockfile ──
    let current_lock = savhub_local::presets::read_project_lockfile(workdir)?;
    let current_locked_slugs: BTreeSet<String> = current_lock
        .skills
        .iter()
        .map(|s| s.slug().to_string())
        .collect();

    let to_add: Vec<String> = desired_skills
        .difference(&current_locked_slugs)
        .cloned()
        .collect();
    let to_remove: Vec<String> = current_locked_slugs
        .difference(&desired_skills)
        .cloned()
        .collect();

    // ── Check if anything actually changed ──
    let toml_exists = workdir.join("savhub.toml").exists();
    let lock_exists = workdir.join("savhub.lock").exists();
    if to_add.is_empty() && to_remove.is_empty() && toml_exists && lock_exists {
        // Also check if selectors/flocks config changed
        let old_matched_names: BTreeSet<String> = config
            .selectors
            .matched
            .iter()
            .map(|m| m.selector.clone())
            .collect();
        let new_matched_names: BTreeSet<String> = result
            .matched
            .iter()
            .map(|m| m.selector.name.clone())
            .collect();
        let old_flocks: BTreeSet<String> = config.flocks.matched.iter().cloned().collect();
        let new_flocks: BTreeSet<String> = selected_flocks.iter().cloned().collect();
        if old_matched_names == new_matched_names && old_flocks == new_flocks {
            println!("\nProject is already up to date. Nothing to do.");
            return Ok(());
        }
    }

    // ── Show plan ──
    if !to_add.is_empty() {
        println!("\nSkills to add:");
        for s in &to_add {
            println!("  \x1b[32m[+]\x1b[0m {s}");
        }
    }
    if !to_remove.is_empty() {
        println!("\nSkills to remove:");
        for s in &to_remove {
            println!("  \x1b[31m[-]\x1b[0m {s}");
        }
    }
    if to_add.is_empty() && to_remove.is_empty() {
        println!("\nNo skill changes, updating selector configuration only.");
    }

    if args.dry_run {
        println!("\n\x1b[2m(dry-run: no changes made)\x1b[0m");
        return Ok(());
    }

    if !args.yes && opts.input_allowed && (!to_add.is_empty() || !to_remove.is_empty()) {
        let proceed = Confirm::new()
            .with_prompt("Apply these changes?")
            .default(true)
            .interact()?;
        if !proceed {
            println!("Cancelled.");
            return Ok(());
        }
    }

    // ── Apply: update savhub.toml selectors (replace, not accumulate) ──
    {
        let mut cfg = savhub_local::presets::read_project_config(workdir)?;
        cfg.selectors.matched = result
            .matched
            .iter()
            .map(|m| {
                let selector_flocks: Vec<String> = m
                    .flocks
                    .iter()
                    .filter(|f| selected_flocks.contains(f))
                    .cloned()
                    .collect();
                savhub_local::presets::ProjectSelectorMatch {
                    selector: m.selector.name.clone(),
                    flocks: selector_flocks,
                    skills: m.skills.clone(),
                    repos: m.repos.clone(),
                }
            })
            .collect();
        // Collect all matched flocks into flocks.matched
        let mut all_matched_flocks: Vec<String> = Vec::new();
        for m in &cfg.selectors.matched {
            for f in &m.flocks {
                if !all_matched_flocks.contains(f) {
                    all_matched_flocks.push(f.clone());
                }
            }
        }
        cfg.flocks.matched = all_matched_flocks;

        // Save interactive unchecked items to manual_skipped
        for s in &skipped_selectors {
            if !cfg.selectors.manual_skipped.contains(s) {
                cfg.selectors.manual_skipped.push(s.clone());
            }
        }
        // Remove re-checked items from manual_skipped
        cfg.selectors
            .manual_skipped
            .retain(|s| !selected_selectors.contains(s) || !matched_selector_names.contains(s));

        for f in &skipped_flocks {
            if !cfg.flocks.manual_skipped.contains(f) {
                cfg.flocks.manual_skipped.push(f.clone());
            }
        }
        cfg.flocks
            .manual_skipped
            .retain(|f| !selected_flocks.contains(f) || !matched_flocks.contains(f));

        // Merge CLI --skills/--skip-skills/--flocks/--skip-flocks
        for s in &args.add_skills {
            if !s.is_empty()
                && !cfg
                    .skills
                    .manual_added
                    .iter()
                    .any(|e| e.path == *s || e.sign.as_deref() == Some(s))
            {
                cfg.skills
                    .manual_added
                    .push(savhub_local::presets::ProjectAddedSkill {
                        sign: if s.contains('/') {
                            Some(s.clone())
                        } else {
                            None
                        },
                        path: s.rsplit('/').next().unwrap_or(s).to_string(),
                        slug: s.rsplit('/').next().unwrap_or(s).to_string(),
                        local: None,
                        version: None,
                        installed_at: 0,
                    });
            }
        }
        for s in &args.skip_skills {
            if !s.is_empty() && !cfg.skills.manual_skipped.contains(s) {
                cfg.skills.manual_skipped.push(s.clone());
            }
        }
        for f in &args.add_flocks {
            if !f.is_empty() && !cfg.flocks.manual_added.contains(f) {
                cfg.flocks.manual_added.push(f.clone());
            }
        }
        for f in &args.skip_flocks {
            if !f.is_empty() && !cfg.flocks.manual_skipped.contains(f) {
                cfg.flocks.manual_skipped.push(f.clone());
            }
        }

        savhub_local::presets::write_project_config_force(workdir, &cfg)?;
    }

    // ── Remove skills that are no longer in desired set (grouped by repo) ──
    if !to_remove.is_empty() {
        let all_clients = savhub_local::clients::detect_clients();
        // Group by repo from current lock
        for slug in &to_remove {
            for client in &all_clients {
                if !client.installed {
                    continue;
                }
                let Some(rel_dir) = client.kind.project_skills_dir() else {
                    continue;
                };
                let _ = std::fs::remove_dir_all(workdir.join(rel_dir).join(slug));
            }
            println!("  \x1b[31m\u{2717}\x1b[0m {slug} (removed)");
        }
    }

    // ── Apply: batch-install skills via registry (one git op per repo) ──
    use savhub_local::skills::copy_skill_folder;

    let mut installed_count = 0usize;

    // Filter AI clients (respecting --agents/--skip-agents)
    let all_clients = savhub_local::clients::detect_clients();
    let filtered_clients: Vec<_> = all_clients
        .into_iter()
        .filter(|c| {
            let name = c.kind.as_str();
            if !args.agents.is_empty() {
                return args.agents.iter().any(|a| a.eq_ignore_ascii_case(name));
            }
            if !args.skip_agents.is_empty() {
                return !args
                    .skip_agents
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(name));
            }
            true
        })
        .collect();

    if !to_add.is_empty() {
        eprintln!("Installing {} skill(s)...", to_add.len());
    }
    let batch_results = registry::install_skills_batch(&to_add)?;

    // Build lock entries: start from current, remove deleted, add new
    let mut lock = current_lock.clone();
    lock.skills
        .retain(|s| !to_remove.iter().any(|r| r == s.slug()));

    // Group by repo for display
    {
        for info in &batch_results {
            let skill_sign =
                savhub_local::registry::make_skill_sign(&info.repo_sign, &info.skill_path);
            let mut copied_to_any_client = false;
            for client in &filtered_clients {
                if !client.installed {
                    continue;
                }
                let Some(rel_dir) = client.kind.project_skills_dir() else {
                    continue;
                };
                let target_dir = workdir.join(rel_dir);
                let _ = std::fs::create_dir_all(&target_dir);
                let target = target_dir.join(&info.slug);
                if let Err(e) = copy_skill_folder(&info.local_path, &target) {
                    eprintln!(
                        "  \x1b[33m!\x1b[0m {}: failed to copy to {}: {e}",
                        info.slug, rel_dir
                    );
                    continue;
                }
                copied_to_any_client = true;
                println!(
                    "  \x1b[32m\u{2713}\x1b[0m {} -> {rel_dir}/{}",
                    info.slug, info.slug
                );
            }
            if !copied_to_any_client {
                println!("  \x1b[32m\u{2713}\x1b[0m {} (cached)", info.slug);
            }

            // Record in savhub.lock
            if !lock.skills.iter().any(|s| s.slug() == info.slug) {
                let vi = savhub_local::skills::read_skill_version_info(&info.local_path)
                    .unwrap_or_default();
                lock.skills.push(savhub_local::presets::ProjectLockedSkill {
                    sign: skill_sign,
                    version: vi.version,
                    commit_hash: vi.git_commit,
                });
            }
            installed_count += 1;
        }
    }

    // Always create savhub.lock (even if empty)
    savhub_local::presets::write_project_lockfile_force(workdir, &lock)?;

    // Register this project so desktop can see it
    let _ = savhub_local::config::add_project(&workdir.display().to_string());

    // Fire-and-forget install tracking
    if !batch_results.is_empty() {
        if let Ok(client) = optional_client(opts) {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                for info in &batch_results {
                    let slug = info.slug.clone();
                    let client = client.clone();
                    handle.spawn(async move {
                        let _ = client
                            .post_json::<serde_json::Value, serde_json::Value>(
                                &format!("/collect?skill={slug}"),
                                &json!({ "client_type": "cli" }),
                            )
                            .await;
                    });
                }
            }
        }
    }

    let removed_count = to_remove.len();
    if installed_count > 0 || removed_count > 0 {
        println!(
            "\n\x1b[32mDone.\x1b[0m +{installed_count} -{removed_count} skill(s), {} selector(s) matched.",
            result.matched.len()
        );
    } else {
        println!(
            "\n\x1b[32mDone.\x1b[0m Configuration updated, {} selector(s) matched.",
            result.matched.len()
        );
    }
    Ok(())
}
