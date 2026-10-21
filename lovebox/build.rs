//! Bakes build metadata into the binary: the target triple and the git short SHA.
//!
//! `LOVEBOX_GIT_SHA` in the environment overrides `git rev-parse` (useful in containers
//! without git); without either, the SHA is `unknown`.

use std::{env, path::Path, process::Command};

fn main() {
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_owned());
    println!("cargo:rustc-env=LOVEBOX_TARGET={target}");

    let sha = env::var("LOVEBOX_GIT_SHA")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| git(&["rev-parse", "--short", "HEAD"]))
        .unwrap_or_else(|| "unknown".to_owned());
    println!("cargo:rustc-env=LOVEBOX_GIT_SHA={sha}");

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=LOVEBOX_GIT_SHA");
    // Rebuild when HEAD moves. Only existing paths are emitted: cargo treats a missing
    // rerun-if-changed path as always changed.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        let git_dir = Path::new(&git_dir);
        rerun_if_exists(&git_dir.join("HEAD"));
        rerun_if_exists(&git_dir.join("packed-refs"));
        if let Some(head_ref) = git(&["symbolic-ref", "-q", "HEAD"]) {
            rerun_if_exists(&git_dir.join(head_ref));
        }
    }
}

/// Runs git with `args` and returns trimmed stdout, or `None` on any failure.
fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?.trim().to_owned();
    (!s.is_empty()).then_some(s)
}

fn rerun_if_exists(path: &Path) {
    if path.exists() {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}
