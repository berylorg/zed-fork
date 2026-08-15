use super::*;

impl StreamingTextFragment {
    /// Exact consumer logical range represented by this fragment.
    pub fn logical_range(&self) -> Range<StreamingLayoutPosition> {
        self.logical_range.clone()
    }

    /// Ordinary GPUI wrapped layout for the independently shaped segment.
    pub fn line(&self) -> &Arc<WrappedLine> {
        &self.line
    }

    /// Style runs retained with the fragment for exact payload ownership.
    pub fn runs(&self) -> &[TextRun] {
        &self.retained_runs
    }

    /// Exact fragment origin relative to the streaming session origin.
    pub fn origin(&self) -> Point<Pixels> {
        self.origin
    }

    /// Exact caret/hit-test map facts at every shaped glyph boundary and segment end.
    pub fn maps(&self) -> &[StreamingLayoutMap] {
        &self.maps
    }

    /// Returns exact caret geometry for one composite position in this fragment.
    pub fn position_for_logical_position(
        &self,
        logical_position: StreamingLayoutPosition,
    ) -> Result<Option<Point<Pixels>>, StreamingLayoutError> {
        logical_position.validate()?;
        if logical_position == self.logical_range.end {
            return Ok(None);
        }
        let index = if logical_position == self.logical_range.start {
            0
        } else if logical_position.gap == StreamingObjectGap::no_objects()
            && logical_position.byte_offset > self.logical_range.start.byte_offset
            && logical_position.byte_offset < self.logical_range.end.byte_offset
        {
            usize::try_from(logical_position.byte_offset - self.logical_range.start.byte_offset)
                .map_err(|_| StreamingLayoutError::Overflow(StreamingLayoutComponent::Maps))?
        } else {
            return Ok(None);
        };
        let Some(mut position) = self.line.position_for_index(index, self.line_height) else {
            return Ok(None);
        };
        if position.y == Pixels::ZERO {
            position.x = checked::checked_pixel_add(
                position.x,
                self.first_line_inline_offset,
                StreamingLayoutComponent::Maps,
            )?;
        } else {
            let first_line_extra = checked::checked_pixel_sub(
                self.first_line_block_extent,
                self.line_height,
                StreamingLayoutComponent::Maps,
            )?;
            position.y = checked::checked_pixel_add(
                position.y,
                first_line_extra,
                StreamingLayoutComponent::Maps,
            )?;
        }
        Ok(Some(checked::checked_point_add(
            position,
            self.origin,
            StreamingLayoutComponent::Maps,
        )?))
    }

    /// Hit-tests without claiming the trailing boundary shared with the following fragment.
    ///
    /// The following fragment owns a shared logical boundary. An end-of-logical-line fragment owns
    /// its terminal boundary. Geometric points before/after this fragment return the corresponding
    /// non-owning variant, while an exact leading edge belongs to this fragment.
    pub fn closest_logical_position_for_position(
        &self,
        mut position: Point<Pixels>,
    ) -> Result<Option<StreamingLayoutHit>, StreamingLayoutError> {
        checked::validate_point(position, StreamingLayoutMetric::HitPosition)?;
        let start = self
            .maps
            .first()
            .ok_or(StreamingLayoutError::InvalidSegment)?;
        let end = self
            .maps
            .last()
            .ok_or(StreamingLayoutError::InvalidSegment)?;
        if position == start.position {
            return Ok(Some(StreamingLayoutHit::Gap(self.logical_range.start)));
        }
        if position == end.position {
            return Ok(None);
        }
        let start_block_end = checked::checked_pixel_add(
            start.position.y,
            self.first_line_block_extent,
            StreamingLayoutComponent::Maps,
        )?;
        if position.y < start.position.y
            || (position.y < start_block_end && position.x < start.position.x)
        {
            return Ok(None);
        }
        let final_line_extent = if self.line.wrap_boundaries().is_empty() {
            self.first_line_block_extent
        } else {
            self.line_height
        };
        let end_block_end = checked::checked_pixel_add(
            end.position.y,
            final_line_extent,
            StreamingLayoutComponent::Maps,
        )?;
        if position.y >= end.position.y && position.y < end_block_end && position.x > end.position.x
        {
            return Ok(None);
        }
        let last_block = self
            .maps
            .iter()
            .map(|map| map.position.y)
            .max()
            .unwrap_or(self.origin.y);
        if position.y < self.origin.y {
            return Ok(None);
        }
        let block_end = checked::checked_pixel_add(
            last_block,
            final_line_extent,
            StreamingLayoutComponent::Maps,
        )?;
        if position.y >= block_end {
            return Ok(None);
        }
        position =
            checked::checked_point_sub(position, self.origin, StreamingLayoutComponent::Maps)?;
        if position.y < self.first_line_block_extent {
            position.x = checked::checked_pixel_sub(
                position.x,
                self.first_line_inline_offset,
                StreamingLayoutComponent::Maps,
            )?;
            position.y = Pixels::ZERO;
        } else {
            let first_line_extra = checked::checked_pixel_sub(
                self.first_line_block_extent,
                self.line_height,
                StreamingLayoutComponent::Maps,
            )?;
            position.y = checked::checked_pixel_sub(
                position.y,
                first_line_extra,
                StreamingLayoutComponent::Maps,
            )?;
        }
        let index = self
            .line
            .closest_index_for_position(position, self.line_height)
            .unwrap_or_else(|index| index);
        let index = checked::usize_to_u64(index, StreamingLayoutComponent::Maps)?;
        let byte_offset = checked::checked_u64_add(
            self.logical_range.start.byte_offset,
            index,
            StreamingLayoutComponent::Maps,
        )?;
        let logical_position = if byte_offset == self.logical_range.start.byte_offset {
            self.logical_range.start
        } else if byte_offset == self.logical_range.end.byte_offset {
            self.logical_range.end
        } else {
            StreamingLayoutPosition::at(byte_offset)
        };
        if logical_position == self.logical_range.end {
            Ok(None)
        } else {
            Ok(Some(StreamingLayoutHit::Gap(logical_position)))
        }
    }

