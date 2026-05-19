# External CLI Delegation: `trix` as a Driver

## Status

Accepted — 2026-05-19.

## Context

`trix` is the Tx3 package manager and workspace orchestrator. It does not
implement the operations it exposes; it coordinates a set of dependent tools:
the language compiler (`tx3c`), the local chain node (`dolos`), the wallet /
transaction client (`cshell`), and others over time.

Each dependent tool evolves on its own cadence, and the internal APIs of
several are unstable by design. Binding `trix` to any of them as a linked
library couples their release cadences: a `trix` release would be pinned to a
specific implementation version, and an unrelated change in that tool would
risk `trix` churn. `trix` and its dependents must move independently.

## Decision

**`trix` links no implementation crate of a dependent tool. It interacts with
each tool only by invoking its binary as a subprocess. A tool's command-line
surface — its subcommands, flags, and structured (JSON) I/O — is the contract
between `trix` and that tool.**

This is the driver pattern, as `cargo` uses with `rustc`: the stable,
user-facing verbs live in the driver (`trix`); each capability lives behind a
small, flag-driven tool the driver shells out to. The toolchain manager
(`tx3up`) installs and pairs compatible versions, as `rustup` does for the
`cargo`/`rustc` pair.

### Division of responsibility

Each tool owns its domain; `trix` owns orchestration and presentation —
selecting what to run, supplying project/profile context, and rendering
results from each tool's structured output. No domain logic of a dependent
tool is reimplemented in `trix`.

The language compiler is the primary instance: `trix` delegates all low-level
Tx3 operations (parse, analyze, lower, decode an artifact) to `tx3c` through
its CLI, reconstructing diagnostics and IR presentation from `tx3c`'s JSON
output. `dolos` (devnet) and `cshell` (wallets, submission) are delegated the
same way. The mechanism is uniform; the tools differ only in domain.

## The contract and its versioning

Because `trix` shares no types with a dependent tool, that tool's CLI is an
interface `trix` must version. The principles are tool-agnostic:

1. **No in-band schema markers.** Versioning belongs to the surface, not to
   each payload. Stamping every message with a schema tag versions one
   payload, not the contract, and bloats the wire.

2. **Gate on the binary version, against a per-tool window.** Compatibility
   for every dependent tool lives in one matrix (`trix`'s `spawn::compat`):
   an inclusive lower bound (the oldest release whose surface `trix` relies
   on) and an exclusive upper bound at the **next major version**. A breaking
   change to a tool's CLI is expected to be signalled by a major version bump
   (semver); `trix` therefore accepts any release within the same major and
   needs updating only when a tool makes a breaking, major change — not on
   every minor. `spawn::ensure_supported(tool)` probes `<tool> --version`,
   range-checks, caches per process, and fails with a distinct, actionable
   message per direction (too old → update the toolchain via `tx3up`; too new
   → update `trix`).

3. **Escape hatch for unreleased toolchains.** A locally built tool carries
   the new surface but still reports its pre-release version. An environment
   override bypasses the window for development and CI against an unreleased
   toolchain; it is not for end users.

Structured payloads are objects, not bare arrays, so they stay extensible:
additive fields are backward-compatible and need no version change; only
breaking changes do, paired with a tool major bump and a matrix update.

## Consequences

**Positive**

- `trix`'s version is decoupled from every dependent tool's. Each ships on its
  own cadence; `trix` follows only to adopt a new capability or a major break.
- The integration surface is small, explicit, and testable from outside.
- Implementation-internal API instability cannot leak into `trix`.
- One place (`spawn::compat`) describes every external-tool compatibility
  requirement.

**Costs**

- A process boundary per operation. Acceptable: operations are coarse-grained,
  so spawn cost is negligible against the work, and `trix` already spawned
  these tools.
- Each tool's CLI / structured I/O is a real interface with real discipline:
  a breaking change requires a major version bump on that tool and a
  matrix-window update, coordinated across repos.
- Release sequencing: a tool release satisfying `trix`'s window must be
  published and resolvable (via the toolchain dir / `tx3up`) before the
  `trix` that requires it.

## Alternatives considered

- **Link a dependent tool's crates, track versions tightly.** The coupling
  this decision exists to avoid; it keeps that tool's API instability inside
  `trix`.
- **Link only for a subset of operations.** A partial dependency still pins
  `trix` to a crate version and complicates the build; the marginal
  in-process speedup is irrelevant for coarse operations.
- **In-band payload version markers.** Brittle; superseded by binary-version
  windowing.
- **An exclusive upper bound at the next minor.** Rejected: it forces a `trix`
  update for every dependent-tool minor even when nothing breaks. The next
  *major* is the semver-correct breaking-change signal.
