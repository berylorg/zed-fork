use gpui::ImagePreparationLimits;

#[cfg(feature = "test-support")]
use gpui::{
    DevicePixels, ImagePreparationFailureKind, ImagePreparationOutcome, ImageRenderRequestSlot,
    ImageRenderSource, TestAppContext, size, spawn_image_upload_preparation,
    spawn_image_upload_preparation_with_limits,
};
#[cfg(feature = "test-support")]
use image::{DynamicImage, RgbaImage};
#[cfg(feature = "test-support")]
use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering::SeqCst},
    },
};

#[test]
fn image_preparation_limits_are_configurable_without_test_support() {
    let limits = ImagePreparationLimits::default()
        .with_max_source_pixels(123)
        .with_max_upload_pixels(45);

    assert_eq!(limits.max_source_pixels(), 123);
    assert_eq!(limits.max_upload_pixels(), 45);
}

#[cfg(feature = "test-support")]
fn png_bytes(width: u32, height: u32) -> Arc<[u8]> {
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([x as u8, y as u8, 128, 255]);
    }

    let mut cursor = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut cursor, image::ImageFormat::Png)
        .unwrap();
    Arc::from(cursor.into_inner())
}

#[cfg(feature = "test-support")]
fn temp_png_path() -> PathBuf {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);

    std::env::temp_dir().join(format!(
        "gpui-image-prepare-{}-{}.png",
        std::process::id(),
        NEXT_ID.fetch_add(1, SeqCst)
    ))
}

#[cfg(feature = "test-support")]
#[gpui::test]
async fn file_preparation_runs_off_main_thread_and_caps_dimensions(cx: &mut TestAppContext) {
    let path = temp_png_path();
    fs::write(&path, png_bytes(120, 80).as_ref()).unwrap();

    let source = ImageRenderSource::file(path.clone());
    let request = source.render_request(0, 1.0, size(DevicePixels(30), DevicePixels(20)));
    let task = spawn_image_upload_preparation(
        &cx.executor(),
        cx.update(|cx| cx.svg_renderer()),
        source,
        request,
    );

    let upload = match task.await {
        ImagePreparationOutcome::Ready(upload) => upload,
        ImagePreparationOutcome::Failed(failure) => {
            panic!("preparation failed: {:?}", failure.kind())
        }
    };
    let _ = fs::remove_file(path);

    assert!(!upload.prepared_on_main_thread_for_test());
    assert_eq!(upload.request().id(), request.id());
    assert!(upload.size().width <= request.requested_size().width);
    assert!(upload.size().height <= request.requested_size().height);
    assert_eq!(
        upload.pixels().len(),
        upload.size().width.0 as usize * upload.size().height.0 as usize * 4
    );
}

#[cfg(feature = "test-support")]
#[gpui::test]
async fn requested_dimensions_cap_byte_backed_upload_dimensions(cx: &mut TestAppContext) {
    let source = ImageRenderSource::bytes(png_bytes(80, 40));
    let request = source.render_request(0, 1.0, size(DevicePixels(20), DevicePixels(10)));
    let task = spawn_image_upload_preparation(
        &cx.executor(),
        cx.update(|cx| cx.svg_renderer()),
        source,
        request,
    );

    let upload = match task.await {
        ImagePreparationOutcome::Ready(upload) => upload,
        ImagePreparationOutcome::Failed(failure) => {
            panic!("preparation failed: {:?}", failure.kind())
        }
    };

    assert_eq!(upload.size(), request.requested_size());
}

#[cfg(feature = "test-support")]
#[gpui::test]
async fn stale_preparation_results_do_not_become_live(cx: &mut TestAppContext) {
    let source = ImageRenderSource::bytes(png_bytes(80, 40));
    let first_request = source.render_request(0, 1.0, size(DevicePixels(40), DevicePixels(20)));
    let second_request = source.render_request(0, 1.0, size(DevicePixels(20), DevicePixels(10)));
    let mut slot = ImageRenderRequestSlot::default();
    slot.request(first_request);
    slot.request(second_request);

    let first_task = spawn_image_upload_preparation(
        &cx.executor(),
        cx.update(|cx| cx.svg_renderer()),
        source.clone(),
        first_request,
    );
    let first_outcome = first_task.await;

    assert!(slot.accept_ready(first_outcome).is_err());
    assert_eq!(slot.live_request(), None);

    let second_task = spawn_image_upload_preparation(
        &cx.executor(),
        cx.update(|cx| cx.svg_renderer()),
        source,
        second_request,
    );
    let second_outcome = second_task.await;

    let upload = match slot.accept_ready(second_outcome) {
        Ok(upload) => upload,
        Err(_) => panic!("fresh request was not accepted"),
    };
    assert_eq!(upload.request().id(), second_request.id());
    assert_eq!(slot.live_request(), Some(second_request.id()));
}

#[cfg(feature = "test-support")]
#[gpui::test]
async fn image_preparation_failures_are_stable(cx: &mut TestAppContext) {
    let missing_source = ImageRenderSource::file(temp_png_path());
    let missing_request =
        missing_source.render_request(0, 1.0, size(DevicePixels(20), DevicePixels(20)));
    let missing_task = spawn_image_upload_preparation(
        &cx.executor(),
        cx.update(|cx| cx.svg_renderer()),
        missing_source,
        missing_request,
    );

    let missing_outcome = missing_task.await;
    assert!(matches!(
        missing_outcome.failure_kind(),
        Some(ImagePreparationFailureKind::Unavailable(_))
    ));

    let invalid_source = ImageRenderSource::bytes(Arc::from(&b"not an image"[..]));
    let invalid_request =
        invalid_source.render_request(0, 1.0, size(DevicePixels(20), DevicePixels(20)));
    let first_invalid = spawn_image_upload_preparation(
        &cx.executor(),
        cx.update(|cx| cx.svg_renderer()),
        invalid_source.clone(),
        invalid_request,
    )
    .await;
    let second_invalid = spawn_image_upload_preparation(
        &cx.executor(),
        cx.update(|cx| cx.svg_renderer()),
        invalid_source,
        invalid_request,
    )
    .await;

    assert!(matches!(
        first_invalid.failure_kind(),
        Some(ImagePreparationFailureKind::Unsupported(_))
    ));
    assert!(matches!(
        second_invalid.failure_kind(),
        Some(ImagePreparationFailureKind::Unsupported(_))
    ));
}

#[cfg(feature = "test-support")]
#[gpui::test]
async fn too_large_sources_are_rejected_before_decode(cx: &mut TestAppContext) {
    let source = ImageRenderSource::bytes(png_bytes(16, 16));
    let request = source.render_request(0, 1.0, size(DevicePixels(16), DevicePixels(16)));
    let limits = ImagePreparationLimits::default().with_max_source_pixels(64);
    let task = spawn_image_upload_preparation_with_limits(
        &cx.executor(),
        cx.update(|cx| cx.svg_renderer()),
        source,
        request,
        limits,
    );

    let outcome = task.await;
    assert!(matches!(
        outcome.failure_kind(),
        Some(ImagePreparationFailureKind::TooLarge { .. })
    ));
}
