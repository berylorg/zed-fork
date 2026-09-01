# Goals

Maintain a narrow Beryl-oriented fork of upstream Zed that carries targeted GPUI changes needed for Beryl's memory footprint and default build dependency constraints.

## Non-goals

- Documenting or governing the full Zed architecture.
- Defining policy for unrelated Zed crates or upstream Zed development.
- Expanding the fork into a general-purpose divergence from upstream Zed.

# Decisions

## Documentation Authority

- This root design owns the bounded fork scope and reusable GPUI public-boundary decisions carried
  specifically for Beryl. It does not govern the full upstream Zed architecture.
- [Hidden-window first publication](features/hidden-window-first-publication/design.md) owns the
  consumer-visible behavior of constructing a native window hidden and explicitly publishing that
  same window as its first visible surface.

## Fork Scope

The fork exists for targeted GPUI changes needed by Beryl and should remain easy to compare with upstream Zed.

## Clone-Stable Scroll-Handle Identity

GPUI `ScrollHandle` provides an app-neutral comparison that reports whether two handles share the
same retained scroll state. The result is stable across clones of one handle and distinguishes
separately created handles even when their visible scroll state is equal.

The comparison exposes no pointer value, numeric address, inner retained state, mutable authority,
consumer token, or application concept. It exists only to let reusable GPUI components bind
retained interaction lifecycles to the actual handle that owns them.

## Glyph Batching Memory Optimization

The fork may carry GPUI glyph batching memory optimization work that reduces transient allocation pressure and resident memory for Beryl's text-heavy UI.

## Bounded Streaming Text Layout

The fork provides a GPUI streaming text-layout primitive for consumers whose logical text can exceed
one bounded resident string. The primitive accepts an ordered sequence of bounded shaping segments,
preserves a compact continuation across them, and produces immutable visual-line fragments, wrap
transitions, caret and hit-test maps, and exact retained-payload charges without retaining or
requiring the complete logical line.

Every ordered input, continuation, fragment map, caret lookup, and hit result uses one canonical
composite position. A composite position contains an exact logical UTF-8 byte offset and one
constant-size gap witness for the ordered source-zero-width opaque objects at that offset. The
witness identifies the immediately preceding and following objects by their fixed-size opaque
identities and order keys, using explicit before-all, after-all, and no-object edges. It therefore
distinguishes the gap before the first object, every gap between adjacent objects, and the gap after
the last object without assigning source bytes to an object. A bare byte offset is not an alternate
position whenever more than one composite position exists at that offset.

A source-zero-width opaque object has one stable fixed-size identity and one stable fixed-size order
key. The consumer's revision-bound object-source authority guarantees that each identity occurs at
most once in that revision and that order keys are unique at one anchor. GPUI does not prove global
identity uniqueness. Objects at an anchor are admitted in strict order. Each admission names
adjacent leading and trailing composite positions at the same UTF-8 offset, so it advances the
ordered stream by exactly one object while leaving the byte offset unchanged. GPUI validates that
the leading position exactly continues the current position, both object-facing edges agree with
the admitted identity and order key, the outer edges form the stated adjacent gaps, and same-anchor
order strictly increases. Any locally observable disagreement, skipped or nonadjacent gap,
decreasing anchor, or discontinuous position is invalid.

Zero-width objects enter the same bounded streaming-layout session as ordinary text and
source-covering oversize atoms. Each object admission carries only bounded app-neutral presentation,
style, metrics, and paint facts. It contributes measured inline and block geometry but no source
bytes. Arbitrarily many objects may share one anchor through repeated bounded admissions; work may
grow with their count, while the session retains only the current fixed-size composite position and
placement continuation rather than the preceding object collection.

Bounded shaping segments are canonical layout boundaries. A consumer derives them independently of
source-page boundaries and request timing: an ordinary segment ends at the last complete grapheme or
opaque-atom boundary within its configured UTF-8 byte cap. Its exact composite endpoints delimit its
nonempty source-byte range and prevent it from crossing an undisclosed object position. GPUI shapes
each segment as an independent text-shaping context and carries only visual-line placement state
across segment boundaries. Layout is exact for this canonical segmented algorithm; the API does not
claim byte-for-byte glyph or bidi equivalence with shaping the entire unbounded logical line as one
platform paragraph.

An indivisible grapheme or source-covering opaque atom that exceeds the shaping-segment cap remains
representable by a compact oversize-layout atom. It has a nonempty exact source range between
composite endpoints and bounded presentation metadata instead of retained complete source bytes.
It advances the UTF-8 offset across that range and is distinct from a source-zero-width object.
Copying, editing, and source identity continue to use the original nonempty logical range.

Wrapping treats each zero-width object as one indivisible inline item. If its measured width would
overflow a nonempty visual line, it starts the next visual line; an object wider than the wrap width
is admitted on an empty line and retains its exact width. Objects admitted consecutively at one
anchor wrap in their canonical order. The current visual line's block extent is the maximum of the
configured line height and every admitted inline item's height on that line.

Visual-line nonemptiness is one explicit fixed-size continuation fact, not an inference from the
inline pixel offset. Every admitted text segment, source-covering atom, and source-zero-width object
makes its current visual line logically nonempty even when its measured width is zero, so a
following overflowing item wraps correctly across session resume.

Logical-line finalization is an explicit ordered stream fact, not an inferred consequence of a
zero-byte object. It begins at the exact current composite position, consumes the exact delimiter
range when one exists, advances to the stated next composite position, completes the current visual
line once, and resets inline placement. End of source is accepted only at the exact source extent
and the after-all or no-object gap, after every object at that anchor. It explicitly finalizes the
remaining logical line, including the sole line of an empty source and the empty final line after a
terminal delimiter, and makes subsequent admission invalid.

