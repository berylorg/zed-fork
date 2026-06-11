#[cfg(feature = "test-support")]
use gpui::{
    AnyView, AnyWindowHandle, AppContext, Context, DevicePixels, Entity, ImageRenderSource,
    InteractiveElement, IntoElement, Pixels, Render, RenderImage, Size,
    SourceBackedImageRequestStatus, StyleRefinement, Styled, TestAppContext, VisualTestContext,
    Window, div, img, prelude::ParentElement, px, size,
};
#[cfg(feature = "test-support")]
use image::{DynamicImage, Frame, RgbaImage};
#[cfg(feature = "test-support")]
use std::{
    io::Cursor,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

#[test]
fn image_render_resources_test_binary_builds_without_test_support() {}

#[cfg(feature = "test-support")]
fn png_bytes(width: u32, height: u32) -> Arc<[u8]> {
    let mut image = RgbaImage::new(width, height);
    for (x, y, pixel) in image.enumerate_pixels_mut() {
        *pixel = image::Rgba([x as u8, y as u8, 192, 255]);
    }

    let mut bytes = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    Arc::from(bytes)
}

#[cfg(feature = "test-support")]
fn run_until_image_resource_count(cx: &mut VisualTestContext, expected: usize) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw_and_present_for_test(cx));
        let actual = cx.cx.update(|cx| {
            cx.renderer_diagnostic_snapshot().windows[0]
                .renderer
                .image_resources
                .resource_count
        });
        if actual == expected {
            return;
        }
    }

    let actual = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .resource_count
    });
    assert_eq!(actual, expected);
}

#[cfg(feature = "test-support")]
fn run_until_source_backed_failed_count(cx: &mut VisualTestContext, expected: usize) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw_and_present_for_test(cx));
        let actual = cx.cx.update(|cx| {
            cx.renderer_diagnostic_snapshot().windows[0]
                .source_backed_images
                .failed_count
        });
        if actual == expected {
            return;
        }
    }

    let actual = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .source_backed_images
            .failed_count
    });
    assert_eq!(actual, expected);
}

