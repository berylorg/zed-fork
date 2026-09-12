use crate::{
    App, Bounds, Hsla, Pixels, Point, ShapedLine, SharedString, TextRun, Window, WindowTextSystem,
    WrappedLine, point, size,
};
use std::{mem::size_of, ops::Range, sync::Arc};

mod accounting;
pub(crate) mod checked;
mod finalization;
mod fragments;
mod inline;
mod inputs;
mod map_positions;
mod maps;
mod object_fragments;
mod position;
mod session;

pub use inputs::*;
pub use position::*;

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
    /// A composite position or adjacent-object witness is malformed.
    InvalidPosition,
    /// Text or style runs do not exactly describe the logical range.
    InvalidSegment,
    /// The session already accepted end of source.
    Ended,
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
    /// Inline-object identity, order, and geometry facts.
    Objects,
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
    /// Inline-object identity, order, and geometry facts.
    pub objects: usize,
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
                self.objects,
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
    /// Composite-position records.
    pub positions: usize,
    /// Adjacent-object gap-witness records.
    pub gap_witnesses: usize,
    /// Retained object-identity records.
    pub object_ids: usize,
    /// Retained object-order records.
    pub object_orders: usize,
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
                self.positions,
                self.gap_witnesses,
                self.object_ids,
                self.object_orders,
                self.fragments,
                self.continuations,
            ],
            StreamingLayoutComponent::Total,
        )
    }
}

/// Exact result owned by one fragment's hit-test geometry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamingLayoutHit {
    /// The point resolves to an exact composite gap owned by this fragment.
    Gap(StreamingLayoutPosition),
    /// The point resolves to one realized source-zero-width object.
    Object(StreamingObjectId),
}

/// A compact exact source-to-inline placement fact.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamingLayoutMap {
    /// Exact composite stream position.
    pub logical_position: StreamingLayoutPosition,
    /// Position relative to the session origin.
    pub position: Point<Pixels>,
}

/// Compact bounded placement state carried between independent shaping segments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StreamingLayoutContinuation {
    /// Immutable-input identity required by a resumed session.
    pub input_id: u64,
    /// Canonical segment-policy identity required by a resumed session.
    pub segment_policy_id: u64,
    /// Ordinal required by the next admission.
    pub next_ordinal: u64,
    /// Exact composite position required by the next admission.
    pub next_position: StreamingLayoutPosition,
    /// Current visual-line inline placement.
    pub inline_offset: Pixels,
    /// Current visual-line block placement.
    pub block_offset: Pixels,
    /// Maximum block extent contributed by content on the current visual line.
    pub line_block_extent: Pixels,
    /// Whether the current visual line contains an admitted item, including a zero-width item.
    pub line_has_content: bool,
    /// Number of completed visual lines.
    pub visual_lines: u64,
    /// Number of explicitly finalized logical lines.
    pub finalized_logical_lines: u64,
    /// Whether the preceding ordered input was an explicit line finalization.
    pub line_finalized: bool,
    /// Whether end of source has been accepted.
    pub ended: bool,
}

/// An immutable admitted text or oversize-atom fragment.
#[derive(Clone, Debug)]
pub enum StreamingLayoutFragment {
    /// One independently shaped canonical text segment.
    Text(StreamingTextFragment),
    /// One compact oversize presentation atom with no source bytes.
    OversizeAtom(StreamingAtomFragment),
    /// One source-zero-width opaque object.
    InlineObject(StreamingObjectFragment),
    /// Explicit logical-line or end-of-source boundary geometry.
    Boundary(StreamingBoundaryFragment),
}

/// Immutable shape, placement, and interaction facts for one ordinary segment.
#[derive(Clone, Debug)]
pub struct StreamingTextFragment {
    logical_range: Range<StreamingLayoutPosition>,
    line: Arc<WrappedLine>,
    retained_runs: Arc<[TextRun]>,
    origin: Point<Pixels>,
    first_line_inline_offset: Pixels,
    line_height: Pixels,
    first_line_block_extent: Pixels,
    maps: Arc<[StreamingLayoutMap]>,
}

