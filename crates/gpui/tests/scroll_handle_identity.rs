use gpui::{ScrollHandle, point, px};

#[test]
fn scroll_handle_is_identical_to_itself() {
    let handle = ScrollHandle::new();

    assert!(handle.ptr_eq(&handle));
}

#[test]
fn cloned_scroll_handles_share_identity() {
    let handle = ScrollHandle::new();
    let clone = handle.clone();

    assert!(handle.ptr_eq(&clone));
    assert!(clone.ptr_eq(&handle));
}

#[test]
fn separately_created_scroll_handles_have_distinct_identity() {
    let first = ScrollHandle::new();
    let second = ScrollHandle::new();
    first.set_offset(point(px(-12.0), px(-34.0)));
    second.set_offset(point(px(-56.0), px(-78.0)));

    assert!(!first.ptr_eq(&second));
}

#[test]
fn equal_state_does_not_make_distinct_scroll_handles_identical() {
    let first = ScrollHandle::new();
    let second = ScrollHandle::new();
    let shared_offset = point(px(-12.0), px(-34.0));
    first.set_offset(shared_offset);
    second.set_offset(shared_offset);

    assert_eq!(first.offset(), second.offset());
    assert!(!first.ptr_eq(&second));
}
