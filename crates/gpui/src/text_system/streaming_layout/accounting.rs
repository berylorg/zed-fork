use super::*;
use crate::{LineLayout, WrappedLineLayout};
use smallvec::SmallVec;

pub(super) fn decoration_runs(
    runs: &[TextRun],
) -> Result<SmallVec<[crate::DecorationRun; 32]>, StreamingLayoutError> {
    let mut decorations = SmallVec::<[crate::DecorationRun; 32]>::new();
    for run in runs {
        if let Some(last) = decorations.last_mut()
            && last.color == run.color
            && last.underline == run.underline
            && last.strikethrough == run.strikethrough
            && last.background_color == run.background_color
        {
            let run_len = checked::usize_to_u32(run.len, StreamingLayoutComponent::Decorations)?;
            last.len = last
                .len
                .checked_add(run_len)
                .ok_or(StreamingLayoutError::Overflow(
                    StreamingLayoutComponent::Decorations,
                ))?;
        } else {
            decorations.push(crate::DecorationRun {
                len: checked::usize_to_u32(run.len, StreamingLayoutComponent::Decorations)?,
                color: run.color,
                background_color: run.background_color,
                underline: run.underline,
                strikethrough: run.strikethrough,
            });
        }
    }
    Ok(decorations)
}

pub(super) use super::maps::build_maps;

pub(super) fn continue_after_text(
    prior: StreamingLayoutContinuation,
    layout: &WrappedLineLayout,
    next_position: StreamingLayoutPosition,
    line_height: Pixels,
) -> Result<StreamingLayoutContinuation, StreamingLayoutError> {
    let wrap_count = layout.wrap_boundaries.len();
    let inline_offset = if let Some(boundary) = layout.wrap_boundaries.last() {
        let glyph = &layout.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix];
        checked::checked_pixel_sub(
            layout.unwrapped_layout.width,
            glyph.position.x,
            StreamingLayoutComponent::Continuation,
        )?
    } else {
        checked::checked_pixel_add(
            prior.inline_offset,
            layout.unwrapped_layout.width,
            StreamingLayoutComponent::Continuation,
        )?
    };
    let (wrapped_block, line_block_extent) = if wrap_count == 0 {
        (Pixels::ZERO, prior.line_block_extent)
    } else {
        let following_wraps = wrap_count
            .checked_sub(1)
            .ok_or(StreamingLayoutError::Overflow(
                StreamingLayoutComponent::Continuation,
            ))?;
        let following_block = checked::checked_pixel_mul_usize(
            line_height,
            following_wraps,
            StreamingLayoutComponent::Continuation,
        )?;
        (
            checked::checked_pixel_add(
                prior.line_block_extent,
                following_block,
                StreamingLayoutComponent::Continuation,
            )?,
            line_height,
        )
    };
    let wrap_count_u64 = checked::usize_to_u64(wrap_count, StreamingLayoutComponent::Continuation)?;
    let continuation =
        StreamingLayoutContinuation {
            next_ordinal: prior.next_ordinal.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            next_position,
            inline_offset,
            block_offset: checked::checked_pixel_add(
                prior.block_offset,
                wrapped_block,
                StreamingLayoutComponent::Continuation,
            )?,
            line_block_extent,
            line_has_content: true,
            visual_lines: prior.visual_lines.checked_add(wrap_count_u64).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            line_finalized: false,
            ..prior
        };
    validate_continuation(continuation, line_height)?;
    Ok(continuation)
}

pub(super) fn charge_text(
    fragment: &StreamingTextFragment,
    _continuation: StreamingLayoutContinuation,
) -> Result<StreamingLayoutCharge, StreamingLayoutError> {
    let layout = &fragment.line.layout;
    let run_bytes = charge_runs(&fragment.retained_runs)?;
    Ok(StreamingLayoutCharge {
        segment_text: fragment.line.text.len(),
        runs: run_bytes,
        decorations: checked::checked_mul(
            fragment.line.decoration_runs.len(),
            size_of::<crate::DecorationRun>(),
            StreamingLayoutComponent::Decorations,
        )?,
        glyphs: charge_glyphs(fragment.line.runs())?,
        wrap_facts: checked::checked_mul(
            layout.wrap_boundaries.len(),
            size_of::<crate::WrapBoundary>(),
            StreamingLayoutComponent::WrapFacts,
        )?,
        maps: checked::checked_mul(
            fragment.maps.len(),
            size_of::<StreamingLayoutMap>(),
            StreamingLayoutComponent::Maps,
        )?,
        objects: 0,
        fragments: checked::checked_sum(
            [
                size_of::<Range<StreamingLayoutPosition>>(),
                size_of::<Point<Pixels>>(),
                checked::checked_mul(3, size_of::<Pixels>(), StreamingLayoutComponent::Fragments)?,
                size_of::<Option<Pixels>>(),
                charge_line_layout_metadata()?,
            ],
            StreamingLayoutComponent::Fragments,
        )?,
        continuation: size_of::<StreamingLayoutContinuation>(),
    })
}

