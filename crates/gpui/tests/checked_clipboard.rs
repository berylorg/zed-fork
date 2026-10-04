#![allow(dead_code)]

pub use gpui::{ClipboardEntry, ClipboardItem, Image, ImageFormat};
use std::sync::Arc;
#[path = "../src/checked_clipboard.rs"]
mod checked_clipboard;
#[path = "support/clipboard_native_fake.rs"]
mod clipboard_native_fake;
#[path = "checked_clipboard/encoded_images.rs"]
mod encoded_images;
use checked_clipboard::{ClipboardError as Error, ClipboardLimits as Limits, Format};
use clipboard_native_fake::{Fake, limits, wide};

#[test]
fn clearing_failure_does_not_publish_or_acknowledge() {
    let mut native = Fake {
        fail_empty: true,
        ..Fake::text("original")
    };
    let item = ClipboardItem::new_string("replacement".into());
    assert_eq!(
        checked_clipboard::write(&mut native, &item, limits(64)),
        Err(Error::Ownership)
    );
    assert!(native.state.borrow().publications.is_empty());
    assert_eq!(native.state.borrow().closes, 1);
}

#[test]
fn text_exact_fit_and_one_over_admission() {
    let mut native = Fake::text("ab");
    let snapshot = checked_clipboard::read(&mut native, limits(8)).unwrap();
    assert_eq!(snapshot.item.text().as_deref(), Some("ab"));
    assert_eq!(snapshot.sequence, 17);
    assert_eq!(native.state.borrow().output_allocations, 1);
    let mut native = Fake::text("ab");
    assert_eq!(
        checked_clipboard::read(&mut native, limits(7)).unwrap_err(),
        Error::OverLimit
    );
    assert_eq!(native.state.borrow().output_allocations, 0);
    assert_eq!(native.state.borrow().closes, 1);
    let item = ClipboardItem::new_string("ab".into());
    let mut native = Fake::default();
    assert_eq!(
        checked_clipboard::write(&mut native, &item, limits(7)),
        Err(Error::OverLimit)
    );
    assert_eq!(native.state.borrow().allocations, 0);
    assert_eq!(native.state.borrow().opens, 0);
    checked_clipboard::write(&mut native, &item, limits(8)).unwrap();
    assert_eq!(native.state.borrow().publications, [Format::Text]);
}

#[test]
fn text_metadata_and_hash_are_one_complete_acknowledgement() {
    let item = ClipboardItem::new_string_with_metadata("ab".into(), "xy".into());
    let mut native = Fake::default();
    checked_clipboard::write(&mut native, &item, limits(24)).unwrap();
    let snapshot = checked_clipboard::read(&mut native, limits(24)).unwrap();
    assert_eq!(snapshot.item, item);
    for format in [Format::Text, Format::Metadata, Format::TextHash] {
        let mut native = Fake {
            fail_publication: Some(format),
            ..Fake::default()
        };
        assert_eq!(
            checked_clipboard::write(&mut native, &item, limits(24)),
            Err(Error::Write(format.representation()))
        );
        assert_eq!(native.state.borrow().empties, 1);
        assert_eq!(native.state.borrow().closes, 1);
    }
    let mut native = Fake::default();
    assert_eq!(
        checked_clipboard::write(&mut native, &item, limits(23)),
        Err(Error::OverLimit)
    );
    assert_eq!(native.state.borrow().allocations, 0);
}

#[test]
fn individual_and_total_limits_all_apply() {
    let item = ClipboardItem::new_string_with_metadata("ab".into(), "xy".into());
    for boundary in [
        Limits {
            text_bytes: 7,
            ..limits(24)
        },
        Limits {
            metadata_bytes: 7,
            ..limits(24)
        },
        Limits {
            total_bytes: 23,
            ..limits(24)
        },
    ] {
        let mut native = Fake::default();
        assert_eq!(
            checked_clipboard::write(&mut native, &item, boundary),
            Err(Error::OverLimit)
        );
        assert_eq!(native.state.borrow().allocations, 0);
        checked_clipboard::write(&mut native, &item, limits(24)).unwrap();
        assert_eq!(
            checked_clipboard::read(&mut native, boundary).unwrap_err(),
            Error::OverLimit
        );
        assert_eq!(native.state.borrow().output_allocations, 0);
    }
}

