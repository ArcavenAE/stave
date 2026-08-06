//! Shared harness for the stave-cli integration tests.
//!
//! Every test runs the real `stave` binary inside a sandbox. Config,
//! audit trail, and token cache all live in a per-test tempdir, and
//! every `STAVE_*` variable the SDK reads is cleared first, so an
//! ambient shell cannot change a result and a test cannot touch the
//! developer's real state.
//!
//! Two hermeticity rules the whole suite keeps:
//!
//! * **No network beyond wiremock.** The only reachable endpoint is a
//!   URI a test hands over explicitly (`STAVE_BASE_URL`,
//!   `STAVE_TOKEN_URL`, or a config `api_url`).
//! * **No platform keyring.** The client-secret chain is env, then
//!   keyring, then config. Tests that need a secret supply
//!   `STAVE_CLIENT_SECRET`, which resolves at the first layer, so the
//!   keyring layer is never reached. Tests of the "nothing resolved"
//!   path deliberately withhold the *client ID* rather than the secret,
//!   because the ID resolves from env and config only.
//!
//! All data is synthetic: tenant `00000000-0000-0000-0000-000000000000`,
//! region `example1`, accounts `123456789012`, resources `example-*`.

#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use assert_cmd::cargo::CommandCargoExt;
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;
use tempfile::TempDir;

/// Synthetic tenant identifier. Never a real GUID.
pub const SYNTHETIC_TENANT_ID: &str = "00000000-0000-0000-0000-000000000000";

/// Synthetic data-center claim. Derives to
/// `https://api.example1.app.wiz.io/graphql`, which no test ever calls.
pub const SYNTHETIC_DC: &str = "example1";

/// Every environment variable the SDK consults, cleared on each
/// invocation. Keep in sync with the `*_ENV` constants in
/// `stave-sdk::{auth, client, token, audit, mcp}`.
const STAVE_ENV: &[&str] = &[
    "STAVE_ACCESS_TOKEN",
    "STAVE_ALLOW_WRITE",
    "STAVE_API_URL",
    "STAVE_AUDIT",
    "STAVE_AUDIT_DIR",
    "STAVE_BASE_URL",
    "STAVE_CLIENT_ID",
    "STAVE_CLIENT_SECRET",
    "STAVE_CONFIG",
    "STAVE_KEYRING",
    "STAVE_SESSION_ID",
    "STAVE_MCP_URL",
    "STAVE_REGISTRY_PASSWORD",
    "STAVE_TOKEN_CACHE_DIR",
    "STAVE_TOKEN_URL",
];

/// An isolated home for one test's config, audit trail, and token cache.
/// Removed when the value drops, so a passing run leaves nothing behind.
pub struct Sandbox {
    dir: TempDir,
}

impl Sandbox {
    pub fn new() -> Self {
        Self {
            dir: tempfile::Builder::new()
                .prefix("stave-test-")
                .tempdir()
                .expect("create sandbox tempdir"),
        }
    }

    pub fn root(&self) -> &Path {
        self.dir.path()
    }

    /// Config path handed to the binary as `STAVE_CONFIG`. The file does
    /// not exist until a test writes it or the binary creates it.
    pub fn config_path(&self) -> PathBuf {
        self.dir.path().join("config.toml")
    }

    pub fn audit_dir(&self) -> PathBuf {
        self.dir.path().join("audit")
    }

    pub fn token_cache_dir(&self) -> PathBuf {
        self.dir.path().join("token-cache")
    }

    pub fn token_cache_file(&self) -> PathBuf {
        self.token_cache_dir().join("token.json")
    }

    /// The `stave` binary, pointed at this sandbox and stripped of every
    /// ambient `STAVE_*` variable. Tests add back only what they mean to
    /// exercise.
    pub fn cmd(&self) -> Command {
        let mut c = Command::cargo_bin("stave").expect("stave binary built");
        for key in STAVE_ENV {
            c.env_remove(key);
        }
        // Tracing writes to stderr at warn level by default; a stray
        // RUST_LOG in the parent shell would add lines the assertions
        // do not expect.
        c.env_remove("RUST_LOG");
        c.env("STAVE_CONFIG", self.config_path())
            .env("STAVE_AUDIT_DIR", self.audit_dir())
            .env("STAVE_TOKEN_CACHE_DIR", self.token_cache_dir())
            // Hermetic by construction: the sandbox must never open the
            // user's real keychain. A resident `stave` item plus a
            // rebuilt (re-signed) test binary makes macOS raise an
            // access-control prompt that hangs a headless run.
            .env("STAVE_KEYRING", "off");
        c
    }

    /// Seed the config file. Body is TOML written verbatim.
    pub fn write_config(&self, body: &str) {
        std::fs::write(self.config_path(), body).expect("write sandbox config");
    }

