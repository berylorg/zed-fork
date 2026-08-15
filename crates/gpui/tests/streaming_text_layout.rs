#[cfg(feature = "test-support")]
use gpui::{
    SharedString, StreamingEndOfSource, StreamingInlineObject, StreamingLayoutBinding,
    StreamingLayoutCharge, StreamingLayoutComponent, StreamingLayoutContinuation,
    StreamingLayoutError, StreamingLayoutFragment, StreamingLayoutHit, StreamingLayoutItemCharge,
    StreamingLayoutLimits, StreamingLayoutMetric, StreamingLayoutPosition,
    StreamingLineFinalization, StreamingObjectGap, StreamingObjectId, StreamingObjectOrder,
    StreamingOversizeAtom, StreamingTextSegment, TestAppContext, TextRun, WindowTextSystem, font,
    point, px,
};

#[test]
fn streaming_text_layout_test_binary_builds_without_test_support() {}

#[cfg(feature = "test-support")]
fn limits() -> StreamingLayoutLimits {
    StreamingLayoutLimits {
        segment_bytes: 256,
        runs: 16,
        decorations: 16,
        glyphs: 256,
        wraps: 64,
        maps: 257,
        fragments: 1,
        retained_items: 4096,
        retained_bytes: 128 * 1024,
    }
}

#[cfg(feature = "test-support")]
fn binding(
    start_position: StreamingLayoutPosition,
    wrap_width: gpui::Pixels,
) -> StreamingLayoutBinding {
    StreamingLayoutBinding {
        input_id: 7,
        segment_policy_id: 11,
        start_position,
        wrap_width,
        font_size: px(10.),
        line_height: px(14.),
        limits: limits(),
    }
}

#[cfg(feature = "test-support")]
fn run(len: usize) -> TextRun {
    TextRun {
        len,
        font: font(".SystemUIFont"),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    }
}

#[cfg(feature = "test-support")]
fn pos(byte_offset: u64) -> StreamingLayoutPosition {
    StreamingLayoutPosition::at(byte_offset)
}

#[cfg(feature = "test-support")]
fn segment(
    ordinal: u64,
    start: StreamingLayoutPosition,
    end: StreamingLayoutPosition,
    text: &str,
) -> StreamingTextSegment {
    StreamingTextSegment {
        input_id: 7,
        segment_policy_id: 11,
        ordinal,
        logical_range: start..end,
        text: SharedString::new(text.to_owned()),
        runs: vec![run(text.len())],
    }
}

#[cfg(feature = "test-support")]
fn atom(ordinal: u64, start: u64, end: u64) -> StreamingOversizeAtom {
    StreamingOversizeAtom {
        input_id: 7,
        segment_policy_id: 11,
        ordinal,
        logical_range: pos(start)..pos(end),
        presentation: SharedString::new_static("[item]"),
        runs: vec![run(6)],
        width: px(90.),
        height: px(20.),
        baseline: px(12.),
        background: Some(gpui::black()),
    }
}

#[cfg(feature = "test-support")]
fn with_text_system(test: &mut TestAppContext, f: impl FnOnce(&WindowTextSystem)) {
    let cx = test.add_empty_window();
    cx.update(|window, _| f(window.text_system()));
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn ordinary_text_preserves_shape_and_composite_maps(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let text = "aa bbb cccc";
        let ordinary = text_system
            .shape_text(
                SharedString::new(text.to_owned()),
                px(10.),
                &[run(text.len())],
                Some(px(35.)),
                None,
            )
            .unwrap();
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(35.)))
            .unwrap();
        let admitted = session
            .admit_text(segment(0, pos(0), pos(text.len() as u64), text))
            .unwrap();
        let StreamingLayoutFragment::Text(fragment) = &admitted.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(
            fragment.line().wrap_boundaries(),
            ordinary[0].wrap_boundaries()
        );
        for index in [0, 3] {
            let logical = pos(index as u64);
            assert_eq!(
                fragment.position_for_logical_position(logical).unwrap(),
                ordinary[0].position_for_index(index, px(14.))
            );
        }
        assert_eq!(
            fragment
                .position_for_logical_position(pos(text.len() as u64))
                .unwrap(),
            None
        );
        let start = fragment.maps()[0].position;
        assert_eq!(
            fragment
                .closest_logical_position_for_position(start)
                .unwrap(),
            Some(StreamingLayoutHit::Gap(pos(0)))
        );
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn nonempty_oversize_atom_remains_source_covering(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let admitted = text_system
            .streaming_layout_session(binding(pos(0), px(40.)))
            .unwrap()
            .admit_oversize_atom(atom(0, 0, 10_000))
            .unwrap();
        let StreamingLayoutFragment::OversizeAtom(fragment) = &admitted.fragments[0] else {
            panic!("expected atom")
        };
        assert_eq!(fragment.logical_range, pos(0)..pos(10_000));
        assert_eq!(fragment.maps()[0].logical_position, pos(0));
        assert_eq!(fragment.maps()[1].logical_position, pos(10_000));
        assert_eq!(admitted.charge.segment_text, 0);
    });
}

#[cfg(feature = "test-support")]
#[test]
fn composite_values_and_continuation_have_no_heap_ownership() {
    assert!(!std::mem::needs_drop::<StreamingObjectId>());
    assert!(!std::mem::needs_drop::<StreamingObjectOrder>());
    assert!(!std::mem::needs_drop::<StreamingObjectGap>());
    assert!(!std::mem::needs_drop::<StreamingLayoutPosition>());
    assert!(!std::mem::needs_drop::<StreamingLayoutContinuation>());
    assert!(std::mem::size_of::<StreamingLayoutContinuation>() <= 256);
}

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/objects.rs"]
mod objects;

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/finalization.rs"]
mod finalization;

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/accounting.rs"]
mod accounting;

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/endpoint_continuity.rs"]
mod endpoint_continuity;

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/validation_regressions.rs"]
mod validation_regressions;

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/layout_regressions.rs"]
mod layout_regressions;

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/charge_regressions.rs"]
mod charge_regressions;