#[test]
fn malformed_native_text_has_no_output_allocation() {
    for bytes in [vec![], vec![1], vec![65, 0], vec![0, 0xd8, 0, 0]] {
        let mut native = Fake {
            values: vec![(Format::Text, bytes.into())],
            ..Fake::default()
        };
        assert_eq!(
            checked_clipboard::read(&mut native, limits(64)).unwrap_err(),
            Error::Malformed
        );
        assert_eq!(native.state.borrow().output_allocations, 0);
        assert_eq!(native.state.borrow().closes, 1);
    }
}

#[test]
fn supplementary_unicode_accounts_both_encodings() {
    let item = ClipboardItem::new_string("😀".into());
    let mut native = Fake::default();
    checked_clipboard::write(&mut native, &item, limits(10)).unwrap();
    assert_eq!(
        checked_clipboard::read(&mut native, limits(10))
            .unwrap()
            .item,
        item
    );
    assert_eq!(
        checked_clipboard::read(&mut native, limits(9)).unwrap_err(),
        Error::OverLimit
    );
}

#[test]
fn missing_malformed_or_mismatched_metadata_returns_no_partial_item() {
    let item = ClipboardItem::new_string_with_metadata("ab".into(), "xy".into());
    for corruption in 0..5 {
        let mut native = Fake::default();
        checked_clipboard::write(&mut native, &item, limits(64)).unwrap();
        match corruption {
            0 => native
                .values
                .retain(|(format, _)| *format != Format::Metadata),
            1 => native
                .values
                .retain(|(format, _)| *format != Format::TextHash),
            2 => {
                native
                    .values
                    .iter_mut()
                    .find(|(format, _)| *format == Format::Metadata)
                    .unwrap()
                    .1 = Arc::from([65, 0])
            }
            3 => {
                native
                    .values
                    .iter_mut()
                    .find(|(format, _)| *format == Format::TextHash)
                    .unwrap()
                    .1 = Arc::from([0u8; 8])
            }
            _ => {
                native
                    .values
                    .iter_mut()
                    .find(|(format, _)| *format == Format::TextHash)
                    .unwrap()
                    .1 = Arc::from([0u8; 7])
            }
        }
        assert_eq!(
            checked_clipboard::read(&mut native, limits(64)).unwrap_err(),
            Error::Malformed
        );
        assert_eq!(native.state.borrow().locks, 0);
    }
}

#[test]
fn ownership_allocation_read_and_close_failures_stay_typed() {
    let item = ClipboardItem::new_string("ab".into());
    for error in [Error::Ownership, Error::Allocation, Error::Close] {
        let mut native = Fake {
            error: Some(error),
            ..Fake::text("ab")
        };
        assert_eq!(
            checked_clipboard::read(&mut native, limits(64)).unwrap_err(),
            error
        );
        assert_eq!(native.state.borrow().locks, 0);
        let mut native = Fake {
            error: Some(error),
            ..Fake::default()
        };
        assert_eq!(
            checked_clipboard::write(&mut native, &item, limits(64)),
            Err(error)
        );
        if error == Error::Close {
            assert_eq!(native.state.borrow().closes, 2);
        }
    }
    let mut native = Fake {
        error: Some(Error::Read),
        ..Fake::text("ab")
    };
    assert_eq!(
        checked_clipboard::read(&mut native, limits(64)).unwrap_err(),
        Error::Read
    );
    assert_eq!(native.state.borrow().closes, 1);
}

#[test]
fn late_allocation_failure_and_allocator_padding_precede_mutation() {
    let item = ClipboardItem::new_string_with_metadata("ab".into(), "xy".into());
    for index in 1..=3 {
        let mut native = Fake {
            fail_allocation_at: Some(index),
            ..Fake::default()
        };
        assert_eq!(
            checked_clipboard::write(&mut native, &item, limits(24)),
            Err(Error::Allocation)
        );
        assert_eq!(native.state.borrow().empties, 0);
        assert_eq!(native.state.borrow().opens, 0);
    }
    let mut native = Fake {
        allocation_padding: 2,
        ..Fake::default()
    };
    assert_eq!(
        checked_clipboard::write(
            &mut native,
            &ClipboardItem::new_string("ab".into()),
            limits(8)
        ),
        Err(Error::OverLimit)
    );
    assert_eq!(native.state.borrow().empties, 0);
}

