use crate::{
    App, Bounds, Hsla, Pixels, Point, ShapedLine, SharedString, TextRun, Window, WindowTextSystem,
    WrappedLine, point, size,
};
use std::{mem::size_of, ops::Range, sync::Arc};

mod accounting;
pub(crate) mod checked;
mod fragments;
mod session;

/// Finite admission limits for one bounded streaming text-layout session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamingLayoutLimits {
    /// Maximum UTF-8 bytes in one ordinary canonical shaping segment.
    pub segment_bytes: usize,
    /// Maximum style runs retained by one returned fragment.
    pub runs: usize,
    /// Maximum decoration runs retained by one returned fragment.
    pub decorations: usize,
    /// Maximum shaped glyphs retained by one returned fragment.
    pub glyphs: usize,
    /// Maximum wrap facts retained by one returned fragment.
    pub wraps: usize,
    /// Maximum caret/hit-test map facts retained by one returned fragment.
    pub maps: usize,
    /// Maximum fragments returned by one admission.
    pub fragments: usize,
    /// Maximum total retained payload bytes returned by one admission.
    pub retained_bytes: usize,
}

/// Immutable glyph- and placement-affecting inputs for a streaming session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamingLayoutBinding {
    /// Stable identity of the caller's complete immutable shaping-input set.
    pub input_id: u64,
    /// Stable identity of the canonical segmentation policy.
    pub segment_policy_id: u64,
    /// Width available to every visual line.
    pub wrap_width: Pixels,
    /// Font size used for shaping.
    pub font_size: Pixels,
    /// Height used to place successive visual lines.
    pub line_height: Pixels,
    /// Finite session admission limits.
    pub limits: StreamingLayoutLimits,
}

/// One already-bounded ordinary canonical shaping segment.
#[derive(Clone, Debug)]
pub struct StreamingTextSegment {
    /// Immutable-input identity, which must match the session binding.
    pub input_id: u64,
    /// Canonical-policy identity, which must match the session binding.
    pub segment_policy_id: u64,
    /// Monotonic segment ordinal.
    pub ordinal: u64,
    /// Exact consumer logical byte range represented by `text`.
    pub logical_range: Range<u64>,
    /// Exact next logical offset, including any consumer-owned line delimiter.
    pub next_logical_offset: u64,
    /// Complete text for this bounded shaping context.
    pub text: SharedString,
    /// Complete style runs for this segment.
    pub runs: Vec<TextRun>,
    /// Whether this segment terminates its consumer logical line.
    pub ends_logical_line: bool,
}

/// Bounded presentation for an indivisible logical range whose source is not retained.
#[derive(Clone, Debug)]
pub struct StreamingOversizeAtom {
    /// Immutable-input identity, which must match the session binding.
    pub input_id: u64,
    /// Canonical-policy identity, which must match the session binding.
    pub segment_policy_id: u64,
    /// Monotonic segment ordinal.
    pub ordinal: u64,
    /// Exact consumer logical range represented by this atom.
    pub logical_range: Range<u64>,
    /// Exact next logical offset, including any consumer-owned line delimiter.
    pub next_logical_offset: u64,
    /// Bounded visible placeholder; this is presentation, not source content.
    pub presentation: SharedString,
    /// Complete style runs for the bounded presentation.
    pub runs: Vec<TextRun>,
    /// Exact inline extent of the presentation atom.
    pub width: Pixels,
    /// Exact block extent of the presentation atom.
    pub height: Pixels,
    /// Baseline offset from the atom's top edge.
    pub baseline: Pixels,
    /// Optional app-neutral atom background.
    pub background: Option<Hsla>,
    /// Whether this atom terminates its consumer logical line.
    pub ends_logical_line: bool,
}

/// Typed streaming-layout rejection. Rejection never advances session continuation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StreamingLayoutError {
    /// A configured limit was zero or a metric was not positive.
    InvalidConfiguration,
    /// A pixel input was non-finite, or zero/negative where forbidden.
    InvalidMetric(StreamingLayoutMetric),
    /// The submitted immutable-input identity differs from the session binding.
    InputMismatch,
    /// The submitted segment policy differs from the session binding.
    SegmentPolicyMismatch,
    /// The ordinal or exact logical range is not the next ordered input.
    OutOfOrder,
    /// Text or style runs do not exactly describe the logical range.
    InvalidSegment,
    /// Actual retained component payload would exceed a finite limit.
    CapacityExceeded(StreamingLayoutComponent),
    /// Exact arithmetic or an integer conversion could not be represented.
    Overflow(StreamingLayoutComponent),
    /// The session was cancelled.
    Cancelled,
}

impl std::fmt::Display for StreamingLayoutError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for StreamingLayoutError {}

