//! A stand-in `tx3c` for the contract test suite (`tests/contract/`).
//!
//! NOT a cargo target: the harness compiles this file with plain `rustc` at
//! test time (see `harness::fake_tx3c_path`), so it never appears in release
//! artifacts and adds nothing to the dist/publish surface. std-only.
//!
//! It implements exactly the CLI surface trix relies on (the contract pinned
//! by `src/spawn/tx3c.rs` + `src/spawn/compat.rs`) and is steered per test
//! via environment variables:
//!
//! - `FAKE_TX3C_VERSION`    version reported by `--version` (default 0.22.0)
//! - `FAKE_TX3C_ARGS_LOG`   file to append each invocation's argv to,
//!   US-separated (`\x1f`), one line per invocation
//! - `FAKE_TX3C_DIAGNOSTICS` raw stdout for `build --diagnostics-format json`
//!   (default `{"diagnostics":[]}`); pass non-JSON to simulate a broken tool
//! - `FAKE_TX3C_EXIT`       exit with this code (after logging argv),
//!   printing `FAKE_TX3C_STDERR` to stderr — simulates tool failure

use std::io::Write as _;

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn json_escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();

    if let Ok(log) = std::env::var("FAKE_TX3C_ARGS_LOG") {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&log) {
            let _ = writeln!(f, "{}", args.join("\u{1f}"));
        }
    }

    // The version probe must answer even when the fake is told to fail,
    // so compat gating and failure simulation compose per invocation.
    if args.iter().any(|a| a == "--version") {
        let version =
            std::env::var("FAKE_TX3C_VERSION").unwrap_or_else(|_| "0.22.0".to_string());
        println!("tx3c {}", version);
        return;
    }

    if let Ok(code) = std::env::var("FAKE_TX3C_EXIT") {
        let code: i32 = code.parse().unwrap_or(1);
        if code != 0 {
            let msg = std::env::var("FAKE_TX3C_STDERR")
                .unwrap_or_else(|_| "fake tx3c: simulated failure".to_string());
            eprintln!("{}", msg);
            std::process::exit(code);
        }
    }

    match args.first().map(String::as_str) {
        Some("build") => {
            if args.iter().any(|a| a == "--diagnostics-format") {
                // `check` path: print the envelope; trix parses stdout
                // regardless of exit status.
                let envelope = std::env::var("FAKE_TX3C_DIAGNOSTICS")
                    .unwrap_or_else(|_| r#"{"diagnostics":[]}"#.to_string());
                println!("{}", envelope);
                return;
            }
            match flag_value(&args, "--emit").as_deref() {
                Some("tii") => {
                    let output = flag_value(&args, "--output").expect("--output missing");
                    std::fs::write(
                        &output,
                        r#"{"protocol":{"name":"fake"},"transactions":{"transfer":{}}}"#,
                    )
                    .expect("write tii");
                }
                Some("tir-json") => {
                    let tx = flag_value(&args, "--tx").expect("--tx missing");
                    let source = args.get(1).cloned().unwrap_or_default();
                    println!(
                        r#"{{"tx":"{}","from":"source","source":"{}"}}"#,
                        json_escape(&tx),
                        json_escape(&source)
                    );
                }
                other => {
                    eprintln!("fake tx3c: unsupported build emit {:?}", other);
                    std::process::exit(2);
                }
            }
        }
        Some("decode") => {
            let tii = flag_value(&args, "--tii").expect("--tii missing");
            let tx = flag_value(&args, "--tx").expect("--tx missing");
            println!(
                r#"{{"tx":"{}","from":"tii","tii":"{}"}}"#,
                json_escape(&tx),
                json_escape(&tii)
            );
        }
        Some("codegen") => {
            let tii = flag_value(&args, "--tii").expect("--tii missing");
            let template = flag_value(&args, "--template").expect("--template missing");
            let output = flag_value(&args, "--output").expect("--output missing");
            // Optional: the template-options channel. Absent means `{}` —
            // real tx3c defaults it, so trix omits the flag when it has
            // nothing to say.
            let options = flag_value(&args, "--options").unwrap_or_else(|| "{}".to_string());
            let dest = std::path::Path::new(&output).join("bindings.txt");
            std::fs::write(
                &dest,
                format!("tii={}\ntemplate={}\noptions={}\n", tii, template, options),
            )
            .expect("write bindings");
        }
        other => {
            eprintln!("fake tx3c: unknown subcommand {:?}", other);
            std::process::exit(2);
        }
    }
}
