use crate::{
    AnyWindowHandle, AtlasDiagnosticSnapshot, AtlasImageTileDiagnostic, AtlasKey,
    AtlasKindDiagnostic, AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, DispatchEventResult,
    GpuSpecs, ImageResource, ImageResourceDiagnostic, ImageResourceDiagnosticSnapshot,
    ImageResourceId, Pixels, PlatformAtlas, PlatformDisplay, PlatformImageResources, PlatformInput,
    PlatformInputHandler, PlatformRendererDiagnosticSnapshot, PlatformWindow, Point,
    PreparedImageUpload, PromptButton, RequestFrameOptions, Size, TestPlatform, TileId,
    WindowAppearance, WindowBackgroundAppearance, WindowBounds, WindowControlArea, WindowParams,
    image_resource_bytes,
};
use collections::HashMap;
use parking_lot::Mutex;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{
    rc::{Rc, Weak},
    sync::{self, Arc},
};

pub(crate) struct TestWindowState {
    pub(crate) bounds: Bounds<Pixels>,
    pub(crate) handle: AnyWindowHandle,
    display: Rc<dyn PlatformDisplay>,
    pub(crate) title: Option<String>,
    pub(crate) edited: bool,
    platform: Weak<TestPlatform>,
    sprite_atlas: Arc<dyn PlatformAtlas>,
    image_resources: Arc<dyn PlatformImageResources>,
    pub(crate) should_close_handler: Option<Box<dyn FnMut() -> bool>>,
    hit_test_window_control_callback: Option<Box<dyn FnMut() -> Option<WindowControlArea>>>,
    input_callback: Option<Box<dyn FnMut(PlatformInput) -> DispatchEventResult>>,
    active_status_change_callback: Option<Box<dyn FnMut(bool)>>,
    hover_status_change_callback: Option<Box<dyn FnMut(bool)>>,
    resize_callback: Option<Box<dyn FnMut(Size<Pixels>, f32)>>,
    moved_callback: Option<Box<dyn FnMut()>>,
    input_handler: Option<PlatformInputHandler>,
    is_fullscreen: bool,
}

#[derive(Clone)]
pub(crate) struct TestWindow(pub(crate) Rc<Mutex<TestWindowState>>);

impl HasWindowHandle for TestWindow {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        unimplemented!("Test Windows are not backed by a real platform window")
    }
}

impl HasDisplayHandle for TestWindow {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        unimplemented!("Test Windows are not backed by a real platform window")
    }
}

impl TestWindow {
    pub fn new(
        handle: AnyWindowHandle,
        params: WindowParams,
        platform: Weak<TestPlatform>,
        display: Rc<dyn PlatformDisplay>,
    ) -> Self {
        Self(Rc::new(Mutex::new(TestWindowState {
            bounds: params.bounds,
            display,
            platform,
            handle,
            sprite_atlas: Arc::new(TestAtlas::new()),
            image_resources: Arc::new(TestImageResources::new()),
            title: Default::default(),
            edited: false,
            should_close_handler: None,
            hit_test_window_control_callback: None,
            input_callback: None,
            active_status_change_callback: None,
            hover_status_change_callback: None,
            resize_callback: None,
            moved_callback: None,
            input_handler: None,
            is_fullscreen: false,
        })))
    }

    pub fn simulate_resize(&mut self, size: Size<Pixels>) {
        let scale_factor = self.scale_factor();
        let mut lock = self.0.lock();
        let Some(mut callback) = lock.resize_callback.take() else {
            return;
        };
        lock.bounds.size = size;
        drop(lock);
        callback(size, scale_factor);
        self.0.lock().resize_callback = Some(callback);
    }

    pub(crate) fn simulate_active_status_change(&self, active: bool) {
        let mut lock = self.0.lock();
        let Some(mut callback) = lock.active_status_change_callback.take() else {
            return;
        };
        drop(lock);
        callback(active);
        self.0.lock().active_status_change_callback = Some(callback);
    }

    pub fn simulate_input(&mut self, event: PlatformInput) -> bool {
        let mut lock = self.0.lock();
        let Some(mut callback) = lock.input_callback.take() else {
            return false;
        };
        drop(lock);
        let result = callback(event);
        self.0.lock().input_callback = Some(callback);
        !result.propagate
    }
}