#[cfg(feature = "test-support")]
fn test_file_path(bytes: Arc<[u8]>) -> PathBuf {
    static NEXT_ID: AtomicUsize = AtomicUsize::new(0);
    let mut path = std::env::temp_dir();
    path.push(format!(
        "gpui-source-backed-image-{}-{}.png",
        std::process::id(),
        NEXT_ID.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::write(&path, bytes.as_ref()).unwrap();
    path
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_images_upload_without_decoded_asset_or_atlas_retention() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let (_view, cx) = test_cx.add_window_view(|_, _| SourceImagesView::single(source));
    run_until_image_resource_count(cx, 1);

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    assert_eq!(snapshot.decoded_image_assets.asset_count, 0);
    assert_eq!(snapshot.decoded_image_assets.decoded_bytes_estimate, 0);
    assert_eq!(snapshot.windows[0].renderer.atlas.image_tiles, 0);

    let image_resources = &snapshot.windows[0].renderer.image_resources;
    assert_eq!(image_resources.resource_count, 1);
    assert_eq!(image_resources.decoded_cpu_bytes_estimate, 0);
    assert_eq!(image_resources.gpu_bytes_estimate, 40 * 20 * 4);
    assert_eq!(image_resources.upload_bytes, 40 * 20 * 4);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_window_diagnostics_track_pending_decode_and_eviction() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let (view, cx) = test_cx.add_window_view(|_, _| SourceImagesView::new(Vec::new(), Vec::new()));

    set_sources(&view, cx, vec![source]);
    cx.update(|window, cx| window.draw(cx).clear());

    let pending = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    assert_eq!(pending.windows[0].source_backed_images.request_count, 1);
    assert_eq!(
        pending.windows[0].source_backed_images.pending_decode_count,
        1
    );
    assert_eq!(
        pending.windows[0]
            .source_backed_images
            .pending_upload_decoded_cpu_bytes_estimate,
        0
    );
    assert_eq!(
        pending.windows[0].renderer.image_resources.resource_count,
        0
    );

    cx.run_until_parked();
    cx.update(|window, cx| window.draw_and_present_for_test(cx));
    let live = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    assert_eq!(live.windows[0].source_backed_images.pending_upload_count, 0);
    assert_eq!(
        live.windows[0]
            .source_backed_images
            .pending_upload_decoded_cpu_bytes_estimate,
        0
    );
    assert_eq!(live.windows[0].source_backed_images.live_count, 1);
    assert_eq!(
        live.windows[0].source_backed_images.live_gpu_bytes_estimate,
        40 * 20 * 4
    );

    set_sources(&view, cx, Vec::new());
    run_until_image_resource_count(cx, 0);
    let released = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    assert_eq!(released.windows[0].source_backed_images.request_count, 0);
    assert_eq!(
        released.windows[0].source_backed_images.known_source_count,
        0
    );
    assert!(
        released.windows[0]
            .source_backed_images
            .evicted_resource_count
            >= 1
    );
    assert_eq!(
        released.windows[0].renderer.image_resources.resource_count,
        0
    );
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_failed_sources_remain_failed_until_released() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(Arc::from(&b"not an image"[..]));
    let (view, cx) = test_cx.add_window_view(|_, _| SourceImagesView::single(source));

    run_until_source_backed_failed_count(cx, 1);
    let failed = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .source_backed_images
            .clone()
    });
    assert_eq!(failed.failed_source_count, 1);
    assert_eq!(failed.pending_decode_count, 0);
    assert_eq!(failed.pending_upload_count, 0);
    assert_eq!(failed.live_count, 0);

    cx.update(|window, cx| window.draw_and_present_for_test(cx));
    let still_failed = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .source_backed_images
            .clone()
    });
    assert_eq!(still_failed.failed_count, 1);
    assert_eq!(still_failed.failed_source_count, 1);

    set_sources(&view, cx, Vec::new());
    cx.update(|window, cx| window.draw_and_present_for_test(cx));
    let released = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .source_backed_images
            .clone()
    });
    assert_eq!(released.request_count, 0);
    assert_eq!(released.failed_count, 0);
    assert_eq!(released.failed_source_count, 0);
    assert_eq!(released.known_source_count, 0);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn atlas_images_and_standalone_images_report_separate_diagnostics() {
    let mut test_cx = TestAppContext::single();
    let render_image = Arc::new(RenderImage::new(vec![Frame::new(
        RgbaImage::from_raw(2, 3, vec![127; 2 * 3 * 4]).unwrap(),
    )]));
    let source = ImageRenderSource::bytes(png_bytes(20, 10));
    let (_view, cx) = test_cx.add_window_view(|_, _| AtlasAndSourceView {
        render_image: render_image.clone(),
        source,
    });

    run_until_image_resource_count(cx, 1);

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    assert_eq!(snapshot.windows[0].renderer.atlas.image_tiles, 1);
    assert_eq!(
        snapshot.windows[0].renderer.image_resources.resource_count,
        1
    );
    assert_eq!(
        snapshot.windows[0]
            .renderer
            .image_resources
            .gpu_bytes_estimate,
        20 * 10 * 4
    );
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn multiple_source_backed_images_use_separate_standalone_resources() {
    let mut test_cx = TestAppContext::single();
    let sources = vec![
        ImageRenderSource::bytes(png_bytes(20, 10)),
        ImageRenderSource::bytes(png_bytes(24, 12)),
        ImageRenderSource::bytes(png_bytes(30, 20)),
    ];
    let (_view, cx) = test_cx.add_window_view(|_, _| {
        SourceImagesView::new(sources.clone(), vec![(px(30.), px(20.)); 3])
    });
    run_until_image_resource_count(cx, 3);

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    assert_eq!(snapshot.windows[0].renderer.atlas.image_tiles, 0);
    assert_eq!(
        snapshot.windows[0].renderer.image_resources.resource_count,
        3
    );
    assert_eq!(
        snapshot.windows[0]
            .renderer
            .image_resources
            .gpu_bytes_estimate,
        (20 * 10 + 24 * 12 + 30 * 20) * 4
    );
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_resource_survives_cached_view_paint_replay() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let (_view, cx) = test_cx.add_window_view(|_, cx| CachedImageRoot {
        cached_child: cx
            .new(|_| CachedImageChild {
                source: source.clone(),
            })
            .into(),
        extra_source: source,
        show_extra: false,
    });

    run_until_image_resource_count(cx, 1);
    let before = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    let before_images = &before.windows[0].renderer.image_resources;
    let before_source_backed = &before.windows[0].source_backed_images;
    assert_eq!(before_images.resource_count, 1);
    assert_eq!(before_source_backed.live_count, 1);

    cx.update(|window, cx| window.draw_and_present_for_test(cx));
    let after = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    let after_images = &after.windows[0].renderer.image_resources;
    let after_source_backed = &after.windows[0].source_backed_images;
    assert_eq!(after_images.resource_count, 1);
    assert_eq!(after_images.upload_count, before_images.upload_count);
    assert_eq!(
        after_source_backed.evicted_resource_count,
        before_source_backed.evicted_resource_count
    );
    assert_eq!(after_source_backed.live_count, 1);
    assert_eq!(after_source_backed.painted_resource_count, 1);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn clipped_source_backed_image_does_not_request_or_upload_resource() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let (_view, cx) = test_cx.add_window_view(|_, _| ClippedSourceImageView { source });

    for _ in 0..3 {
        cx.run_until_parked();
        cx.update(|window, cx| window.draw_and_present_for_test(cx));
    }

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    let source_backed = &snapshot.windows[0].source_backed_images;
    let image_resources = &snapshot.windows[0].renderer.image_resources;
    assert_eq!(source_backed.request_count, 0);
    assert_eq!(source_backed.requested_this_frame_count, 0);
    assert_eq!(source_backed.live_count, 0);
    assert_eq!(image_resources.resource_count, 0);
    assert_eq!(image_resources.upload_count, 0);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_preload_uploads_without_scene_sprite() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let (_view, cx) =
        test_cx.add_window_view(move |_, _| PreloadImagesView::preload_only(source.clone()));

    run_until_image_resource_count(cx, 1);

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    let source_backed = &snapshot.windows[0].source_backed_images;
    assert_eq!(source_backed.final_scene_resource_count, 0);
    assert_eq!(source_backed.painted_resource_count, 0);
    assert_eq!(source_backed.preload_live_count, 1);
    assert_eq!(source_backed.pending_upload_decoded_cpu_bytes_estimate, 0);
    assert_eq!(
        snapshot.windows[0].renderer.image_resources.resource_count,
        1
    );
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_preload_status_reports_live_request() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let status_source = source.clone();
    let (_view, cx) =
        test_cx.add_window_view(move |_, _| PreloadImagesView::preload_only(source.clone()));

    run_until_image_resource_count(cx, 1);

    let status = cx.update(|window, _| {
        let scale_factor = window.scale_factor();
        let requested_size = size(
            DevicePixels((f32::from(px(40.)) * scale_factor).ceil() as i32),
            DevicePixels((f32::from(px(20.)) * scale_factor).ceil() as i32),
        );
        let request = status_source.render_request(0, scale_factor, requested_size);
        window.source_backed_image_request_status(request)
    });
    assert_eq!(status, SourceBackedImageRequestStatus::Live);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn visible_source_backed_image_reuses_matching_preload() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let preload_source = source.clone();
    let (view, cx) = test_cx
        .add_window_view(move |_, _| PreloadImagesView::preload_only(preload_source.clone()));

    run_until_image_resource_count(cx, 1);
    let upload_count = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .upload_count
    });
    assert_eq!(upload_count, 1);

    view.update(cx, |view, cx| {
        view.preloads.clear();
        view.visible = vec![source];
        cx.notify();
    });
    cx.update(|window, cx| window.draw_and_present_for_test(cx));

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    assert_eq!(
        snapshot.windows[0].renderer.image_resources.upload_count,
        upload_count
    );
    assert_eq!(snapshot.windows[0].source_backed_images.live_count, 1);
    assert_eq!(
        snapshot.windows[0]
            .source_backed_images
            .final_scene_resource_count,
        1
    );
    assert_eq!(
        snapshot.windows[0].source_backed_images.preload_live_count,
        0
    );
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn visible_source_backed_image_reuses_near_matching_preload() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(80, 40));
    let preload_source = source.clone();
    let (view, cx) = test_cx.add_window_view(move |_, _| {
        PreloadImagesView::new(
            vec![(preload_source.clone(), size(px(42.), px(21.)))],
            Vec::new(),
        )
    });

    run_until_image_resource_count(cx, 1);
    let upload_count = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .upload_count
    });
    assert_eq!(upload_count, 1);

    view.update(cx, |view, cx| {
        view.preloads.clear();
        view.visible = vec![source];
        cx.notify();
    });
    cx.update(|window, cx| window.draw_and_present_for_test(cx));

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    assert_eq!(
        snapshot.windows[0].renderer.image_resources.upload_count,
        upload_count
    );
    assert_eq!(
        snapshot.windows[0].renderer.image_resources.resource_count,
        1
    );
    assert_eq!(snapshot.windows[0].source_backed_images.live_count, 1);
    assert_eq!(
        snapshot.windows[0]
            .source_backed_images
            .final_scene_resource_count,
        1
    );
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_preload_budget_defers_preloads_before_visible_resources() {
    let mut test_cx = TestAppContext::single();
    let visible_source = ImageRenderSource::bytes(png_bytes(40, 20));
    let preload_a = ImageRenderSource::bytes(png_bytes(40, 20));
    let preload_b = ImageRenderSource::bytes(png_bytes(40, 20));
    let visible_source_id = visible_source.id().as_u64();
    let (_view, cx) = test_cx.add_window_view(move |window, _| {
        window.set_source_backed_image_resource_budget_for_test(16, u64::MAX);
        window.set_source_backed_image_preload_budget_for_test(1, u64::MAX);
        PreloadImagesView::new(
            vec![
                (preload_a, size(px(40.), px(20.))),
                (preload_b, size(px(40.), px(20.))),
            ],
            vec![visible_source],
        )
    });

    cx.run_until_parked();
    cx.update(|window, cx| window.draw_and_present_for_test(cx));

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    let source_backed = &snapshot.windows[0].source_backed_images;
    assert_eq!(
        snapshot.windows[0].renderer.image_resources.resource_count,
        2
    );
    assert_eq!(source_backed.final_scene_resource_count, 1);
    assert_eq!(source_backed.preload_live_count, 1);
    assert!(source_backed.preload_budget_deferral_count >= 1);
    let resource_sources = snapshot.windows[0]
        .renderer
        .image_resources
        .items
        .iter()
        .map(|item| item.source_id)
        .collect::<Vec<_>>();
    assert!(resource_sources.contains(&visible_source_id));
    assert_eq!(resource_sources.len(), 2);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn released_pending_source_backed_preload_cannot_resurrect_resource() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(80, 40));
    let window = test_cx.add_window(|_, _| PreloadImagesView::preload_only(source));
    let view = window.root(&mut test_cx).unwrap();
    let window: AnyWindowHandle = window.into();
    let mut cx = VisualTestContext::from_window(window, &test_cx);

    cx.update(|window, cx| window.draw(cx).clear());
    view.update(&mut cx, |view, cx| {
        view.preloads.clear();
        cx.notify();
    });
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();

    let image_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    assert_eq!(image_resources.resource_count, 0);
    assert_eq!(image_resources.upload_count, 0);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_preload_budget_deferral_drops_decoded_upload_buffer() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let (_view, cx) = test_cx.add_window_view(move |window, _| {
        window.set_source_backed_image_preload_budget_for_test(0, u64::MAX);
        PreloadImagesView::preload_only(source)
    });

    cx.run_until_parked();
    cx.update(|window, cx| window.draw_and_present_for_test(cx));

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    let source_backed = &snapshot.windows[0].source_backed_images;
    assert_eq!(snapshot.windows[0].renderer.image_resources.upload_count, 0);
    assert!(source_backed.preload_budget_deferral_count >= 1);
    assert_eq!(source_backed.pending_upload_decoded_cpu_bytes_estimate, 0);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_resources_evict_and_reload_from_byte_source() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(80, 40));
    let (view, cx) = test_cx.add_window_view(|_, _| SourceImagesView::single(source.clone()));

    run_until_image_resource_count(cx, 1);
    let first_upload_count = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .upload_count
    });
    assert_eq!(first_upload_count, 1);

    set_sources(&view, cx, Vec::new());
    run_until_image_resource_count(cx, 0);

    set_sources(&view, cx, vec![source]);
    run_until_image_resource_count(cx, 1);
    let second_upload_count = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .upload_count
    });
    assert_eq!(second_upload_count, 2);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_resources_evict_and_reload_from_file_source() {
    let mut test_cx = TestAppContext::single();
    let path = test_file_path(png_bytes(80, 40));
    let source = ImageRenderSource::file(path.clone());
    let (view, cx) = test_cx.add_window_view(|_, _| SourceImagesView::single(source.clone()));

    run_until_image_resource_count(cx, 1);
    set_sources(&view, cx, Vec::new());
    run_until_image_resource_count(cx, 0);

    set_sources(&view, cx, vec![source]);
    run_until_image_resource_count(cx, 1);
    let image_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    assert_eq!(image_resources.upload_count, 2);

    std::fs::remove_file(path).ok();
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_resources_reload_after_renderer_resources_are_cleared() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(80, 40));
    let (view, cx) = test_cx.add_window_view(|_, _| SourceImagesView::single(source));

    run_until_image_resource_count(cx, 1);
    cx.update(|window, _| window.clear_source_backed_image_resources_for_test());
    run_until_image_resource_count(cx, 0);

    view.update(cx, |_, cx| cx.notify());
    run_until_image_resource_count(cx, 1);
    let image_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    assert_eq!(image_resources.upload_count, 2);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_resize_replaces_stale_gpu_resource() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(200, 100));
    let (view, cx) = test_cx.add_window_view(|_, _| SourceImagesView::single(source));

    run_until_image_resource_count(cx, 1);
    assert_image_resource_size(cx, 80, 40);

    view.update(cx, |view, cx| {
        view.sizes = vec![(px(60.), px(30.))];
        cx.notify();
    });
    run_until_image_resource_count(cx, 1);
    assert_image_resource_size(cx, 120, 60);

    let image_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    assert_eq!(image_resources.upload_count, 2);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_resource_count_budget_is_enforced_without_permanent_failure() {
    let mut test_cx = TestAppContext::single();
    let sources = vec![
        ImageRenderSource::bytes(png_bytes(40, 20)),
        ImageRenderSource::bytes(png_bytes(40, 20)),
    ];
    let view_sources = sources.clone();
    let (view, cx) = test_cx.add_window_view(move |window, _| {
        window.set_source_backed_image_resource_budget_for_test(1, u64::MAX);
        SourceImagesView::new(Vec::new(), Vec::new())
    });

    set_sources(&view, cx, view_sources);
    run_until_image_resource_count(cx, 1);
    let first_source_id = sources[0].id().as_u64();
    assert_image_resource_sources(cx, &[first_source_id]);

    set_sources(&view, cx, vec![sources[1].clone()]);
    run_until_image_resource_count(cx, 1);
    assert_image_resource_sources(cx, &[sources[1].id().as_u64()]);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn replayed_source_backed_resource_counts_toward_budget() {
    let mut test_cx = TestAppContext::single();
    let first_source = ImageRenderSource::bytes(png_bytes(40, 20));
    let second_source = ImageRenderSource::bytes(png_bytes(40, 20));
    let first_source_id = first_source.id().as_u64();
    let (_view, cx) = test_cx.add_window_view(|_, cx| CachedImageRoot {
        cached_child: cx
            .new(|_| CachedImageChild {
                source: first_source.clone(),
            })
            .into(),
        extra_source: second_source,
        show_extra: true,
    });

    run_until_image_resource_count(cx, 2);
    let upload_count_before = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .upload_count
    });
    assert_eq!(upload_count_before, 2);

    cx.update(|window, _| {
        window.set_source_backed_image_resource_budget_for_test(1, u64::MAX);
    });
    cx.update(|window, cx| window.draw_and_present_for_test(cx));

    let snapshot = cx.cx.update(|cx| cx.renderer_diagnostic_snapshot());
    let image_resources = &snapshot.windows[0].renderer.image_resources;
    let source_backed = &snapshot.windows[0].source_backed_images;
    assert_eq!(image_resources.resource_count, 1);
    assert_eq!(image_resources.upload_count, upload_count_before);
    assert_eq!(source_backed.live_count, 1);
    assert_eq!(source_backed.budget_deferred_count, 1);
    assert_eq!(source_backed.painted_resource_count, 1);
    assert_image_resource_sources(cx, &[first_source_id]);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn source_backed_gpu_byte_budget_is_enforced_without_uploading_oversized_resource() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(40, 20));
    let (view, cx) = test_cx.add_window_view(|window, _| {
        window.set_source_backed_image_resource_budget_for_test(16, 40 * 20 * 4 - 1);
        SourceImagesView::new(Vec::new(), Vec::new())
    });

    set_sources(&view, cx, vec![source]);
    run_until_image_resource_count(cx, 0);
    let rejected_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    assert_eq!(rejected_resources.upload_count, 0);

    cx.update(|window, _| {
        window.set_source_backed_image_resource_budget_for_test(16, u64::MAX);
        window.refresh();
    });
    run_until_image_resource_count(cx, 1);
    let accepted_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    assert_eq!(accepted_resources.upload_count, 1);
}

