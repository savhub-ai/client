mod handler;
mod protocol;
mod transport;

use std::path::PathBuf;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "savhub-mcp",
    version,
    about = "Savhub MCP server for dynamic AI skills"
)]
struct Args {
    /// Project working directory. If omitted, uses the current directory.
    #[arg(long)]
    workdir: Option<PathBuf>,

    /// Override the preset to use (instead of project binding).
    #[arg(long, alias = "profile")]
    preset: Option<String>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    let workdir = args
        .workdir
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    let handler = handler::McpHandler::new(workdir, args.preset);

    if let Err(e) = transport::run_stdio(handler).await {
        eprintln!("savhub-mcp error: {e}");
        std::process::exit(1);
    }
}
