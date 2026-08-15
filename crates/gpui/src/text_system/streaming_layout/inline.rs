use super::accounting::*;
use super::*;

pub(super) struct PreparedInline {
    pub line: Arc<ShapedLine>,
    pub bounds: Bounds<Pixels>,
    pub inline_end: Pixels,
    pub line_block_extent: Pixels,
    pub visual_lines: u64,
}

impl StreamingLayoutSession<'_> {
    pub(super) fn prepare_inline(
        &self,
        prior: StreamingLayoutContinuation,
        presentation: &SharedString,
        runs: &[TextRun],
        width: Pixels,
        height: Pixels,
        baseline: Pixels,
    ) -> Result<PreparedInline, StreamingLayoutError> {
        checked::validate_nonnegative(width, StreamingLayoutMetric::AtomWidth)?;
        checked::validate_positive(height, StreamingLayoutMetric::AtomHeight)?;
        checked::validate_nonnegative(baseline, StreamingLayoutMetric::AtomBaseline)?;
        if baseline > height {
            return Err(StreamingLayoutError::InvalidMetric(
                StreamingLayoutMetric::AtomBaseline,
            ));
        }
        if presentation.len() > self.binding.limits.segment_bytes {
            return Err(StreamingLayoutError::CapacityExceeded(
                StreamingLayoutComponent::Fragments,
            ));
        }
        self.check_items(
            StreamingLayoutComponent::Runs,
            runs.len(),
            self.binding.limits.runs,
        )?;
        checked::validate_style_run_boundaries(presentation.as_ref(), runs)?;
        let decorations = decoration_runs(runs)?;
        self.check_items(
            StreamingLayoutComponent::Decorations,
            decorations.len(),
            self.binding.limits.decorations,
        )?;

        let mut inline = prior.inline_offset;
        let mut block = prior.block_offset;
        let mut visual_lines = prior.visual_lines;
        let mut line_block_extent = prior.line_block_extent;
        let proposed_inline =
            checked::checked_pixel_add(inline, width, StreamingLayoutComponent::Continuation)?;
        if prior.line_has_content && proposed_inline > self.binding.wrap_width {
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
        let inline_end =
            checked::checked_pixel_add(inline, width, StreamingLayoutComponent::Continuation)?;
        line_block_extent = line_block_extent.max(height);
        let line = Arc::new(ShapedLine {
            layout: self.text_system.layout_line_uncached(
                presentation.as_ref(),
                self.binding.font_size,
                runs,
                Some(width),
            ),
            text: presentation.clone(),
            decoration_runs: decorations,
        });
        validate_layout_metrics(&line)?;
        if !presentation.is_empty() {
            if baseline < line.ascent {
                return Err(StreamingLayoutError::InvalidMetric(
                    StreamingLayoutMetric::AtomBaseline,
                ));
            }
            if checked::checked_pixel_add(
                baseline,
                line.descent,
                StreamingLayoutComponent::Fragments,
            )? > height
            {
                return Err(StreamingLayoutError::InvalidMetric(
                    StreamingLayoutMetric::AtomHeight,
                ));
            }
        }
        let glyphs = line.runs.iter().try_fold(0usize, |total, run| {
            checked::checked_add(total, run.glyphs.len(), StreamingLayoutComponent::Glyphs)
        })?;
        self.check_items(
            StreamingLayoutComponent::Glyphs,
            glyphs,
            self.binding.limits.glyphs,
        )?;
        self.check_items(StreamingLayoutComponent::Maps, 2, self.binding.limits.maps)?;
        self.check_items(
            StreamingLayoutComponent::Fragments,
            1,
            self.binding.limits.fragments,
        )?;
        Ok(PreparedInline {
            line,
            bounds: Bounds::new(point(inline, block), size(width, height)),
            inline_end,
            line_block_extent,
            visual_lines,
        })
    }

    pub(super) fn validate_object_edges(
        &self,
        object: &StreamingInlineObject,
    ) -> Result<(), StreamingLayoutError> {
        use StreamingObjectEdge::*;
        object.leading.validate()?;
        object.trailing.validate()?;
        if object.leading.byte_offset != object.trailing.byte_offset {
            return Err(StreamingLayoutError::InvalidPosition);
        }
        let admitted = Object {
            id: object.id,
            order: object.order,
        };
        if object.leading.gap.following != admitted || object.trailing.gap.preceding != admitted {
            return Err(StreamingLayoutError::InvalidPosition);
        }
        match object.leading.gap.preceding {
            BeforeAll => {}
            Object { id, order } if id != object.id && order < object.order => {}
            _ => return Err(StreamingLayoutError::InvalidPosition),
        }
        match object.trailing.gap.following {
            AfterAll => {}
            Object { id, order } if id != object.id && order > object.order => {}
            _ => return Err(StreamingLayoutError::InvalidPosition),
        }
        Ok(())
    }

    pub(super) fn inline_charge(
        &self,
        presentation: &SharedString,
        line: &ShapedLine,
        additional_fragment_bytes: usize,
    ) -> Result<StreamingLayoutCharge, StreamingLayoutError> {
        Ok(StreamingLayoutCharge {
            decorations: checked::checked_mul(
                line.decoration_runs.len(),
                size_of::<crate::DecorationRun>(),
                StreamingLayoutComponent::Decorations,
            )?,
            glyphs: charge_glyphs(&line.runs)?,
            maps: checked::checked_mul(
                2,
                size_of::<StreamingLayoutMap>(),
                StreamingLayoutComponent::Maps,
            )?,
            fragments: checked::checked_sum(
                [
                    presentation.len(),
                    size_of::<Bounds<Pixels>>(),
                    size_of::<Pixels>(),
                    size_of::<Option<Hsla>>(),
                    charge_line_layout_metadata()?,
                    additional_fragment_bytes,
                ],
                StreamingLayoutComponent::Fragments,
            )?,
            continuation: size_of::<StreamingLayoutContinuation>(),
            ..Default::default()
        })
    }
}
