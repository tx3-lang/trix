# Protocol Dependencies

## Overview

Trix supports two workflows: **authoring** a protocol and **consuming**
existing ones. This document describes consumption — declaring a published
protocol as a dependency, caching it locally, and how the rest of the
toolchain accounts for it.

A consumed protocol is treated as an **isolated sibling**: each `.tx3`
protocol is compiled on its own. There is no source stitching and no tx3
language change — dependencies do not merge symbols into the consuming
project's compilation unit. Integration happens entirely above the parser.

## Reference grammar

Every place a protocol or a transaction is named — CLI flags, `trix.toml`
values, error messages — goes through one canonical grammar
(`src/refs.rs`). It is the only place these strings are parsed or formatted.

```
protocol_ref ::= alias | registry_ref
alias        ::= IDENT                        e.g. "widget"
registry_ref ::= SCOPE "/" NAME [":" VERSION] e.g. "acme/widget:0.1.0"

tx_ref       ::= [protocol_ref "::"] TX_NAME  e.g. "transfer",
                                                   "widget::transfer",
                                                   "acme/widget:0.1.0::transfer"

IDENT, SCOPE, NAME, TX_NAME ::= [a-zA-Z_][a-zA-Z0-9_.-]*
VERSION                     ::= OCI tag (incl. "latest")
```

Disambiguation: a protocol token containing `/` is a `registry_ref`,
otherwise an `alias`. `::` is the only separator between protocol and tx. A
bare `tx_ref` (no protocol qualifier) means the project's own protocol.

`ProtocolRef` and `TxRef` are typed values with `FromStr`/`Display` and serde
adapters, so the same parser and the same error text apply whether the string
came from a CLI flag or `trix.toml`. A separate `Resolver` maps a parsed
reference to a concrete artifact (the project, or a declared dependency) —
parsing and resolution are deliberately distinct concerns.

## Declaring a dependency: `trix use`

```
trix use <scope>/<name>[:<version>] [--alias <name>] [--force] [--dry-run]
```

`trix use` accepts a `registry_ref` only (an alias carries no version). It:

1. Resolves the registry URL via `RootConfig::registry_url()` — the explicit
   `[registry].url`, else the hardcoded `DEFAULT_REGISTRY_URL`
   (`https://oci.tx3.land`). No configuration is required for the common case.
2. Anonymously pulls the OCI artifact (layers: `application/tx3`,
   `application/tii+json`, optional `text/markdown`; plus the image-config
   JSON).
3. **Pins a concrete version.** The publisher-recorded version is preferred,
   then a concrete request tag. A mutable-only tag with no concrete version
   is a hard error pointing at the publisher — trix never invents a
   digest-based pseudo-version.
4. Writes the cache (below) and records a pinned, lockfile-style entry in
   `trix.toml`.

### `trix.toml` schema

```toml
[registry]
url = "https://oci.tx3.land"   # optional; defaults to DEFAULT_REGISTRY_URL

[dependencies.widget]
ref    = "acme/widget:0.1.3"   # one canonical ProtocolRef::Registry string
digest = "sha256:..."          # OCI manifest digest, captured at `trix use`
```

The table key (`widget`) is the alias. `ref` must be a registry reference
with a concrete version — alias-only or `latest` refs are rejected on load
with the same diagnostic the CLI uses. Validation
(`RootConfig::validate_dependencies`, run at the top of every scoped
command): alias is a valid identifier, alias is not the project's own name,
and no two entries map to the same `(scope, name)`.

Existing projects are unaffected: `dependencies` is
`#[serde(default, skip_serializing_if = "NamedMap::is_empty")]`, so
dependency-free `trix.toml` files parse and round-trip unchanged.

## The cache

Dependencies are cached, project-local, under `.tx3/` (gitignored,
toolchain-owned):

```
.tx3/protocols/<scope>/<name>/<version>/
    ├── main.tx3       (application/tx3 layer)
    ├── main.tii       (application/tii+json layer — the published TII)
    ├── README.md      (text/markdown layer, optional)
    └── metadata.json  (ProtocolManifest: scope/name/version/digest/…)
```

The directory key is the **concrete version**, not a content hash. The cache
is project-local and validation guarantees one entry per `(scope, name)`, so
version-keyed directories cannot collide. Content identity is guarded
separately by the digest (below); the directory name is for human
readability and debuggability.

### Restore semantics

`dependencies::restore_all` runs at the top of every command that needs
dependencies. It is a no-op when `[dependencies]` is empty (no
`.tx3/protocols/` is created). For each entry it inspects the cache in a
single pass (`verify_cached` → `CacheStatus`):

| State | Action |
|---|---|
| Valid (present, parses, digest matches `trix.toml`) | use cache, no network |
| Missing (a required file absent) | fetch from the registry, write the cache |
| Invalid (digest mismatch / corrupt metadata / malformed TII) | hard error; surface it directly, do **not** silently refetch — directs the user to `trix use --force` |

The digest in `trix.toml` is the lockfile: it is verified on every restore,
and a registry that rotated a tag's underlying image is a hard error rather
than a silent content swap.

## How commands account for dependencies

Each command treats the project and each dependency as independent
protocols.

- **`trix check`** — parses and analyzes the project's `main.tx3` and each
  dependency's cached `main.tx3` as separate compilation units; diagnostics
  are aggregated and attributed per protocol.
- **`trix build`** — builds only the project's TII
  (`.tx3/tii/main.tii`). Dependency TIIs are taken as published and are not
  recompiled; `restore_all` (which parses and validates each dep TII as JSON)
  is the integrity guarantee.
- **`trix inspect tir --tx <TX_REF>`** — a bare tx targets the project;
  `<alias>::<tx>` or a fully-qualified `registry_ref::<tx>` targets a
  dependency, read from its cached source.
- **`trix invoke --from <PROTOCOL_REF>`** — selects which protocol's TII to
  invoke against (omit → project; alias or registry ref → dependency's cached
  TII). The transaction is chosen interactively by the wallet.
- **`trix publish`** — unchanged; it does not yet record a published
  artifact's own dependencies (future work).

The guiding principle, mirrored across `build`/`inspect`/`invoke`: **trust
the published TII**. Dependency source is only re-compiled where a code path
has no TII-driven alternative.

## Codegen

Dependency-aware codegen lives **only in the unstable codegen path**
(`src/commands/codegen.rs`, `#[cfg(feature = "unstable")]`). The default
legacy path (`src/commands/codegen_legacy.rs`) is intentionally untouched and
remains single-protocol for backward compatibility.

The unstable path delegates rendering to `tx3c codegen --tii`. For each
codegen plugin it runs once per protocol:

- the project's TII is built from source via `builder::build_tii`;
- each dependency's TII is the **cached pre-built published `main.tii`**
  (not recompiled), consistent with `trix build`.

Output layout is **unified**: every protocol is written to its own subdir,
`<output_dir>/<project_name>/` and `<output_dir>/<alias>/`, **regardless of
whether any dependencies are declared**. The path a binding lands at never
depends on dependency count — there is no "layout shifts the moment you add
your first dependency" cliff. This is a deliberate one-time change confined
to the unstable path; the default/legacy layout is unchanged.

## Out of scope (future work)

- `trix publish` recording its own dependencies in the published artifact
  (e.g. an OCI annotation), enabling transitive resolution.
- Cache garbage collection — `trix use --force` repinning leaves prior
  version directories behind.
- Discovery commands (`trix search` / registry listing). Protocols are found
  out-of-band; `trix use` takes the reference directly.
