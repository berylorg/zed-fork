# Goals

Maintain a narrow Beryl-oriented fork of upstream Zed that carries targeted GPUI changes needed for Beryl's memory footprint and default build dependency constraints.

## Non-goals

- Documenting or governing the full Zed architecture.
- Defining policy for unrelated Zed crates or upstream Zed development.
- Expanding the fork into a general-purpose divergence from upstream Zed.

# Decisions

## Fork Scope

The fork exists for targeted GPUI changes needed by Beryl and should remain easy to compare with upstream Zed.

## Glyph Batching Memory Optimization

The fork may carry GPUI glyph batching memory optimization work that reduces transient allocation pressure and resident memory for Beryl's text-heavy UI.

## Image Asset Renderer Cleanup

The fork treats completed GPUI image asset removal as the lifecycle boundary for corresponding rendered image resources.

When an image asset that has completed decoding is removed through GPUI's image asset removal path, the fork must release matching rendered image entries from live window renderer atlases. Renderer-side image texture pages and CPU mirrors must not remain live solely because the image had previously been painted.

This cleanup belongs inside GPUI's image lifecycle. Consumers should not need to retain decoded `RenderImage` handles, inspect renderer internals, or issue renderer-specific cleanup calls after removing an image asset.

## Optional GPUI HTTP Client Stack

GPUI-owned HTTP client integration is optional in this fork. Default GPUI builds used by Beryl must not pull the `http_client`, `zed-reqwest`, or `ring` dependency stack.

Consumers that enable the GPUI HTTP client feature retain the GPUI HTTP client boundary. Code that only needs URL parsing or HTTP status constants should use lighter dependencies instead of requiring the GPUI HTTP client stack.

Avoiding `ring` by default avoids the LLVM-linked native build cost that Beryl does not need when it does not use GPUI HTTP APIs.
