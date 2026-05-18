use assert_cmd::Command;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use trix::commands::test::Test as TestConfig;
use trix::config::RootConfig;
use trix::devnet::Config as DevnetConfig;

/// A test context that provides an isolated temporary directory.
/// Tests can run in parallel because each has its own temp directory.
pub struct TestContext {
    pub temp_dir: TempDir,
}

impl TestContext {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");
        Self { temp_dir }
    }

    /// Returns the path to the temporary directory
    pub fn path(&self) -> &Path {
        self.temp_dir.path()
    }

    /// Run trix command in this temp directory
    pub fn run_trix(&self, args: &[&str]) -> CommandResult {
        let mut cmd = Command::cargo_bin("trix").expect("Failed to find trix binary");
        cmd.args(args);
        cmd.current_dir(self.path());

        for (key, value) in self.tool_envs() {
            cmd.env(key, value);
        }

        let output = cmd.output().expect("Failed to execute trix command");

        CommandResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            status: output.status,
        }
    }

    /// Run trix command with environment overrides
    pub fn run_trix_with_env(&self, args: &[&str], envs: &[(&str, &str)]) -> CommandResult {
        let mut cmd = Command::cargo_bin("trix").expect("Failed to find trix binary");
        cmd.args(args);
        cmd.current_dir(self.path());

        for (key, value) in self.tool_envs() {
            cmd.env(key, value);
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

    pub fn tx3c_path(&self) -> Option<PathBuf> {
        resolve_tool_path("tx3c")
    }

    fn tool_envs(&self) -> Vec<(String, String)> {
        let mut envs = Vec::new();

        if let Some(path) = resolve_tool_path("tx3c") {
            envs.push((
                "TX3_TX3C_PATH".to_string(),
                path.to_string_lossy().to_string(),
            ));
        }

        envs
    }

    /// Get full path to a file in the temp directory
    pub fn file_path(&self, path: impl AsRef<Path>) -> PathBuf {
        self.path().join(path)
    }

    /// Read file from temp directory
    pub fn read_file(&self, path: impl AsRef<Path>) -> String {
        let full_path = self.file_path(path);
        fs::read_to_string(&full_path)
            .unwrap_or_else(|_| panic!("Failed to read file: {}", full_path.display()))
    }

    /// Write file to temp directory (creates parent directories)
    pub fn write_file(&self, path: impl AsRef<Path>, content: &str) {
        let full_path = self.file_path(&path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|_| panic!("Failed to create directory: {}", parent.display()));
        }
        fs::write(&full_path, content)
            .unwrap_or_else(|_| panic!("Failed to write file: {}", full_path.display()));
    }

    /// Assert file exists
    pub fn assert_file_exists(&self, path: impl AsRef<Path>) {
        let full_path = self.file_path(&path);
        assert!(
            full_path.exists(),
            "Expected file to exist: {}",
            full_path.display()
        );
    }

    /// Assert file contains pattern
    pub fn assert_file_contains(&self, path: impl AsRef<Path>, pattern: &str) {
        let content = self.read_file(path);
        assert!(
            content.contains(pattern),
            "Expected file to contain '{}', but it didn't.\n\nContent:\n{}",
            pattern,
            content
        );
    }

    /// Load trix.toml config file and return the parsed RootConfig
    pub fn load_trix_config(&self) -> RootConfig {
        let path = self.file_path("trix.toml");
        RootConfig::load(&path).expect("Failed to load trix.toml config")
    }

    /// Load devnet.toml config file and return the parsed DevnetConfig
    pub fn load_devnet_config(&self) -> DevnetConfig {
        let path = self.file_path("devnet.toml");
        DevnetConfig::load(&path).expect("Failed to load devnet.toml config")
    }

    /// Load tests/basic.toml config file and return the parsed TestConfig
    pub fn load_test_config(&self) -> TestConfig {
        let path = self.file_path("tests/basic.toml");
        TestConfig::load(&path).expect("Failed to load tests/basic.toml config")
    }

    /// Copy the use-stub fixture into the temp dir's `.tx3/protocols/...` cache
    /// for `(scope, name, version)`. Returns the fixture's digest so the
    /// caller can write a matching trix.toml entry.
    pub fn prime_dep_cache(&self, scope: &str, name: &str, version: &str) -> String {
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/e2e/fixtures/use-stub")
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
            .join(".tx3/protocols")
            .join(scope)
            .join(name)
            .join(version);
        fs::create_dir_all(&dest_root).expect("create dep cache dir");
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

    /// Append a `[dependencies.<alias>]` table to the project's trix.toml so
    /// the rest of the project sees the primed cache as a declared dep.
    pub fn declare_dep(
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
            "\n[dependencies.{}]\nref = \"{}/{}:{}\"\ndigest = \"{}\"\n",
            alias, scope, name, version, digest,
        ));
        self.write_file("trix.toml", &content);
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
}

pub fn assert_success(result: &CommandResult) {
    assert!(
        result.success(),
        "Expected command to succeed but it failed.\n\nSTDOUT:\n{}\n\nSTDERR:\n{}",
        result.stdout,
        result.stderr
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

/// Wait for a port to be open with timeout
pub fn wait_for_port(port: u16, timeout_secs: u64) -> bool {
    use std::net::TcpStream;
    use std::time::{Duration, Instant};

    let start = Instant::now();
    let timeout = Duration::from_secs(timeout_secs);

    while start.elapsed() < timeout {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Check if a process is running by PID (Unix only)
#[cfg(unix)]
pub fn is_process_running(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

#[cfg(not(unix))]
pub fn is_process_running(_pid: u32) -> bool {
    // On non-Unix systems, we can't easily check if a process is running
    // This is a simplified check that always returns true
    true
}

#[cfg(feature = "unstable")]
pub mod codegen_deps;
pub mod edge_cases;
pub mod happy_path;
pub mod smoke;
pub mod use_command;

fn resolve_tool_path(tool: &str) -> Option<PathBuf> {
    let env_var = format!("TX3_{}_PATH", tool.to_uppercase());
    if let Ok(path) = std::env::var(&env_var) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let cargo_bin_var = format!("CARGO_BIN_EXE_{tool}");
    if let Ok(path) = std::env::var(&cargo_bin_var) {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }

    let cargo_home = std::env::var("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|_| {
            std::env::var("HOME")
                .map(PathBuf::from)
                .map(|home| home.join(".cargo"))
        })
        .ok()?;

    let mut path = cargo_home.join("bin").join(tool);
    if cfg!(target_os = "windows") {
        path.set_extension("exe");
    }

    if path.is_file() {
        return Some(path);
    }

    None
}
