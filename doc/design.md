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

## Optional GPUI HTTP Client Stack

GPUI-owned HTTP client integration is optional in this fork. Default GPUI builds used by Beryl must not pull the `http_client`, `zed-reqwest`, or `ring` dependency stack.

Consumers that enable the GPUI HTTP client feature retain the GPUI HTTP client boundary. Code that only needs URL parsing or HTTP status constants should use lighter dependencies instead of requiring the GPUI HTTP client stack.

Avoiding `ring` by default avoids the LLVM-linked native build cost that Beryl does not need when it does not use GPUI HTTP APIs.
