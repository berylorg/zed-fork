use crate::{
    AnyWindowHandle, AtlasDiagnosticSnapshot, AtlasImageTileDiagnostic, AtlasKey,
    AtlasKindDiagnostic, AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, DispatchEventResult,
    GpuSpecs, ImageResource, ImageResourceDiagnostic, ImageResourceDiagnosticSnapshot,
    ImageResourceId, InitialWindowState, Pixels, PlatformAtlas, PlatformDisplay,
    PlatformImageResources, PlatformInput, PlatformInputHandler,
    PlatformRendererDiagnosticSnapshot, PlatformWindow, Point, PreparedImageUpload, PromptButton,
    RequestFrameOptions, Size, TestPlatform, TileId, WindowAppearance, WindowBackgroundAppearance,
    WindowBounds, WindowControlArea, WindowParams, image_resource_bytes,
};
use collections::HashMap;
use parking_lot::Mutex;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use std::{
    rc::{Rc, Weak},
    sync::{self, Arc},
};

pub(crate) struct TestWindowState {
    minimum: Option<Size<Pixels>>,
    minimum_active: bool,
    minimum_failure: Option<bool>,
    normal_restore_size: Size<Pixels>,
    closed: bool,
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
    pub(crate) window_state: TestWindowStateKind,
    pub(crate) visible: bool,
    pub(crate) visibility_change_count: usize,
    pub(crate) fail_next_visibility_change: bool,
    pub(crate) active: bool,
    pub(crate) activation_change_count: usize,
    pub(crate) minimized: bool,
    pub(crate) minimize_change_count: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub enum TestWindowStateKind {
    Windowed,
    Maximized,
    Fullscreen,
}

pub(crate) struct TestWindow(pub(crate) Rc<Mutex<TestWindowState>>, bool);

impl Clone for TestWindow {
    fn clone(&self) -> Self {
        // Observation clones do not own the platform window's close.
        Self(self.0.clone(), false)
    }
}

impl Drop for TestWindow {
    fn drop(&mut self) {
        if self.1 {
            self.0.lock().closed = true;
        }
    }
}

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
    ) -> anyhow::Result<Self> {
        let mut initial_bounds = params.bounds;
        if let Some(minimum) = params.window_min_size {
            let required = crate::window_minimum::checked_tracking_minimum(minimum, 2., (0, 0))?;
            if params.initial_state == InitialWindowState::Windowed {
                initial_bounds.size.width = initial_bounds
                    .size
                    .width
                    .max(crate::px(required.width.0 as f32 / 2.));
                initial_bounds.size.height = initial_bounds
                    .size
                    .height
                    .max(crate::px(required.height.0 as f32 / 2.));
            } else {
                anyhow::ensure!(
                    initial_bounds.size.width.0 * 2. >= required.width.0 as f32
                        && initial_bounds.size.height.0 * 2. >= required.height.0 as f32,
                    "initial test window cannot satisfy its declared minimum"
                );
            }
        }
        let active = params.show && params.focus;
        let window = Self(
            Rc::new(Mutex::new(TestWindowState {
                minimum: params.window_min_size,
                minimum_active: false,
                minimum_failure: None,
                normal_restore_size: initial_bounds.size,
                closed: false,
                bounds: initial_bounds,
                display,
                platform: platform.clone(),
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
                window_state: match params.initial_state {
                    InitialWindowState::Windowed => TestWindowStateKind::Windowed,
                    InitialWindowState::Maximized => TestWindowStateKind::Maximized,
                    InitialWindowState::Fullscreen => TestWindowStateKind::Fullscreen,
                },
                visible: params.show,
                visibility_change_count: 0,
                fail_next_visibility_change: false,
                active,
                activation_change_count: usize::from(active),
                minimized: false,
                minimize_change_count: 0,
            })),
            true,
        );
        if active && let Some(platform) = platform.upgrade() {
            platform.set_active_window(Some(window.clone()));
        }
        Ok(window)
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
        self.set_active_state(active);
        let mut lock = self.0.lock();
        let Some(mut callback) = lock.active_status_change_callback.take() else {
            return;
        };
        drop(lock);
        callback(active);
        self.0.lock().active_status_change_callback = Some(callback);
    }

    pub(crate) fn set_active_state(&self, active: bool) {
        let mut lock = self.0.lock();
        if lock.active != active {
            lock.active = active;
            lock.activation_change_count += 1;
        }
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
    #[cfg(feature = "test-support")]
    fn minimum_size_for_test(&self) -> Option<Size<Pixels>> {
        self.0.lock().minimum
    }

    #[cfg(feature = "test-support")]
    fn fail_next_minimum_update_for_test(&mut self, unverified: bool) {
        self.0.lock().minimum_failure = Some(unverified);
    }

    fn update_minimum_size(
        &mut self,
        minimum: Size<Pixels>,
    ) -> Result<crate::WindowMinimumUpdate, crate::WindowMinimumUpdateError> {
        use crate::{
            Platform, WindowMinimumUpdateError as Error, WindowMinimumUpdateFailure as Failure,
            WindowMinimumUpdateOutcome as Outcome,
        };
        let required = crate::window_minimum::checked_minimum_pixels(minimum, 2., (0, 0))?;
        let executor = {
            let mut state = self.0.lock();
            if state.closed {
                return Err(Error::Closed);
            }
            if state.minimum_active {
                return Err(Error::Busy);
            }
            let executor = state
                .platform
                .upgrade()
                .ok_or(Error::Closed)?
                .foreground_executor();
            state.minimum_active = true;
            executor
        };
        let (sender, completion) = crate::window_minimum::minimum_update_channel();
        let owner = self.0.clone();
        executor
            .spawn(async move {
                let (outcome, callback, previous) = {
                    let mut state = owner.lock();
                    let previous = state.minimum;
                    let previous_size = state.bounds.size;
                    let outcome = if state.closed {
                        Outcome::NotApplied(Failure::Closed)
                    } else if state.minimized {
                        Outcome::NotApplied(Failure::Minimized)
                    } else if state.window_state == TestWindowStateKind::Fullscreen {
                        Outcome::NotApplied(Failure::UnsupportedState)
                    } else if let Some(unverified) = state.minimum_failure.take() {
                        if unverified {
                            Outcome::Unverified
                        } else {
                            Outcome::NotApplied(Failure::NativeFailure)
                        }
                    } else if state.window_state == TestWindowStateKind::Maximized
                        && (state.bounds.size.width.0 * 2. < required.width.0 as f32
                            || state.bounds.size.height.0 * 2. < required.height.0 as f32
                            || state.normal_restore_size.width.0 * 2. < required.width.0 as f32
                            || state.normal_restore_size.height.0 * 2. < required.height.0 as f32)
                    {
                        Outcome::NotApplied(Failure::InsufficientSpace)
                    } else {
                        state.minimum = Some(minimum);
                        state.bounds.size.width = state
                            .bounds
                            .size
                            .width
                            .max(crate::px(required.width.0 as f32 / 2.));
                        state.bounds.size.height = state
                            .bounds
                            .size
                            .height
                            .max(crate::px(required.height.0 as f32 / 2.));
                        if state.window_state == TestWindowStateKind::Windowed {
                            state.normal_restore_size = state.bounds.size;
                        }
                        Outcome::Applied {
                            minimum,
                            client_size: state.bounds.size,
                        }
                    };
                    if !matches!(outcome, Outcome::Applied { .. }) {
                        state.minimum = previous;
                    }
                    let callback = if state.bounds.size != previous_size {
                        state.resize_callback.take()
                    } else {
                        None
                    };
                    (outcome, callback, previous)
                };
                if let Some(mut callback) = callback {
                    if let Outcome::Applied { client_size, .. } = outcome {
                        callback(client_size, 2.);
                    }
                    owner.lock().resize_callback = Some(callback);
                }
                let mut state = owner.lock();
                state.minimum_active = false;
                let outcome = if state.closed {
                    state.minimum = previous;
                    Outcome::NotApplied(Failure::Closed)
                } else {
                    outcome
                };
                drop(state);
                let _ = sender.send(outcome);
            })
            .detach();
        Ok(completion)
    }

    fn bounds(&self) -> Bounds<Pixels> {
        self.0.lock().bounds
    }

    fn window_bounds(&self) -> WindowBounds {
        let window_state = self.0.lock().window_state;
        match window_state {
            TestWindowStateKind::Windowed => WindowBounds::Windowed(self.bounds()),
            TestWindowStateKind::Maximized => WindowBounds::Maximized(self.bounds()),
            TestWindowStateKind::Fullscreen => WindowBounds::Fullscreen(self.bounds()),
        }
    }

    fn is_maximized(&self) -> bool {
        self.0.lock().window_state == TestWindowStateKind::Maximized
    }

    fn is_minimized(&self) -> Option<bool> {
        let state = self.0.lock();
        (!state.closed).then_some(state.minimized)
    }

    fn content_size(&self) -> Size<Pixels> {
        self.bounds().size
    }

    fn resize(&mut self, size: Size<Pixels>) {
        let mut lock = self.0.lock();
        if lock.minimum_active || lock.closed {
            return;
        }
        lock.bounds.size = size;
        if lock.window_state == TestWindowStateKind::Windowed {
            lock.normal_restore_size = size;
        }
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
        use crate::Platform;

        let (platform, restored) = {
            let mut state = self.0.lock();
            if state.minimum_active || state.closed {
                return;
            }
            let restored = if state.minimized {
                state.minimized = false;
                state
                    .resize_callback
                    .take()
                    .map(|callback| (callback, state.bounds.size))
            } else {
                None
            };
            (state.platform.upgrade().unwrap(), restored)
        };
        if let Some((mut callback, size)) = restored {
            let owner = self.0.clone();
            platform
                .foreground_executor()
                .spawn(async move {
                    callback(size, 2.);
                    owner.lock().resize_callback = Some(callback);
                })
                .detach();
        }
        platform.set_active_window(Some(self.clone()))
    }

    fn is_active(&self) -> bool {
        self.0.lock().active
    }

    fn is_hovered(&self) -> bool {
        false
    }

    fn set_title(&mut self, title: &str) {
        self.0.lock().title = Some(title.to_owned());
    }

    fn set_app_id(&mut self, _app_id: &str) {}

    fn map_window(&mut self) -> anyhow::Result<()> {
        let mut lock = self.0.lock();
        anyhow::ensure!(
            !lock.minimum_active && !lock.closed,
            "native minimum operation or closure prevents publication"
        );
        if lock.fail_next_visibility_change {
            lock.fail_next_visibility_change = false;
            anyhow::bail!("test visibility transition failed")
        }
        lock.visible = true;
        lock.visibility_change_count += 1;
        Ok(())
    }

    fn uses_native_initial_state(&self) -> bool {
        true
    }

    fn set_background_appearance(&self, _background: WindowBackgroundAppearance) {}

    fn set_edited(&mut self, edited: bool) {
        self.0.lock().edited = edited;
    }

    fn show_character_palette(&self) {
        unimplemented!()
    }

    fn minimize(&self) {
        let mut lock = self.0.lock();
        if lock.minimum_active || lock.closed {
            return;
        }
        lock.minimized = true;
        lock.minimize_change_count += 1;
    }

    fn zoom(&self) {
        let mut lock = self.0.lock();
        if lock.minimum_active || lock.closed {
            return;
        }
        lock.window_state = if lock.window_state == TestWindowStateKind::Maximized {
            TestWindowStateKind::Windowed
        } else {
            TestWindowStateKind::Maximized
        };
    }

    fn toggle_fullscreen(&self) {
        let mut lock = self.0.lock();
        if lock.minimum_active || lock.closed {
            return;
        }
        lock.window_state = if lock.window_state == TestWindowStateKind::Fullscreen {
            TestWindowStateKind::Windowed
        } else {
            TestWindowStateKind::Fullscreen
        };
    }

    fn is_fullscreen(&self) -> bool {
        self.0.lock().window_state == TestWindowStateKind::Fullscreen
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
