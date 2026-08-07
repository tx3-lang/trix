//! Shared harness for the trix test suites (`tests/cli.rs`, `tests/contract.rs`).
//!
//! Every spawned `trix` is hermetic by construction:
//! - `TX3_HOME` points at a per-test throwaway root, so the global config,
//!   telemetry state, and default tool lookup never touch the developer's
//!   real `~/.tx3` (and parallel tests can't race on it). Works on every OS,
//!   unlike faking `$HOME`.
//! - `PATH` points at an empty directory, so nothing on the machine
//!   (a real `tx3up`, `tx3c`, …) can leak into a test.
//! - Inherited `TX3_*` variables are scrubbed.
//!
//! Consequently no test here can depend on an installed toolchain: a test
//! either needs no tool at all (`tests/cli/`) or drives the fake `tx3c`
//! (`tests/contract/`, see [`fake_tx3c_path`]).

#![allow(dead_code)] // shared by multiple test crates; each uses a subset

use assert_cmd::cargo::cargo_bin_cmd;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use tempfile::TempDir;

use trix::commands::test::Test as TestConfig;
use trix::config::RootConfig;
use trix::devnet::Config as DevnetConfig;

pub struct TestContext {
    temp: TempDir,
}

impl TestContext {
    /// The standard context: telemetry pre-disabled so runs are silent and
    /// never attempt an OTLP export.
    pub fn new() -> Self {
        let ctx = Self::new_unseeded();
        ctx.write_home_file("trix/config.toml", "[telemetry]\nenabled = false\n");
        ctx
    }

    /// A context with a pristine `TX3_HOME` — for tests covering the
    /// first-run behavior itself (global config creation, telemetry banner).
    pub fn new_unseeded() -> Self {
        let temp = TempDir::new().expect("failed to create temp directory");
        fs::create_dir_all(temp.path().join("project")).unwrap();
        fs::create_dir_all(temp.path().join("tx3-home")).unwrap();
        fs::create_dir_all(temp.path().join("empty-path")).unwrap();
        Self { temp }
    }

    /// The project directory `trix` runs in.
    pub fn path(&self) -> PathBuf {
        self.temp.path().join("project")
    }

    /// The isolated stand-in for `~/.tx3`.
    pub fn tx3_home(&self) -> PathBuf {
        self.temp.path().join("tx3-home")
    }

