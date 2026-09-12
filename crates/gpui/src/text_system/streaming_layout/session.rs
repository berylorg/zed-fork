use super::accounting::*;
use super::*;
use crate::WrappedLineLayout;

impl StreamingLayoutSession<'_> {
    /// Immutable session binding.
    pub fn binding(&self) -> &StreamingLayoutBinding {
        &self.binding
    }

    /// Current compact continuation, or `None` after cancellation.
    pub fn continuation(&self) -> Option<StreamingLayoutContinuation> {
        self.continuation
    }

    /// Exact bytes retained by the active session itself.
    pub fn retained_charge(&self) -> StreamingLayoutCharge {
        StreamingLayoutCharge {
            continuation: self
                .continuation
                .is_some()
                .then_some(size_of::<StreamingLayoutContinuation>())
                .unwrap_or(0),
            ..Default::default()
        }
    }

    /// Exact semantic records retained by the active session itself.
    pub fn retained_item_charge(&self) -> StreamingLayoutItemCharge {
        let Some(continuation) = self.continuation else {
            return StreamingLayoutItemCharge::default();
        };
        continuation_item_charge(continuation)
    }

    /// Cancels the session and releases its compact continuation.
    pub fn cancel(&mut self) {
        self.continuation = None;
    }

    /// Shapes and atomically admits one ordered ordinary canonical segment.
    pub fn admit_text(
        &mut self,
        segment: StreamingTextSegment,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        let prior = self.validate_header(
            segment.input_id,
            segment.segment_policy_id,
            segment.ordinal,
            segment.logical_range.start,
        )?;
        let logical_range = segment.logical_range.clone();
        self.validate_source_range(&logical_range)?;
        if segment.text.len() > self.binding.limits.segment_bytes {
            return Err(StreamingLayoutError::CapacityExceeded(
                StreamingLayoutComponent::SegmentText,
            ));
        }
        let text_len =
            checked::usize_to_u64(segment.text.len(), StreamingLayoutComponent::SegmentText)?;
        if logical_range.end.byte_offset - logical_range.start.byte_offset != text_len
            || segment.text.contains('\n')
        {
            return Err(StreamingLayoutError::InvalidSegment);
        }
        self.check_items(
            StreamingLayoutComponent::Runs,
            segment.runs.len(),
            self.binding.limits.runs,
        )?;
        checked::validate_style_run_boundaries(segment.text.as_ref(), &segment.runs)?;

        let layout = self.text_system.layout_line_uncached(
            segment.text.as_ref(),
            self.binding.font_size,
            &segment.runs,
            None,
        );
        validate_layout_metrics(&layout)?;
        let mut wrap_boundaries = layout.compute_streaming_wrap_boundaries(
            segment.text.as_ref(),
            self.binding.wrap_width,
            prior.inline_offset,
            prior.line_has_content,
            None,
        )?;
        let mut placement_prior = prior;
        if wrap_boundaries.first().is_some_and(|boundary| {
            boundary.run_ix == 0 && boundary.glyph_ix == 0 && prior.line_has_content
        }) {
            wrap_boundaries.remove(0);
            placement_prior.inline_offset = Pixels::ZERO;
            placement_prior.block_offset = checked::checked_pixel_add(
                placement_prior.block_offset,
                placement_prior.line_block_extent,
                StreamingLayoutComponent::Continuation,
            )?;
            placement_prior.line_block_extent = self.binding.line_height;
            placement_prior.line_has_content = false;
            placement_prior.visual_lines = placement_prior.visual_lines.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?;
        }
        self.check_items(
            StreamingLayoutComponent::WrapFacts,
            wrap_boundaries.len(),
            self.binding.limits.wraps,
        )?;
        let decoration_runs = decoration_runs(&segment.runs)?;
        self.check_items(
            StreamingLayoutComponent::Decorations,
            decoration_runs.len(),
            self.binding.limits.decorations,
        )?;
        let glyph_count = layout.runs.iter().try_fold(0usize, |total, run| {
            checked::checked_add(total, run.glyphs.len(), StreamingLayoutComponent::Glyphs)
        })?;
        self.check_items(
            StreamingLayoutComponent::Glyphs,
            glyph_count,
            self.binding.limits.glyphs,
        )?;

        let line = Arc::new(WrappedLine {
            layout: Arc::new(WrappedLineLayout {
                unwrapped_layout: layout,
                wrap_boundaries,
                wrap_width: Some(self.binding.wrap_width),
            }),
            text: segment.text.clone(),
            decoration_runs,
        });
        let origin = point(Pixels::ZERO, placement_prior.block_offset);
        let maps = build_maps(
            &line.layout,
            &logical_range,
            placement_prior.inline_offset,
            origin,
            self.binding.line_height,
            placement_prior.line_block_extent,
        )?;
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
        let mut continuation = continue_after_text(
            placement_prior,
            &line.layout,
            logical_range.end,
            self.binding.line_height,
        )?;
        continuation.line_finalized = false;
        let fragment = StreamingTextFragment {
            logical_range,
            line,
            retained_runs: segment.runs.into(),
            origin,
            first_line_inline_offset: placement_prior.inline_offset,
            line_height: self.binding.line_height,
            first_line_block_extent: placement_prior.line_block_extent,
            maps: maps.into(),
        };
        let charge = charge_text(&fragment, continuation)?;
        let item_charge = item_charge_text(&fragment, continuation)?;
        self.finish_admission(
            StreamingLayoutFragment::Text(fragment),
            continuation,
            charge,
            item_charge,
        )
    }
}

