use super::*;

fn object_positions(byte: u64) -> [StreamingLayoutPosition; 3] {
    let a = StreamingObjectId(1);
    let b = StreamingObjectId(2);
    let a_order = StreamingObjectOrder(10);
    let b_order = StreamingObjectOrder(20);
    [
        StreamingLayoutPosition::with_gap(byte, StreamingObjectGap::before(a, a_order)),
        StreamingLayoutPosition::with_gap(
            byte,
            StreamingObjectGap::between(a, a_order, b, b_order),
        ),
        StreamingLayoutPosition::with_gap(byte, StreamingObjectGap::after(b, b_order)),
    ]
}

fn first_object(positions: [StreamingLayoutPosition; 3]) -> StreamingInlineObject {
    StreamingInlineObject {
        input_id: 7,
        segment_policy_id: 11,
        ordinal: 0,
        id: StreamingObjectId(1),
        order: StreamingObjectOrder(10),
        leading: positions[0],
        trailing: positions[1],
        presentation: SharedString::new_static("[]"),
        runs: vec![run(2)],
        width: px(10.),
        height: px(20.),
        baseline: px(12.),
        background: None,
    }
}

#[gpui::test]
fn text_cannot_start_between_remaining_same_anchor_objects(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let positions = object_positions(0);
        let mut session = text_system
            .streaming_layout_session(binding(positions[0], px(80.)))
            .unwrap();
        session
            .admit_inline_object(first_object(positions))
            .unwrap();
        let prior = session.continuation();

        assert_eq!(
            session
                .admit_text(segment(1, positions[1], pos(1), "x"))
                .unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        assert_eq!(session.continuation(), prior);
    });
}

#[gpui::test]
fn text_and_oversize_cannot_end_between_objects_at_their_end_anchor(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let end_positions = object_positions(1);
        let mut text_session = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let prior = text_session.continuation();
        assert_eq!(
            text_session
                .admit_text(segment(0, pos(0), end_positions[1], "x"))
                .unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        assert_eq!(text_session.continuation(), prior);

        let mut oversize_session = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let prior = oversize_session.continuation();
        let mut candidate = atom(0, 0, 1);
        candidate.logical_range = pos(0)..end_positions[2];
        assert_eq!(
            oversize_session.admit_oversize_atom(candidate).unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        assert_eq!(oversize_session.continuation(), prior);
    });
}

#[gpui::test]
fn oversize_and_delimiter_cannot_start_before_remaining_same_anchor_objects(
    cx: &mut TestAppContext,
) {
    with_text_system(cx, |text_system| {
        let positions = object_positions(0);
        let mut oversize_session = text_system
            .streaming_layout_session(binding(positions[0], px(80.)))
            .unwrap();
        oversize_session
            .admit_inline_object(first_object(positions))
            .unwrap();
        let prior = oversize_session.continuation();
        let mut candidate = atom(1, 0, 1);
        candidate.logical_range = positions[1]..pos(1);
        assert_eq!(
            oversize_session.admit_oversize_atom(candidate).unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        assert_eq!(oversize_session.continuation(), prior);

        let mut delimiter_session = text_system
            .streaming_layout_session(binding(positions[0], px(80.)))
            .unwrap();
        delimiter_session
            .admit_inline_object(first_object(positions))
            .unwrap();
        let prior = delimiter_session.continuation();
        assert_eq!(
            delimiter_session
                .finalize_logical_line(StreamingLineFinalization {
                    input_id: 7,
                    segment_policy_id: 11,
                    ordinal: 1,
                    delimiter_range: Some(positions[1]..pos(1)),
                    next_position: pos(1),
                })
                .unwrap_err(),
            StreamingLayoutError::InvalidPosition
        );
        assert_eq!(delimiter_session.continuation(), prior);
    });
}

#[gpui::test]
fn delimiter_cannot_end_after_or_between_objects_at_its_end_anchor(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let end_positions = object_positions(1);
        for invalid_end in [end_positions[1], end_positions[2]] {
            let mut session = text_system
                .streaming_layout_session(binding(pos(0), px(80.)))
                .unwrap();
            let prior = session.continuation();
            assert_eq!(
                session
                    .finalize_logical_line(StreamingLineFinalization {
                        input_id: 7,
                        segment_policy_id: 11,
                        ordinal: 0,
                        delimiter_range: Some(pos(0)..invalid_end),
                        next_position: invalid_end,
                    })
                    .unwrap_err(),
                StreamingLayoutError::InvalidPosition
            );
            assert_eq!(session.continuation(), prior);
        }
    });
}
