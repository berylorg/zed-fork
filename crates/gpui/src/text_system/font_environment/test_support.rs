use crate::{
    Bounds, DevicePixels, Font, FontId, FontMetrics, FontRun, GlyphId, LineLayout, NoopTextSystem,
    Pixels, PlatformTextSystem, RenderGlyphParams, Result, Size, TextSystem, WindowTextSystem, px,
};
use parking_lot::Mutex;
use std::{borrow::Cow, sync::Arc};

pub struct FontEnvironmentTestContext {
    backend: Arc<ControlledTextSystem>,
    pub text_system: Arc<TextSystem>,
}

#[derive(Default)]
struct ControlledState {
    generation: u64,
    unqualified: bool,
    resolutions: usize,
    shapes: usize,
    change_on_resolution: bool,
    change_on_shape: bool,
}

struct ControlledTextSystem(Mutex<ControlledState>);

impl Default for FontEnvironmentTestContext {
    fn default() -> Self {
        Self::new()
    }
}

impl FontEnvironmentTestContext {
    pub fn new() -> Self {
        let backend = Arc::new(ControlledTextSystem(Mutex::new(ControlledState::default())));
        let text_system = Arc::new(TextSystem::new(backend.clone()));
        Self {
            backend,
            text_system,
        }
    }

    pub fn window_text_system(&self) -> WindowTextSystem {
        WindowTextSystem::new(self.text_system.clone())
    }

    pub fn font_id(&self, font: &Font) -> Result<FontId> {
        self.text_system.font_id(font)
    }

    pub fn set_generation(&self, generation: u64) {
        self.backend.0.lock().generation = generation;
    }

    pub fn make_unqualified(&self) {
        self.backend.0.lock().unqualified = true;
    }

    pub fn change_during_next_resolution(&self) {
        self.backend.0.lock().change_on_resolution = true;
    }

    pub fn change_during_next_shape(&self) {
        self.backend.0.lock().change_on_shape = true;
    }

    pub fn counts(&self) -> (usize, usize) {
        let state = self.backend.0.lock();
        (state.resolutions, state.shapes)
    }

    pub fn exhaust_owner(&mut self) {
        Arc::get_mut(&mut self.text_system)
            .unwrap()
            .font_environment_owner = None;
    }
}

impl ControlledState {
    fn advance(&mut self) {
        if let Some(next) = self.generation.checked_add(1) {
            self.generation = next;
        } else {
            self.unqualified = true;
        }
    }
}

impl PlatformTextSystem for ControlledTextSystem {
    fn font_environment_generation(&self) -> Option<u64> {
        let state = self.0.lock();
        (!state.unqualified).then_some(state.generation)
    }

    fn add_fonts(&self, fonts: Vec<Cow<'static, [u8]>>) -> Result<()> {
        if !fonts.is_empty() {
            self.0.lock().advance();
        }
        Ok(())
    }

    fn all_font_names(&self) -> Vec<String> {
        Vec::new()
    }

    fn font_id(&self, font: &Font) -> Result<FontId> {
        let mut state = self.0.lock();
        state.resolutions += 1;
        let id = FontId(state.generation as usize);
        if state.change_on_resolution {
            state.change_on_resolution = false;
            state.advance();
        }
        anyhow::ensure!(!font.family.starts_with("missing"), "missing font family");
        Ok(id)
    }

    fn font_metrics(&self, id: FontId) -> FontMetrics {
        NoopTextSystem.font_metrics(id)
    }
    fn typographic_bounds(&self, id: FontId, glyph: GlyphId) -> Result<Bounds<f32>> {
        NoopTextSystem.typographic_bounds(id, glyph)
    }
    fn advance(&self, id: FontId, glyph: GlyphId) -> Result<Size<f32>> {
        NoopTextSystem.advance(id, glyph)
    }
    fn glyph_for_char(&self, id: FontId, ch: char) -> Option<GlyphId> {
        NoopTextSystem.glyph_for_char(id, ch)
    }
    fn glyph_raster_bounds(&self, params: &RenderGlyphParams) -> Result<Bounds<DevicePixels>> {
        NoopTextSystem.glyph_raster_bounds(params)
    }
    fn rasterize_glyph(
        &self,
        params: &RenderGlyphParams,
        bounds: Bounds<DevicePixels>,
    ) -> Result<(Size<DevicePixels>, Vec<u8>)> {
        NoopTextSystem.rasterize_glyph(params, bounds)
    }
    fn layout_line(&self, text: &str, size: Pixels, runs: &[FontRun]) -> LineLayout {
        let mut state = self.0.lock();
        state.shapes += 1;
        let mut layout = NoopTextSystem.layout_line(text, size, runs);
        layout.width = px(10.0 + state.generation as f32);
        if state.change_on_shape {
            state.change_on_shape = false;
            state.advance();
        }
        layout
    }
}
