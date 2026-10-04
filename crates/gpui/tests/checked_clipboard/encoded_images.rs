use crate::checked_clipboard::{
    self, ClipboardError as Error, ClipboardLimits as Limits, ClipboardRepresentation, Format,
};
use crate::clipboard_native_fake::{Fake, limits};
use crate::{ClipboardEntry, ClipboardItem, Image, ImageFormat};

fn item(format: ImageFormat, length: usize) -> ClipboardItem {
    ClipboardItem::from(Image::from_bytes(
        format,
        (1..=length).map(|value| value as u8).collect(),
    ))
}

fn padded_limits(length: usize, padding: usize) -> Limits {
    Limits {
        total_bytes: 2 * length + 24 + 2 * padding,
        text_bytes: 1,
        metadata_bytes: 24 + padding,
        image_bytes: 2 * length + padding,
    }
}

fn replace(native: &mut Fake, format: Format, bytes: Vec<u8>) {
    native
        .values
        .iter_mut()
        .find(|(candidate, _)| *candidate == format)
        .unwrap()
        .1 = bytes.into();
}

#[test]
fn native_image_and_companion_padding_preserve_the_exact_encoded_prefix() {
    for format in [
        ImageFormat::Png,
        ImageFormat::Gif,
        ImageFormat::Jpeg,
        ImageFormat::Svg,
    ] {
        for length in [1, 3, 5, 9] {
            let item = item(format, length);
            let mut native = Fake {
                allocation_padding: 3,
                ..Fake::default()
            };
            let ceiling = padded_limits(length, 3);
            checked_clipboard::write(&mut native, &item, ceiling).unwrap();
            let companion = native
                .values
                .iter()
                .find(|(format, _)| *format == Format::ImageMetadata)
                .unwrap()
                .1
                .as_ref();
            assert_eq!(companion.len(), 27);
            assert_eq!(u32::from_le_bytes(companion[0..4].try_into().unwrap()), 1);
            assert_eq!(
                u64::from_le_bytes(companion[8..16].try_into().unwrap()),
                length as u64
            );
            let snapshot = checked_clipboard::read(&mut native, ceiling).unwrap();
            assert_eq!(snapshot.item, item);
            assert_eq!(
                native.state.borrow().publications,
                [Format::Image(format), Format::ImageMetadata]
            );
            assert_eq!(native.state.borrow().output_allocations, 1);
            assert_eq!(native.state.borrow().locks, 0);
        }
    }
}

#[test]
fn all_actual_image_companion_and_total_ceilings_precede_output_or_mutation() {
    let item = item(ImageFormat::Png, 3);
    let exact = padded_limits(3, 3);
    for ceiling in [
        Limits {
            image_bytes: exact.image_bytes - 1,
            ..exact
        },
        Limits {
            metadata_bytes: exact.metadata_bytes - 1,
            ..exact
        },
        Limits {
            total_bytes: exact.total_bytes - 1,
            ..exact
        },
    ] {
        let mut native = Fake {
            allocation_padding: 3,
            ..Fake::default()
        };
        assert_eq!(
            checked_clipboard::write(&mut native, &item, ceiling),
            Err(Error::OverLimit)
        );
        assert_eq!(native.state.borrow().opens, 0);
        assert_eq!(native.state.borrow().empties, 0);
        assert!(native.state.borrow().publications.is_empty());
        checked_clipboard::write(&mut native, &item, exact).unwrap();
        assert_eq!(
            checked_clipboard::read(&mut native, ceiling).unwrap_err(),
            Error::OverLimit
        );
        assert_eq!(native.state.borrow().output_allocations, 0);
        assert_eq!(native.state.borrow().locks, 0);
    }
    for ceiling in [
        Limits {
            image_bytes: 5,
            ..limits(30)
        },
        Limits {
            metadata_bytes: 23,
            ..limits(30)
        },
        limits(29),
    ] {
        let mut native = Fake::default();
        assert_eq!(
            checked_clipboard::write(&mut native, &item, ceiling),
            Err(Error::OverLimit)
        );
        assert_eq!(native.state.borrow().allocations, 0);
    }
}

#[test]
fn malformed_image_companions_refuse_without_fallback_or_output_allocation() {
    for corruption in 0..11 {
        let mut native = Fake {
            allocation_padding: 3,
            ..Fake::default()
        };
        checked_clipboard::write(&mut native, &item(ImageFormat::Png, 3), limits(128)).unwrap();
        let mut companion = native
            .values
            .iter()
            .find(|(format, _)| *format == Format::ImageMetadata)
            .unwrap()
            .1
            .to_vec();
        match corruption {
            0 => companion.clear(),
            1 => companion.truncate(1),
            2 => companion.truncate(23),
            3 => companion[0..4].copy_from_slice(&0u32.to_le_bytes()),
            4 => companion[0..4].copy_from_slice(&2u32.to_le_bytes()),
            5 => companion[4..8].copy_from_slice(&99u32.to_le_bytes()),
            6 => companion[4..8].copy_from_slice(&2u32.to_le_bytes()),
            7 => companion[8..16].copy_from_slice(&0u64.to_le_bytes()),
            8 => companion[8..16].copy_from_slice(&7u64.to_le_bytes()),
            9 => companion[8..16].copy_from_slice(&u64::MAX.to_le_bytes()),
            _ => companion[16] ^= 1,
        }
        replace(&mut native, Format::ImageMetadata, companion);
        assert_eq!(
            checked_clipboard::read(&mut native, limits(128)).unwrap_err(),
            Error::Malformed
        );
        assert_eq!(native.state.borrow().output_allocations, 0);
        assert_eq!(native.state.borrow().locks, 0);
    }
}