#[test]
fn metadata_acquisition_failure_releases_the_text_lock() {
    let item = ClipboardItem::new_string_with_metadata("ab".into(), "xy".into());
    for format in [Format::Metadata, Format::TextHash] {
        let mut native = Fake::default();
        checked_clipboard::write(&mut native, &item, limits(64)).unwrap();
        native.fail_read = Some(format);
        assert_eq!(
            checked_clipboard::read(&mut native, limits(64)).unwrap_err(),
            Error::Read
        );
        assert_eq!(native.state.borrow().locks, 0);
        assert_eq!(native.state.borrow().output_allocations, 0);
    }
}

#[test]
fn captured_sequence_cannot_retarget_an_acquired_value() {
    let mut native = Fake::text("ab");
    let snapshot = checked_clipboard::read(&mut native, limits(64)).unwrap();
    native.values = vec![(Format::Text, wide("changed").into())];
    native.sequence.set(18);
    assert_eq!(snapshot.item.text().as_deref(), Some("ab"));
    assert_eq!(snapshot.sequence, 17);
    let mut native = Fake {
        change_sequence: true,
        ..Fake::text("ab")
    };
    assert_eq!(
        checked_clipboard::read(&mut native, limits(64)).unwrap_err(),
        Error::SnapshotChanged
    );
}

#[test]
fn encoded_images_publish_exact_payload_and_consistency_without_decoding() {
    // Deliberately undecodable payload proves the boundary never invokes an image decoder.
    for format in [
        ImageFormat::Png,
        ImageFormat::Gif,
        ImageFormat::Jpeg,
        ImageFormat::Svg,
    ] {
        let item = ClipboardItem::from(Image::from_bytes(format, vec![1, 2, 3]));
        let mut native = Fake::default();
        checked_clipboard::write(&mut native, &item, limits(30)).unwrap();
        assert_eq!(
            native.state.borrow().publications,
            [Format::Image(format), Format::ImageMetadata]
        );
        let snapshot = checked_clipboard::read(&mut native, limits(30)).unwrap();
        assert_eq!(snapshot.item, item);
        assert_eq!(
            checked_clipboard::read(&mut native, limits(29)).unwrap_err(),
            Error::OverLimit
        );
        let mut rejected = Fake::default();
        assert_eq!(
            checked_clipboard::write(&mut rejected, &item, limits(29)),
            Err(Error::OverLimit)
        );
        assert_eq!(rejected.state.borrow().allocations, 0);
    }
}

#[test]
fn empty_unsupported_malformed_and_invalid_limits_do_not_mutate() {
    for (item, error) in [
        (ClipboardItem::new_string(String::new()), Error::Empty),
        (ClipboardItem::new_string("a\0b".into()), Error::Malformed),
        (
            ClipboardItem::new_string_with_metadata("a".into(), "x\0y".into()),
            Error::Malformed,
        ),
        (ClipboardItem::from(Image::empty()), Error::Empty),
        (
            ClipboardItem::from(Image::from_bytes(ImageFormat::Webp, vec![1])),
            Error::Unsupported,
        ),
    ] {
        let mut native = Fake::default();
        assert_eq!(
            checked_clipboard::write(&mut native, &item, limits(64)),
            Err(error)
        );
        assert_eq!(native.state.borrow().empties, 0);
        assert_eq!(native.state.borrow().allocations, 0);
    }
    let mut native = Fake::default();
    assert_eq!(
        checked_clipboard::read(&mut native, limits(64)).unwrap_err(),
        Error::NoValue
    );
    native.error = Some(Error::Unsupported);
    assert_eq!(
        checked_clipboard::read(&mut native, limits(64)).unwrap_err(),
        Error::Unsupported
    );
    assert_eq!(
        checked_clipboard::read(&mut native, limits(0)).unwrap_err(),
        Error::InvalidLimits
    );
    assert_eq!(
        checked_clipboard::write(
            &mut native,
            &ClipboardItem::new_string("ab".into()),
            limits(0)
        ),
        Err(Error::InvalidLimits)
    );
}
