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

    /// Exact payload retained by the session itself.
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

    /// Exact semantic records retained by the session itself.
    pub fn retained_item_charge(&self) -> StreamingLayoutItemCharge {
        StreamingLayoutItemCharge {
            continuations: usize::from(self.continuation.is_some()),
            ..Default::default()
        }
    }

    /// Cancels the session and releases its continuation. Later admissions are rejected.
    pub fn cancel(&mut self) {
        self.continuation = None;
    }

    /// Shapes and atomically admits one ordered ordinary segment.
    pub fn admit_text(
        &mut self,
        segment: StreamingTextSegment,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        let prior = self.validate_order(
            segment.input_id,
            segment.segment_policy_id,
            segment.ordinal,
            &segment.logical_range,
            segment.next_logical_offset,
            segment.ends_logical_line,
        )?;
        validate_continuation(prior, self.binding.line_height)?;
        if segment.text.len() > self.binding.limits.segment_bytes {
            return Err(StreamingLayoutError::CapacityExceeded(
                StreamingLayoutComponent::SegmentText,
            ));
        }
        let text_len =
            checked::usize_to_u64(segment.text.len(), StreamingLayoutComponent::SegmentText)?;
        let logical_len = segment
            .logical_range
            .end
            .checked_sub(segment.logical_range.start)
            .ok_or(StreamingLayoutError::InvalidSegment)?;
        let run_len = segment.runs.iter().try_fold(0usize, |total, run| {
            checked::checked_add(total, run.len, StreamingLayoutComponent::Runs)
        })?;
        if segment.text.contains('\n') || logical_len != text_len || run_len != segment.text.len() {
            return Err(StreamingLayoutError::InvalidSegment);
        }
        self.check_items(
            StreamingLayoutComponent::Runs,
            segment.runs.len(),
            self.binding.limits.runs,
        )?;

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
            None,
        )?;
        let mut placement_prior = prior;
        if wrap_boundaries.first().is_some_and(|boundary| {
            boundary.run_ix == 0 && boundary.glyph_ix == 0 && prior.inline_offset > Pixels::ZERO
        }) {
            wrap_boundaries.remove(0);
            placement_prior.inline_offset = Pixels::ZERO;
            placement_prior.block_offset = checked::checked_pixel_add(
                placement_prior.block_offset,
                placement_prior.line_block_extent,
                StreamingLayoutComponent::Continuation,
            )?;
            placement_prior.line_block_extent = self.binding.line_height;
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
        let map_count = checked::checked_add(glyph_count, 1, StreamingLayoutComponent::Maps)?;
        self.check_items(
            StreamingLayoutComponent::Maps,
            map_count,
            self.binding.limits.maps,
        )?;

        let line = Arc::new(WrappedLine {
            layout: Arc::new(WrappedLineLayout {
                unwrapped_layout: layout.clone(),
                wrap_boundaries,
                wrap_width: Some(self.binding.wrap_width),
            }),
            text: segment.text.clone(),
            decoration_runs,
        });
        let origin = point(Pixels::ZERO, placement_prior.block_offset);
        let maps = build_maps(
            &line,
            &segment.logical_range,
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
        let continuation = continue_after_text(
            placement_prior,
            &line.layout,
            segment.next_logical_offset,
            segment.ends_logical_line,
            self.binding.line_height,
        )?;
        let fragment = StreamingTextFragment {
            logical_range: segment.logical_range,
            line,
            retained_runs: segment.runs.into(),
            origin,
            first_line_inline_offset: placement_prior.inline_offset,
            line_height: self.binding.line_height,
            first_line_block_extent: placement_prior.line_block_extent,
            owns_trailing_boundary: segment.ends_logical_line,
            maps: maps.into(),
        };
        let charge = charge_text(&fragment, continuation)?;
        let item_charge = item_charge_text(&fragment)?;
        item_charge.total()?;
        self.check_total(charge)?;

        let admission = StreamingLayoutAdmission {
            fragments: Arc::from([StreamingLayoutFragment::Text(fragment)]),
            continuation,
            charge,
            item_charge,
        };
        self.continuation = Some(continuation);
        Ok(admission)
    }

    /// Atomically admits one ordered compact oversize atom.
    pub fn admit_oversize_atom(
        &mut self,
        atom: StreamingOversizeAtom,
    ) -> Result<StreamingLayoutAdmission, StreamingLayoutError> {
        let prior = self.validate_order(
            atom.input_id,
            atom.segment_policy_id,
            atom.ordinal,
            &atom.logical_range,
            atom.next_logical_offset,
            atom.ends_logical_line,
        )?;
        validate_continuation(prior, self.binding.line_height)?;
        if atom.logical_range.start == atom.logical_range.end {
            return Err(StreamingLayoutError::InvalidSegment);
        }
        checked::validate_nonnegative(atom.width, StreamingLayoutMetric::AtomWidth)?;
        checked::validate_positive(atom.height, StreamingLayoutMetric::AtomHeight)?;
        checked::validate_nonnegative(atom.baseline, StreamingLayoutMetric::AtomBaseline)?;
        if atom.baseline > atom.height {
            return Err(StreamingLayoutError::InvalidMetric(
                StreamingLayoutMetric::AtomBaseline,
            ));
        }
        if atom.presentation.len() > self.binding.limits.segment_bytes {
            return Err(StreamingLayoutError::CapacityExceeded(
                StreamingLayoutComponent::Fragments,
            ));
        }
        let presentation_run_len = atom.runs.iter().try_fold(0usize, |total, run| {
            checked::checked_add(total, run.len, StreamingLayoutComponent::Runs)
        })?;
        if presentation_run_len != atom.presentation.len() {
            return Err(StreamingLayoutError::InvalidSegment);
        }
        self.check_items(
            StreamingLayoutComponent::Runs,
            atom.runs.len(),
            self.binding.limits.runs,
        )?;
        self.check_items(
            StreamingLayoutComponent::Fragments,
            1,
            self.binding.limits.fragments,
        )?;
        let presentation_decorations = decoration_runs(&atom.runs)?;
        self.check_items(
            StreamingLayoutComponent::Decorations,
            presentation_decorations.len(),
            self.binding.limits.decorations,
        )?;

        let mut inline = prior.inline_offset;
        let mut block = prior.block_offset;
        let mut visual_lines = prior.visual_lines;
        let mut line_block_extent = prior.line_block_extent;
        let proposed_inline =
            checked::checked_pixel_add(inline, atom.width, StreamingLayoutComponent::Continuation)?;
        if inline > Pixels::ZERO && proposed_inline > self.binding.wrap_width {
            inline = Pixels::ZERO;
            block = checked::checked_pixel_add(
                block,
                line_block_extent,
                StreamingLayoutComponent::Continuation,
            )?;
            line_block_extent = self.binding.line_height;
            visual_lines = visual_lines
                .checked_add(1)
                .ok_or(StreamingLayoutError::Overflow(
                    StreamingLayoutComponent::Continuation,
                ))?;
        }
        let atom_inline_end =
            checked::checked_pixel_add(inline, atom.width, StreamingLayoutComponent::Continuation)?;
        if atom.height > line_block_extent {
            line_block_extent = atom.height;
        }
        let presentation_line = Arc::new(ShapedLine {
            layout: self.text_system.layout_line_uncached(
                atom.presentation.as_ref(),
                self.binding.font_size,
                &atom.runs,
                Some(atom.width),
            ),
            text: atom.presentation.clone(),
            decoration_runs: presentation_decorations,
        });
        validate_layout_metrics(&presentation_line)?;
        if !atom.presentation.is_empty() {
            if atom.baseline < presentation_line.ascent {
                return Err(StreamingLayoutError::InvalidMetric(
                    StreamingLayoutMetric::AtomBaseline,
                ));
            }
            let presentation_bottom = checked::checked_pixel_add(
                atom.baseline,
                presentation_line.descent,
                StreamingLayoutComponent::Fragments,
            )?;
            if presentation_bottom > atom.height {
                return Err(StreamingLayoutError::InvalidMetric(
                    StreamingLayoutMetric::AtomHeight,
                ));
            }
        }
        let presentation_glyphs =
            presentation_line
                .runs
                .iter()
                .try_fold(0usize, |total, run| {
                    checked::checked_add(total, run.glyphs.len(), StreamingLayoutComponent::Glyphs)
                })?;
        self.check_items(
            StreamingLayoutComponent::Glyphs,
            presentation_glyphs,
            self.binding.limits.glyphs,
        )?;
        let logical_start = atom.logical_range.start;
        let logical_end = atom.logical_range.end;
        let fragment = StreamingAtomFragment {
            logical_range: atom.logical_range,
            presentation: atom.presentation,
            presentation_line,
            bounds: Bounds::new(point(inline, block), size(atom.width, atom.height)),
            baseline: atom.baseline,
            background: atom.background,
            owns_trailing_boundary: atom.ends_logical_line,
            maps: [
                StreamingLayoutMap {
                    logical_offset: logical_start,
                    position: point(inline, block),
                },
                StreamingLayoutMap {
                    logical_offset: logical_end,
                    position: point(atom_inline_end, block),
                },
            ],
        };
        let mut continuation = StreamingLayoutContinuation {
            next_ordinal: prior.next_ordinal.checked_add(1).ok_or(
                StreamingLayoutError::Overflow(StreamingLayoutComponent::Continuation),
            )?,
            next_logical_offset: atom.next_logical_offset,
            inline_offset: atom_inline_end,
            block_offset: block,
            line_block_extent,
            visual_lines,
        };
        if atom.ends_logical_line {
            continuation.inline_offset = Pixels::ZERO;
            continuation.block_offset = checked::checked_pixel_add(
                continuation.block_offset,
                continuation.line_block_extent,
                StreamingLayoutComponent::Continuation,
            )?;
            continuation.line_block_extent = self.binding.line_height;
            continuation.visual_lines =
                continuation
                    .visual_lines
                    .checked_add(1)
                    .ok_or(StreamingLayoutError::Overflow(
                        StreamingLayoutComponent::Continuation,
                    ))?;
        }
        let charge = StreamingLayoutCharge {
            decorations: checked::checked_mul(
                fragment.presentation_line.decoration_runs.len(),
                size_of::<crate::DecorationRun>(),
                StreamingLayoutComponent::Decorations,
            )?,
            glyphs: charge_glyphs(&fragment.presentation_line.runs)?,
            fragments: checked::checked_sum(
                [
                    size_of::<Range<u64>>(),
                    fragment.presentation.len(),
                    size_of::<Bounds<Pixels>>(),
                    size_of::<Pixels>(),
                    size_of::<Option<Hsla>>(),
                    size_of::<bool>(),
                    charge_line_layout_metadata()?,
                ],
                StreamingLayoutComponent::Fragments,
            )?,
            maps: checked::checked_mul(
                2,
                size_of::<StreamingLayoutMap>(),
                StreamingLayoutComponent::Maps,
            )?,
            continuation: size_of::<StreamingLayoutContinuation>(),
            ..Default::default()
        };
        let item_charge = item_charge_atom(&fragment)?;
        item_charge.total()?;
        self.check_items(StreamingLayoutComponent::Maps, 2, self.binding.limits.maps)?;
        self.check_total(charge)?;

        let admission = StreamingLayoutAdmission {
            fragments: Arc::from([StreamingLayoutFragment::OversizeAtom(fragment)]),
            continuation,
            charge,
            item_charge,
        };
        self.continuation = Some(continuation);
        Ok(admission)
    }

    fn validate_order(
        &self,
        input_id: u64,
        segment_policy_id: u64,
        ordinal: u64,
        logical_range: &Range<u64>,
        next_logical_offset: u64,
        ends_logical_line: bool,
    ) -> Result<StreamingLayoutContinuation, StreamingLayoutError> {
        let prior = self.continuation.ok_or(StreamingLayoutError::Cancelled)?;
        if input_id != self.binding.input_id {
            return Err(StreamingLayoutError::InputMismatch);
        }
        if segment_policy_id != self.binding.segment_policy_id {
            return Err(StreamingLayoutError::SegmentPolicyMismatch);
        }
        if ordinal != prior.next_ordinal || logical_range.start != prior.next_logical_offset {
            return Err(StreamingLayoutError::OutOfOrder);
        }
        if logical_range.start > logical_range.end
            || (logical_range.start == logical_range.end && !ends_logical_line)
            || next_logical_offset < logical_range.end
            || (!ends_logical_line && next_logical_offset != logical_range.end)
        {
            return Err(StreamingLayoutError::InvalidSegment);
        }
        Ok(prior)
    }

    fn check_items(
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
}
