# Protocol Interfaces

## Overview

Trix supports two workflows: **authoring** a protocol and **interacting**
with already-published ones. This document describes the second — declaring a
published protocol as an **interface**, caching it locally, and how the rest
of the toolchain accounts for it.

### Why "interface", not "dependency"

A dependency is a build-graph edge: *A depends on B* means B is an input to
producing A — remove B and building A breaks. These external protocols are
not that. The project's protocol compiles in complete isolation (no source
stitching, no tx3 language change); removing every declared interface does
not change a single byte of the project's build output.

What we actually have is an **orthogonal interaction link**: other protocols,
already built and deployed by someone else, that the user wants the tooling
to let them *talk to* — invoke a transaction against, generate a client for,
inspect. The relationship lives entirely at the interaction layer, never at
the compilation layer. This is the Solidity model: you don't compile a
deployed contract, you hold its **interface** and call it. The published
artifact trix consumes is literally the TII — the Transaction Invoke
**Interface**.

### Normative vs. informative

The **normative** artifact of an interface is its published TII (`main.tii`).
That is the only part any command consumes. The cached `main.tx3` and
`README.md` are kept purely as human-readable references — **informative,
never compiled or otherwise treated as authoritative**. No command parses,
analyzes, or lowers a cached `.tx3`; even `inspect tir` recovers IR from the
TII (it carries the encoded TIR per transaction), not from source.

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
reference to a concrete artifact (the project, or a declared interface) —
parsing and resolution are deliberately distinct concerns.

## Declaring an interface: `trix use`

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

[interfaces.widget]
ref    = "acme/widget:0.1.3"   # one canonical ProtocolRef::Registry string
digest = "sha256:..."          # OCI manifest digest, captured at `trix use`
```

The table key (`widget`) is the alias. `ref` must be a registry reference
with a concrete version — alias-only or `latest` refs are rejected on load
with the same diagnostic the CLI uses. Validation
(`RootConfig::validate_interfaces`): alias is a valid identifier, alias is
not the project's own name, and no two entries map to the same
`(scope, name)`. It is run by the *consuming* commands and by `trix use`
before writing — never by `build`/`check`.

Existing projects are unaffected: `interfaces` is
`#[serde(default, skip_serializing_if = "NamedMap::is_empty")]`, so
`trix.toml` files with no interfaces parse and round-trip unchanged.

## The cache

Interfaces and the project's own built TII share **one uniform layout**,
project-local under `.tx3/` (gitignored, toolchain-owned):

```
.tx3/tii/<scope>/<name>/<version>/
    ├── main.tx3       (application/tx3 layer — informative)
    ├── main.tii       (application/tii+json layer — normative)
    ├── README.md      (text/markdown layer, optional — informative)
    └── metadata.json  (ProtocolManifest: scope/name/version/digest/…)
```

The project's own TII lands in the same tree
(`.tx3/tii/<scope>/<name>/<version>/main.tii`), using the `local` scope when
`[protocol] scope` is absent. There is no separate "project" vs. "fetched"
directory shape — the toolchain treats every protocol it knows about
uniformly.

The directory key is the **concrete version**, not a content hash. The cache
is project-local and validation guarantees one entry per `(scope, name)`, so
version-keyed directories cannot collide. Content identity is guarded
separately by the digest (below); the directory name is for human
readability and debuggability.

### Restore semantics

`interfaces::restore_all` runs at the top of every *consuming* command. It is
a no-op when `[interfaces]` is empty (no per-interface directory is created).
For each entry it inspects the cache in a single pass (`verify_cached` →
`CacheStatus`):

| State | Action |
|---|---|
| Valid (present, parses, digest matches `trix.toml`) | use cache, no network |
| Missing (a required file absent) | fetch from the registry, write the cache |
| Invalid (digest mismatch / corrupt metadata / malformed TII) | hard error; surface it directly, do **not** silently refetch — directs the user to `trix use --force` |

The digest in `trix.toml` is the lockfile: it is verified on every restore,
and a registry that rotated a tag's underlying image is a hard error rather
than a silent content swap.

## How commands account for interfaces

The toolchain splits cleanly into **project-only** commands and
**consuming** commands. Interface machinery (`validate_interfaces`,
`restore_all`) lives *only* in the consuming set.

### Project-only — never touch interfaces

- **`trix build`** — produces the project's own TII and nothing else.
  Interfaces are not inputs to this build, so it neither validates nor
  restores them. Symmetric with `check`.
- **`trix check`** — parses and analyzes the project's own `main.tx3` and
  nothing else. An interface's source is the publisher's responsibility
  (validated at *their* publish time); re-analyzing it would only surface
  diagnostics the consumer cannot act on, and could be a false failure under
  a different compiler version.

### Consuming — validate + restore, then read the normative TII

- **`trix invoke --from <PROTOCOL_REF>`** — selects which protocol's TII to
  invoke against (omit → the project's freshly built TII; alias or registry
  ref → the interface's cached `main.tii`). The transaction is chosen
  interactively by the wallet.
- **`trix codegen`** (unstable path only — see below) — generates a client
  per protocol from each one's TII.
- **`trix inspect tir --tx <TX_REF>`** — a bare tx targets the project
  (lowered from the author's normative source); `<alias>::<tx>` or a
  fully-qualified `registry_ref::<tx>` targets an interface, with the IR
  decoded straight out of the **normative cached TII** (the `.tii` carries
  the encoded TIR per transaction). The informative `.tx3` is never read.

`trix publish` is unchanged; it does not yet record a published artifact's
own interfaces (future work).

The guiding principle, uniform across every consuming command: **the
published TII is the contract**. Interface source is never recompiled,
anywhere.

## Codegen

Interface-aware codegen lives **only in the unstable codegen path**
(`src/commands/codegen.rs`, `#[cfg(feature = "unstable")]`). The default
legacy path (`src/commands/codegen_legacy.rs`) is intentionally untouched and
remains single-protocol for backward compatibility.

The unstable path delegates rendering to `tx3c codegen --tii`. For each
codegen plugin it runs once per protocol:

- the project's TII is built via `builder::build_tii`;
- each interface's TII is the **cached pre-built published `main.tii`**
  (not recompiled), consistent with `trix invoke`.

Output layout is **unified**: every protocol is written to its own subdir,
`<output_dir>/<project_name>/` and `<output_dir>/<alias>/`, **regardless of
whether any interfaces are declared**. The path a binding lands at never
depends on interface count — there is no "layout shifts the moment you add
your first interface" cliff. This is a deliberate one-time change confined
to the unstable path; the default/legacy layout is unchanged.

## Out of scope (future work)

- `trix publish` recording its own interfaces in the published artifact
  (e.g. an OCI annotation), enabling transitive resolution.
- Cache garbage collection — `trix use --force` repinning leaves prior
  version directories behind.
- Discovery commands (`trix search` / registry listing). Protocols are found
  out-of-band; `trix use` takes the reference directly.