impl PlatformWindow for TestWindow {
    fn bounds(&self) -> Bounds<Pixels> {
        self.0.lock().bounds
    }

    fn window_bounds(&self) -> WindowBounds {
        WindowBounds::Windowed(self.bounds())
    }

    fn is_maximized(&self) -> bool {
        false
    }

    fn content_size(&self) -> Size<Pixels> {
        self.bounds().size
    }

    fn resize(&mut self, size: Size<Pixels>) {
        let mut lock = self.0.lock();
        lock.bounds.size = size;
    }

    fn scale_factor(&self) -> f32 {
        2.0
    }

    fn appearance(&self) -> WindowAppearance {
        WindowAppearance::Light
    }

    fn display(&self) -> Option<std::rc::Rc<dyn crate::PlatformDisplay>> {
        Some(self.0.lock().display.clone())
    }

    fn mouse_position(&self) -> Point<Pixels> {
        Point::default()
    }

    fn modifiers(&self) -> crate::Modifiers {
        crate::Modifiers::default()
    }

    fn capslock(&self) -> crate::Capslock {
        crate::Capslock::default()
    }

    fn set_input_handler(&mut self, input_handler: PlatformInputHandler) {
        self.0.lock().input_handler = Some(input_handler);
    }

    fn take_input_handler(&mut self) -> Option<PlatformInputHandler> {
        self.0.lock().input_handler.take()
    }

    fn prompt(
        &self,
        _level: crate::PromptLevel,
        msg: &str,
        detail: Option<&str>,
        answers: &[PromptButton],
    ) -> Option<futures::channel::oneshot::Receiver<usize>> {
        Some(
            self.0
                .lock()
                .platform
                .upgrade()
                .expect("platform dropped")
                .prompt(msg, detail, answers),
        )
    }

    fn activate(&self) {
        self.0
            .lock()
            .platform
            .upgrade()
            .unwrap()
            .set_active_window(Some(self.clone()))
    }

    fn is_active(&self) -> bool {
        false
    }

    fn is_hovered(&self) -> bool {
        false
    }

    fn set_title(&mut self, title: &str) {
        self.0.lock().title = Some(title.to_owned());
    }

    fn set_app_id(&mut self, _app_id: &str) {}

    fn set_background_appearance(&self, _background: WindowBackgroundAppearance) {}

    fn set_edited(&mut self, edited: bool) {
        self.0.lock().edited = edited;
    }

    fn show_character_palette(&self) {
        unimplemented!()
    }

    fn minimize(&self) {
        unimplemented!()
    }

    fn zoom(&self) {
        unimplemented!()
    }

    fn toggle_fullscreen(&self) {
        let mut lock = self.0.lock();
        lock.is_fullscreen = !lock.is_fullscreen;
    }

    fn is_fullscreen(&self) -> bool {
        self.0.lock().is_fullscreen
    }

    fn on_request_frame(&self, _callback: Box<dyn FnMut(RequestFrameOptions)>) {}

    fn on_input(&self, callback: Box<dyn FnMut(crate::PlatformInput) -> DispatchEventResult>) {
        self.0.lock().input_callback = Some(callback)
    }