    /// Paints this text fragment with the first line continuing at its admitted inline offset.
    pub fn paint(
        &self,
        session_origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> crate::Result<()> {
        checked::validate_point(session_origin, StreamingLayoutMetric::PaintOrigin)?;
        self.line.paint_streaming(
            checked::checked_point_add(
                session_origin,
                self.origin,
                StreamingLayoutComponent::Fragments,
            )?,
            self.first_line_inline_offset,
            self.line_height,
            self.first_line_block_extent,
            window,
            cx,
        )
    }

    /// Paints this text fragment's decoration backgrounds with streaming placement.
    pub fn paint_background(
        &self,
        session_origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> crate::Result<()> {
        checked::validate_point(session_origin, StreamingLayoutMetric::PaintOrigin)?;
        self.line.paint_background_streaming(
            checked::checked_point_add(
                session_origin,
                self.origin,
                StreamingLayoutComponent::Fragments,
            )?,
            self.first_line_inline_offset,
            self.line_height,
            self.first_line_block_extent,
            window,
            cx,
        )
    }
}

impl StreamingAtomFragment {
    /// Exact hit-test/caret facts at the atom's logical boundaries.
    pub fn maps(&self) -> &[StreamingLayoutMap; 2] {
        &self.maps
    }

    /// Baseline offset from the atom's top edge.
    pub fn baseline(&self) -> Pixels {
        self.baseline
    }

    /// Returns the exact caret position for the atom's owned leading logical boundary.
    pub fn position_for_logical_position(
        &self,
        position: StreamingLayoutPosition,
    ) -> Option<Point<Pixels>> {
        match position {
            position if position == self.logical_range.start => Some(self.maps[0].position),
            _ => None,
        }
    }

    /// Hit-tests the atom with a deterministic midpoint split and trailing ownership rule.
    pub fn closest_logical_position_for_position(
        &self,
        position: Point<Pixels>,
    ) -> Result<Option<StreamingLayoutHit>, StreamingLayoutError> {
        checked::validate_point(position, StreamingLayoutMetric::HitPosition)?;
        let left = self.bounds.origin.x;
        let right = checked::checked_pixel_add(
            left,
            self.bounds.size.width,
            StreamingLayoutComponent::Maps,
        )?;
        let bottom = checked::checked_pixel_add(
            self.bounds.origin.y,
            self.bounds.size.height,
            StreamingLayoutComponent::Maps,
        )?;
        if position.y < self.bounds.origin.y || position.x < left {
            return Ok(None);
        }
        if position.y >= bottom || position.x > right {
            return Ok(None);
        }
        let midpoint = checked::checked_pixel_add(
            left,
            checked::checked_pixel_mul_f32(
                self.bounds.size.width,
                0.5,
                StreamingLayoutComponent::Maps,
            )?,
            StreamingLayoutComponent::Maps,
        )?;
        if position.x < midpoint {
            Ok(Some(StreamingLayoutHit::Gap(self.logical_range.start)))
        } else {
            Ok(None)
        }
    }

    /// Paints the bounded presentation at the atom's exact baseline and geometry.
    pub fn paint(
        &self,
        session_origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> crate::Result<()> {
        checked::validate_point(session_origin, StreamingLayoutMetric::PaintOrigin)?;
        let origin = checked::checked_point_add(
            session_origin,
            self.bounds.origin,
            StreamingLayoutComponent::Fragments,
        )?;
        let text_origin = point(
            origin.x,
            checked::checked_pixel_add(
                origin.y,
                checked::checked_pixel_sub(
                    self.baseline,
                    self.presentation_line.ascent,
                    StreamingLayoutComponent::Fragments,
                )?,
                StreamingLayoutComponent::Fragments,
            )?,
        );
        self.presentation_line
            .paint(text_origin, self.bounds.size.height, window, cx)
    }

    /// Paints the optional app-neutral background at the atom's exact geometry.
    pub fn paint_background(
        &self,
        session_origin: Point<Pixels>,
        window: &mut Window,
    ) -> crate::Result<()> {
        checked::validate_point(session_origin, StreamingLayoutMetric::PaintOrigin)?;
        if let Some(background) = self.background {
            let origin = checked::checked_point_add(
                session_origin,
                self.bounds.origin,
                StreamingLayoutComponent::Fragments,
            )?;
            window.paint_quad(crate::fill(
                Bounds::new(origin, self.bounds.size),
                background,
            ));
        }
        Ok(())
    }
}
