use super::*;

fn single_object() -> (
    StreamingLayoutPosition,
    StreamingLayoutPosition,
    StreamingInlineObject,
) {
    let id = StreamingObjectId(1);
    let order = StreamingObjectOrder(2);
    let leading = StreamingLayoutPosition::with_gap(0, StreamingObjectGap::before(id, order));
    let trailing = StreamingLayoutPosition::with_gap(0, StreamingObjectGap::after(id, order));
    (
        leading,
        trailing,
        StreamingInlineObject {
            input_id: 7,
            segment_policy_id: 11,
            ordinal: 0,
            id,
            order,
            leading,
            trailing,
            presentation: SharedString::new_static("[]"),
            runs: vec![run(2)],
            width: px(10.),
            height: px(20.),
            baseline: px(12.),
            background: None,
        },
    )
}

#[gpui::test]
fn object_byte_and_semantic_caps_accept_exact_fit_and_reject_one_under_atomically(
    cx: &mut TestAppContext,
) {
    with_text_system(cx, |text_system| {
        let (leading, _, candidate) = single_object();
        let admitted = text_system
            .streaming_layout_session(binding(leading, px(80.)))
            .unwrap()
            .admit_inline_object(candidate.clone())
            .unwrap();
        let exact_bytes = admitted.charge.total().unwrap();
        let exact_items = admitted.item_charge.total().unwrap();
        assert!(admitted.charge.objects > 0);
        assert!(admitted.item_charge.positions >= 4);
        assert!(admitted.item_charge.gap_witnesses >= 4);
        assert!(admitted.item_charge.object_ids > 0);
        assert_eq!(
            admitted.item_charge.object_ids,
            admitted.item_charge.object_orders
        );

        let mut exact = binding(leading, px(80.));
        exact.limits.retained_bytes = exact_bytes;
        exact.limits.retained_items = exact_items;
        text_system
            .streaming_layout_session(exact)
            .unwrap()
            .admit_inline_object(candidate.clone())
            .unwrap();

        let mut byte_under = binding(leading, px(80.));
        byte_under.limits.retained_bytes = exact_bytes - 1;
        let mut rejected = text_system.streaming_layout_session(byte_under).unwrap();
        let prior = rejected.continuation();
        assert_eq!(
            rejected.admit_inline_object(candidate.clone()).unwrap_err(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
        );
        assert_eq!(rejected.continuation(), prior);

        let mut item_under = binding(leading, px(80.));
        item_under.limits.retained_items = exact_items - 1;
        let mut rejected = text_system.streaming_layout_session(item_under).unwrap();
        let prior = rejected.continuation();
        assert_eq!(
            rejected.admit_inline_object(candidate).unwrap_err(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
        );
        assert_eq!(rejected.continuation(), prior);
    });
}

#[gpui::test]
fn cancellation_and_drop_release_session_and_returned_payload(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let (leading, _, candidate) = single_object();
        let mut session = text_system
            .streaming_layout_session(binding(leading, px(80.)))
            .unwrap();
        let admitted = session.admit_inline_object(candidate.clone()).unwrap();
        let weak = std::sync::Arc::downgrade(&admitted.fragments);
        assert_eq!(session.retained_item_charge().continuations, 1);
        drop(admitted);
        assert!(weak.upgrade().is_none());
        session.cancel();
        assert_eq!(session.retained_charge(), StreamingLayoutCharge::default());
        assert_eq!(
            session.retained_item_charge(),
            StreamingLayoutItemCharge::default()
        );
        assert_eq!(
            session.admit_inline_object(candidate).unwrap_err(),
            StreamingLayoutError::Cancelled
        );
    });
}

#[gpui::test]
fn resume_rejects_mismatched_immutable_inputs(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let binding = binding(pos(0), px(80.));
        let continuation = text_system
            .streaming_layout_session(binding.clone())
            .unwrap()
            .continuation()
            .unwrap();
        let mut wrong_input = binding.clone();
        wrong_input.input_id += 1;
        assert_eq!(
            text_system
                .resume_streaming_layout_session(wrong_input, continuation)
                .err()
                .unwrap(),
            StreamingLayoutError::InputMismatch
        );
        let mut wrong_policy = binding;
        wrong_policy.segment_policy_id += 1;
        assert_eq!(
            text_system
                .resume_streaming_layout_session(wrong_policy, continuation)
                .err()
                .unwrap(),
            StreamingLayoutError::SegmentPolicyMismatch
        );
    });
}

#[gpui::test]
fn fresh_session_continuation_caps_accept_exact_and_reject_one_under(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let baseline = text_system
            .streaming_layout_session(binding(pos(0), px(80.)))
            .unwrap();
        let exact_bytes = baseline.retained_charge().total().unwrap();
        let exact_items = baseline.retained_item_charge().total().unwrap();

        let mut exact = binding(pos(0), px(80.));
        exact.limits.retained_bytes = exact_bytes;
        exact.limits.retained_items = exact_items;
        text_system.streaming_layout_session(exact).unwrap();

        let mut byte_under = binding(pos(0), px(80.));
        byte_under.limits.retained_bytes = exact_bytes - 1;
        assert_eq!(
            text_system
                .streaming_layout_session(byte_under)
                .err()
                .unwrap(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
        );

        let mut item_under = binding(pos(0), px(80.));
        item_under.limits.retained_items = exact_items - 1;
        assert_eq!(
            text_system
                .streaming_layout_session(item_under)
                .err()
                .unwrap(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
        );
    });
}

#[gpui::test]
fn resumed_session_continuation_caps_accept_exact_and_reject_one_under(cx: &mut TestAppContext) {
    with_text_system(cx, |text_system| {
        let (leading, _, candidate) = single_object();
        let binding = binding(leading, px(80.));
        let mut source = text_system
            .streaming_layout_session(binding.clone())
            .unwrap();
        source.admit_inline_object(candidate).unwrap();
        let continuation = source.continuation().unwrap();
        let exact_bytes = source.retained_charge().total().unwrap();
        let exact_items = source.retained_item_charge().total().unwrap();

        let mut exact = binding.clone();
        exact.limits.retained_bytes = exact_bytes;
        exact.limits.retained_items = exact_items;
        text_system
            .resume_streaming_layout_session(exact, continuation)
            .unwrap();

        let mut byte_under = binding.clone();
        byte_under.limits.retained_bytes = exact_bytes - 1;
        assert_eq!(
            text_system
                .resume_streaming_layout_session(byte_under, continuation)
                .err()
                .unwrap(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
        );

        let mut item_under = binding;
        item_under.limits.retained_items = exact_items - 1;
        assert_eq!(
            text_system
                .resume_streaming_layout_session(item_under, continuation)
                .err()
                .unwrap(),
            StreamingLayoutError::CapacityExceeded(StreamingLayoutComponent::Total)
        );
    });
}

#[test]
fn aggregate_charge_arithmetic_is_checked() {
    assert_eq!(
        StreamingLayoutCharge {
            segment_text: usize::MAX,
            runs: 1,
            ..Default::default()
        }
        .total(),
        Err(StreamingLayoutError::Overflow(
            StreamingLayoutComponent::Total
        ))
    );
}
