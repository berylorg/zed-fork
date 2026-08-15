use super::*;

#[gpui::test]
fn empty_source_has_one_explicit_terminal_logical_line(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let ended = session
            .end_source(StreamingEndOfSource {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 0,
                source_extent: 0,
                position: pos(0),
            })
            .unwrap();
        assert_eq!(ended.continuation.visual_lines, 1);
        assert_eq!(ended.continuation.finalized_logical_lines, 1);
        assert!(ended.continuation.ended);
        assert!(matches!(
            ended.fragments[0],
            StreamingLayoutFragment::Boundary(_)
        ));
        assert_eq!(
            session
                .end_source(StreamingEndOfSource {
                    input_id: 7,
                    segment_policy_id: 11,
                    ordinal: 1,
                    source_extent: 0,
                    position: pos(0),
                })
                .unwrap_err(),
            StreamingLayoutError::Ended
        );
    });
}

#[gpui::test]
fn terminal_delimiter_creates_and_eof_finalizes_the_empty_final_line(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let line = session
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 0,
                delimiter_range: Some(pos(0)..pos(1)),
                next_position: pos(1),
            })
            .unwrap();
        assert_eq!(line.continuation.visual_lines, 1);
        assert_eq!(line.continuation.next_position, pos(1));
        let eof = session
            .end_source(StreamingEndOfSource {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 1,
                source_extent: 1,
                position: pos(1),
            })
            .unwrap();
        assert_eq!(eof.continuation.visual_lines, 2);
        assert_eq!(eof.continuation.finalized_logical_lines, 2);
    });
}

#[gpui::test]
fn consecutive_delimiters_finalize_consecutive_empty_lines(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        for ordinal in 0..2 {
            session
                .finalize_logical_line(StreamingLineFinalization {
                    input_id: 7,
                    segment_policy_id: 11,
                    ordinal,
                    delimiter_range: Some(pos(ordinal)..pos(ordinal + 1)),
                    next_position: pos(ordinal + 1),
                })
                .unwrap();
        }
        assert_eq!(session.continuation().unwrap().visual_lines, 2);
        assert_eq!(session.continuation().unwrap().finalized_logical_lines, 2);
    });
}

#[gpui::test]
fn marker_only_line_finalizes_after_its_last_gap(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let identity = StreamingObjectId(3);
        let object_order = StreamingObjectOrder(4);
        let leading = StreamingLayoutPosition::with_gap(
            0,
            StreamingObjectGap::before(identity, object_order),
        );
        let trailing =
            StreamingLayoutPosition::with_gap(0, StreamingObjectGap::after(identity, object_order));
        let mut session = text_system
            .streaming_layout_session(binding(leading, px(80.)))
            .unwrap();
        session
            .admit_inline_object(StreamingInlineObject {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 0,
                id: identity,
                order: object_order,
                leading,
                trailing,
                presentation: SharedString::new_static("[]"),
                runs: vec![run(2)],
                width: px(10.),
                height: px(20.),
                baseline: px(12.),
                background: None,
            })
            .unwrap();
        let finalized = session
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 1,
                delimiter_range: Some(trailing..pos(1)),
                next_position: pos(1),
            })
            .unwrap();
        assert_eq!(finalized.continuation.block_offset, px(20.));
        assert_eq!(finalized.continuation.next_position, pos(1));
    });
}

#[gpui::test]
fn eof_before_remaining_object_and_mismatched_delimiter_are_atomic(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let identity = StreamingObjectId(1);
        let object_order = StreamingObjectOrder(1);
        let leading = StreamingLayoutPosition::with_gap(
            0,
            StreamingObjectGap::before(identity, object_order),
        );
        let mut session = text_system
            .streaming_layout_session(binding(leading, px(80.)))
            .unwrap();
        let prior = session.continuation();
        assert_eq!(
            session
                .end_source(StreamingEndOfSource {
                    input_id: 7,
                    segment_policy_id: 11,
                    ordinal: 0,
                    source_extent: 0,
                    position: leading,
                })
                .unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        assert_eq!(
            session
                .finalize_logical_line(StreamingLineFinalization {
                    input_id: 7,
                    segment_policy_id: 11,
                    ordinal: 0,
                    delimiter_range: Some(leading..pos(1)),
                    next_position: pos(2),
                })
                .unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        assert_eq!(session.continuation(), prior);
    });
}