#[cfg(feature = "test-support")]
#[gpui::test]
fn released_pending_source_backed_work_cannot_resurrect_resources() {
    let mut test_cx = TestAppContext::single();
    let source = ImageRenderSource::bytes(png_bytes(80, 40));
    let window = test_cx.add_window(|_, _| SourceImagesView::single(source));
    let view = window.root(&mut test_cx).unwrap();
    let window: AnyWindowHandle = window.into();
    let mut cx = VisualTestContext::from_window(window, &test_cx);

    cx.update(|window, cx| window.draw(cx).clear());
    set_sources(&view, &mut cx, Vec::new());
    cx.update(|window, cx| window.draw(cx).clear());
    cx.run_until_parked();

    let image_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    assert_eq!(image_resources.resource_count, 0);
    assert_eq!(image_resources.upload_count, 0);
}

#[cfg(feature = "test-support")]
fn cached_image_style() -> StyleRefinement {
    StyleRefinement::default().w(px(40.)).h(px(20.))
}

#[cfg(feature = "test-support")]
struct CachedImageRoot {
    cached_child: AnyView,
    extra_source: ImageRenderSource,
    show_extra: bool,
}

#[cfg(feature = "test-support")]
impl Render for CachedImageRoot {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div()
            .flex()
            .child(self.cached_child.clone().cached(cached_image_style()));
        if self.show_extra {
            root = root.child(
                img(self.extra_source.clone())
                    .id("extra-source-backed-image")
                    .w(px(40.))
                    .h(px(20.)),
            );
        }
        root
    }
}

