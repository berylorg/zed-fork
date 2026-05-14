use std::{path::PathBuf, sync::Arc};

use gpui::{DevicePixels, ImageRenderSource, ImageSource, size};

fn assert_send_sync_clone<T: Send + Sync + Clone>() {}

#[test]
fn file_source_identity_is_stable() {
    let path = PathBuf::from(r"C:\images\city.png");
    let source = ImageRenderSource::file(path.clone());
    let same_source = ImageRenderSource::file(path);
    let other_source = ImageRenderSource::file(PathBuf::from(r"C:\images\other.png"));

    assert_eq!(source.id(), same_source.id());
    assert_ne!(source.id(), other_source.id());
    assert!(source.is_reloadable());
    assert_eq!(source.retained_byte_len(), None);
}

#[test]
fn byte_source_identity_uses_arc_allocation() {
    let bytes: Arc<[u8]> = Arc::from(vec![1, 2, 3, 4]);
    let source = ImageRenderSource::bytes(bytes.clone());
    let same_source = ImageRenderSource::bytes(bytes.clone());
    let same_contents: Arc<[u8]> = Arc::from(vec![1, 2, 3, 4]);
    let other_source = ImageRenderSource::bytes(same_contents);

    assert_eq!(source.id(), same_source.id());
    assert_ne!(source.id(), other_source.id());
    assert!(!source.is_reloadable());
    assert_eq!(source.retained_byte_len(), Some(4));
}

#[test]
fn render_request_identity_includes_frame_scale_and_size() {
    let source = ImageRenderSource::file(PathBuf::from("city.png"));
    let requested_size = size(DevicePixels(320), DevicePixels(180));
    let request = source.render_request(0, 2.0, requested_size);
    let same_request = source.render_request(0, 2.0, requested_size);
    let other_frame = source.render_request(1, 2.0, requested_size);
    let other_scale = source.render_request(0, 1.0, requested_size);
    let other_size = source.render_request(0, 2.0, size(DevicePixels(640), DevicePixels(360)));

    assert_eq!(request.id(), same_request.id());
    assert_ne!(request.id(), other_frame.id());
    assert_ne!(request.id(), other_scale.id());
    assert_ne!(request.id(), other_size.id());
    assert_eq!(request.source_id(), source.id());
    assert_eq!(request.frame_index(), 0);
    assert_eq!(request.scale_factor(), 2.0);
    assert_eq!(request.requested_size(), requested_size);
}

#[test]
fn render_request_identity_buckets_subpixel_jitter_to_even_device_pixels() {
    let source = ImageRenderSource::file(PathBuf::from("city.png"));
    let odd_request = source.render_request(0, 1.5, size(DevicePixels(625), DevicePixels(416)));
    let even_request = source.render_request(0, 1.5, size(DevicePixels(626), DevicePixels(416)));
    let larger_request = source.render_request(0, 1.5, size(DevicePixels(628), DevicePixels(416)));

    assert_eq!(odd_request.id(), even_request.id());
    assert_eq!(
        odd_request.requested_size(),
        size(DevicePixels(626), DevicePixels(416))
    );
    assert_ne!(odd_request.id(), larger_request.id());
}

#[test]
fn image_source_constructors_preserve_source_backing() {
    let path = PathBuf::from("city.png");
    let file_source = ImageSource::file(path.clone());
    let file_render_source = file_source.render_source().unwrap();

    assert_eq!(file_render_source, ImageRenderSource::file(path));
    assert!(
        file_source
            .render_request(0, 1.0, size(DevicePixels(10), DevicePixels(10)))
            .is_some()
    );

    let bytes: Arc<[u8]> = Arc::from(vec![1, 2, 3, 4]);
    let byte_source = ImageSource::bytes(bytes.clone());
    let byte_render_source = byte_source.render_source().unwrap();

    assert_eq!(byte_render_source, ImageRenderSource::bytes(bytes));
}

#[test]
fn byte_source_is_thread_shareable() {
    assert_send_sync_clone::<ImageRenderSource>();
}