impl StreamingLayoutSession<'_> {
    /// Atomically admits one ordered compact nonempty source-covering atom.
    pub fn admit_oversize_atom(
        &mut self,
        atom: StreamingOversizeAtom,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        let prior = self.validate_header(
            atom.input_id,
            atom.segment_policy_id,
            atom.ordinal,
            atom.logical_range.start,
        )?;
        self.validate_source_range(&atom.logical_range)?;
        let prepared = self.prepare_inline(
            prior,
            &atom.presentation,
            &atom.runs,
            atom.width,
            atom.height,
            atom.baseline,
        )?;
        let logical_range = atom.logical_range;
        let maps = [
            StreamingLayoutMap {
                logical_position: logical_range.start,
                position: prepared.bounds.origin,
            },
            StreamingLayoutMap {
                logical_position: logical_range.end,
                position: point(prepared.inline_end, prepared.bounds.origin.y),
            },
        ];
        let logical_end = logical_range.end;
        let fragment = StreamingAtomFragment {
            logical_range,
            presentation: atom.presentation,
            presentation_line: prepared.line,
            bounds: prepared.bounds,
            baseline: atom.baseline,
            background: atom.background,
            maps,
        };
        let continuation = StreamingLayoutContinuation {
            next_ordinal: prior.next_ordinal.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            next_position: logical_end,
            inline_offset: prepared.inline_end,
            block_offset: prepared.bounds.origin.y,
            line_block_extent: prepared.line_block_extent,
            line_has_content: true,
            visual_lines: prepared.visual_lines,
            line_finalized: false,
            ..prior
        };
        let charge = self.inline_charge(
            &fragment.presentation,
            &fragment.presentation_line,
            size_of::<Range<StreamingLayoutPosition>>(),
        )?;
        let item_charge = item_charge_atom(&fragment, continuation)?;
        self.finish_admission(
            StreamingLayoutFragment::OversizeAtom(fragment),
            continuation,
            charge,
            item_charge,
        )
    }

    /// Atomically admits one ordered source-zero-width opaque object.
    pub fn admit_inline_object(
        &mut self,
        object: StreamingInlineObject,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        let prior = self.validate_header(
            object.input_id,
            object.segment_policy_id,
            object.ordinal,
            object.leading,
        )?;
        self.validate_object_edges(&object)?;
        let prepared = self.prepare_inline(
            prior,
            &object.presentation,
            &object.runs,
            object.width,
            object.height,
            object.baseline,
        )?;
        let maps = [
            StreamingLayoutMap {
                logical_position: object.leading,
                position: prepared.bounds.origin,
            },
            StreamingLayoutMap {
                logical_position: object.trailing,
                position: point(prepared.inline_end, prepared.bounds.origin.y),
            },
        ];
        let fragment = StreamingObjectFragment {
            id: object.id,
            order: object.order,
            leading: object.leading,
            trailing: object.trailing,
            presentation: object.presentation,
            presentation_line: prepared.line,
            bounds: prepared.bounds,
            baseline: object.baseline,
            background: object.background,
            maps,
        };
        let continuation = StreamingLayoutContinuation {
            next_ordinal: prior.next_ordinal.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            next_position: object.trailing,
            inline_offset: prepared.inline_end,
            block_offset: prepared.bounds.origin.y,
            line_block_extent: prepared.line_block_extent,
            line_has_content: true,
            visual_lines: prepared.visual_lines,
            line_finalized: false,
            ..prior
        };
        let mut charge =
            self.inline_charge(&fragment.presentation, &fragment.presentation_line, 0)?;
        charge.objects = checked::checked_sum(
            [
                size_of::<StreamingObjectId>(),
                size_of::<StreamingObjectOrder>(),
                2 * size_of::<StreamingLayoutPosition>(),
            ],
            StreamingLayoutComponent::Objects,
        )?;
        let item_charge = item_charge_object(&fragment, continuation)?;
        self.finish_admission(
            StreamingLayoutFragment::InlineObject(fragment),
            continuation,
            charge,
            item_charge,
        )
    }
}

