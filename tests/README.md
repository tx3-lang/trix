# Trix test suites

Trix delegates all language work to helper binaries (`tx3c`, `dolos`,
`cshell`) it spawns as subprocesses. That process boundary is also the test
boundary: **every assertion lives at the innermost layer that can make it,
and no test in this repo requires a real helper binary.** Anything that
needs real cross-binary interop belongs to the umbrella's DX e2e journeys
(`solution/e2e/` in the `tx3` umbrella repo), which gate toolchain releases
with real released binaries on real runners.

The three layers, innermost first:

| Layer | Where | Spawns | Asks |
|---|---|---|---|
| **Unit** | `src/**` `#[cfg(test)]` | nothing | Are the rules right? (ref grammar, `[interfaces]` validation, cache integrity, resolver, compat window) |
| **CLI** (`tests/cli.rs`) | `tests/cli/` | `trix` only | Is the binary wired to the rules? (arg parsing, exit codes, `init` filesystem behavior, fail-before-spawn error paths, registry pulls against the in-process OCI stub) |
| **Contract** (`tests/contract.rs`) | `tests/contract/` | `trix` + a **fake** `tx3c` | Is trix's side of the tx3c contract right? (argv, paths, ordering, output interpretation, version gate) |

There is deliberately no fourth layer. A trix test that needs a real `tx3c`
is asserting cross-repo behavior — journey territory — and would make CI
depend on a toolchain install.

## Hermeticity

Every spawned `trix` runs with:

- `TX3_HOME` pointing at a per-test throwaway root (the `~/.tx3` override
  seam in `src/home.rs`) — no test touches the real `~/.tx3`, no parallel
  tests race on the global config, and default tool lookup can never pick
  up a developer's installed toolchain;
- `PATH` pointing at an empty directory — nothing on the machine can leak in;
- inherited `TX3_*` variables scrubbed;
- telemetry pre-disabled (except tests covering first-run behavior itself).

The suites behave identically on a developer laptop with a full toolchain
and on a bare CI runner.

## The fake tx3c

`tests/harness/fake_tx3c.rs` is a std-only stand-in for `tx3c`, compiled at
test time with plain `rustc` (see `harness::fake_tx3c_path`) — deliberately
**not** a cargo target, so it can never end up in release artifacts or the
dist/publish surface. It implements exactly the CLI surface pinned by
`src/spawn/tx3c.rs` + `src/spawn/compat.rs`, records every invocation's
argv, and is steered per test via `FAKE_TX3C_*` environment variables
(reported version, canned diagnostics, simulated failure). The fake also
unlocks coverage real binaries can't offer: the compat gate is exercised
against arbitrary reported versions.

If trix's spawn contract changes (new flag, new subcommand), update the
fake alongside `src/spawn/` — and expect the umbrella journeys to catch any
drift between the fake's pretense and the real tool at release time.

## The OCI registry stub

`tests/harness/oci_stub.rs` is an in-process OCI Distribution registry
serving the read side of an anonymous pull (`/v2/` probe, manifest, blobs)
on `127.0.0.1:<random>`, with real sha256 digests because `oci_client`
verifies every blob against its descriptor. A test builds a
`StubProtocolImage`, serves its routes, and points the project at it with
`TestContext::set_registry_url`. It records every request path, so a test
can assert *which* repository path the client addressed — the way the
lowercase-addressing regression is locked.

Pulls are trix's own code, not a helper binary, so registry tests belong to
the CLI layer and the suite stays offline.

## Running

```bash
cargo test                     # everything: unit + cli + contract
cargo test --lib               # unit layer only
cargo test --test cli          # CLI suite
cargo test --test contract     # contract suite
```

No setup, no installed toolchain, no network.

## Adding a test

Ask what the assertion is about:

- **A rule** (parsing, validation, resolution, version arithmetic) → unit
  test next to the rule in `src/`. If the rule is buried in an I/O path,
  extract it (see `interfaces::verify_cache_at` for the pattern).
- **Wiring** (does command X actually run rule Y, with which exit code and
  message) → `tests/cli/`. One probe per chokepoint; don't re-enumerate the
  rule's variants here. A pull-path assertion goes here too, against the
  OCI stub.
- **The tx3c contract** (what trix passes, which artifact feeds which
  subcommand, how output/failures are interpreted) → `tests/contract/`,
  asserting on `ctx.tx3c_invocations()` and the fake's file outputs.
- **Real binaries composing** (devnet round-trips, real codegen output,
  install flows) → not here; add a journey in the umbrella's `solution/e2e/`
  (see its README and the `add-e2e-journey` skill).

Fixtures live in `tests/fixtures/` (`use-stub/` — a cached interface, also
the layer payload the OCI stub serves; `codegen-template/` — a minimal
codegen plugin). `tests/infra/` is
unrelated observability tooling, not part of the suites.