/// Immutable geometry and bounded presentation for one oversize atom.
#[derive(Clone, Debug)]
pub struct StreamingAtomFragment {
    /// Exact consumer logical range represented without source bytes.
    pub logical_range: Range<StreamingLayoutPosition>,
    /// Bounded presentation placeholder.
    pub presentation: SharedString,
    presentation_line: Arc<ShapedLine>,
    /// Exact bounds relative to the streaming session origin.
    pub bounds: Bounds<Pixels>,
    baseline: Pixels,
    background: Option<Hsla>,
    maps: [StreamingLayoutMap; 2],
}

/// Immutable geometry and interaction facts for one zero-width object.
#[derive(Clone, Debug)]
pub struct StreamingObjectFragment {
    /// Stable opaque identity.
    pub id: StreamingObjectId,
    /// Stable same-anchor order key.
    pub order: StreamingObjectOrder,
    /// Exact leading composite gap.
    pub leading: StreamingLayoutPosition,
    /// Exact trailing composite gap.
    pub trailing: StreamingLayoutPosition,
    /// Bounded presentation placeholder.
    pub presentation: SharedString,
    presentation_line: Arc<ShapedLine>,
    /// Exact bounds relative to the streaming session origin.
    pub bounds: Bounds<Pixels>,
    baseline: Pixels,
    background: Option<Hsla>,
    maps: [StreamingLayoutMap; 2],
}

/// Kind of explicit stream boundary represented by a boundary fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamingBoundaryKind {
    /// One explicitly finalized logical line.
    LogicalLine,
    /// The terminal remaining logical line.
    EndOfSource,
}

/// Immutable caret geometry for an explicit stream boundary.
#[derive(Clone, Debug)]
pub struct StreamingBoundaryFragment {
    /// Boundary transition represented by this fragment.
    pub kind: StreamingBoundaryKind,
    maps: Arc<[StreamingLayoutMap]>,
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
        let input_id = binding.input_id;
        let segment_policy_id = binding.segment_policy_id;
        let start_position = binding.start_position;

        let continuation = StreamingLayoutContinuation {
            input_id,
            segment_policy_id,
            next_ordinal: 0,
            next_position: start_position,
            inline_offset: Pixels::ZERO,
            block_offset: Pixels::ZERO,
            line_block_extent: line_height,
            line_has_content: false,
            visual_lines: 0,
            finalized_logical_lines: 0,
            line_finalized: false,
            ended: false,
        };
        validate_continuation_capacity(&binding, continuation)?;

        Ok(StreamingLayoutSession {
            text_system: self,
            binding,
            continuation: Some(continuation),
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
        if continuation.input_id != binding.input_id {
            return Err(StreamingLayoutError::InputMismatch);
        }
        if continuation.segment_policy_id != binding.segment_policy_id {
            return Err(StreamingLayoutError::SegmentPolicyMismatch);
        }
        validate_continuation_capacity(&binding, continuation)?;
        Ok(StreamingLayoutSession {
            text_system: self,
            binding,
            continuation: Some(continuation),
        })
    }
}

fn validate_continuation_capacity(
    binding: &StreamingLayoutBinding,
    continuation: StreamingLayoutContinuation,
) -> Result<(), StreamingLayoutError> {
    if size_of::<StreamingLayoutContinuation>() > binding.limits.retained_bytes
        || accounting::continuation_item_charge(continuation).total()?
            > binding.limits.retained_items
    {
        Err(StreamingLayoutError::CapacityExceeded(
            StreamingLayoutComponent::Total,
        ))
    } else {
        Ok(())
    }
}

fn validate_binding(binding: &StreamingLayoutBinding) -> Result<(), StreamingLayoutError> {
    binding.start_position.validate()?;
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
        || limits.retained_items == 0
        || limits.retained_bytes == 0
    {
        return Err(StreamingLayoutError::InvalidConfiguration);
    }
    Ok(())
}