#[cfg(feature = "test-support")]
struct CachedImageChild {
    source: ImageRenderSource,
}

#[cfg(feature = "test-support")]
impl Render for CachedImageChild {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        img(self.source.clone())
            .id("cached-source-backed-image")
            .w(px(40.))
            .h(px(20.))
    }
}

#[cfg(feature = "test-support")]
struct ClippedSourceImageView {
    source: ImageRenderSource,
}

#[cfg(feature = "test-support")]
impl Render for ClippedSourceImageView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div().overflow_hidden().w(px(40.)).h(px(20.)).child(
            img(self.source.clone())
                .id("clipped-source-backed-image")
                .mt(px(40.))
                .w(px(40.))
                .h(px(20.)),
        )
    }
}

#[cfg(feature = "test-support")]
struct AtlasAndSourceView {
    render_image: Arc<RenderImage>,
    source: ImageRenderSource,
}

#[cfg(feature = "test-support")]
impl Render for AtlasAndSourceView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .child(
                img(self.render_image.clone())
                    .id("atlas-image")
                    .w(px(2.))
                    .h(px(3.)),
            )
            .child(
                img(self.source.clone())
                    .id("source-backed-image")
                    .w(px(40.))
                    .h(px(20.)),
            )
    }
}

#[cfg(feature = "test-support")]
struct SourceImagesView {
    sources: Vec<ImageRenderSource>,
    sizes: Vec<(Pixels, Pixels)>,
}