impl StreamingLayoutSession<'_> {
    pub(super) fn validate_header(
        &self,
        input_id: u64,
        segment_policy_id: u64,
        ordinal: u64,
        start: StreamingLayoutPosition,
    ) -> Result<StreamingLayoutContinuation, StreamingLayoutError> {
        let prior = self.continuation.ok_or(StreamingLayoutError::Cancelled)?;
        if prior.ended {
            return Err(StreamingLayoutError::Ended);
        }
        if input_id != self.binding.input_id {
            return Err(StreamingLayoutError::InputMismatch);
        }
        if segment_policy_id != self.binding.segment_policy_id {
            return Err(StreamingLayoutError::SegmentPolicyMismatch);
        }
        start.validate()?;
        if ordinal != prior.next_ordinal || start != prior.next_position {
            return Err(StreamingLayoutError::OutOfOrder);
        }
        validate_continuation(prior, self.binding.line_height)?;
        Ok(prior)
    }

    pub(super) fn validate_source_range(
        &self,
        range: &Range<StreamingLayoutPosition>,
    ) -> Result<(), StreamingLayoutError> {
        range.start.validate()?;
        range.end.validate()?;
        if range.start.byte_offset >= range.end.byte_offset {
            return Err(StreamingLayoutError::InvalidSegment);
        }
        if !range.start.gap.is_terminal() || !range.end.gap.is_source_range_end() {
            return Err(StreamingLayoutError::InvalidPosition);
        }
        Ok(())
    }

    pub(super) fn finish_admission(
        &mut self,
        fragment: StreamingLayoutFragment,
        continuation: StreamingLayoutContinuation,
        charge: StreamingLayoutCharge,
        item_charge: StreamingLayoutItemCharge,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        validate_continuation(continuation, self.binding.line_height)?;
        self.check_total(charge)?;
        self.check_semantic_total(item_charge)?;
        let admission = StreamingLayoutAdmission {
            fragments: Arc::from([fragment]),
            continuation,
            charge,
            item_charge,
        };
        self.continuation = Some(continuation);
        Ok(admission)
    }

    pub(super) fn check_items(
        &self,
        component: StreamingLayoutComponent,
        actual: usize,
        limit: usize,
    ) -> Result<(), StreamingLayoutError> {
        if actual > limit {
            Err(StreamingLayoutError::CapacityExceeded(component))
        } else {
            Ok(())
        }
    }

    fn check_total(&self, charge: StreamingLayoutCharge) -> Result<(), StreamingLayoutError> {
        if charge.total()? > self.binding.limits.retained_bytes {
            Err(StreamingLayoutError::CapacityExceeded(
                StreamingLayoutComponent::Total,
            ))
        } else {
            Ok(())
        }
    }

    fn check_semantic_total(
        &self,
        charge: StreamingLayoutItemCharge,
    ) -> Result<(), StreamingLayoutError> {
        if charge.total()? > self.binding.limits.retained_items {
            Err(StreamingLayoutError::CapacityExceeded(
                StreamingLayoutComponent::Total,
            ))
        } else {
            Ok(())
        }
    }
}
