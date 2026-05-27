# Protocol Logos

## Overview

A published protocol may carry a small **logo image** alongside its source,
TII, and README. The registry frontend renders it in the catalogue card and
protocol detail view; other consumers (sdks, agents, IDE plugins) are free
to display or ignore it. This document describes how the logo is shipped
on the wire, what the publisher declares, and what each consumer is
expected to do — or not do — with it.

## Where the logo lives: a blob layer, not a metadata URL

Two shapes were considered:

1. **External URL in `ImageMetadata`.** Cheap to add, but pushes durability
   onto the publisher: links rot, hosts apply CORS, and crucially the
   pointed-at bytes can be swapped without re-publishing the protocol —
   silently breaking the registry's per-version immutability promise.
2. **OCI blob layer.** Chosen. Mirrors the existing pattern for the
   `application/tx3`, `application/tii+json`, and `text/markdown` layers.
   Self-contained, content-addressed by digest, immutably tied to the
   tag, no external dependency, no CORS, and zot already serves blob
   bytes natively.

Adding a new layer with a new media type is purely additive to the OCI
manifest. zot, the registry backend, and every existing client treat
unknown media types as ignored layers, so this lands without a wire-format
break anywhere in the stack.

## Wire format

A logo is a single image layer on the OCI image manifest:

- **Media type:** `image/png`
- **Maximum encoded size:** 256 KiB (`262144` bytes)
- **Cardinality:** 0 or 1 per image. The first layer with a matching
  media type wins on the read path.

PNG is the only accepted format in v1. SVG is intentionally deferred —
rendering untrusted SVG in a browser is a sanitization problem that does
not belong in v1 of a visual nicety. Future formats (SVG, AVIF, dark-mode
variants) are layered in as additional media types; old single-PNG
artifacts keep working.

The presence of the layer is the signal. No `ImageMetadata` field, no
annotation. A consumer that wants to know "does this protocol have a
logo?" inspects `manifest.layers` for the media type and does not need
to download blob bytes.

## Publisher contract: `[protocol].logo`

`trix.toml` gains one optional field:

```toml
[protocol]
name        = "widget"
scope       = "acme"
version     = "0.1.0"
main        = "main.tx3"
readme      = "README.md"
logo        = "logo.png"   # NEW — path relative to trix.toml
repository  = "https://github.com/acme/widget"
```

`trix publish` validates before pushing:

| Check | Failure |
|---|---|
| File exists at `logo` path | hard error, miette diagnostic pointing at `trix.toml` |
| File starts with the PNG magic bytes `89 50 4E 47 0D 0A 1A 0A` | hard error — wrong format, even if the extension is `.png` |
| File size ≤ 256 KiB | hard error — publish refuses oversize blobs |

All three failure modes surface a publisher-side diagnostic. The logo is
optional; if `[protocol].logo` is absent the publish proceeds with no
logo layer attached and nothing else changes.

## Consumer contract

The registry backend pulls the image as usual. After accepting the
`image/png` media type in its layer-pull allowlist, it exposes the logo
two ways:

- **`has_logo: bool`** on the GraphQL `Protocol` type. Derived from the
  manifest's layer list — no blob fetch required to populate it.
- **`GET /protocols/:scope/:name/logo`** — streams the blob bytes with
  `Content-Type: image/png` and a long `Cache-Control` (blobs are
  immutable per version). Returns `404` when no logo layer is present.

The frontend renders the logo in two places (catalogue card, protocol
detail header) and falls back to a placeholder when `hasLogo` is false
or the `<img>` errors. No other surfaces consume the logo in v1.

Other consumers — sdks, agents, IDE plugins — are free to read the
layer themselves; nothing in the toolchain depends on the logo being
fetched, and ignoring it is a no-op.

## Backwards compatibility

- **Old artifacts** (published before this design lands) carry no logo
  layer. `has_logo` resolves to `false`; the frontend renders the
  placeholder. No upgrade path or backfill is required.
- **Old consumers** receiving a new artifact with a logo layer ignore
  the unknown media type — they were already ignoring `text/markdown`
  in the same way before READMEs landed.
- **`trix.toml` schema** treats `logo` as
  `Option<PathBuf>` with serde default, so existing files round-trip
  unchanged.

## Threat model

| Threat | Defense |
|---|---|
| Oversized blob inflates registry storage / DoS | 256 KiB cap enforced at publish time |
| Non-PNG bytes served as `image/png` to browsers | Magic-byte validation at publish; backend re-asserts media type on response |
| Logo content swap without re-publish | Impossible by construction — the blob is part of the digest-pinned manifest |
| Malicious SVG (XSS) | SVG is out of scope in v1; only PNG accepted |
| Logo as covert channel for tracking pixels / network beacons | Logo is served by the registry from the OCI blob; no client-side fetch to a publisher-controlled host |

The identity/trust chain from `003-protocol-interfaces.md` and the
GitHub-anchored identity work (#112, #113) extends to the logo for free:
the logo layer is part of the same OCI manifest whose digest is
sigstore-attested (OIDC tier) or registry-attested (App tier). No
separate signing path is introduced for the image.

## Out of scope (future work)

- **SVG / AVIF / dark-mode variants.** Add new media types alongside
  `image/png`; the read path picks the best match for the consumer.
- **Multiple sizes / responsive sources.** Same mechanism; structured
  with OCI annotations on the layer.
- **`trix init` scaffolding a placeholder `logo.png`.** Out of scope —
  the field is optional, and templating a binary is more friction than
  it saves in v1.
- **Logo-only update flow.** A logo change requires a re-publish like any
  other artifact change. There is no partial re-tag.
