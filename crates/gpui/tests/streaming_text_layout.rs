#[cfg(feature = "test-support")]
use gpui::{
    SharedString, StreamingLayoutBinding, StreamingLayoutCharge, StreamingLayoutComponent,
    StreamingLayoutContinuation, StreamingLayoutError, StreamingLayoutFragment, StreamingLayoutHit,
    StreamingLayoutItemCharge, StreamingLayoutLimits, StreamingLayoutMetric, StreamingOversizeAtom,
    StreamingTextSegment, TestAppContext, TextRun, WindowTextSystem, font, point, px,
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
        retained_bytes: 128 * 1024,
    }
}

#[cfg(feature = "test-support")]
fn binding(wrap_width: gpui::Pixels) -> StreamingLayoutBinding {
    StreamingLayoutBinding {
        input_id: 7,
        segment_policy_id: 11,
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
fn segment(ordinal: u64, start: u64, text: &str, ends: bool) -> StreamingTextSegment {
    let text_len = u64::try_from(text.len()).unwrap();
    StreamingTextSegment {
        input_id: 7,
        segment_policy_id: 11,
        ordinal,
        logical_range: start..start.checked_add(text_len).unwrap(),
        next_logical_offset: start.checked_add(text_len).unwrap(),
        text: SharedString::new(text.to_owned()),
        runs: vec![run(text.len())],
        ends_logical_line: ends,
    }
}

#[cfg(feature = "test-support")]
fn atom(
    ordinal: u64,
    logical_range: std::ops::Range<u64>,
    presentation: &'static str,
    ends: bool,
) -> StreamingOversizeAtom {
    StreamingOversizeAtom {
        input_id: 7,
        segment_policy_id: 11,
        ordinal,
        next_logical_offset: logical_range.end,
        logical_range,
        presentation: SharedString::new_static(presentation),
        runs: (!presentation.is_empty())
            .then(|| run(presentation.len()))
            .into_iter()
            .collect(),
        width: px(90.),
        height: px(20.),
        baseline: px(12.),
        background: Some(gpui::black()),
        ends_logical_line: ends,
    }
}

#[cfg(feature = "test-support")]
fn with_text_system(test: &mut TestAppContext, f: impl FnOnce(&WindowTextSystem)) {
    let cx = test.add_empty_window();
    cx.update(|window, _| f(window.text_system()));
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn one_segment_preserves_shape_text_layout(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let text = SharedString::new("aa bbb cccc".to_owned());
        let ordinary = text_system
            .shape_text(
                text.clone(),
                px(10.),
                &[run(text.len())],
                Some(px(35.)),
                None,
            )
            .unwrap();
        let mut session = text_system
            .streaming_layout_session(binding(px(35.)))
            .unwrap();
        let admitted = session
            .admit_text(segment(0, 0, text.as_ref(), true))
            .unwrap();
        let StreamingLayoutFragment::Text(fragment) = &admitted.fragments[0] else {
            panic!("expected text fragment")
        };

        assert_eq!(fragment.line().len(), ordinary[0].len());
        assert_eq!(fragment.line().width(), ordinary[0].width());
        assert_eq!(
            fragment.line().wrap_boundaries(),
            ordinary[0].wrap_boundaries()
        );
        for index in [0, 3, text.len()] {
            assert_eq!(
                fragment.position_for_index(index).unwrap(),
                ordinary[0].position_for_index(index, px(14.))
            );
        }
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn segments_carry_visual_line_placement_and_boundary_maps(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(35.)))
            .unwrap();
        let first = session.admit_text(segment(0, 0, "abc", false)).unwrap();
        assert!(first.continuation.inline_offset > px(0.));
        let second = session.admit_text(segment(1, 3, "defgh", true)).unwrap();
        let StreamingLayoutFragment::Text(fragment) = &second.fragments[0] else {
            panic!("expected text fragment")
        };

        assert_eq!(fragment.maps().first().unwrap().logical_offset, 3);
        assert_eq!(fragment.maps().last().unwrap().logical_offset, 8);
        assert!(second.continuation.block_offset >= px(14.));
        let boundary_position = fragment.position_for_index(0).unwrap().unwrap();
        assert_eq!(
            fragment
                .closest_logical_offset_for_position(boundary_position)
                .unwrap(),
            StreamingLayoutHit::Offset(3)
        );
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn segment_first_glyph_overflow_starts_a_new_visual_line(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(20.)))
            .unwrap();
        let first = session.admit_text(segment(0, 0, "abc", false)).unwrap();
        let second = session.admit_text(segment(1, 3, "d", false)).unwrap();
        let StreamingLayoutFragment::Text(fragment) = &second.fragments[0] else {
            panic!("expected text fragment")
        };

        assert!(fragment.origin().y > px(0.));
        assert_eq!(fragment.position_for_index(0).unwrap().unwrap().x, px(0.));
        assert!(second.continuation.visual_lines > first.continuation.visual_lines);
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn mixed_runs_and_oversize_atom_retain_bounded_presentation(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut mixed = segment(0, 0, "abcdef", false);
        let mut alternate = run(3);
        alternate.color = gpui::white();
        alternate.background_color = Some(gpui::black());
        mixed.runs = vec![run(3), alternate];
        let mut session = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        let text = session.admit_text(mixed).unwrap();
        let StreamingLayoutFragment::Text(fragment) = &text.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(fragment.runs().len(), 2);
        assert!(text.charge.runs > 2 * std::mem::size_of::<TextRun>());
        assert!(text.charge.decorations > 0);

        let atom = session
            .admit_oversize_atom(atom(1, 6..10_006, "[item]", true))
            .unwrap();
        let StreamingLayoutFragment::OversizeAtom(fragment) = &atom.fragments[0] else {
            panic!("expected atom fragment")
        };
        assert_eq!(fragment.logical_range, 6..10_006);
        assert_eq!(fragment.presentation, "[item]");
        assert_eq!(fragment.maps()[0].logical_offset, 6);
        assert_eq!(fragment.maps()[1].logical_offset, 10_006);
        assert_eq!(atom.charge.segment_text, 0);
        assert_eq!(fragment.bounds.size.width, px(90.));
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn actual_component_limits_fail_before_continuation_publication(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(40.)))
            .unwrap();
        let prior = session.continuation().unwrap();
        let error = session
            .admit_text(segment(0, 0, &"x".repeat(257), false))
            .unwrap_err();
        assert_eq!(
            error,
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::SegmentText)
        );
        assert_eq!(session.continuation(), Some(prior));

        let admitted = session.admit_text(segment(0, 0, "abcdef", false)).unwrap();
        let exact_total = admitted.charge.total().unwrap();
        let mut capped_binding = binding(px(40.));
        capped_binding.limits.retained_bytes = exact_total - 1;
        let mut capped = text_system
            .streaming_layout_session(capped_binding)
            .unwrap();
        let error = capped
            .admit_text(segment(0, 0, "abcdef", false))
            .unwrap_err();
        assert_eq!(
            error,
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
        );
        assert_eq!(capped.continuation().unwrap().next_ordinal, 0);
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn order_input_and_policy_mismatches_are_typed_and_atomic(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        let prior = session.continuation();
        assert_eq!(
            session.admit_text(segment(1, 0, "a", false)).unwrap_err(),
            StreamingLayoutError::OutOfOrder
        );
        let mut wrong_input = segment(0, 0, "a", false);
        wrong_input.input_id = 8;
        assert_eq!(
            session.admit_text(wrong_input).unwrap_err(),
            StreamingLayoutError::InputMismatch
        );
        let mut wrong_policy = segment(0, 0, "a", false);
        wrong_policy.segment_policy_id = 12;
        assert_eq!(
            session.admit_text(wrong_policy).unwrap_err(),
            StreamingLayoutError::SegmentPolicyMismatch
        );
        assert_eq!(session.continuation(), prior);
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn cancellation_and_drop_release_all_session_owned_payload(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        let admitted = session.admit_text(segment(0, 0, "payload", false)).unwrap();
        let weak_fragments = std::sync::Arc::downgrade(&admitted.fragments);
        assert!(session.retained_charge().continuation > 0);
        drop(admitted);
        assert!(weak_fragments.upgrade().is_none());

        session.cancel();
        assert_eq!(session.retained_charge().total().unwrap(), 0);
        assert_eq!(
            session
                .admit_text(segment(1, 7, "late", false))
                .unwrap_err(),
            StreamingLayoutError::Cancelled
        );
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn glyph_and_zero_width_fragment_limits_fail_closed(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut glyph_binding = binding(px(80.));
        glyph_binding.limits.glyphs = 2;
        let mut session = text_system.streaming_layout_session(glyph_binding).unwrap();
        let prior = session.continuation();
        assert_eq!(
            session.admit_text(segment(0, 0, "abc", false)).unwrap_err(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Glyphs)
        );
        assert_eq!(session.continuation(), prior);

        let mut atoms = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        for ordinal in 0..32u64 {
            let admitted = atoms
                .admit_oversize_atom(StreamingOversizeAtom {
                    width: px(0.),
                    height: px(1.),
                    baseline: px(0.),
                    background: None,
                    ..atom(ordinal, ordinal..ordinal + 1, "", false)
                })
                .unwrap();
            assert_eq!(admitted.fragments.len(), 1);
            assert_eq!(
                atoms.retained_charge().continuation,
                std::mem::size_of_val(&admitted.continuation)
            );
        }
        assert_eq!(atoms.continuation().unwrap().inline_offset, px(0.));
    });
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn empty_logical_line_preserves_ordinary_zero_length_layout(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let ordinary = text_system
            .shape_text(
                SharedString::new_static(""),
                px(10.),
                &[],
                Some(px(80.)),
                None,
            )
            .unwrap();
        let mut session = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        let admitted = session
            .admit_text(StreamingTextSegment {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 0,
                logical_range: 0..0,
                next_logical_offset: 1,
                text: SharedString::new_static(""),
                runs: Vec::new(),
                ends_logical_line: true,
            })
            .unwrap();
        let StreamingLayoutFragment::Text(fragment) = &admitted.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(fragment.line().len(), ordinary[0].len());
        assert_eq!(fragment.maps().len(), 1);
        assert_eq!(admitted.continuation.next_logical_offset, 1);
        assert_eq!(admitted.continuation.visual_lines, 1);
    });
}

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/corrections.rs"]
mod corrections;

#[cfg(feature = "test-support")]
#[path = "streaming_text_layout/item_charges.rs"]
mod item_charges;
