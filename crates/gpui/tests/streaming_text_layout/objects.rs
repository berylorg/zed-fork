use super::*;

fn id(value: u128) -> StreamingObjectId {
    StreamingObjectId(value)
}

fn order(value: u128) -> StreamingObjectOrder {
    StreamingObjectOrder(value)
}

fn at(byte: u64, gap: StreamingObjectGap) -> StreamingLayoutPosition {
    StreamingLayoutPosition::with_gap(byte, gap)
}

fn object(
    ordinal: u64,
    identity: StreamingObjectId,
    object_order: StreamingObjectOrder,
    leading: StreamingLayoutPosition,
    trailing: StreamingLayoutPosition,
    width: f32,
) -> StreamingInlineObject {
    StreamingInlineObject {
        input_id: 7,
        segment_policy_id: 11,
        ordinal,
        id: identity,
        order: object_order,
        leading,
        trailing,
        presentation: SharedString::new_static("[]"),
        runs: vec![run(2)],
        width: px(width),
        height: px(20.),
        baseline: px(12.),
        background: Some(gpui::black()),
    }
}

fn two_object_positions(byte: u64) -> [StreamingLayoutPosition; 3] {
    [
        at(byte, StreamingObjectGap::before(id(1), order(10))),
        at(
            byte,
            StreamingObjectGap::between(id(1), order(10), id(2), order(20)),
        ),
        at(byte, StreamingObjectGap::after(id(2), order(20))),
    ]
}

#[gpui::test]
fn multiple_same_anchor_objects_preserve_every_gap_and_constant_time_hits(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let positions = two_object_positions(0);
        let mut session = text_system
            .streaming_layout_session(binding(positions[0], px(80.)))
            .unwrap();
        let first = session
            .admit_inline_object(object(0, id(1), order(10), positions[0], positions[1], 12.))
            .unwrap();
        let second = session
            .admit_inline_object(object(1, id(2), order(20), positions[1], positions[2], 14.))
            .unwrap();
        assert_eq!(session.continuation().unwrap().next_position, positions[2]);

        let StreamingLayoutFragment::InlineObject(first) = &first.fragments[0] else {
            panic!("expected object")
        };
        let StreamingLayoutFragment::InlineObject(second) = &second.fragments[0] else {
            panic!("expected object")
        };
        assert_eq!(first.maps()[0].logical_position, positions[0]);
        assert_eq!(first.maps()[1].logical_position, positions[1]);
        assert_eq!(second.maps()[0].logical_position, positions[1]);
        assert_eq!(second.maps()[1].logical_position, positions[2]);
        assert_eq!(first.maps()[1].position, second.maps()[0].position);
        assert_eq!(first.position_for_logical_position(positions[1]), None);
        assert_eq!(
            second.position_for_logical_position(positions[1]),
            Some(second.maps()[0].position)
        );
        assert_eq!(
            first
                .closest_logical_position_for_position(first.maps()[1].position)
                .unwrap(),
            None
        );
        assert_eq!(
            second
                .closest_logical_position_for_position(second.maps()[0].position)
                .unwrap(),
            Some(StreamingLayoutHit::Gap(positions[1]))
        );
        assert_eq!(
            second
                .closest_logical_position_for_position(point(px(13.), px(1.)))
                .unwrap(),
            Some(StreamingLayoutHit::Object(id(2)))
        );
    });
}

#[gpui::test]
fn bounded_same_anchor_run_resumes_from_only_the_compact_continuation(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let positions = two_object_positions(7);
        let binding = binding(positions[0], px(80.));
        let first = text_system
            .streaming_layout_session(binding.clone())
            .unwrap()
            .admit_inline_object(object(0, id(1), order(10), positions[0], positions[1], 10.))
            .unwrap();
        let mut resumed = text_system
            .resume_streaming_layout_session(binding, first.continuation)
            .unwrap();
        resumed
            .admit_inline_object(object(1, id(2), order(20), positions[1], positions[2], 10.))
            .unwrap();
        assert_eq!(resumed.continuation().unwrap().next_position, positions[2]);
        assert_eq!(resumed.retained_item_charge().continuations, 1);
    });
}

