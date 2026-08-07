//! Reference handling at the CLI boundary. The grammar and resolution rules
//! are unit-tested in `src/refs.rs` and `src/interfaces/resolve.rs`; each
//! test here probes that one chokepoint is actually wired to them.

use crate::harness::*;

/// `trix use` rejects an alias-only reference at clap parse time
/// (`ProtocolRef::parse_registry` as value parser), because aliases don't
/// carry version info.
#[test]
fn use_rejects_alias_only_reference() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix(&["use", "widget"]);
    assert!(
        !result.success(),
        "expected failure, got: {:?}",
        result.stdout
    );
    let combined = result.combined();
    assert!(
        combined.contains("alias")
            || combined.contains("registry reference")
            || combined.contains("scope"),
        "output should explain the registry-only requirement:\n{}",
        combined
    );
}

/// Unknown alias on `inspect tir --tx`: the resolver rejects it by name
/// before any tool would be spawned — this fails identically on a machine
/// with no toolchain installed.
#[test]
fn inspect_tir_rejects_unknown_alias() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let result = ctx.run_trix(&["inspect", "tir", "--tx", "ghost::transfer"]);
    assert_failure_mentioning(&result, "ghost");
}

/// A hand-edited `[interfaces]` entry that breaks the lockfile rules is
/// rejected by every interface-aware command via `interfaces::validate`,
/// before any tool or network access. One wiring probe; the rule variants
/// (latest, unpinned, duplicates, …) are unit tests on `validate`.
#[test]
fn scoped_commands_reject_invalid_interfaces_table() {
    let ctx = TestContext::new();
    assert_success(&ctx.run_trix(&["init", "--yes"]));

    let mut content = ctx.read_file("trix.toml");
    content.push_str("\n[interfaces.widget]\nref = \"widget\"\ndigest = \"sha256:deadbeef\"\n");
    ctx.write_file("trix.toml", &content);

    let result = ctx.run_trix(&["inspect", "tir", "--tx", "transfer"]);
    assert_failure_mentioning(&result, "alias-only");
}
