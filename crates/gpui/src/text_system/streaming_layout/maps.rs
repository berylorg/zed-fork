use super::{checked, map_positions};
use crate::{
    Pixels, Point, StreamingLayoutComponent, StreamingLayoutError, StreamingLayoutMap,
    StreamingLayoutPosition, WrappedLineLayout,
};
use std::ops::Range;

pub(super) fn build_maps(
    line: &WrappedLineLayout,
    logical_range: &Range<StreamingLayoutPosition>,
    first_inline: Pixels,
    origin: Point<Pixels>,
    line_height: Pixels,
    first_line_block_extent: Pixels,
) -> Result<Vec<StreamingLayoutMap>, StreamingLayoutError> {
    let first_line_extra = checked::checked_pixel_sub(
        first_line_block_extent,
        line_height,
        StreamingLayoutComponent::Maps,
    )?;
    map_positions::build_positions(line, line_height)
        .map(|(index, position)| {
            let mut position = position.ok_or(StreamingLayoutError::InvalidSegment)?;
            if position.y == Pixels::ZERO {
                position.x = checked::checked_pixel_add(
                    position.x,
                    first_inline,
                    StreamingLayoutComponent::Maps,
                )?;
            } else {
                position.y = checked::checked_pixel_add(
                    position.y,
                    first_line_extra,
                    StreamingLayoutComponent::Maps,
                )?;
            }
            let byte_offset = checked::checked_u64_add(
                logical_range.start.byte_offset,
                checked::usize_to_u64(index, StreamingLayoutComponent::Maps)?,
                StreamingLayoutComponent::Maps,
            )?;
            let logical_position = if index == 0 {
                logical_range.start
            } else if index == line.len() {
                logical_range.end
            } else {
                StreamingLayoutPosition::at(byte_offset)
            };
            Ok(StreamingLayoutMap {
                logical_position,
                position: checked::checked_point_add(
                    position,
                    origin,
                    StreamingLayoutComponent::Maps,
                )?,
            })
        })
        .collect()
}