/// Pixel input identified by a typed invalid-metric rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamingLayoutMetric {
    /// Session wrapping width.
    WrapWidth,
    /// Session shaping font size.
    FontSize,
    /// Session visual-line height.
    LineHeight,
    /// Carried inline placement.
    InlineOffset,
    /// Carried block placement.
    BlockOffset,
    /// Current visual line's maximum block extent.
    LineBlockExtent,
    /// Oversize atom width.
    AtomWidth,
    /// Oversize atom height.
    AtomHeight,
    /// Oversize atom baseline.
    AtomBaseline,
    /// A paint origin coordinate.
    PaintOrigin,
    /// A hit-test coordinate.
    HitPosition,
}

/// Retained-payload component used by admission errors and exact charges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamingLayoutComponent {
    /// Ordinary segment UTF-8 text.
    SegmentText,
    /// Style-run records and their variable font metadata.
    Runs,
    /// Decoration-run records.
    Decorations,
    /// Shaped glyph records.
    Glyphs,
    /// Wrap-placement facts.
    WrapFacts,
    /// Caret and hit-test map facts.
    Maps,
    /// Fragment records.
    Fragments,
    /// Compact continuation record.
    Continuation,
    /// Total retained component payload.
    Total,
}

/// Exact retained payload according to GPUI's component accounting model.
///
/// The model counts each initialized semantic record exactly once using its public record type's
/// `size_of`, plus variable UTF-8/string payload owned or logically retained by that record.
/// Enclosing enum, smart-pointer, vector/container, allocator, and spare-capacity bookkeeping is
/// excluded, as are shared-cache storage and global process residency.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamingLayoutCharge {
    /// Segment text bytes.
    pub segment_text: usize,
    /// Style-run records and variable font metadata.
    pub runs: usize,
    /// Decoration-run records.
    pub decorations: usize,
    /// Shaped glyph records.
    pub glyphs: usize,
    /// Wrap facts.
    pub wrap_facts: usize,
    /// Caret/hit-test map facts.
    pub maps: usize,
    /// Fragment records and bounded atom presentation.
    pub fragments: usize,
    /// Compact continuation record.
    pub continuation: usize,
}

impl StreamingLayoutCharge {
    /// Total exactly charged component payload bytes.
    pub fn total(self) -> Result<usize, StreamingLayoutError> {
        checked::checked_sum(
            [
                self.segment_text,
                self.runs,
                self.decorations,
                self.glyphs,
                self.wrap_facts,
                self.maps,
                self.fragments,
                self.continuation,
            ],
            StreamingLayoutComponent::Total,
        )
    }
}

/// Exact retained semantic-record counts for one streaming-layout graph.
///
/// Counts are computed by GPUI from the final retained fragment graph. Input-only records that are
/// discarded before an admission is returned are excluded. In particular, an oversize atom's
/// caller-provided style runs are not retained even though its shaped runs and glyphs are.
/// Each fragment has one logical text or presentation payload record, including for an empty
/// payload; aliased `SharedString` handles inside that fragment do not duplicate the record.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamingLayoutItemCharge {
    /// Retained logical text or bounded-presentation payload records.
    pub text_payloads: usize,
    /// Caller style-run records retained by ordinary text fragments.
    pub style_runs: usize,
    /// Private shaped-run records retained by shaped line layouts.
    pub shaped_runs: usize,
    /// Shaped glyph records retained by shaped runs.
    pub glyphs: usize,
    /// Decoration records retained by shaped lines.
    pub decorations: usize,
    /// Wrap-boundary facts retained by wrapped text lines.
    pub wrap_facts: usize,
    /// Caret and hit-test map records.
    pub maps: usize,
    /// Text or oversize-atom fragment records.
    pub fragments: usize,
    /// Compact continuation records.
    pub continuations: usize,
}

impl StreamingLayoutItemCharge {
    /// Checked total of all retained semantic records.
    pub fn total(self) -> Result<usize, StreamingLayoutError> {
        checked::checked_sum(
            [
                self.text_payloads,
                self.style_runs,
                self.shaped_runs,
                self.glyphs,
                self.decorations,
                self.wrap_facts,
                self.maps,
                self.fragments,
                self.continuations,
            ],
            StreamingLayoutComponent::Total,
        )
    }
}

/// Result of hit-testing one fragment without stealing an adjacent shared boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamingLayoutHit {
    /// The point precedes this fragment's geometric ownership.
    BeforeFragment,
    /// The point resolves to an exact logical offset owned by this fragment.
    Offset(u64),
    /// The point follows this fragment, including its unowned trailing shared boundary.
    AfterFragment,
}

/// A compact exact source-to-inline placement fact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamingLayoutMap {
    /// Exact consumer logical offset.
    pub logical_offset: u64,
    /// Position relative to the session origin.
    pub position: Point<Pixels>,
}

