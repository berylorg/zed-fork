#![cfg(feature = "test-support")]

use gpui::{
    FontId, GlyphId, LineLayout, ShapedGlyph, ShapedRun, WrapBoundary, WrappedLineLayout, point, px,
};
use gpui::{
    Pixels, Point, StreamingLayoutComponent, StreamingLayoutError, StreamingLayoutMap,
    StreamingLayoutMetric, StreamingLayoutPosition, TextRun,
};
use std::{hint::black_box, sync::Arc, time::Instant};

#[path = "../src/text_system/streaming_layout/map_positions.rs"]
mod map_positions;

#[allow(dead_code)]
#[path = "../src/text_system/streaming_layout/checked.rs"]
mod checked;
#[path = "../src/text_system/streaming_layout/maps.rs"]
mod maps;

#[gpui::test]
fn placement_failure_precedes_later_invalid_geometry(cx: &mut gpui::TestAppContext) {
    let mut line = layout(&[0, 1, 3], &[], 2, glyph_id(cx));
    let glyphs = &mut Arc::get_mut(&mut line.unwrapped_layout).unwrap().runs[0].glyphs;
    for (glyph, x) in glyphs.iter_mut().zip([1., 0., 0.]) {
        glyph.position.x = px(x);
    }
    let start = StreamingLayoutPosition::at(0);
    let end = StreamingLayoutPosition::at(2);
    assert_eq!(
        maps::build_maps(
            &line,
            &(start..end),
            px(0.),
            point(px(0.), px(0.)),
            px(1.),
            px(1.)
        ),
        Err(StreamingLayoutError::Overflow(
            StreamingLayoutComponent::Maps
        ))
    );
}

fn layout(indices: &[usize], wraps: &[usize], len: usize, glyph_id: GlyphId) -> WrappedLineLayout {
    let mut result = WrappedLineLayout {
        unwrapped_layout: Arc::new(LineLayout {
            font_size: px(10.),
            width: px(indices.len() as f32 * 5. + 1.),
            ascent: px(8.),
            descent: px(2.),
            len,
            runs: vec![ShapedRun {
                font_id: FontId(0),
                glyphs: indices
                    .iter()
                    .enumerate()
                    .map(|(ordinal, &index)| ShapedGlyph {
                        id: glyph_id,
                        position: point(px(ordinal as f32 * 5.), px(0.)),
                        index,
                        is_emoji: false,
                    })
                    .collect(),
            }],
        }),
        ..Default::default()
    };
    result
        .wrap_boundaries
        .extend(wraps.iter().map(|&glyph_ix| WrapBoundary {
            run_ix: 0,
            glyph_ix,
        }));
    result
}

fn glyph_id(cx: &mut gpui::TestAppContext) -> GlyphId {
    cx.add_empty_window().update(|window, _| {
        window
            .text_system()
            .shape_text(
                gpui::SharedString::new_static("a"),
                px(10.),
                &[gpui::TextRun {
                    len: 1,
                    font: gpui::font(".SystemUIFont"),
                    color: gpui::black(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
                None,
            )
            .unwrap()[0]
            .runs()[0]
            .glyphs[0]
            .id
    })
}

#[gpui::test]
fn bounded_map_geometry_measurement(cx: &mut gpui::TestAppContext) {
    let glyph_id = glyph_id(cx);
    for count in [256, 1024, 4096] {
        let indices = (0..count).collect::<Vec<_>>();
        let wraps = (64..count).step_by(64).collect::<Vec<_>>();
        let line = layout(&indices, &wraps, count, glyph_id);
        assert_parity(&line);
        let iterations = 8;
        let started = Instant::now();
        for _ in 0..iterations {
            for index in 0..=count {
                black_box(line.position_for_index(black_box(index), px(14.)).unwrap());
            }
        }
        eprintln!(
            "map_geometry glyphs={count} wraps={} iterations={iterations} reference_us={}",
            wraps.len(),
            started.elapsed().as_micros()
        );
        let started = Instant::now();
        for _ in 0..iterations {
            black_box(
                map_positions::build_positions(black_box(&line), px(14.)).collect::<Vec<_>>(),
            );
        }
        eprintln!(
            "map_geometry glyphs={count} iterations={iterations} bulk_us={}",
            started.elapsed().as_micros()
        );
    }
}

fn assert_parity(line: &WrappedLineLayout) {
    let mut indices = line
        .unwrapped_layout
        .runs
        .iter()
        .flat_map(|run| run.glyphs.iter().map(|glyph| glyph.index))
        .collect::<Vec<_>>();
    indices.push(line.len());
    indices.sort_unstable();
    indices.dedup();
    let expected = indices
        .into_iter()
        .map(|index| {
            line.position_for_index(index, px(13.25))
                .map(|position| (index, position))
                .ok_or(StreamingLayoutError::InvalidSegment)
        })
        .collect::<Result<Vec<_>, _>>();
    assert_eq!(
        map_positions::build_positions(line, px(13.25))
            .map(|(index, position)| position
                .map(|position| (index, position))
                .ok_or(StreamingLayoutError::InvalidSegment))
            .collect::<Result<Vec<_>, _>>(),
        expected
    );
}

#[gpui::test]
fn bulk_geometry_preserves_storage_order_and_wrap_edges(cx: &mut gpui::TestAppContext) {
    let id = glyph_id(cx);
    for indices in [
        vec![],
        vec![0],
        vec![3, 3, 7],
        vec![7, 1, 3, 1, 9],
        vec![0, 5, 3, 9, 7],
        vec![0, 2, 4, 6, 8],
    ] {
        for len in [0, 5, 10] {
            assert_parity(&layout(&indices, &[], len, id));
            for first in 0..indices.len() {
                assert_parity(&layout(&indices, &[first], len, id));
                for second in 0..indices.len() {
                    let mut line = layout(&indices, &[first, second], len, id);
                    assert_parity(&line);
                    let unwrapped = Arc::get_mut(&mut line.unwrapped_layout).unwrap();
                    let right = unwrapped.runs[0].glyphs.split_off(indices.len() / 2);
                    unwrapped.runs.push(ShapedRun {
                        font_id: FontId(1),
                        glyphs: right,
                    });
                    for boundary in &mut line.wrap_boundaries {
                        if boundary.glyph_ix >= indices.len() / 2 {
                            boundary.run_ix = 1;
                            boundary.glyph_ix -= indices.len() / 2;
                        }
                    }
                    assert_parity(&line);
                }
            }
        }
    }
}