    /// Seed the exploratory read posture (D11), the config state under
    /// which ad-hoc `--query` documents are permitted. Curated
    /// operations do not need this. All tests exercising ad-hoc reads
    /// against a LOCAL mock server set this; no real tenant is ever
    /// contacted (MANDATORY SAFETY RULES).
    pub fn write_exploratory_config(&self) {
        self.write_config("[default]\nposture = \"exploratory\"\n");
    }

    pub fn read_config(&self) -> String {
        std::fs::read_to_string(self.config_path()).expect("read sandbox config")
    }

    pub fn config_exists(&self) -> bool {
        self.config_path().exists()
    }

    /// Every audit line emitted into this sandbox, parsed.
    pub fn audit_lines(&self) -> Vec<Value> {
        let mut out = Vec::new();
        let Ok(entries) = std::fs::read_dir(self.audit_dir()) else {
            return out;
        };
        for entry in entries.flatten() {
            if entry.path().extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let body = std::fs::read_to_string(entry.path()).expect("read audit file");
            for line in body.lines() {
                if line.trim().is_empty() {
                    continue;
                }
                out.push(serde_json::from_str(line).expect("audit line parses as JSON"));
            }
        }
        out
    }

    /// Audit lines carrying an `operation` block, meaning an API call
    /// rather than a stream-transform verb.
    pub fn api_audit_lines(&self) -> Vec<Value> {
        self.audit_lines()
            .into_iter()
            .filter(|v| v.get("operation").is_some())
            .collect()
    }
}

impl Default for Sandbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Run a prepared command, feeding `input` on stdin and capturing both
/// output streams.
pub fn run_with_stdin(cmd: &mut Command, input: &str) -> Output {
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().expect("spawn stave");
    child
        .stdin
        .as_mut()
        .expect("stdin is piped")
        .write_all(input.as_bytes())
        .expect("write stdin");
    drop(child.stdin.take());
    child.wait_with_output().expect("wait for stave")
}

/// Run a prepared command with stdin closed, so a primitive that reads
/// stdin sees an empty stream instead of blocking on the terminal.
pub fn run(cmd: &mut Command) -> Output {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd.output().expect("run stave")
}

pub fn stdout_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn stderr_of(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

/// Parse a JSONL stream, skipping blank lines.
pub fn jsonl(body: &str) -> Vec<Value> {
    body.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| panic!("record is not JSON: {l} ({e})"))
        })
        .collect()
}

/// The `id` field of every record in a JSONL stream, in order.
pub fn ids(body: &str) -> Vec<String> {
    jsonl(body)
        .iter()
        .map(|r| {
            r.get("id")
                .and_then(Value::as_str)
                .unwrap_or("<no id>")
                .to_string()
        })
        .collect()
}

/// A synthetic Wiz fixture, read from `examples/fixtures/`.
pub fn fixture(kind: &str) -> String {
    std::fs::read_to_string(fixture_path(kind))
        .unwrap_or_else(|e| panic!("read fixture {kind}: {e}"))
}

pub fn fixture_path(kind: &str) -> String {
    format!("../../examples/fixtures/{kind}.jsonl")
}

/// An unsigned JWT carrying `payload`. The signature segment is the
/// literal `fakesig`: stave reads the data-center claim without
/// verifying anything, which is the only property under test.
pub fn fake_jwt(payload: Value) -> String {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256","typ":"JWT"}"#);
    let body = URL_SAFE_NO_PAD.encode(payload.to_string().as_bytes());
    format!("{header}.{body}.fakesig")
}

/// A token whose `dc` claim derives the synthetic tenant endpoint.
pub fn jwt_with_dc_claim() -> String {
    fake_jwt(serde_json::json!({"dc": SYNTHETIC_DC, "sub": "svc-example"}))
}

/// One page of a GraphQL connection response, ready to hand to
/// wiremock: `{"data": {"<root>": {"nodes": [...], "pageInfo": {...}}}}`.
pub fn connection_page(root_field: &str, nodes: Vec<Value>, next_cursor: Option<&str>) -> Value {
    let page_info = match next_cursor {
        Some(cursor) => serde_json::json!({"hasNextPage": true, "endCursor": cursor}),
        None => serde_json::json!({"hasNextPage": false, "endCursor": null}),
    };
    serde_json::json!({"data": {root_field: {"nodes": nodes, "pageInfo": page_info}}})
}

/// The `variables` object of a GraphQL request body captured by wiremock.
pub fn request_variables(body: &[u8]) -> Value {
    let parsed: Value = serde_json::from_slice(body).expect("request body is JSON");
    parsed.get("variables").cloned().unwrap_or_default()
}
