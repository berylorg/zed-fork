use super::*;

#[gpui::test]
fn streaming_layout_never_populates_the_ordinary_line_cache(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let before = text_system.line_layout_cache_entry_count();
        let mut accepted = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        let admission = accepted
            .admit_text(segment(0, 0, "unique-stream-accepted", false))
            .unwrap();
        assert_eq!(text_system.line_layout_cache_entry_count(), before);
        drop(admission);
        accepted.cancel();
        drop(accepted);
        assert_eq!(text_system.line_layout_cache_entry_count(), before);

        let mut rejected_binding = binding(px(80.));
        rejected_binding.limits.glyphs = 1;
        let mut rejected = text_system
            .streaming_layout_session(rejected_binding)
            .unwrap();
        rejected
            .admit_text(segment(0, 0, "unique-stream-rejected", false))
            .unwrap_err();
        assert_eq!(text_system.line_layout_cache_entry_count(), before);

        let mut atom_session = text_system
            .streaming_layout_session(binding(px(80.)))
            .unwrap();
        atom_session
            .admit_oversize_atom(atom(0, 0..1, "unique-atom", false))
            .unwrap();
        assert_eq!(text_system.line_layout_cache_entry_count(), before);

        let mut rejected_atom_binding = binding(px(80.));
        rejected_atom_binding.limits.glyphs = 1;
        let mut rejected_atom = text_system
            .streaming_layout_session(rejected_atom_binding)
            .unwrap();
        rejected_atom
            .admit_oversize_atom(atom(0, 0..1, "unique-rejected-atom", false))
            .unwrap_err();
        assert_eq!(text_system.line_layout_cache_entry_count(), before);

        text_system.layout_line("ordinary-cache-entry", px(10.), &[run(20)], None);
        assert!(text_system.line_layout_cache_entry_count() > before);
    });
}

#[gpui::test]
fn tall_atom_extent_advances_end_wrap_and_multiple_atom_lines(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(px(200.)))
            .unwrap();
        let tall = session
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                height: px(40.),
                baseline: px(12.),
                ..atom(0, 0..1, "[]", false)
            })
            .unwrap();
        assert_eq!(tall.continuation.block_offset, px(0.));
        assert_eq!(tall.continuation.line_block_extent, px(40.));
        let text = session.admit_text(segment(1, 1, "x", false)).unwrap();
        let StreamingLayoutFragment::Text(text_fragment) = &text.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(text_fragment.origin().y, px(0.));
        assert_eq!(text.continuation.line_block_extent, px(40.));

        let ended = session
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                height: px(60.),
                baseline: px(12.),
                ..atom(2, 2..3, "[]", true)
            })
            .unwrap();
        assert_eq!(ended.continuation.block_offset, px(60.));
        assert_eq!(ended.continuation.line_block_extent, px(14.));
        let following = session.admit_text(segment(3, 3, "z", false)).unwrap();
        let StreamingLayoutFragment::Text(following) = &following.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(following.origin().y, px(60.));

        let mut wrap_session = text_system
            .streaming_layout_session(binding(px(30.)))
            .unwrap();
        wrap_session
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                height: px(50.),
                baseline: px(12.),
                ..atom(0, 0..1, "[]", false)
            })
            .unwrap();
        let wrapped = wrap_session
            .admit_text(segment(1, 1, "abcdefgh", false))
            .unwrap();
        let StreamingLayoutFragment::Text(wrapped_fragment) = &wrapped.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(wrapped_fragment.origin().y, px(0.));
        assert!(
            wrapped_fragment
                .maps()
                .iter()
                .any(|map| map.position.y >= px(50.))
        );
        assert!(wrapped.continuation.block_offset >= px(50.));
        assert_eq!(wrapped.continuation.line_block_extent, px(14.));
    });
}

#[gpui::test]
fn resumed_tall_line_extent_is_validated_and_preserved(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let continuation = StreamingLayoutContinuation {
            next_ordinal: 9,
            next_logical_offset: 42,
            inline_offset: px(10.),
            block_offset: px(70.),
            line_block_extent: px(45.),
            visual_lines: 3,
        };
        let mut resumed = text_system
            .resume_streaming_layout_session(binding(px(200.)), continuation)
            .unwrap();
        let admitted = resumed
            .admit_oversize_atom(StreamingOversizeAtom {
                ordinal: 9,
                logical_range: 42..43,
                next_logical_offset: 43,
                width: px(10.),
                height: px(30.),
                ..atom(9, 42..43, "[]", true)
            })
            .unwrap();
        assert_eq!(admitted.continuation.block_offset, px(115.));
        assert_eq!(admitted.continuation.line_block_extent, px(14.));

        let invalid = StreamingLayoutContinuation {
            line_block_extent: px(13.),
            ..continuation
        };
        assert_eq!(
            text_system
                .resume_streaming_layout_session(binding(px(200.)), invalid)
                .err()
                .unwrap(),
            StreamingLayoutError::InvalidMetric(StreamingLayoutMetric::LineBlockExtent)
        );
    });
}

#[gpui::test]
fn atom_charge_matches_retained_payload_and_exact_total_is_atomic(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let candidate = atom(0, 0..10, "[item]", false);
        let mut session = text_system
            .streaming_layout_session(binding(px(200.)))
            .unwrap();
        let admitted = session.admit_oversize_atom(candidate.clone()).unwrap();
        let expected_line_metadata =
            4 * std::mem::size_of::<gpui::Pixels>() + std::mem::size_of::<usize>();
        let expected_fragment = std::mem::size_of::<std::ops::Range<u64>>()
            + candidate.presentation.len()
            + std::mem::size_of::<gpui::Bounds<gpui::Pixels>>()
            + std::mem::size_of::<gpui::Pixels>()
            + std::mem::size_of::<Option<gpui::Hsla>>()
            + std::mem::size_of::<bool>()
            + expected_line_metadata;
        assert_eq!(admitted.charge.segment_text, 0);
        assert_eq!(admitted.charge.runs, 0);
        assert_eq!(admitted.charge.fragments, expected_fragment);
        assert!(admitted.charge.decorations > 0);
        assert!(admitted.charge.glyphs > 0);

        let exact_total = admitted.charge.total().unwrap();
        let mut exact_binding = binding(px(200.));
        exact_binding.limits.retained_bytes = exact_total;
        text_system
            .streaming_layout_session(exact_binding)
            .unwrap()
            .admit_oversize_atom(candidate.clone())
            .unwrap();

        let mut rejected_binding = binding(px(200.));
        rejected_binding.limits.retained_bytes = exact_total - 1;
        let mut rejected = text_system
            .streaming_layout_session(rejected_binding)
            .unwrap();
        let prior = rejected.continuation();
        assert_eq!(
            rejected.admit_oversize_atom(candidate).unwrap_err(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
        );
        assert_eq!(rejected.continuation(), prior);
    });
}
