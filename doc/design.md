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

Native test support exposes void input dispatch through the same Window event path used by the
platform. It adds no alternate input handling, activation policy or public dispatch-result type.
This boundary is available only for tests or the existing `test-support` feature and is qualified
by consumers' native command-mounting tests.

The void wrapper was independently reviewed and qualified on Windows by Beryl's four real Compact
pointer-dispatch cases, nextest run `001846ed-569d-467f-b2d0-13687fa4f253`, on 2026-10-10.
The default Beryl backend/App/Home library check also passed with test support disabled. Consumer
acceptance details belong to [Beryl's mounting evidence](../../beryl/doc/audits/context-compaction-mount-readiness.md).

## Windows Native Save Destination Choice

A narrow Windows save-choice capability returns one accepted destination and whether replacement
was authorized by native overwrite confirmation for that same choice. Its overwrite callback owns
one native TaskDialog confirmation, parented to the live native save dialog, with native Yes/No
buttons, dismissal/cancellation and No as the default. Only explicit Yes records confirmed path
authority and accepts that overwrite; No, Escape or dismissal refuses it and retains no authority.
The generic picker API and its ordinary native behavior remain unchanged.

Clear prior confirmation state before each overwrite attempt and on selection/folder/type change
or share refusal. Match the confirmed path with the final accepted result. Native callbacks hold
no state lock across COM calls or modal confirmation. Cancellation returns no choice; native
`ERROR_CANCELLED` alone is cancellation, while creation, setup, confirmation, `Show`, callback and
result-extraction failures are errors. A declined overwrite, including a later same-name new-file
choice after external deletion, cannot carry earlier replacement authority. Unsupported native
backends report unavailable through this capability without changing their generic picker APIs.

This boundary chooses a destination and supplies no file writer or publication policy. Consumers
own exact output and cancellation/effect lifecycle, use non-replacing publication for a choice
without overwrite authority, and may replace the accepted path only with that authority. The
choice grants path-level replacement intent, not a file-identity or filesystem-lock guarantee.
Verification qualifies new choices, confirmed replacement, declined replacement followed by both
a different new choice and an externally deleted same-name new choice, cancellation, failures and
cleanup of the bounded native event subscription.
## Checked Windows Clipboard Boundary

GPUI exposes caller-bounded clipboard acquisition and acknowledged clipboard writing independently
of its legacy convenience methods. Windows is the qualified backend for this boundary; unsupported
backends return explicit unavailable rather than calling an unbounded read or assuming write success.
The boundary owns no application marker metadata, token registry, draft identity or retry policy.

A checked write accepts one text representation with optional opaque metadata, or one supported
encoded image representation, plus explicit positive byte limits. It preflights all native encoding
and allocation capacities with checked arithmetic before clearing/writing the clipboard. Empty,
malformed, unsupported and over-limit requests fail before native mutation. Native ownership-open,
allocation, format publication and required close errors remain typed. Complete success means all
requested representations were accepted by the native clipboard, including metadata. A later error
may leave changed or partial native content but returns no success; the consumer must never infer
that its prior clipboard value was preserved or delete content on that failure. No readback heuristic,
automatic retry, image transcoding or extra derived representation supplies acknowledgement.

Checked image writes publish the standard encoded image format together with a fixed-size private
companion containing a version, image format, exact encoded length and content consistency digest.
Native allocation padding is permitted and charged, never treated as additional encoded content
when the companion is present. Both representations must be published for success. The companion
is GPUI representation metadata, not application provenance or authentication; no decoding or
transcoding is involved. Its native backing counts against the metadata ceiling, image native
backing plus owned encoded bytes against the image ceiling, and all backing/output against total.

A checked read holds one native clipboard snapshot while inspecting formats and exact native
allocation lengths, validating bounded terminators/encodings, and copying one complete supported
representation within the caller's total and individual text/metadata/image ceilings. Account native
UTF-16 backing and decoded UTF-8 capacity before allocating owned output, with checked arithmetic;
metadata is included in the total. Image bytes are acquired encoded and never decoded by this API.
Unsupported formats, malformed lengths/text, over-limit data, ownership failure and no supported
value have distinct outcomes. No complete-value allocation may precede preflight. Snapshot ownership
ends on every outcome, and failure returns no partial item or second copy of an acquired value.

For images with the private companion, checked reads validate its version, format, encoded length
within the actual image allocation, and digest before allocating output, then copy exactly that
encoded prefix. Bound the companion's native allocation against metadata and total ceilings.
Malformed, unsupported-version or mismatched companion data refuses the image without fallback.
Foreign images without the companion are copied as the entire bounded native representation;
this boundary does not infer their logical encoded length from allocation size. Image padding and
companion padding remain subject to admission before publication or output allocation.

Read results carry the exact native clipboard sequence for replacement detection. That sequence is
an equality fence for the captured snapshot, not a reusable lifetime capability or authorization of
application metadata. Text/metadata consistency remains checked, but applications must authenticate
their own token and complete text identity. A later clipboard replacement cannot retarget an already
captured immutable value; consumers decide whether it remains eligible.

Verification injects ownership, allocation, text and metadata publication, close and read failures;
checks exact fit and one-over limits, malformed/native encoding and changed sequence; and proves
allocation never precedes admission. Native Windows qualification exercises actual ownership and
format publication for nonaligned image payloads and the companion as well as text/metadata;
injected cases cover padded allocation, malformed/missing companion, consistency and publication
failure. The isolated native harness owns a create-only private window station and desktop so it
never acquires or mutates the Operator's clipboard. It restores the original process station and
thread desktop and closes only test-owned handles after destroying its message-only owner window.
Independent semantic review covers unsafe native memory access, complete acknowledgement and bounds.

## Checked Window Minimum Updates On Windows

`Window::update_minimum_size` accepts a logical client `Size<Pixels>` for the original live
window and returns a move-only `WindowMinimumUpdate` future or a `WindowMinimumUpdateError`.
Its typed completion is `WindowMinimumUpdateOutcome`, carrying the accepted minimum and actual
client size on `Applied`, a `WindowMinimumUpdateFailure` on `NotApplied`, or `Unverified`.
`Window::is_minimized` provides a read-only `Option<bool>` observation for Windows and the test
platform; unavailable backends or a lost native owner return `None`. A window-bounds wakeup alone
does not prove restoration. This observation authorizes no native mutation or minimum publication.
Admission accepts positive finite dimensions representable at the window's native scale; invalid
dimensions, another active native operation, or closing/destruction return `InvalidMinimum`,
`Busy`, or `Closed` without mutation. Unsupported native backends return `Unavailable`.
An admitted operation retains the original native owner and executes on the existing foreground
executor outside App, entity and platform-state borrows. It creates no replacement or operation
queue and does not publish, activate, focus, reorder, minimize, maximize or restore the window.

For an ordinary windowed owner, including an unpublished owner, grow only deficient dimensions
while preserving origin. `Applied` acknowledges the new persistent tracking minimum and actual
client dimensions satisfying it. Adequate dimensions remain unchanged. Hidden initial placement,
visibility and first-publication state remain authoritative; checking a minimum consumes no
first-publication lease. A minimized owner returns `NotApplied(Minimized)` without mutation.
A maximized owner, including an unpublished initially maximized owner, accepts the update only
when its drawable area and existing normal restore dimensions both satisfy it; otherwise return
`NotApplied(InsufficientSpace)` without changing state. This boundary writes no minimized or
maximized restore placement and never changes native state to make a request fit.
Fullscreen owners return `NotApplied(UnsupportedState)` without mutation.

Native failure with verified resulting state returns `NotApplied(NativeFailure)`; inability to
verify the resulting state returns `Unverified`. Non-success restores the previous tracking
minimum and authorizes no consumer candidate publication. An enlargement already performed may
remain; exact bounds rollback is not promised. Closure invalidates consumer publication.
Dropping the completion receiver does not cancel admitted work: its original owner releases
native-operation exclusion and settles deferred closure independently of receipt consumption.

Construction applies the declared initial client minimum before exposure, growing only deficient
initial windowed dimensions while preserving adequate requested geometry. Initial normal versus
maximized state remains fixed. Geometry that cannot satisfy the minimum in its declared state
fails explicitly rather than exposing an undersized surface or changing that state.

The test platform models minimum state, checked outcomes and resize notification outside its
state borrow. Qualification covers ordinary and hidden growth, unchanged adequate dimensions,
minimized deferral and ordinary restoration, maximized acceptance/refusal, invalid dimensions,
native refusal and failed verification, receiver disposal and closure during an admitted update.
Native Windows evidence must prove actual client bounds, original identity and preserved native
state/visibility/focus; simulated bounds alone do not establish those effects. Independent
semantic review covers operation custody, acknowledgement and consumer publication eligibility.

## Application Lifetime After The Last Window Closes

`Application::with_quit_on_last_window_close(bool)` is a startup-only option configured before
`Application::run`. Passing `false` suppresses automatic event-loop termination caused solely by
closing the final window. Native destruction, destruction notifications, foreground dispatch and
later window creation continue normally, including an interval with zero windows. Consumers own
their cleanup and eventually request explicit quit; the option creates no window or keep-alive
token and adds no shutdown worker or callback protocol.

The default and `true` preserve each backend's existing behavior: Windows, X11 and Wayland stop
after the final window closes, while macOS and the test backend retain their existing behavior.
Calling the builder repeatedly before launch uses the last value. There is no public runtime
setter. Explicit `App::quit`, operating-system termination and platform-error handling are unchanged;
this option is not a veto over explicit application or system shutdown.

Native Windows verification must destroy the last window, observe its actual destruction, run a
deferred continuation with no windows and create a successor before explicit quit. It must also
prove default automatic exit and explicit quit with zero or remaining windows. Other native
backends require equivalent source integration, with platform qualification recorded separately.

## Owned Native Confirmation On Windows

### Foreground Progress During Native Modal Dispatch

Windows foreground dispatch leaves not-yet-executed tasks reachable by nested native message
dispatch. A native modal call made outside app/entity borrows cannot strand another queued task
inside an outer dispatch batch. This applies to ordinary foreground work as well as confirmation
cancellation and owner removal. Verification enters a nested native message pump with multiple
tasks already queued and proves a later task runs before the first returns.

### Confirmation Lifetime

GPUI provides a Windows-native two-button confirmation owned by one live published window.
The immutable request supplies a title, message, cancel label and affirmative label. UTF-8 byte
limits are 256 for the title, 4096 for the message and 128 for each label; empty strings and
embedded NUL are rejected before native creation. Cancel is explicitly the default. Escape,
native close, owner removal and caller cancellation never produce an affirmative result.
This boundary is separate from the existing generic prompt and never substitutes a GPUI overlay.
Other platforms retain their existing prompt behavior; this Windows capability does not claim
qualification of another native backend.

Admission returns one move-only control and one completion future. The control can reveal the
same dialog or request cancellation, but cannot select the affirmative action. One owner admits
at most one confirmation until its native operation has settled, including queued creation.
Duplicate creation refuses without changing the first operation. Reveal before creation retains
one pending intent; cancellation before creation prevents exposure. Dropping the control requests
cancellation; dropping the completion receiver neither confirms nor abandons native cleanup.
An operating-system refusal to foreground an existing dialog is reported without replacing it or
changing its confirmation authority. Queued creation rechecks that its exact owner remains visible
and enabled before calling the native modal API.

The operation retains the exact owner lifetime through native settlement. Owner removal cancels
the dialog and defers owner destruction until that settlement. No command or callback targets a
recycled native handle. Native calls and nested message dispatch hold no GPUI entity borrow or
confirmation-state borrow. Cancellation is sticky and wins over a later affirmative result.
Native focus returns to the surviving owner after dismissal; the consumer retains its logical
GPUI focus identity and restores it on cancellation.

Completion reports Confirmed or Cancelled only after dialog destruction and return of the native
modal call. Creation failure is an explicit error; missing destruction evidence after creation
is an error that keeps owner destruction blocked. Neither channel loss nor an absent handle is
successful disposal evidence. The implementation retains only one bounded request and fixed
operation state per owner, with no command queue, background worker or global handle registry.

The retained control exposes read-only cleanup-settlement evidence for its exact operation.
It is false while queued, running or lacking destruction evidence. It becomes true only at the
same native settlement boundary that releases the owner's confirmation exclusion, including a
failed creation that left no native dialog. Completion failure and cleanup settlement remain
separate facts; a consumer may release a failed attempt only when cleanup is proven complete.
This evidence never conveys affirmative user intent and cannot reopen or destroy the owner.

Windows acceptance exercises real native creation, exact title and Cancel default, affirmative
selection, Escape/dismissal, queued and active cancellation, duplicate refusal and reveal, owner
removal, receiver/control drop, and destruction ordering. Failure injection covers native-open
failure and lost settlement evidence. Independent lifecycle review checks callback reentrancy,
native identity and the absence of unintended affirmative authority.

## Recoverable Native Window Destruction On Windows

GPUI exposes a separate owned destruction attempt for a live published Windows window. Admission
retains the exact GPUI window slot, root, platform wrapper and native identity until native
settlement. It does not mark the window removed, emit window-closed notification or relinquish
its wrapper before destruction is proven. Consumers supply their own interaction and durable-state
fences; this boundary does not authorize saving, application shutdown or process termination.

Each window admits at most one attempt. Native dispatch and confirmation cancellation settle
without GPUI/entity borrows held. The result distinguishes proven destruction from settled failure
with the same surviving window and from unresolved native settlement. Surviving failure requires
proof that no queued or active destruction for that attempt can later remove the window. An error
string, absent handle or lost receiver is not settlement. Retain exact operation identity and
custody across receiver drop, duplicate calls and delayed callbacks; no recycled handle is targeted.
Early native destruction callbacks cannot remove the GPUI slot ahead of terminal native settlement.
Partial native teardown cannot produce a surviving-window result. Every fresh admitted attempt
has independent completion identity; stale completion cannot settle or remove its successor.

Proven destruction removes the exact GPUI slot and emits its close notification once, then releases
the retained wrapper without requesting destruction again. Settled surviving failure leaves the
same GPUI/native window available for consumer restoration and permits a separately activated fresh
attempt. An unresolved attempt keeps its bounded custody and excludes another attempt. Ordinary
window removal and owner-drop cleanup must not bypass an active attempt or schedule duplicate
destruction. No background worker, unbounded registry or automatic destruction retry is introduced.

The existing irreversible removal path retains its contract for consumers that do not request this
capability. Other backends explicitly refuse unsupported admission without removing a window.
Windows native verification covers a real successful destruction, injected DestroyWindow refusal
with exact surviving window/root identity, then fresh successful destruction; queued/in-flight
confirmation cancellation, duplicate/stale callbacks and receiver abandonment; and one close
notification only after actual destruction. Independently review reentrancy, custody and identity.

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

### Live Paint Colors

Consumers may paint an immutable streaming text, oversize-atom, or inline-object fragment with a
replacement foreground color without reshaping it or changing its stored presentation. The
replacement also supplies the fallback color for underline and strikethrough decorations;
explicit decoration colors and color-emoji rendering remain unchanged. Atom and inline-object
background painting accepts a replacement color or no background. Ordinary per-run text
backgrounds remain part of the retained run presentation.

These overrides apply only to the current paint call. They preserve the fragment's shared layout,
wrapping, placement, composite positions, hit and caret maps, baseline, and retained-payload
accounting, and create no replacement runs or persistent style state. Existing paint calls retain
their stored colors. Consumers own coherent appearance selection across their paint passes;
GPUI does not add an appearance coordinator or a geometry transition for color changes.

### Layout Contract

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

Streaming sessions explicitly select `StreamingLayoutMode::SoftWrap` or `NoWrap` through
`WindowTextSystem::streaming_layout_session_with_mode` and
`resume_streaming_layout_session_with_mode`. Existing session entry points select soft-wrap.
In no-wrap mode, text segments,
source-covering oversize atoms and source-zero-width objects continue on the current logical line
regardless of the binding's wrap width. Only explicit logical-line finalization or end-of-source
finalizes that line. Text fragment wrap facts are empty and cumulative inline placement remains
finite and checked; oversized or overflowing geometry follows the ordinary atomic failure contract.
Mode is a compact continuation fact and a resumed session must select the same mode. A soft-wrap
entry point cannot resume no-wrap state, even when the caller reuses the other binding identities.
The width field is used and validated only for soft-wrap mode. Neither mode interprets tabs or
chooses source grapheme segmentation on behalf of the consumer; consumers supply canonical segments
and a consistent bounded tab presentation policy.

One streaming session is bound to immutable shaping inputs: layout mode, wrapping width, font and size, text
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

## Observed Font Environment Generation

The shared GPUI text system exposes an optional fixed-size font-environment key through both its
shared and window text-system boundaries. The opaque equality-comparable key identifies the exact
text-system owner and its generation; it cannot compare equal across distinct owners. A qualified
generation identifies the effective
platform font collections and fallback inputs used for font resolution and shaping. Its value is
stable while those inputs remain unchanged, changes on an observed collection adoption or custom
font addition, and never wraps or reuses an earlier value. Unsupported observation, exhausted
generation identity or a failed environment transition yields unavailable qualification; consumers
requiring exact epochs must treat that outcome explicitly rather than substitute a primary FontId.

Windows observes system collection expiration through the retained collection's DirectWrite
expiration event. Observation performs a zero-timeout event check and at most one collection
refresh attempt; it creates no polling thread, inventory scan or font-install workflow. An
unsignalled event and missing-family resolution preserve the collection and generation. A signalled
event admits one replacement only after its observation capability is established. Refresh failure
does not publish a qualified replacement. The event belongs to its retained collection and is not
closed independently. This contract covers platform-observed adoption, with no immediate OS font
installation notification or observation-latency guarantee. Platforms without a qualified
observation mechanism report unavailable; the immutable test backend has a stable generation.

Collection adoption and custom-font changes qualify or invalidate positive and negative requested-
font resolution caches, platform selection/identifier caches and ordinary line-layout caches as
one environment transition. Unchanged missing-family requests cannot trigger alternating refresh
and invalidation cycles. FontIds already referenced by returned layouts and renderer resources are
never recycled or overwritten; old publication remains paintable until its consumer releases it.
New resolution uses the adopted environment. Cache publication checks the generation surrounding
resolution or shaping and refuses insertion across an intervening transition. Checked shaping and
canonical consumers report that mismatch through their typed local failure. Legacy infallible
shaping may return its coherent uncached result from the captured platform environment; it cannot
silently return empty text to represent an epoch failure or cache that result in the successor epoch.
Generation-aware native face retention has a fixed 16,384-face ceiling, including faces from prior
epochs. A face admission beyond that ceiling refuses before changing the face registry or replacing
an existing FontId; it never silently evicts a face still used by published glyphs. Generation state
and current resolver maps retain no historical per-epoch registry. This bounds the new historical
retention consequence; ordinary source and fragment capacity remains separately caller-owned.

Canonical streaming consumers bind the generation with their other immutable inputs and compare
it before and after each bounded layout quantum and before publication. A transition invalidates
incompatible candidates and continuations; no derived geometry or terminal width may be published
under the successor environment. This API does not own consumer source anchors or viewport policy.
Acceptance covers stable repeated missing-family requests, positive and negative cache epochs,
custom-font/system adoption transitions, failures and unsupported observation, old FontId lifetime,
intervening-change refusal, and native Windows expiration-event access without installing fonts.

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
