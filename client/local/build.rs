use std::process::Command;

fn main() {
    // Capture git commit hash at compile time
    let output = Command::new("git")
        .args(["rev-parse", "--short=9", "HEAD"])
        .output();
    let git_sha = match output {
        Ok(out) if out.status.success() => String::from_utf8(out.stdout)
            .unwrap_or_default()
            .trim()
            .to_string(),
        _ => "unknown".to_string(),
    };
    println!("cargo:rustc-env=SAVHUB_GIT_HASH={git_sha}");

    // Re-run if git HEAD changes
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads/");
}
