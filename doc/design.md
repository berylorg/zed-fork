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

## Bounded Streaming Text Layout

The fork provides a GPUI streaming text-layout primitive for consumers whose logical text can exceed
one bounded resident string. The primitive accepts an ordered sequence of bounded shaping segments,
preserves a compact continuation across them, and produces immutable visual-line fragments, wrap
transitions, caret and hit-test maps, and exact retained-payload charges without retaining or
requiring the complete logical line.

Bounded shaping segments are canonical layout boundaries. A consumer derives them independently of
source-page boundaries and request timing: an ordinary segment ends at the last complete grapheme or
opaque-atom boundary within its configured UTF-8 byte cap. GPUI shapes each segment as an independent
text-shaping context and carries only visual-line placement state across segment boundaries. Layout
is exact for this canonical segmented algorithm; the API does not claim byte-for-byte glyph or bidi
equivalence with shaping the entire unbounded logical line as one platform paragraph.

When one indivisible grapheme or opaque atom exceeds the shaping-segment cap, the streaming primitive
accepts a compact oversize-layout atom containing its exact logical range and bounded presentation
metadata, not its complete source bytes. The consumer remains responsible for discovering the exact
range through its bounded source cursor and for choosing the app-neutral placeholder presentation.
Copying, editing, and source identity continue to use the original logical range.

One streaming session is bound to immutable shaping inputs: wrapping width, font and size, text
runs, line metrics, segment policy, atom geometry, and other glyph-affecting values. It rejects
out-of-order segments or mismatched inputs. Its continuation and every returned fragment have
configured finite byte and item limits. Admission uses actual retained payload for segment text,
runs, decorations, glyphs, wrap facts, hit maps, and fragment metadata; callers do not assert a
synthetic retained-byte total.

Each successful admission also reports an exact GPUI-computed semantic-item charge for its final
retained graph. The item charge distinguishes retained text payload records, caller style runs,
shaped runs, glyphs, decoration records, wrap facts, caret and hit-test maps, fragment records, and
compact continuations, then exposes one checked total that counts each retained semantic record
exactly once. Input-only records that GPUI discards before returning, including oversize-atom caller
runs, are not retained charges. The session exposes the corresponding continuation-only charge for
state retained without an admission. Consumers neither inspect private shaped payload nor infer or
assert these item counts.

The platform text backends may continue receiving complete strings internally because each such
string is one already admitted bounded canonical segment. Cancellation or drop releases the active
segment, continuation, and unpublished fragments. The API exposes no application binding, storage,
scrollbar, persistence, or domain semantics; those identities and lifecycles remain consumer-owned.

## Image Asset Renderer Cleanup

The fork treats completed GPUI image asset removal as the lifecycle boundary for corresponding rendered image resources.

When an image asset that has completed decoding is removed through GPUI's image asset removal path, the fork must release matching rendered image entries from live window renderer atlases. Renderer-side image texture pages and CPU mirrors must not remain live solely because the image had previously been painted.

This cleanup belongs inside GPUI's image lifecycle. Consumers should not need to retain decoded `RenderImage` handles, inspect renderer internals, or issue renderer-specific cleanup calls after removing an image asset.

## Optional GPUI HTTP Client Stack

GPUI-owned HTTP client integration is optional in this fork. Default GPUI builds used by Beryl must not pull the `http_client`, `zed-reqwest`, or `ring` dependency stack.

Consumers that enable the GPUI HTTP client feature retain the GPUI HTTP client boundary. Code that only needs URL parsing or HTTP status constants should use lighter dependencies instead of requiring the GPUI HTTP client stack.

Avoiding `ring` by default avoids the LLVM-linked native build cost that Beryl does not need when it does not use GPUI HTTP APIs.