#[test]
fn image_digest_rejects_content_changes_but_excludes_native_padding() {
    let item = item(ImageFormat::Png, 3);
    let mut native = Fake {
        allocation_padding: 3,
        ..Fake::default()
    };
    checked_clipboard::write(&mut native, &item, limits(128)).unwrap();
    let mut image = native
        .values
        .iter()
        .find(|(format, _)| *format == Format::Image(ImageFormat::Png))
        .unwrap()
        .1
        .to_vec();
    image[3..].fill(255);
    replace(&mut native, Format::Image(ImageFormat::Png), image.clone());
    let mut companion = native
        .values
        .iter()
        .find(|(format, _)| *format == Format::ImageMetadata)
        .unwrap()
        .1
        .to_vec();
    companion[24..].fill(255);
    replace(&mut native, Format::ImageMetadata, companion);
    assert_eq!(
        checked_clipboard::read(&mut native, limits(128))
            .unwrap()
            .item,
        item
    );
    native.state.borrow_mut().output_allocations = 0;
    image[0] ^= 1;
    replace(&mut native, Format::Image(ImageFormat::Png), image);
    assert_eq!(
        checked_clipboard::read(&mut native, limits(128)).unwrap_err(),
        Error::Malformed
    );
    assert_eq!(native.state.borrow().output_allocations, 0);
}

#[test]
fn foreign_images_without_companion_copy_the_entire_bounded_native_representation() {
    let mut native = Fake {
        allocation_padding: 3,
        ..Fake::default()
    };
    checked_clipboard::write(&mut native, &item(ImageFormat::Png, 3), limits(128)).unwrap();
    native
        .values
        .retain(|(format, _)| *format != Format::ImageMetadata);
    let snapshot = checked_clipboard::read(&mut native, limits(12)).unwrap();
    let [ClipboardEntry::Image(image)] = snapshot.item.entries() else {
        panic!("expected image");
    };
    assert_eq!(image.bytes(), [1, 2, 3, 0, 0, 0]);
    native.state.borrow_mut().output_allocations = 0;
    assert_eq!(
        checked_clipboard::read(&mut native, limits(11)).unwrap_err(),
        Error::OverLimit
    );
    assert_eq!(native.state.borrow().output_allocations, 0);
}

#[test]
fn both_image_publications_are_required_for_success() {
    let item = item(ImageFormat::Png, 3);
    for (format, representation, publications) in [
        (
            Format::Image(ImageFormat::Png),
            ClipboardRepresentation::Image,
            1,
        ),
        (
            Format::ImageMetadata,
            ClipboardRepresentation::ImageMetadata,
            2,
        ),
    ] {
        let mut native = Fake {
            fail_publication: Some(format),
            ..Fake::default()
        };
        assert_eq!(
            checked_clipboard::write(&mut native, &item, limits(30)),
            Err(Error::Write(representation))
        );
        assert_eq!(native.state.borrow().publications.len(), publications);
        assert_eq!(native.state.borrow().closes, 1);
        if format == Format::ImageMetadata {
            assert_eq!(native.values.len(), 1);
        }
    }
    let mut native = Fake {
        error: Some(Error::Close),
        ..Fake::default()
    };
    assert_eq!(
        checked_clipboard::write(&mut native, &item, limits(30)),
        Err(Error::Close)
    );
    assert_eq!(native.values.len(), 2);
}

#[test]
fn companion_allocation_and_read_failure_release_resources_without_success() {
    let item = item(ImageFormat::Png, 3);
    let mut native = Fake {
        fail_allocation_at: Some(2),
        ..Fake::default()
    };
    assert_eq!(
        checked_clipboard::write(&mut native, &item, limits(30)),
        Err(Error::Allocation)
    );
    assert_eq!(native.state.borrow().opens, 0);
    assert_eq!(native.state.borrow().empties, 0);
    let mut native = Fake::default();
    checked_clipboard::write(&mut native, &item, limits(30)).unwrap();
    native.fail_read = Some(Format::ImageMetadata);
    assert_eq!(
        checked_clipboard::read(&mut native, limits(30)).unwrap_err(),
        Error::Read
    );
    assert_eq!(native.state.borrow().locks, 0);
    assert_eq!(native.state.borrow().output_allocations, 0);
}
