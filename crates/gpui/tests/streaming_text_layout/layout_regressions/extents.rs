use super::*;

#[gpui::test]
fn first_glyph_wrap_respects_tall_inline_extent(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(22.)))
            .unwrap();
        session
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                height: px(50.),
                baseline: px(12.),
                ..atom(0, 0, 1)
            })
            .unwrap();
        let wrapped = session
            .admit_text(segment(1, pos(1), pos(9), "abcdefgh"))
            .unwrap();
        let StreamingLayoutFragment::Text(fragment) = &wrapped.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(fragment.maps()[0].position.x, px(0.));
        assert_eq!(fragment.maps()[0].position.y, px(50.));
        assert!(fragment.maps().iter().all(|map| map.position.y >= px(50.)));
        assert!(wrapped.continuation.block_offset >= px(50.));
    });
}

#[gpui::test]
fn tall_inline_extents_survive_segments_finalization_and_resume(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(200.)))
            .unwrap();
        let tall = session
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                height: px(40.),
                baseline: px(12.),
                ..atom(0, 0, 1)
            })
            .unwrap();
        assert_eq!(tall.continuation.line_block_extent, px(40.));
        let text = session.admit_text(segment(1, pos(1), pos(2), "x")).unwrap();
        assert_eq!(text.continuation.line_block_extent, px(40.));
        let taller = session
            .admit_oversize_atom(StreamingOversizeAtom {
                width: px(20.),
                height: px(60.),
                baseline: px(12.),
                ..atom(2, 2, 3)
            })
            .unwrap();
        assert_eq!(taller.continuation.line_block_extent, px(60.));
        let finalized = session
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 3,
                delimiter_range: None,
                next_position: pos(3),
            })
            .unwrap();
        assert_eq!(finalized.continuation.block_offset, px(60.));
        let following = session.admit_text(segment(4, pos(3), pos(4), "z")).unwrap();
        let StreamingLayoutFragment::Text(following) = &following.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(following.origin().y, px(60.));

        let initial = text_system
            .streaming_layout_session(binding(pos(42), px(200.)))
            .unwrap()
            .continuation()
            .unwrap();
        let continuation = StreamingLayoutContinuation {
            next_ordinal: 9,
            next_position: pos(42),
            inline_offset: px(10.),
            block_offset: px(70.),
            line_block_extent: px(45.),
            line_has_content: true,
            visual_lines: 3,
            finalized_logical_lines: 2,
            ..initial
        };
        let mut resumed = text_system
            .resume_streaming_layout_session(binding(pos(42), px(200.)), continuation)
            .unwrap();
        resumed
            .admit_oversize_atom(StreamingOversizeAtom {
                ordinal: 9,
                logical_range: pos(42)..pos(43),
                width: px(10.),
                height: px(30.),
                ..atom(9, 42, 43)
            })
            .unwrap();
        let finalized = resumed
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 10,
                delimiter_range: None,
                next_position: pos(43),
            })
            .unwrap();
        assert_eq!(finalized.continuation.block_offset, px(115.));
        assert_eq!(finalized.continuation.line_block_extent, px(14.));
    });
}