    fn on_active_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.lock().active_status_change_callback = Some(callback)
    }

    fn on_hover_status_change(&self, callback: Box<dyn FnMut(bool)>) {
        self.0.lock().hover_status_change_callback = Some(callback)
    }

    fn on_resize(&self, callback: Box<dyn FnMut(Size<Pixels>, f32)>) {
        self.0.lock().resize_callback = Some(callback)
    }

    fn on_moved(&self, callback: Box<dyn FnMut()>) {
        self.0.lock().moved_callback = Some(callback)
    }

    fn on_should_close(&self, callback: Box<dyn FnMut() -> bool>) {
        self.0.lock().should_close_handler = Some(callback);
    }

    fn on_close(&self, _callback: Box<dyn FnOnce()>) {}

    fn on_hit_test_window_control(&self, callback: Box<dyn FnMut() -> Option<WindowControlArea>>) {
        self.0.lock().hit_test_window_control_callback = Some(callback);
    }

    fn on_appearance_changed(&self, _callback: Box<dyn FnMut()>) {}

    fn draw(&self, _scene: &crate::Scene) {}

    fn sprite_atlas(&self) -> sync::Arc<dyn crate::PlatformAtlas> {
        self.0.lock().sprite_atlas.clone()
    }

    fn image_resources(&self) -> sync::Arc<dyn PlatformImageResources> {
        self.0.lock().image_resources.clone()
    }

    fn renderer_diagnostic_snapshot(&self) -> PlatformRendererDiagnosticSnapshot {
        let lock = self.0.lock();
        PlatformRendererDiagnosticSnapshot {
            backend: "test".to_string(),
            resources: Vec::new(),
            image_resources: lock.image_resources.diagnostic_snapshot(),
            atlas: lock.sprite_atlas.diagnostic_snapshot(),
            pipeline_buffers: Vec::new(),
            unavailable_reason: None,
        }
    }

    fn as_test(&mut self) -> Option<&mut TestWindow> {
        Some(self)
    }

    #[cfg(target_os = "windows")]
    fn get_raw_handle(&self) -> windows::Win32::Foundation::HWND {
        unimplemented!()
    }

    fn show_window_menu(&self, _position: Point<Pixels>) {
        unimplemented!()
    }

    fn start_window_move(&self) {
        unimplemented!()
    }

    fn update_ime_position(&self, _bounds: Bounds<Pixels>) {}

    fn gpu_specs(&self) -> Option<GpuSpecs> {
        None
    }
}

pub(crate) struct TestAtlasState {
    next_id: u32,
    tiles: HashMap<AtlasKey, AtlasTile>,
}

pub(crate) struct TestImageResourcesState {
    resources: HashMap<ImageResourceId, ImageResource>,
    upload_count: u64,
    upload_bytes: u64,
}

pub(crate) struct TestImageResources(Mutex<TestImageResourcesState>);

impl TestImageResources {
    pub fn new() -> Self {
        Self(Mutex::new(TestImageResourcesState {
            resources: HashMap::default(),
            upload_count: 0,
            upload_bytes: 0,
        }))
    }
}

impl PlatformImageResources for TestImageResources {
    fn upsert(&self, upload: PreparedImageUpload) -> anyhow::Result<ImageResource> {
        let resource = ImageResource::from_upload(&upload);
        let pixels = upload.into_pixels();
        let expected_bytes = image_resource_bytes(resource.size) as usize;
        anyhow::ensure!(
            pixels.len() == expected_bytes,
            "prepared upload byte length did not match resource size"
        );

        let mut state = self.0.lock();
        state.upload_count = state.upload_count.saturating_add(1);
        state.upload_bytes = state.upload_bytes.saturating_add(pixels.len() as u64);
        state.resources.insert(resource.id, resource.clone());
        Ok(resource)
    }

    fn contains(&self, id: ImageResourceId) -> bool {
        self.0.lock().resources.contains_key(&id)
    }

    fn remove(&self, id: ImageResourceId) {
        self.0.lock().resources.remove(&id);
    }

    fn clear(&self) {
        self.0.lock().resources.clear();
    }

    fn diagnostic_snapshot(&self) -> ImageResourceDiagnosticSnapshot {
        self.0.lock().diagnostic_snapshot()
    }
}

impl TestImageResourcesState {
    fn diagnostic_snapshot(&self) -> ImageResourceDiagnosticSnapshot {
        let mut snapshot = ImageResourceDiagnosticSnapshot {
            resource_count: self.resources.len(),
            upload_count: self.upload_count,
            upload_bytes: self.upload_bytes,
            ..Default::default()
        };
        for resource in self.resources.values() {
            snapshot.gpu_bytes_estimate = snapshot
                .gpu_bytes_estimate
                .saturating_add(resource.gpu_bytes_estimate());
            snapshot.items.push(ImageResourceDiagnostic::new(resource));
        }
        snapshot
    }
}

pub(crate) struct TestAtlas(Mutex<TestAtlasState>);

