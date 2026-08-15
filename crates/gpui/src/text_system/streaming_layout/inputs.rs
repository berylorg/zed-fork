use super::*;

/// Finite admission limits for one bounded streaming text-layout session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamingLayoutLimits {
    /// Maximum UTF-8 bytes in one text or presentation payload.
    pub segment_bytes: usize,
    /// Maximum caller style runs.
    pub runs: usize,
    /// Maximum retained decoration runs.
    pub decorations: usize,
    /// Maximum retained shaped glyphs.
    pub glyphs: usize,
    /// Maximum retained wrap facts.
    pub wraps: usize,
    /// Maximum retained caret and hit maps.
    pub maps: usize,
    /// Maximum fragments returned by one admission.
    pub fragments: usize,
    /// Maximum exact semantic records retained by one admission.
    pub retained_items: usize,
    /// Maximum exact retained payload bytes.
    pub retained_bytes: usize,
}

/// Immutable glyph- and placement-affecting inputs for a streaming session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamingLayoutBinding {
    /// Stable identity of all immutable shaping inputs.
    pub input_id: u64,
    /// Stable identity of the canonical segmentation policy.
    pub segment_policy_id: u64,
    /// Exact first composite position of the ordered stream.
    pub start_position: StreamingLayoutPosition,
    /// Width available to each visual line.
    pub wrap_width: Pixels,
    /// Font size used for shaping.
    pub font_size: Pixels,
    /// Minimum visual-line block extent.
    pub line_height: Pixels,
    /// Finite admission limits.
    pub limits: StreamingLayoutLimits,
}

/// One already-bounded ordinary canonical shaping segment.
#[derive(Clone, Debug)]
pub struct StreamingTextSegment {
    /// Immutable-input identity.
    pub input_id: u64,
    /// Canonical segment-policy identity.
    pub segment_policy_id: u64,
    /// Next ordered input ordinal.
    pub ordinal: u64,
    /// Nonempty exact composite range represented by `text`.
    pub logical_range: Range<StreamingLayoutPosition>,
    /// Complete bounded shaping text.
    pub text: SharedString,
    /// Complete caller style runs for `text`; every cumulative run length must be a UTF-8 scalar
    /// boundary, and the final cumulative length must equal `text.len()`.
    pub runs: Vec<TextRun>,
}

/// Bounded presentation for an indivisible nonempty source range.
#[derive(Clone, Debug)]
pub struct StreamingOversizeAtom {
    /// Immutable-input identity.
    pub input_id: u64,
    /// Canonical segment-policy identity.
    pub segment_policy_id: u64,
    /// Next ordered input ordinal.
    pub ordinal: u64,
    /// Exact nonempty composite source range.
    pub logical_range: Range<StreamingLayoutPosition>,
    /// Bounded presentation, not source content.
    pub presentation: SharedString,
    /// Complete presentation style runs; every cumulative run length must be a UTF-8 scalar
    /// boundary, and the final cumulative length must equal `presentation.len()`.
    pub runs: Vec<TextRun>,
    /// Exact inline extent.
    pub width: Pixels,
    /// Exact block extent.
    pub height: Pixels,
    /// Baseline offset from the top edge.
    pub baseline: Pixels,
    /// Optional app-neutral background.
    pub background: Option<Hsla>,
}

/// Bounded app-neutral presentation for one source-zero-width object.
#[derive(Clone, Debug)]
pub struct StreamingInlineObject {
    /// Immutable-input identity.
    pub input_id: u64,
    /// Canonical segment-policy identity.
    pub segment_policy_id: u64,
    /// Next ordered input ordinal.
    pub ordinal: u64,
    /// Stable opaque object identity.
    pub id: StreamingObjectId,
    /// Stable same-anchor order key.
    pub order: StreamingObjectOrder,
    /// Exact adjacent gap before the object.
    pub leading: StreamingLayoutPosition,
    /// Exact adjacent gap after the object.
    pub trailing: StreamingLayoutPosition,
    /// Bounded app-neutral presentation.
    pub presentation: SharedString,
    /// Complete presentation style runs; every cumulative run length must be a UTF-8 scalar
    /// boundary, and the final cumulative length must equal `presentation.len()`.
    pub runs: Vec<TextRun>,
    /// Exact inline extent.
    pub width: Pixels,
    /// Exact block extent.
    pub height: Pixels,
    /// Baseline offset from the top edge.
    pub baseline: Pixels,
    /// Optional app-neutral background.
    pub background: Option<Hsla>,
}

/// Explicit ordered completion of one logical line.
#[derive(Clone, Debug)]
pub struct StreamingLineFinalization {
    /// Immutable-input identity.
    pub input_id: u64,
    /// Canonical segment-policy identity.
    pub segment_policy_id: u64,
    /// Next ordered input ordinal.
    pub ordinal: u64,
    /// Exact delimiter range, or `None` when the source has no delimiter range.
    pub delimiter_range: Option<Range<StreamingLayoutPosition>>,
    /// Exact position of the next logical line.
    pub next_position: StreamingLayoutPosition,
}

/// Exact terminal fact for one complete source revision.
#[derive(Clone, Copy, Debug)]
pub struct StreamingEndOfSource {
    /// Immutable-input identity.
    pub input_id: u64,
    /// Canonical segment-policy identity.
    pub segment_policy_id: u64,
    /// Next ordered input ordinal.
    pub ordinal: u64,
    /// Exact source UTF-8 extent.
    pub source_extent: u64,
    /// Exact terminal composite position.
    pub position: StreamingLayoutPosition,
}