/// Compact bounded placement state carried between independent shaping segments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamingLayoutContinuation {
    /// Ordinal required by the next admission.
    pub next_ordinal: u64,
    /// Exact logical offset required by the next admission.
    pub next_logical_offset: u64,
    /// Current visual-line inline placement.
    pub inline_offset: Pixels,
    /// Current visual-line block placement.
    pub block_offset: Pixels,
    /// Maximum block extent contributed by content on the current visual line.
    pub line_block_extent: Pixels,
    /// Number of completed visual lines.
    pub visual_lines: u64,
}

/// An immutable admitted text or oversize-atom fragment.
#[derive(Clone, Debug)]
pub enum StreamingLayoutFragment {
    /// One independently shaped canonical text segment.
    Text(StreamingTextFragment),
    /// One compact oversize presentation atom with no source bytes.
    OversizeAtom(StreamingAtomFragment),
}

/// Immutable shape, placement, and interaction facts for one ordinary segment.
#[derive(Clone, Debug)]
pub struct StreamingTextFragment {
    logical_range: Range<u64>,
    line: Arc<WrappedLine>,
    retained_runs: Arc<[TextRun]>,
    origin: Point<Pixels>,
    first_line_inline_offset: Pixels,
    line_height: Pixels,
    first_line_block_extent: Pixels,
    owns_trailing_boundary: bool,
    maps: Arc<[StreamingLayoutMap]>,
}

/// Immutable geometry and bounded presentation for one oversize atom.
#[derive(Clone, Debug)]
pub struct StreamingAtomFragment {
    /// Exact consumer logical range represented without source bytes.
    pub logical_range: Range<u64>,
    /// Bounded presentation placeholder.
    pub presentation: SharedString,
    presentation_line: Arc<ShapedLine>,
    /// Exact bounds relative to the streaming session origin.
    pub bounds: Bounds<Pixels>,
    baseline: Pixels,
    background: Option<Hsla>,
    owns_trailing_boundary: bool,
    maps: [StreamingLayoutMap; 2],
}

/// One atomically admitted result. Errors leave the prior continuation untouched.
#[derive(Clone, Debug)]
pub struct StreamingLayoutAdmission {
    /// Immutable admitted fragments.
    pub fragments: Arc<[StreamingLayoutFragment]>,
    /// Continuation to be used by the next ordered admission.
    pub continuation: StreamingLayoutContinuation,
    /// Exact API-computed retained component charges.
    pub charge: StreamingLayoutCharge,
    /// Exact API-computed retained semantic-record counts.
    pub item_charge: StreamingLayoutItemCharge,
}

/// A window-affine bounded streaming layout session.
pub struct StreamingLayoutSession<'a> {
    text_system: &'a WindowTextSystem,
    binding: StreamingLayoutBinding,
    continuation: Option<StreamingLayoutContinuation>,
}

impl WindowTextSystem {
    /// Starts a bounded session tied to this window's text system.
    ///
    /// Canonical segments remain independent platform shaping contexts. Only exact placement state
    /// crosses calls; this API does not promise equivalence with one unbounded platform paragraph.
    pub fn streaming_layout_session(
        &self,
        binding: StreamingLayoutBinding,
    ) -> Result<StreamingLayoutSession<'_>, StreamingLayoutError> {
        validate_binding(&binding)?;
        let line_height = binding.line_height;

        Ok(StreamingLayoutSession {
            text_system: self,
            binding,
            continuation: Some(StreamingLayoutContinuation {
                next_ordinal: 0,
                next_logical_offset: 0,
                inline_offset: Pixels::ZERO,
                block_offset: Pixels::ZERO,
                line_block_extent: line_height,
                visual_lines: 0,
            }),
        })
    }

    /// Resumes a bounded session from caller-retained compact placement state.
    ///
    /// The caller is responsible for pairing the continuation with the same immutable binding
    /// identities that produced it. Invalid or non-finite carried metrics are rejected.
    pub fn resume_streaming_layout_session(
        &self,
        binding: StreamingLayoutBinding,
        continuation: StreamingLayoutContinuation,
    ) -> Result<StreamingLayoutSession<'_>, StreamingLayoutError> {
        validate_binding(&binding)?;
        accounting::validate_continuation(continuation, binding.line_height)?;
        Ok(StreamingLayoutSession {
            text_system: self,
            binding,
            continuation: Some(continuation),
        })
    }
}

fn validate_binding(binding: &StreamingLayoutBinding) -> Result<(), StreamingLayoutError> {
    let limits = binding.limits;
    checked::validate_positive(binding.wrap_width, StreamingLayoutMetric::WrapWidth)?;
    checked::validate_positive(binding.font_size, StreamingLayoutMetric::FontSize)?;
    checked::validate_positive(binding.line_height, StreamingLayoutMetric::LineHeight)?;
    if limits.segment_bytes == 0
        || limits.runs == 0
        || limits.decorations == 0
        || limits.glyphs == 0
        || limits.wraps == 0
        || limits.maps == 0
        || limits.fragments == 0
        || limits.retained_bytes == 0
    {
        return Err(StreamingLayoutError::InvalidConfiguration);
    }
    Ok(())
}
