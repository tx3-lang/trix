# High-level Trix TOML Configuration Requirements

## Goals

The tx3 toolchain must support a single TOML schema that can describe both:
- a **workspace** (development + execution context)
- one or more **protocols** (publishable packages)

The design must be:
- ergonomic for the common case
- explicit and deterministic for tooling
- extensible to multi-protocol repositories

---

## Core Principles

1. **Every protocol lives in a workspace**
   - A workspace may be *implicit* or *explicit*
   - This mirrors Cargo’s model

2. **Single TOML schema**
   - The same TOML grammar is used in all cases
   - Interpretation depends on which top-level tables are present

3. **Clear ownership rules**
   - Each section is owned either by the workspace or by a protocol
   - Ownership determines publishability and validation

---

## Ownership Rules (Normative)

| TOML Section            | Owner      | Publishable |
|------------------------|------------|-------------|
| `[protocol]`           | protocol   | ✅ yes      |
| `[protocols.*]`        | protocol   | ✅ yes      |
| `[workspace]`          | workspace  | ❌ no       |
| `[ledger]`             | workspace  | ❌ no       |
| `[profiles.*]`         | workspace  | ❌ no       |

---

## Implicit Workspace (Minimal Case)

If a `trix.toml` file contains:
- `[protocol]`
- and no `[workspace]`

Then:
- the directory is treated as an **implicit workspace**
- the protocol is considered **inline**
- workspace-owned sections (`ledger`, `profiles`) are allowed

This is the **default and recommended path** for single-protocol projects.

---

## Explicit Workspace (Multi-Protocol Case)

If a `trix.toml` file contains `[workspace]`:

- the file defines a **workspace root**
- protocols may be:
  - defined inline (optional)
  - or defined in nested TOML files
- protocol membership is declared via:
  - `[workspace.protocols.members]`

Example intent (not syntax-specific):

- root `trix.toml` → workspace-only
- `protocols/*/trix.toml` → protocol-only

---

## Protocol Definition Rules

A protocol definition:
- must include identity and versioning
- must be environment-agnostic
- must not depend on local paths, env files, or devnet config

Allowed fields include (illustrative):
- name / id
- scope / namespace
- version
- entrypoint (`main`)
- description

---

## Workspace Definition Rules

Workspace-level configuration includes:
- ledger constraints (e.g. Cardano era)
- execution profiles
- local / CI / devnet configuration
- environment files

Workspace configuration:
- is never published
- is ignored by registries
- applies to all protocols in the workspace

---

## Ledger Rules

- Ledger configuration is **workspace-owned**
- Protocols implicitly target the workspace ledger
- At minimum, ledger family and minimum era must be declared
- Tooling must refuse to compile if ledger constraints are missing or incompatible

---

## Profile Rules

- Profiles are **workspace-owned**
- Profiles define *where and how* protocols are executed
- Profiles may contain network-specific configuration blocks
- Profiles must not appear in protocol-only TOML files

---

## Publishing Semantics (`trix publish`)

When publishing a protocol:

- Only protocol-owned sections are read
- Workspace-owned sections are ignored
- Publishing fails if:
  - workspace-only fields appear in a protocol-only TOML
  - required protocol metadata is missing

---

## Validation Rules

1. A TOML file may define:
   - `[protocol]`
   - `[workspace]`
   - or both (inline protocol in implicit workspace)
2. A workspace that declares external protocol members:
   - must not define an inline `[protocol]`
3. Nested protocol TOMLs:
   - must not define `[ledger]` or `[profiles]`
4. Tooling must emit clear errors on ownership violations

---

## Design Intent (Summary)

- One schema
- One mental model
- Minimal happy path
- Explicit scaling path
- Cargo-like ergonomics
- No hidden heuristics

> **trix.toml describes a workspace.  
> A workspace may contain zero or more protocols.  
> In the common case, a single protocol is defined inline.**