#[gpui::test]
fn delimiterless_finalization_places_the_next_line_without_owning_its_start(
    cx: &mut TestAppContext,
) {
    with_text_system(cx, |text_system| {
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let finalized = session
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 0,
                delimiter_range: None,
                next_position: pos(0),
            })
            .unwrap();
        let StreamingLayoutFragment::Boundary(boundary) = &finalized.fragments[0] else {
            panic!("expected boundary fragment")
        };
        let next_line_start = point(px(0.), px(14.));
        assert_eq!(boundary.maps().len(), 1);
        assert_eq!(boundary.maps()[0].logical_position, pos(0));
        assert_eq!(boundary.maps()[0].position, next_line_start);
        assert_eq!(finalized.continuation.inline_offset, px(0.));
        assert_eq!(finalized.continuation.block_offset, px(14.));
        assert_eq!(boundary.position_for_logical_position(pos(0)), None);
        assert_eq!(
            boundary
                .closest_logical_position_for_position(next_line_start)
                .unwrap(),
            None
        );

        let following = session.admit_text(segment(1, pos(0), pos(1), "x")).unwrap();
        let StreamingLayoutFragment::Text(following) = &following.fragments[0] else {
            panic!("expected text fragment")
        };
        assert_eq!(following.maps()[0].position, next_line_start);
        assert_eq!(
            following.position_for_logical_position(pos(0)).unwrap(),
            Some(next_line_start)
        );
        assert_eq!(
            following
                .closest_logical_position_for_position(next_line_start)
                .unwrap(),
            Some(StreamingLayoutHit::Gap(pos(0)))
        );
    });
}

#[gpui::test]
fn object_after_delimiterless_finalization_owns_the_shared_gap(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let identity = StreamingObjectId(31);
        let object_order = StreamingObjectOrder(41);
        let leading = StreamingLayoutPosition::with_gap(
            0,
            StreamingObjectGap::before(identity, object_order),
        );
        let trailing =
            StreamingLayoutPosition::with_gap(0, StreamingObjectGap::after(identity, object_order));
        let mut session = text_system
            .streaming_layout_session(binding(leading, px(80.)))
            .unwrap();
        let finalized = session
            .finalize_logical_line(StreamingLineFinalization {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 0,
                delimiter_range: None,
                next_position: leading,
            })
            .unwrap();
        let StreamingLayoutFragment::Boundary(boundary) = &finalized.fragments[0] else {
            panic!("expected boundary fragment")
        };
        let next_line_start = point(px(0.), px(14.));
        assert_eq!(boundary.maps()[0].position, next_line_start);
        assert_eq!(
            boundary
                .closest_logical_position_for_position(next_line_start)
                .unwrap(),
            None
        );

        let admitted = session
            .admit_inline_object(StreamingInlineObject {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 1,
                id: identity,
                order: object_order,
                leading,
                trailing,
                presentation: SharedString::new_static("[]"),
                runs: vec![run(2)],
                width: px(10.),
                height: px(20.),
                baseline: px(12.),
                background: None,
            })
            .unwrap();
        let StreamingLayoutFragment::InlineObject(object) = &admitted.fragments[0] else {
            panic!("expected object fragment")
        };
        assert_eq!(object.maps()[0].position, next_line_start);
        assert_eq!(
            object
                .closest_logical_position_for_position(next_line_start)
                .unwrap(),
            Some(StreamingLayoutHit::Gap(leading))
        );
    });
}