Every admitted text fragment maps its shaped boundaries to composite positions. Every admitted
zero-width object exposes its exact bounds, opaque identity, leading and trailing gap positions,
and exact caret geometry for both gaps. Hit testing returns either the exact object identity or one
exact composite gap position; it never collapses either result to a byte offset. At a shared
geometric boundary, the following fragment owns the position, while a terminal fragment owns its
trailing position. When wrapping moves the following item, that downstream ownership gives the
shared gap its one canonical caret geometry on the new visual line.

Composite positions, gap witnesses, object identities, and order keys are fixed-size Copy-like
value data with no heap ownership. Object support adds no scan or retention of prior objects to
ordinary text shaping, paint, hit-testing, or caret paths and causes no per-lookup allocation in
ordinary hit or caret lookup. Caret and hit geometry for one realized object fragment is obtained in
constant time, while text-fragment map lookup preserves the existing asymptotic behavior. Admission
may perform bounded shaping and allocation for the admitted presentation, but performs no
whole-stream scan or object-registry work. The continuation remains fixed-size, and exact retained
byte and semantic-item accounting includes every retained addition required by object support.

One streaming session is bound to immutable shaping inputs: wrapping width, font and size, text
runs, line metrics, segment policy, object geometry, and other glyph- or placement-affecting values.
Its compact resumable continuation contains the next ordinal and exact composite position, current
inline and block placement, current line block extent and visual-line count, logical-content
occupancy, line-finalization and terminal state, and the immutable-input identities needed to reject
a mismatched resume. It contains no source text, shaped payload, object presentation, or same-anchor
object collection.

The session validates current composite-position continuity, adjacent gap witnesses, admitted-
object agreement with both object-facing edges, strict same-anchor order, UTF-8 ranges, metrics,
line and end-of-source facts, finite limits, immutable-input agreement, and checked arithmetic
before changing its continuation or publishing a fragment. These are exactly the locally available
bounded facts; the session retains no prior-identity collection and does not validate the
consumer-owned global identity guarantee. Any mismatch, malformed input, overflow, cancellation,
or capacity failure rejects the whole admission and leaves the prior continuation and every
previously returned fragment unchanged. There is no partial object, map, wrap, line-finalization,
or terminal publication.

For every text or bounded-presentation payload, each cumulative caller style-run byte length must
land on a UTF-8 scalar boundary and the final cumulative length must equal the payload byte length.
GPUI validates these facts in one bounded pass over the supplied runs before platform shaping.

Every continuation and returned fragment has configured finite byte and semantic-item limits.
Admission charges the actual final retained graph for segment or presentation payload, caller style
runs that remain retained, private shaped runs, glyphs, decorations, wrap facts, composite
positions and gap witnesses, object identity and order facts, caret and hit maps, fragment records,
and the compact continuation. Each record is counted exactly once; discarded input-only records are
not retained charges. Configured component-count and retained-byte caps accept an exact fit and
reject one unit below it atomically; the exact semantic-item charge is returned for caller-owned
admission accounting.

The session separately exposes the exact continuation-only byte and item charges. Cancellation and
drop release the active continuation and every session-owned unpublished payload; dropping returned
fragments releases their retained presentation, shaping, map, object, and gap records. The platform
text backends may receive complete strings internally only because each string is one already
admitted bounded canonical segment. The public boundary remains app-neutral and owns only its
layout identities and lifecycles.

## Image Asset Renderer Cleanup

The fork treats completed GPUI image asset removal as the lifecycle boundary for corresponding rendered image resources.

When an image asset that has completed decoding is removed through GPUI's image asset removal path, the fork must release matching rendered image entries from live window renderer atlases. Renderer-side image texture pages and CPU mirrors must not remain live solely because the image had previously been painted.

This cleanup belongs inside GPUI's image lifecycle. Consumers should not need to retain decoded `RenderImage` handles, inspect renderer internals, or issue renderer-specific cleanup calls after removing an image asset.

## Optional GPUI HTTP Client Stack

GPUI-owned HTTP client integration is optional in this fork. Default GPUI builds used by Beryl must not pull the `http_client`, `zed-reqwest`, or `ring` dependency stack.

Consumers that enable the GPUI HTTP client feature retain the GPUI HTTP client boundary. Code that only needs URL parsing or HTTP status constants should use lighter dependencies instead of requiring the GPUI HTTP client stack.

Avoiding `ring` by default avoids the LLVM-linked native build cost that Beryl does not need when it does not use GPUI HTTP APIs.

## Hidden-Window First-Publication Boundary

The fork exposes one app-neutral GPUI operation that publishes an already-created hidden window
without changing its GPUI identity, native-window identity, root entity, renderer ownership,
callbacks, or configured properties. Drawing or updating the window does not implicitly publish
it. Publication remains separate from activation and focus. It performs only the platform-required
initial insertion into visible stacking or tab order and performs no additional raise or reorder.
The initially windowed, maximized, or fullscreen native state is fixed by construction options;
maximize and fullscreen requests remain inert until first publication, then use ordinary behavior.

Every supported native backend implements the feature-owned first-publication behavior through one
equivalent platform boundary. The public operation defines hidden, already-published, failed, and
closed-handle outcomes without adding a general post-publication visibility toggle.

Hidden fullscreen construction is unsupported on macOS because AppKit cannot establish that native
state without a visible transition. GPUI rejects that request before native exposure; hidden
windowed and maximized construction and ordinary immediate fullscreen behavior remain supported.

# Engineering Rigor

Profile: `production-application/v1`

Modifiers: none

This declaration governs only the fork and GPUI boundary decisions in this document, not unrelated
upstream Zed architecture.