    fn write_home_file(&self, rel: &str, content: &str) {
        let full = self.tx3_home().join(rel);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, content).unwrap();
    }

    /// Run trix in the project directory, hermetically.
    pub fn run_trix(&self, args: &[&str]) -> CommandResult {
        self.run_trix_with_env(args, &[])
    }

    /// Run trix with extra environment variables on top of the hermetic base.
    pub fn run_trix_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> CommandResult {
        let mut cmd = cargo_bin_cmd!("trix");
        cmd.args(args);
        cmd.current_dir(self.path());

        cmd.env("TX3_HOME", self.tx3_home());
        cmd.env("PATH", self.temp.path().join("empty-path"));
        for var in [
            "TX3_TX3C_PATH",
            "TX3_DOLOS_PATH",
            "TX3_CSHELL_PATH",
            "TX3_SKIP_COMPAT_CHECK",
        ] {
            cmd.env_remove(var);
        }

        for (key, value) in envs {
            cmd.env(key, value);
        }

        let output = cmd.output().expect("Failed to execute trix command");

        CommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status,
        }
    }

    /// Run trix against the fake `tx3c` (compiling it on first use), logging
    /// every tx3c invocation for [`Self::tx3c_invocations`]. `fake_envs`
    /// steers the fake — see `tests/harness/fake_tx3c.rs`.
    pub fn run_trix_with_fake_tx3c(
        &self,
        args: &[&str],
        fake_envs: &[(&str, &str)],
    ) -> CommandResult {
        let fake = fake_tx3c_path().display().to_string();
        let log = self.args_log().display().to_string();
        let mut envs: Vec<(&str, &str)> = vec![
            ("TX3_TX3C_PATH", fake.as_str()),
            ("FAKE_TX3C_ARGS_LOG", log.as_str()),
        ];
        envs.extend_from_slice(fake_envs);
        self.run_trix_with_env(args, &envs)
    }

    fn args_log(&self) -> PathBuf {
        self.temp.path().join("tx3c-args.log")
    }

    /// Every `tx3c` invocation the run produced, in order, as argv vectors
    /// (without the executable itself). Empty if tx3c was never spawned.
    pub fn tx3c_invocations(&self) -> Vec<Vec<String>> {
        match fs::read_to_string(self.args_log()) {
            Ok(content) => content
                .lines()
                .map(|line| line.split('\u{1f}').map(str::to_string).collect())
                .collect(),
            Err(_) => Vec::new(),
        }
    }

    // ------------------------------------------------------------------
    // File helpers (relative to the project directory)
    // ------------------------------------------------------------------

    pub fn file_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path().join(path)
    }

    pub fn read_file(&self, path: impl AsRef<Path>) -> String {
        let full_path = self.file_path(path);
        fs::read_to_string(&full_path)
            .unwrap_or_else(|_| panic!("Failed to read file: {}", full_path.display()))
    }

    pub fn write_file(&self, path: impl AsRef<Path>, content: &str) {
        let full_path = self.file_path(&path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|_| panic!("Failed to create directory: {}", parent.display()));
        }
        fs::write(&full_path, content)
            .unwrap_or_else(|_| panic!("Failed to write file: {}", full_path.display()));
    }

    pub fn assert_file_exists(&self, path: impl AsRef<Path>) {
        let full_path = self.file_path(&path);
        assert!(
            full_path.exists(),
            "Expected file to exist: {}",
            full_path.display()
        );
    }

    pub fn assert_file_contains(&self, path: impl AsRef<Path>, pattern: &str) {
        let content = self.read_file(path);
        assert!(
            content.contains(pattern),
            "Expected file to contain '{}', but it didn't.\n\nContent:\n{}",
            pattern,
            content
        );
    }

    // ------------------------------------------------------------------
    // Typed config loaders
    // ------------------------------------------------------------------

    pub fn load_trix_config(&self) -> RootConfig {
        let path = self.file_path("trix.toml");
        RootConfig::load(&path).expect("Failed to load trix.toml config")
    }

    pub fn load_devnet_config(&self) -> DevnetConfig {
        let path = self.file_path("devnet.toml");
        DevnetConfig::load(&path).expect("Failed to load devnet.toml config")
    }

    pub fn load_test_config(&self) -> TestConfig {
        let path = self.file_path("tests/basic.toml");
        TestConfig::load(&path).expect("Failed to load tests/basic.toml config")
    }

    // ------------------------------------------------------------------
    // Interface-cache fixtures
    // ------------------------------------------------------------------

    /// Copy the use-stub fixture into the project's `.tx3/tii/...` cache
    /// for `(scope, name, version)`. Returns the fixture's digest so the
    /// caller can write a matching trix.toml entry.
    pub fn prime_interface_cache(&self, scope: &str, name: &str, version: &str) -> String {
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/use-stub")
            .join(scope)
            .join(name)
            .join(version);
        assert!(
            fixture_root.is_dir(),
            "fixture missing for {}/{}@{}: expected at {}",
            scope,
            name,
            version,
            fixture_root.display(),
        );
        let dest_root = self
            .path()
            .join(".tx3/tii")
            .join(scope)
            .join(name)
            .join(version);
        fs::create_dir_all(&dest_root).expect("create interface cache dir");
        for entry in fs::read_dir(&fixture_root).expect("read fixture") {
            let entry = entry.expect("fixture entry");
            let path = entry.path();
            let file_name = path.file_name().unwrap();
            fs::copy(&path, dest_root.join(file_name)).expect("copy fixture file");
        }
        let metadata = fs::read_to_string(fixture_root.join("metadata.json"))
            .expect("read fixture metadata.json");
        let value: serde_json::Value =
            serde_json::from_str(&metadata).expect("parse fixture metadata.json");
        value
            .get("digest")
            .and_then(|v| v.as_str())
            .expect("digest in fixture metadata.json")
            .to_string()
    }

    /// Append an `[interfaces.<alias>]` table to the project's trix.toml so
    /// the rest of the project sees the primed cache as a declared interface.
    pub fn declare_interface(
        &self,
        alias: &str,
        scope: &str,
        name: &str,
        version: &str,
        digest: &str,
    ) {
        let mut content = self.read_file("trix.toml");
        if !content.ends_with('\n') {
            content.push('\n');
        }
        content.push_str(&format!(
            "\n[interfaces.{}]\nref = \"{}/{}:{}\"\ndigest = \"{}\"\n",
            alias, scope, name, version, digest,
        ));
        self.write_file("trix.toml", &content);
    }

    /// The path of the codegen-template fixture, for `[[codegen]]` entries.
    pub fn codegen_template_dir(&self) -> String {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/codegen-template")
            .to_str()
            .expect("fixture path should be valid UTF-8")
            .to_string()
    }

    /// Append a `[[codegen]]` entry pointing at the local template fixture.
    pub fn declare_codegen(&self) {
        let mut trix_toml = self.read_file("trix.toml");
        trix_toml.push_str(&format!(
            "\n[[codegen]]\noutput_dir = \"gen\"\nplugin = {{ repo = \"{}\", path = \".\" }}\n",
            self.codegen_template_dir()
        ));
        self.write_file("trix.toml", &trix_toml);
    }
}