#[gpui::test]
fn objects_wrap_in_order_and_oversize_width_is_preserved(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let positions = two_object_positions(0);
        let mut session = text_system
            .streaming_layout_session(binding(positions[0], px(20.)))
            .unwrap();
        session
            .admit_inline_object(object(0, id(1), order(10), positions[0], positions[1], 15.))
            .unwrap();
        let wrapped = session
            .admit_inline_object(object(1, id(2), order(20), positions[1], positions[2], 30.))
            .unwrap();
        let StreamingLayoutFragment::InlineObject(fragment) = &wrapped.fragments[0] else {
            panic!("expected object")
        };
        assert_eq!(fragment.bounds.origin.x, px(0.));
        assert_eq!(fragment.bounds.origin.y, px(20.));
        assert_eq!(fragment.bounds.size.width, px(30.));
        assert_eq!(wrapped.continuation.visual_lines, 1);
    });
}

#[gpui::test]
fn zero_width_object_keeps_a_resumed_line_logically_nonempty(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let identity = id(1);
        let object_order = order(10);
        let leading = at(0, StreamingObjectGap::before(identity, object_order));
        let trailing = at(0, StreamingObjectGap::after(identity, object_order));
        let binding = binding(leading, px(10.));
        let object = text_system
            .streaming_layout_session(binding.clone())
            .unwrap()
            .admit_inline_object(object(0, identity, object_order, leading, trailing, 0.))
            .unwrap();
        assert_eq!(object.continuation.inline_offset, px(0.));
        assert!(object.continuation.line_has_content);

        let erased_occupancy = StreamingLayoutContinuation {
            line_has_content: false,
            ..object.continuation
        };
        assert_eq!(
            text_system
                .resume_streaming_layout_session(binding.clone(), erased_occupancy)
                .err()
                .unwrap(),
            StreamingLayoutError::InvalidSegment
        );

        let mut resumed = text_system
            .resume_streaming_layout_session(binding, object.continuation)
            .unwrap();
        let overflow = resumed
            .admit_oversize_atom(StreamingOversizeAtom {
                ordinal: 1,
                logical_range: trailing..pos(1),
                width: px(20.),
                ..atom(1, 0, 1)
            })
            .unwrap();
        let StreamingLayoutFragment::OversizeAtom(fragment) = &overflow.fragments[0] else {
            panic!("expected atom")
        };
        assert_eq!(fragment.bounds.origin, point(px(0.), px(20.)));
        assert_eq!(overflow.continuation.visual_lines, 1);
        assert!(overflow.continuation.line_has_content);
    });
}

#[gpui::test]
fn duplicate_reordered_nonadjacent_and_mismatched_facts_reject_atomically(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let positions = two_object_positions(0);
        let mut session = text_system
            .streaming_layout_session(binding(positions[0], px(80.)))
            .unwrap();
        session
            .admit_inline_object(object(0, id(1), order(10), positions[0], positions[1], 10.))
            .unwrap();
        let prior = session.continuation();

        let duplicate = object(1, id(1), order(10), positions[1], positions[2], 10.);
        assert_eq!(
            session.admit_inline_object(duplicate).unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        let skipped = object(1, id(2), order(20), positions[0], positions[2], 10.);
        assert_eq!(
            session.admit_inline_object(skipped).unwrap_err(),
            StreamingLayoutError::OutOfOrder
        );
        let mismatched_trailing = at(0, StreamingObjectGap::after(id(9), order(20)));
        assert_eq!(
            session
                .admit_inline_object(object(
                    1,
                    id(2),
                    order(20),
                    positions[1],
                    mismatched_trailing,
                    10.,
                ))
                .unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        assert_eq!(session.continuation(), prior);
    });
}

#[gpui::test]
fn eof_object_follows_text_without_synthetic_source_bytes(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let leading = at(1, StreamingObjectGap::before(id(5), order(1)));
        let trailing = at(1, StreamingObjectGap::after(id(5), order(1)));
        let mut session = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        session
            .admit_text(segment(0, pos(0), leading, "x"))
            .unwrap();
        session
            .admit_inline_object(object(1, id(5), order(1), leading, trailing, 10.))
            .unwrap();
        let ended = session
            .end_source(StreamingEndOfSource {
                input_id: 7,
                segment_policy_id: 11,
                ordinal: 2,
                source_extent: 1,
                position: trailing,
            })
            .unwrap();
        assert!(ended.continuation.ended);
    });
}