impl TestAtlas {
    pub fn new() -> Self {
        TestAtlas(Mutex::new(TestAtlasState {
            next_id: 0,
            tiles: HashMap::default(),
        }))
    }
}

impl PlatformAtlas for TestAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: &crate::AtlasKey,
        build: &mut dyn FnMut() -> anyhow::Result<
            Option<(Size<crate::DevicePixels>, std::borrow::Cow<'a, [u8]>)>,
        >,
    ) -> anyhow::Result<Option<crate::AtlasTile>> {
        let mut state = self.0.lock();
        if let Some(tile) = state.tiles.get(key) {
            return Ok(Some(tile.clone()));
        }
        drop(state);

        let Some((size, _)) = build()? else {
            return Ok(None);
        };

        let mut state = self.0.lock();
        state.next_id += 1;
        let texture_id = state.next_id;
        state.next_id += 1;
        let tile_id = state.next_id;

        state.tiles.insert(
            key.clone(),
            crate::AtlasTile {
                texture_id: AtlasTextureId {
                    index: texture_id,
                    kind: key.texture_kind(),
                },
                tile_id: TileId(tile_id),
                padding: 0,
                bounds: crate::Bounds {
                    origin: Point::default(),
                    size,
                },
            },
        );

        Ok(Some(state.tiles[key].clone()))
    }

    fn remove(&self, key: &AtlasKey) {
        let mut state = self.0.lock();
        state.tiles.remove(key);
    }

    fn diagnostic_snapshot(&self) -> AtlasDiagnosticSnapshot {
        self.0.lock().diagnostic_snapshot()
    }
}

impl TestAtlasState {
    fn diagnostic_snapshot(&self) -> AtlasDiagnosticSnapshot {
        let mut snapshot = AtlasDiagnosticSnapshot {
            tile_count: self.tiles.len(),
            ..Default::default()
        };
        let mut monochrome_count = 0_u64;
        let mut polychrome_count = 0_u64;
        for (key, tile) in &self.tiles {
            match key {
                AtlasKey::Glyph(_) => snapshot.glyph_tiles += 1,
                AtlasKey::Svg(_) => snapshot.svg_tiles += 1,
                AtlasKey::Image(params) => {
                    snapshot.image_tiles += 1;
                    let tile_bytes_estimate = tile_bytes_estimate(tile);
                    snapshot.image_tile_bytes_estimate = snapshot
                        .image_tile_bytes_estimate
                        .saturating_add(tile_bytes_estimate);
                    snapshot.image_tile_items.push(AtlasImageTileDiagnostic {
                        render_image_id: params.image_id.0,
                        frame_index: params.frame_index,
                        width: tile.bounds.size.width.0.max(0) as u32,
                        height: tile.bounds.size.height.0.max(0) as u32,
                        tile_bytes_estimate,
                    });
                }
            }

            match key.texture_kind() {
                AtlasTextureKind::Monochrome => monochrome_count += 1,
                AtlasTextureKind::Polychrome => polychrome_count += 1,
            }
        }
        snapshot
            .kinds
            .push(test_atlas_kind_diagnostic("monochrome", monochrome_count));
        snapshot
            .kinds
            .push(test_atlas_kind_diagnostic("polychrome", polychrome_count));
        snapshot
    }
}

fn test_atlas_kind_diagnostic(kind: &str, live_key_count: u64) -> AtlasKindDiagnostic {
    AtlasKindDiagnostic {
        kind: kind.to_string(),
        texture_count: live_key_count as usize,
        free_texture_slots: 0,
        live_key_count,
        gpu_texture_bytes_estimate: 0,
        cpu_mirror_bytes: 0,
        dirty_texture_count: 0,
        dirty_bytes_estimate: 0,
        upload_calls: 0,
        upload_bytes: 0,
        flush_calls: 0,
        flush_bytes: 0,
    }
}

fn tile_bytes_estimate(tile: &AtlasTile) -> u64 {
    (tile.bounds.size.width.0.max(0) as u64)
        .saturating_mul(tile.bounds.size.height.0.max(0) as u64)
        .saturating_mul(4)
}