pub(super) fn item_charge_text(
    fragment: &StreamingTextFragment,
    continuation: StreamingLayoutContinuation,
) -> Result<StreamingLayoutItemCharge, StreamingLayoutError> {
    let (shaped_runs, glyphs) = shaped_item_counts(fragment.line.runs())?;
    let positions = fragment.maps.iter().map(|map| map.logical_position).chain([
        fragment.logical_range.start,
        fragment.logical_range.end,
        continuation.next_position,
    ]);
    let (position_count, object_facts) = position_facts(positions)?;
    Ok(StreamingLayoutItemCharge {
        text_payloads: 1,
        style_runs: fragment.retained_runs.len(),
        shaped_runs,
        glyphs,
        decorations: fragment.line.decoration_runs.len(),
        wrap_facts: fragment.line.layout.wrap_boundaries.len(),
        maps: fragment.maps.len(),
        positions: position_count,
        gap_witnesses: position_count,
        object_ids: object_facts,
        object_orders: object_facts,
        fragments: 1,
        continuations: 1,
    })
}

pub(super) fn item_charge_atom(
    fragment: &StreamingAtomFragment,
    continuation: StreamingLayoutContinuation,
) -> Result<StreamingLayoutItemCharge, StreamingLayoutError> {
    let (shaped_runs, glyphs) = shaped_item_counts(&fragment.presentation_line.runs)?;
    let (position_count, object_facts) =
        position_facts(fragment.maps.iter().map(|map| map.logical_position).chain([
            fragment.logical_range.start,
            fragment.logical_range.end,
            continuation.next_position,
        ]))?;
    Ok(StreamingLayoutItemCharge {
        text_payloads: 1,
        shaped_runs,
        glyphs,
        decorations: fragment.presentation_line.decoration_runs.len(),
        maps: fragment.maps.len(),
        positions: position_count,
        gap_witnesses: position_count,
        object_ids: object_facts,
        object_orders: object_facts,
        fragments: 1,
        continuations: 1,
        ..Default::default()
    })
}

pub(super) fn item_charge_object(
    fragment: &StreamingObjectFragment,
    continuation: StreamingLayoutContinuation,
) -> Result<StreamingLayoutItemCharge, StreamingLayoutError> {
    let (shaped_runs, glyphs) = shaped_item_counts(&fragment.presentation_line.runs)?;
    let (position_count, position_object_facts) =
        position_facts(fragment.maps.iter().map(|map| map.logical_position).chain([
            fragment.leading,
            fragment.trailing,
            continuation.next_position,
        ]))?;
    let object_facts =
        checked::checked_add(position_object_facts, 1, StreamingLayoutComponent::Objects)?;
    Ok(StreamingLayoutItemCharge {
        text_payloads: 1,
        shaped_runs,
        glyphs,
        decorations: fragment.presentation_line.decoration_runs.len(),
        maps: 2,
        positions: position_count,
        gap_witnesses: position_count,
        object_ids: object_facts,
        object_orders: object_facts,
        fragments: 1,
        continuations: 1,
        ..Default::default()
    })
}

pub(super) fn position_facts(
    positions: impl IntoIterator<Item = StreamingLayoutPosition>,
) -> Result<(usize, usize), StreamingLayoutError> {
    positions
        .into_iter()
        .try_fold((0, 0), |(count, objects), position| {
            let edge_objects = usize::from(matches!(
                position.gap.preceding,
                StreamingObjectEdge::Object { .. }
            )) + usize::from(matches!(
                position.gap.following,
                StreamingObjectEdge::Object { .. }
            ));
            Ok((
                checked::checked_add(count, 1, StreamingLayoutComponent::Maps)?,
                checked::checked_add(objects, edge_objects, StreamingLayoutComponent::Objects)?,
            ))
        })
}

pub(super) fn continuation_item_charge(
    continuation: StreamingLayoutContinuation,
) -> StreamingLayoutItemCharge {
    let object_facts = usize::from(matches!(
        continuation.next_position.gap.preceding,
        StreamingObjectEdge::Object { .. }
    )) + usize::from(matches!(
        continuation.next_position.gap.following,
        StreamingObjectEdge::Object { .. }
    ));
    StreamingLayoutItemCharge {
        positions: 1,
        gap_witnesses: 1,
        object_ids: object_facts,
        object_orders: object_facts,
        continuations: 1,
        ..Default::default()
    }
}

fn shaped_item_counts(runs: &[crate::ShapedRun]) -> Result<(usize, usize), StreamingLayoutError> {
    let glyphs = runs.iter().try_fold(0usize, |total, run| {
        checked::checked_add(total, run.glyphs.len(), StreamingLayoutComponent::Glyphs)
    })?;
    Ok((runs.len(), glyphs))
}

pub(super) fn charge_line_layout_metadata() -> Result<usize, StreamingLayoutError> {
    checked::checked_add(
        checked::checked_mul(4, size_of::<Pixels>(), StreamingLayoutComponent::Fragments)?,
        size_of::<usize>(),
        StreamingLayoutComponent::Fragments,
    )
}

