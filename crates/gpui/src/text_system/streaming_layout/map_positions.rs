use crate::{Pixels, Point, WrappedLineLayout, point};

pub(super) fn build_positions(
    line: &WrappedLineLayout,
    line_height: Pixels,
) -> impl Iterator<Item = (usize, Option<Point<Pixels>>)> {
    let mut positions = line
        .unwrapped_layout
        .runs
        .iter()
        .flat_map(|run| {
            run.glyphs
                .iter()
                .map(|glyph| (glyph.index, point(Pixels::ZERO, Pixels::ZERO)))
        })
        .collect::<Vec<_>>();
    positions.push((line.len(), point(Pixels::ZERO, Pixels::ZERO)));
    positions.sort_unstable_by_key(|entry| entry.0);
    positions.dedup_by_key(|entry| entry.0);

    let mut glyphs = line
        .unwrapped_layout
        .runs
        .iter()
        .flat_map(|run| &run.glyphs)
        .peekable();
    for (index, position) in &mut positions {
        while glyphs.peek().is_some_and(|glyph| glyph.index < *index) {
            glyphs.next();
        }
        position.x = glyphs
            .peek()
            .map_or(line.unwrapped_layout.width, |glyph| glyph.position.x);
    }

    let ends = line
        .wrap_boundaries
        .iter()
        .map(|boundary| {
            let index = line.unwrapped_layout.runs[boundary.run_ix].glyphs[boundary.glyph_ix].index;
            let slot = positions
                .binary_search_by_key(&index, |entry| entry.0)
                .unwrap();
            (index, positions[slot].1.x)
        })
        .chain([(line.len(), Pixels::ZERO)])
        .collect::<Vec<_>>();
    let mut row = 0;
    let mut start_index = 0;
    let mut start_x = line.unwrapped_layout.x_for_index(0);
    positions.into_iter().map(move |(index, position)| {
        while row < ends.len() && index > ends[row].0 {
            start_index = ends[row].0;
            start_x = ends[row].1;
            row += 1;
        }
        if row == ends.len() || index < start_index {
            return (index, None);
        }
        (
            index,
            Some(point(position.x - start_x, row as f32 * line_height)),
        )
    })
}