pub struct CommandResult {
    pub stdout: String,
    pub stderr: String,
    pub status: std::process::ExitStatus,
}

impl CommandResult {
    pub fn success(&self) -> bool {
        self.status.success()
    }

    pub fn combined(&self) -> String {
        format!("{}{}", self.stdout, self.stderr)
    }
}

pub fn assert_success(result: &CommandResult) {
    assert!(
        result.success(),
        "Expected command to succeed but it failed.\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
        result.stdout,
        result.stderr
    );
}

pub fn assert_failure_mentioning(result: &CommandResult, pattern: &str) {
    assert!(
        !result.success(),
        "Expected command to fail but it succeeded.\n\nSTDOUT:\n{}",
        result.stdout
    );
    // miette's fancy renderer wraps messages across `│`-guttered lines;
    // normalize both sides to plain single-spaced text before matching.
    let normalize = |s: &str| {
        s.replace('│', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    };
    let combined = result.combined();
    assert!(
        normalize(&combined).contains(&normalize(pattern)),
        "Expected failure output to mention '{}', but it didn't.\n\nOUTPUT:\n{}",
        pattern,
        combined
    );
}

pub fn assert_output_contains(result: &CommandResult, pattern: &str) {
    assert!(
        result.stdout.contains(pattern),
        "Expected stdout to contain '{}', but it didn't.\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
        pattern,
        result.stdout,
        result.stderr
    );
}

/// The fake `tx3c` binary, compiled once per test process from
/// `tests/harness/fake_tx3c.rs` with plain `rustc`. Not a cargo target on
/// purpose: it stays out of release artifacts and the dist/publish surface.
pub fn fake_tx3c_path() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/harness/fake_tx3c.rs");
        let out_dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR"));
        fs::create_dir_all(&out_dir).expect("create target tmp dir");
        // Per-process name: always freshly compiled, never stale.
        let exe = out_dir.join(format!(
            "fake-tx3c-{}{}",
            std::process::id(),
            std::env::consts::EXE_SUFFIX
        ));
        let status = std::process::Command::new("rustc")
            .arg("--edition=2021")
            .arg("-o")
            .arg(&exe)
            .arg(&src)
            .status()
            .expect("rustc must be available (cargo test implies it)");
        assert!(status.success(), "failed to compile the fake tx3c");
        exe
    })
}
