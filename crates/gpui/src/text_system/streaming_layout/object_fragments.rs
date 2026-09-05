use super::*;

impl StreamingObjectFragment {
    /// Exact leading and trailing caret maps.
    pub fn maps(&self) -> &[StreamingLayoutMap; 2] {
        &self.maps
    }

    /// Baseline offset from the object's top edge.
    pub fn baseline(&self) -> Pixels {
        self.baseline
    }

    /// Returns caret geometry for this fragment's owned leading gap in constant time.
    pub fn position_for_logical_position(
        &self,
        position: StreamingLayoutPosition,
    ) -> Option<Point<Pixels>> {
        if position == self.leading {
            Some(self.maps[0].position)
        } else {
            None
        }
    }

    /// Hit-tests one realized object in constant time with no allocation.
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
        if position.x == left {
            return Ok(Some(StreamingLayoutHit::Gap(self.leading)));
        }
        if position.x == right {
            return Ok(None);
        }
        Ok(Some(StreamingLayoutHit::Object(self.id)))
    }

    /// Paints the bounded object presentation.
    pub fn paint(
        &self,
        session_origin: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> crate::Result<()> {
        self.paint_with_foreground(session_origin, None, window, cx)
    }

    #[allow(missing_docs)]
    pub fn paint_with_color(
        &self,
        session_origin: Point<Pixels>,
        color: Hsla,
        window: &mut Window,
        cx: &mut App,
    ) -> crate::Result<()> {
        self.paint_with_foreground(session_origin, Some(color), window, cx)
    }

    fn paint_with_foreground(
        &self,
        session_origin: Point<Pixels>,
        color: Option<Hsla>,
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
        self.presentation_line.paint_with_color(
            text_origin,
            self.bounds.size.height,
            color,
            window,
            cx,
        )
    }

    /// Paints the optional app-neutral object background.
    pub fn paint_background(
        &self,
        session_origin: Point<Pixels>,
        window: &mut Window,
    ) -> crate::Result<()> {
        self.paint_background_with_color(session_origin, self.background, window)
    }

    #[allow(missing_docs)]
    pub fn paint_background_with_color(
        &self,
        session_origin: Point<Pixels>,
        color: Option<Hsla>,
        window: &mut Window,
    ) -> crate::Result<()> {
        checked::validate_point(session_origin, StreamingLayoutMetric::PaintOrigin)?;
        if let Some(background) = color {
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

impl StreamingBoundaryFragment {
    /// Exact boundary caret maps.
    pub fn maps(&self) -> &[StreamingLayoutMap] {
        &self.maps
    }

    /// Constant-time lookup for boundary positions owned by this fragment.
    pub fn position_for_logical_position(
        &self,
        position: StreamingLayoutPosition,
    ) -> Option<Point<Pixels>> {
        let first = self.maps.first()?;
        match self.kind {
            StreamingBoundaryKind::LogicalLine => (self.maps.len() > 1
                && first.logical_position == position)
                .then_some(first.position),
            StreamingBoundaryKind::EndOfSource => {
                (first.logical_position == position).then_some(first.position)
            }
        }
    }

    /// Hit-tests exact boundary geometry without allocation.
    pub fn closest_logical_position_for_position(
        &self,
        position: Point<Pixels>,
    ) -> Result<Option<StreamingLayoutHit>, StreamingLayoutError> {
        checked::validate_point(position, StreamingLayoutMetric::HitPosition)?;
        let first = self
            .maps
            .first()
            .ok_or(StreamingLayoutError::InvalidSegment)?;
        if self.kind == StreamingBoundaryKind::LogicalLine && self.maps.len() == 1 {
            return Ok(None);
        }
        if position == first.position {
            return Ok(Some(StreamingLayoutHit::Gap(first.logical_position)));
        }
        let last = self
            .maps
            .last()
            .ok_or(StreamingLayoutError::InvalidSegment)?;
        if self.kind == StreamingBoundaryKind::EndOfSource && position == last.position {
            Ok(Some(StreamingLayoutHit::Gap(last.logical_position)))
        } else {
            Ok(None)
        }
    }
}