#[cfg(feature = "test-support")]
impl SourceImagesView {
    fn new(sources: Vec<ImageRenderSource>, sizes: Vec<(Pixels, Pixels)>) -> Self {
        Self { sources, sizes }
    }

    fn single(source: ImageRenderSource) -> Self {
        Self::new(vec![source], vec![(px(40.), px(20.))])
    }
}

#[cfg(feature = "test-support")]
impl Render for SourceImagesView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let mut root = div().flex();
        for (index, source) in self.sources.iter().cloned().enumerate() {
            let (width, height) = self.sizes.get(index).copied().unwrap_or((px(40.), px(20.)));
            root = root.child(
                img(source)
                    .id(("source-backed-image", index))
                    .w(width)
                    .h(height),
            );
        }
        root
    }
}

#[cfg(feature = "test-support")]
struct PreloadImagesView {
    preloads: Vec<(ImageRenderSource, Size<Pixels>)>,
    visible: Vec<ImageRenderSource>,
}

#[cfg(feature = "test-support")]
impl PreloadImagesView {
    fn new(
        preloads: Vec<(ImageRenderSource, Size<Pixels>)>,
        visible: Vec<ImageRenderSource>,
    ) -> Self {
        Self { preloads, visible }
    }

    fn preload_only(source: ImageRenderSource) -> Self {
        Self::new(vec![(source, size(px(40.), px(20.)))], Vec::new())
    }
}

