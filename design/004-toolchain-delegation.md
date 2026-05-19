# Toolchain Delegation: `trix` as a Driver, `tx3c` as the Compiler

## Status

Accepted — 2026-05-19.

## Context

`trix` is the Tx3 package manager and workspace orchestrator. Its value —
project layout, profiles, registry, devnet, codegen orchestration — evolves on
its own cadence. The Tx3 language implementation (parsing, analysis, lowering,
IR encoding) evolves on the toolchain's cadence, and its internal APIs are
unstable by design.

Binding `trix` to the language implementation as linked libraries couples these
two cadences: a `trix` release would be pinned to a specific compiler/IR
version, and every compiler change would risk unrelated `trix` churn. The two
concerns must be able to move independently.

## Decision

**`trix` links no `tx3-*` crate. Every low-level Tx3 language operation is
delegated to the `tx3c` binary, invoked as a subprocess. The `tx3c`
command-line surface — its subcommands, flags, and JSON output — is the
contract between the two.**

This is the driver/compiler split that `cargo`↔`rustc` uses: the stable,
user-facing verbs live in the driver (`trix`); the language implementation
lives behind a small, flag-driven compiler (`tx3c`) the driver shells out to.
The toolchain manager (`tx3up`) pairs compatible versions, as `rustup` does for
`cargo`/`rustc`.

### `tx3c` surface

`tx3c` exposes two symmetric verbs, each a single pipeline driven by `--emit`:

- **`build <src>`** — forward pipeline (source → artifact). `--emit tii`
  produces the TII; `--emit tir-json --tx <name>` prints a transaction's
  lowered v1beta0 TIR as JSON; an empty `--emit` runs the front end
  (parse + analyze) and stops before lowering — the "check" semantic, modelled
  on `rustc --emit=metadata`. `--diagnostics-format human|json` selects
  rendering; `json` is the machine contract (mirrors
  `rustc --error-format=json`).
- **`decode --tii <path>`** — reverse pipeline (compiled artifact → report).
  `--emit tir-json --tx <name>` decodes the artifact's TIR payload and prints
  the same JSON shape as `build --emit tir-json`, so a consumer cannot tell
  source-derived from artifact-derived IR (cf. `protoc --decode`,
  `llvm-dis`).

### Division of responsibility

`tx3c` owns all language semantics. `trix` owns orchestration and
presentation: it selects what to compile, supplies project/profile context,
and renders results — reconstructing diagnostic output from the JSON contract,
and serializing IR through its own formatting path. No Tx3 semantics live in
`trix`.

| `trix` flow | `tx3c` invocation |
|---|---|
| `check` | `build … --diagnostics-format json` (no `--emit`) |
| `inspect tir` (project) | `build … --emit tir-json --tx` |
| `inspect tir` (interface) | `decode --tii … --emit tir-json --tx` |
| `build` / `invoke` / `test` / `publish` | `build --emit tii` |
| `codegen` | `build` + `codegen` |

## The contract and its versioning

Because `trix` shares no types with the compiler, the `tx3c` CLI is an
interface that must be versioned. Two principles:

1. **No in-band schema markers.** Versioning belongs to the surface, not to
   each payload. Stamping every JSON message with a schema tag versions one
   payload, not the contract, and bloats the wire.

2. **Gate on the binary version, against a window.** Compatibility is a single
   matrix (`trix`'s `spawn::compat`) of `Compat { tool, min, before }` — an
   inclusive lower and *exclusive upper* bound. Both bounds matter: a too-old
   binary lacks capabilities `trix` relies on; and because the toolchain is
   pre-1.0 (a new minor may change the CLI), a too-new binary may have moved
   the contract. `spawn::ensure_supported(tool)` probes `<tool> --version`,
   range-checks, caches per process, and fails with a distinct, actionable
   message per direction (too old → run `tx3up`; too new → update `trix`). The
   same matrix fronts other external tools (`cshell`, `dolos`), gated when
   they need entries.

JSON shapes are objects, not bare arrays, so they stay extensible: additive
fields are backward-compatible and need no version change; only breaking
changes do, paired with widening the matrix window.

## Consequences

**Positive**

- `trix`'s version is decoupled from the toolchain's. The compiler ships on
  its own cadence; `trix` follows only when it wants a new capability.
- The integration surface is small, explicit, and testable from the outside.
- Compiler-internal API instability cannot leak into `trix`.
- One place (`spawn::compat`) describes every external-tool compatibility
  requirement.

**Costs**

- A process boundary per operation. Acceptable: operations are coarse-grained
  (whole-project build/check), so spawn cost is negligible against the work.
- The CLI/JSON is a real interface with real discipline: breaking changes to
  subcommands, flags, or JSON shapes require a `tx3c` version bump and a
  matrix-window update, coordinated across repos.
- Release sequencing: a `tx3c` satisfying `trix`'s window must be published and
  resolvable (via the toolchain dir / `tx3up`) before the corresponding `trix`.

## Alternatives considered

- **Link the language crates, track versions tightly.** This is the coupling
  the decision exists to avoid; it keeps compiler API instability inside
  `trix`.
- **Link the crates only for a subset of operations.** A partial dependency
  still pins `trix` to a crate version and complicates the build; the marginal
  in-process speedup is irrelevant for coarse operations.
- **In-band payload version markers.** Brittle (see above); superseded by
  binary-version windowing.
- **A dedicated subcommand per operation (`check`, `decode-tir`, …).** Grows
  the long-term CLI surface. Flags on `build` plus one symmetric `decode` verb
  cover every need with minimal surface (the `rustc`/`protoc` precedent).
