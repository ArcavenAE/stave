//! Stamps a build identity into the SDK (surfaced by the CLI's
//! `--version` and the audit trail's `invocation.build_id`).
//!
//! Resolution order:
//! 1. `STAVE_BUILD_ID` env — set by CI to the channel tag computed
//!    before the build (`alpha-…`, `v0.1.0+g<sha7>`).
//! 2. Local git — `dev+g<sha7>[-dirty]`.
//! 3. `unknown` — no env, no `.git` (e.g. a source-tarball build).

use std::process::Command;

fn main() {
    println!("cargo:rerun-if-env-changed=STAVE_BUILD_ID");
    // Re-stamp local builds when HEAD moves. `.git/HEAD` alone does not
    // do that: committing on the current branch rewrites
    // `.git/refs/heads/<branch>` and leaves HEAD byte-identical, so the
    // stamp went stale after every commit that did not switch branches,
    // and a locally built binary reported a commit it did not contain.
    // `.git/logs/HEAD` is appended by commit, checkout, reset and merge
    // alike, so it is the one path covering all HEAD movement. Cargo
    // ignores missing rerun paths, so a repo with the reflog disabled
    // degrades to the old behaviour rather than failing. Working-tree
    // edits still do not retrigger, so the `-dirty` suffix can lag.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/logs/HEAD");

    let build_id = std::env::var("STAVE_BUILD_ID")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(local_git_id);
    println!("cargo:rustc-env=STAVE_BUILD_ID={build_id}");
}

fn local_git_id() -> String {
    let sha = git(&["rev-parse", "--short=7", "HEAD"]);
    match sha {
        Some(sha) => {
            let dirty = git(&["status", "--porcelain"]).is_some_and(|s| !s.is_empty());
            if dirty {
                format!("dev+g{sha}-dirty")
            } else {
                format!("dev+g{sha}")
            }
        }
        None => "unknown".to_string(),
    }
}

fn git(args: &[&str]) -> Option<String> {
    let out = Command::new("git").args(args).output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8(out.stdout).ok()?.trim().to_string())
}
