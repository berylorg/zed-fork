use super::*;

impl StreamingLayoutSession<'_> {
    /// Atomically finalizes one logical line and consumes its exact delimiter when present.
    pub fn finalize_logical_line(
        &mut self,
        finalization: StreamingLineFinalization,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        let start = finalization
            .delimiter_range
            .as_ref()
            .map_or(finalization.next_position, |range| range.start);
        let prior = self.validate_header(
            finalization.input_id,
            finalization.segment_policy_id,
            finalization.ordinal,
            start,
        )?;
        finalization.next_position.validate()?;
        if let Some(range) = &finalization.delimiter_range {
            self.validate_source_range(range)?;
            if range.end != finalization.next_position || !range.start.gap.is_terminal() {
                return Err(StreamingLayoutError::InvalidPosition);
            }
        } else if finalization.next_position != prior.next_position {
            return Err(StreamingLayoutError::InvalidPosition);
        }

        let next_block = checked::checked_pixel_add(
            prior.block_offset,
            prior.line_block_extent,
            StreamingLayoutComponent::Continuation,
        )?;
        let maps = if finalization.delimiter_range.is_none() {
            vec![StreamingLayoutMap {
                logical_position: finalization.next_position,
                position: point(Pixels::ZERO, next_block),
            }]
        } else {
            vec![
                StreamingLayoutMap {
                    logical_position: start,
                    position: point(prior.inline_offset, prior.block_offset),
                },
                StreamingLayoutMap {
                    logical_position: finalization.next_position,
                    position: point(Pixels::ZERO, next_block),
                },
            ]
        };
        self.check_items(
            StreamingLayoutComponent::Maps,
            maps.len(),
            self.binding.limits.maps,
        )?;
        self.check_items(
            StreamingLayoutComponent::Fragments,
            1,
            self.binding.limits.fragments,
        )?;
        let continuation = StreamingLayoutContinuation {
            next_ordinal: prior.next_ordinal.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            next_position: finalization.next_position,
            inline_offset: Pixels::ZERO,
            block_offset: next_block,
            line_block_extent: self.binding.line_height,
            line_has_content: false,
            visual_lines: prior.visual_lines.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            finalized_logical_lines: prior.finalized_logical_lines.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            line_finalized: true,
            ..prior
        };
        self.finish_boundary(StreamingBoundaryKind::LogicalLine, maps, continuation)
    }

    /// Atomically accepts end of source and finalizes the remaining logical line exactly once.
    pub fn end_source(
        &mut self,
        end: StreamingEndOfSource,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        let prior = self.validate_header(
            end.input_id,
            end.segment_policy_id,
            end.ordinal,
            end.position,
        )?;
        if end.position.byte_offset != end.source_extent || !end.position.gap.is_terminal() {
            return Err(StreamingLayoutError::InvalidPosition);
        }
        let next_block = checked::checked_pixel_add(
            prior.block_offset,
            prior.line_block_extent,
            StreamingLayoutComponent::Continuation,
        )?;
        let maps = vec![StreamingLayoutMap {
            logical_position: end.position,
            position: point(prior.inline_offset, prior.block_offset),
        }];
        let continuation = StreamingLayoutContinuation {
            next_ordinal: prior.next_ordinal.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            inline_offset: Pixels::ZERO,
            block_offset: next_block,
            line_block_extent: self.binding.line_height,
            line_has_content: false,
            visual_lines: prior.visual_lines.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            finalized_logical_lines: prior.finalized_logical_lines.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            line_finalized: true,
            ended: true,
            ..prior
        };
        self.finish_boundary(StreamingBoundaryKind::EndOfSource, maps, continuation)
    }

    fn finish_boundary(
        &mut self,
        kind: StreamingBoundaryKind,
        maps: Vec<StreamingLayoutMap>,
        continuation: StreamingLayoutContinuation,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        let (positions, object_facts) = accounting::position_facts(
            maps.iter()
                .map(|map| map.logical_position)
                .chain([continuation.next_position]),
        )?;
        let charge = StreamingLayoutCharge {
            maps: checked::checked_mul(
                maps.len(),
                size_of::<StreamingLayoutMap>(),
                StreamingLayoutComponent::Maps,
            )?,
            fragments: size_of::<StreamingBoundaryKind>(),
            continuation: size_of::<StreamingLayoutContinuation>(),
            ..Default::default()
        };
        let item_charge = StreamingLayoutItemCharge {
            maps: maps.len(),
            positions,
            gap_witnesses: positions,
            object_ids: object_facts,
            object_orders: object_facts,
            fragments: 1,
            continuations: 1,
            ..Default::default()
        };
        self.finish_admission(
            StreamingLayoutFragment::Boundary(StreamingBoundaryFragment {
                kind,
                maps: maps.into(),
            }),
            continuation,
            charge,
            item_charge,
        )
    }
}