pub(super) fn charge_runs(runs: &[TextRun]) -> Result<usize, StreamingLayoutError> {
    runs.iter().try_fold(0usize, |total, run| {
        let feature_bytes = run.font.features.tag_value_list().iter().try_fold(
            0usize,
            |feature_total, (tag, _)| {
                checked::checked_add(
                    feature_total,
                    checked::checked_add(
                        size_of::<(String, u32)>(),
                        tag.len(),
                        StreamingLayoutComponent::Runs,
                    )?,
                    StreamingLayoutComponent::Runs,
                )
            },
        )?;
        let fallback_bytes = run.font.fallbacks.as_ref().map_or(Ok(0), |fallbacks| {
            fallbacks
                .fallback_list()
                .iter()
                .try_fold(0usize, |fallback_total, family| {
                    checked::checked_add(
                        fallback_total,
                        checked::checked_add(
                            size_of::<String>(),
                            family.len(),
                            StreamingLayoutComponent::Runs,
                        )?,
                        StreamingLayoutComponent::Runs,
                    )
                })
        })?;
        checked::checked_add(
            total,
            checked::checked_sum(
                [
                    size_of::<TextRun>(),
                    run.font.family.len(),
                    feature_bytes,
                    fallback_bytes,
                ],
                StreamingLayoutComponent::Runs,
            )?,
            StreamingLayoutComponent::Runs,
        )
    })
}

pub(super) fn charge_glyphs(runs: &[crate::ShapedRun]) -> Result<usize, StreamingLayoutError> {
    let shaped_runs = checked::checked_mul(
        runs.len(),
        size_of::<crate::ShapedRun>(),
        StreamingLayoutComponent::Glyphs,
    )?;
    let glyph_count = runs.iter().try_fold(0usize, |total, run| {
        checked::checked_add(total, run.glyphs.len(), StreamingLayoutComponent::Glyphs)
    })?;
    checked::checked_add(
        shaped_runs,
        checked::checked_mul(
            glyph_count,
            size_of::<crate::ShapedGlyph>(),
            StreamingLayoutComponent::Glyphs,
        )?,
        StreamingLayoutComponent::Glyphs,
    )
}

pub(super) fn validate_continuation(
    continuation: StreamingLayoutContinuation,
    line_height: Pixels,
) -> Result<(), StreamingLayoutError> {
    continuation.next_position.validate()?;
    checked::validate_nonnegative(
        continuation.inline_offset,
        StreamingLayoutMetric::InlineOffset,
    )?;
    checked::validate_nonnegative(
        continuation.block_offset,
        StreamingLayoutMetric::BlockOffset,
    )?;
    checked::validate_positive(
        continuation.line_block_extent,
        StreamingLayoutMetric::LineBlockExtent,
    )?;
    if continuation.line_block_extent < line_height {
        return Err(StreamingLayoutError::InvalidMetric(
            StreamingLayoutMetric::LineBlockExtent,
        ));
    }
    if continuation.finalized_logical_lines > continuation.visual_lines
        || (continuation.line_finalized && continuation.finalized_logical_lines == 0)
        || (continuation.ended && !continuation.line_finalized)
    {
        return Err(StreamingLayoutError::InvalidSegment);
    }
    if continuation.next_ordinal == 0 {
        if continuation.line_has_content
            || continuation.line_finalized
            || continuation.ended
            || continuation.visual_lines != 0
            || continuation.finalized_logical_lines != 0
        {
            return Err(StreamingLayoutError::InvalidSegment);
        }
    } else if !continuation.line_has_content && !continuation.line_finalized {
        return Err(StreamingLayoutError::InvalidSegment);
    }
    if continuation.line_finalized {
        if continuation.inline_offset != Pixels::ZERO {
            return Err(StreamingLayoutError::InvalidMetric(
                StreamingLayoutMetric::InlineOffset,
            ));
        }
        if continuation.line_block_extent != line_height {
            return Err(StreamingLayoutError::InvalidMetric(
                StreamingLayoutMetric::LineBlockExtent,
            ));
        }
        if continuation.line_has_content {
            return Err(StreamingLayoutError::InvalidSegment);
        }
    } else if !continuation.line_has_content && continuation.inline_offset != Pixels::ZERO {
        return Err(StreamingLayoutError::InvalidSegment);
    }
    if continuation.ended && !continuation.next_position.gap.is_terminal() {
        return Err(StreamingLayoutError::InvalidPosition);
    }
    Ok(())
}

pub(super) fn validate_layout_metrics(line: &LineLayout) -> Result<(), StreamingLayoutError> {
    checked::validate_nonnegative(line.width, StreamingLayoutMetric::InlineOffset)?;
    checked::validate_finite(line.ascent, StreamingLayoutMetric::BlockOffset)?;
    checked::validate_finite(line.descent, StreamingLayoutMetric::BlockOffset)?;
    for run in &line.runs {
        for glyph in &run.glyphs {
            checked::validate_finite(glyph.position.x, StreamingLayoutMetric::InlineOffset)?;
            checked::validate_finite(glyph.position.y, StreamingLayoutMetric::BlockOffset)?;
        }
    }
    Ok(())
}