#[cfg(feature = "test-support")]
impl Render for PreloadImagesView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        let preloads = self.preloads.clone();
        let mut root = div().flex().on_children_prepainted(move |_, window, cx| {
            for (source, preload_size) in preloads.iter().cloned() {
                let scale_factor = window.scale_factor();
                let requested_size = size(
                    DevicePixels((f32::from(preload_size.width) * scale_factor).ceil() as i32),
                    DevicePixels((f32::from(preload_size.height) * scale_factor).ceil() as i32),
                );
                let request = source.render_request(0, scale_factor, requested_size);
                window.preload_source_backed_image(source, request, cx);
            }
        });
        for source in self.visible.iter().cloned() {
            root = root.child(
                img(source)
                    .id("visible-source-backed-image")
                    .w(px(40.))
                    .h(px(20.)),
            );
        }
        root
    }
}

#[cfg(feature = "test-support")]
fn set_sources(
    view: &Entity<SourceImagesView>,
    cx: &mut VisualTestContext,
    sources: Vec<ImageRenderSource>,
) {
    view.update(cx, |view, cx| {
        view.sizes = vec![(px(40.), px(20.)); sources.len()];
        view.sources = sources;
        cx.notify();
    });
    cx.update(|window, _| window.refresh());
}

#[cfg(feature = "test-support")]
fn assert_image_resource_size(cx: &mut VisualTestContext, width: u32, height: u32) {
    let image_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    assert_eq!(image_resources.resource_count, 1);
    assert_eq!(image_resources.items[0].width, width);
    assert_eq!(image_resources.items[0].height, height);
}

#[cfg(feature = "test-support")]
fn assert_image_resource_sources(cx: &mut VisualTestContext, source_ids: &[u64]) {
    let image_resources = cx.cx.update(|cx| {
        cx.renderer_diagnostic_snapshot().windows[0]
            .renderer
            .image_resources
            .clone()
    });
    let mut actual = image_resources
        .items
        .iter()
        .map(|item| item.source_id)
        .collect::<Vec<_>>();
    actual.sort_unstable();
    let mut expected = source_ids.to_vec();
    expected.sort_unstable();
    assert_eq!(actual, expected);
}
