use super::*;
use crate::platform::windows::DirectXDevices;

pub struct WindowsFontEnvironmentTestContext {
    backend: Arc<DirectWriteTextSystem>,
    pub text_system: Arc<TextSystem>,
}

impl WindowsFontEnvironmentTestContext {
    pub fn observation_diagnostics(&self) -> anyhow::Result<(String, String)> {
        fn status(collection: &IDWriteFontCollection1) -> String {
            let observation = match collection.cast::<IDWriteFontCollection3>() {
                Ok(observation) => observation,
                Err(error) => return format!("Collection3 QueryInterface failed: {error}"),
            };
            let event = unsafe { observation.GetExpirationEvent() };
            if event.is_invalid() {
                return "Collection3 expiration event null/invalid".into();
            }
            format!("Collection3 zero-timeout wait: {:?}", unsafe {
                WaitForSingleObject(event, 0)
            })
        }
        let state = self.backend.0.read();
        let mut legacy = None;
        unsafe {
            state
                .components
                .factory
                .GetSystemFontCollection(false, &mut legacy, true)?
        };
        Ok((
            status(&legacy.context("legacy collection unavailable")?),
            status(&state.system_font_collection),
        ))
    }
    pub fn new() -> anyhow::Result<Self> {
        let devices = DirectXDevices::new()?;
        let backend = Arc::new(DirectWriteTextSystem::new(&devices)?);
        let text_system = Arc::new(TextSystem::new(backend.clone()));
        Ok(Self {
            backend,
            text_system,
        })
    }

    pub fn window_text_system(&self) -> WindowTextSystem {
        WindowTextSystem::new(self.text_system.clone())
    }

    pub fn counts(&self) -> (usize, usize, usize, usize) {
        let state = self.backend.0.read();
        (
            state.fonts.len(),
            state.font_selections.len(),
            state.font_id_by_identifier.len(),
            state.refresh_attempts,
        )
    }

    pub fn adopt_retained_collection(&self) {
        let mut state = self.backend.0.write();
        let collection = state.system_font_collection.clone();
        let fallback = state.system_font_fallback.clone();
        state.adopt_observed_font_environment(WAIT_OBJECT_0, || Some((collection, fallback)));
    }

    pub fn observe_failed_wait(&self) {
        self.backend
            .0
            .write()
            .adopt_observed_font_environment(WAIT_FAILED, || panic!("unexpected adoption"));
    }

    pub fn observe_failed_adoption(&self) {
        self.backend
            .0
            .write()
            .adopt_observed_font_environment(WAIT_OBJECT_0, || None);
    }

    pub fn observe_expired_replacement(&self) {
        let mut state = self.backend.0.write();
        let collection = state.system_font_collection.clone();
        let fallback = state.system_font_fallback.clone();
        state.adopt_observed_font_environment_with_observer(
            WAIT_OBJECT_0,
            || Some((collection, fallback)),
            |collection| {
                observable_collection_with_wait(collection.cast().unwrap(), |_| WAIT_OBJECT_0)
            },
        );
    }

    pub fn observe_unobservable_replacement(&self) {
        let mut state = self.backend.0.write();
        let collection = state.custom_font_collection.clone();
        let fallback = state.system_font_fallback.clone();
        state.adopt_observed_font_environment(WAIT_OBJECT_0, || Some((collection, fallback)));
    }

    pub fn retained_face_bytes(&self, id: FontId) -> anyhow::Result<Vec<u8>> {
        let state = self.backend.0.read();
        let face = &state.fonts[id.0].font_face;
        unsafe {
            let mut count = 0;
            face.GetFiles(&mut count, None)?;
            anyhow::ensure!(count == 1, "font fixture must have one file");
            let mut file = None;
            face.GetFiles(&mut count, Some(&mut file))?;
            let file = file.context("font file unavailable")?;
            let mut key = std::ptr::null_mut();
            let mut key_size = 0;
            file.GetReferenceKey(&mut key, &mut key_size)?;
            let stream = file.GetLoader()?.CreateStreamFromKey(key, key_size)?;
            let size = stream.GetFileSize()?;
            anyhow::ensure!(
                size > 0 && size <= 64 * 1024 * 1024,
                "font fixture size out of bounds"
            );
            let mut start = std::ptr::null_mut();
            let mut context = std::ptr::null_mut();
            stream.ReadFileFragment(&mut start, 0, size, &mut context)?;
            let bytes = std::slice::from_raw_parts(start as *const u8, size as usize).to_vec();
            stream.ReleaseFileFragment(context);
            Ok(bytes)
        }
    }

    pub fn observe_unsignalled_event(&self) {
        self.backend
            .0
            .write()
            .adopt_observed_font_environment(WAIT_TIMEOUT, || panic!("unexpected adoption"));
    }

    pub fn exhaust_generation(&self) {
        let mut state = self.backend.0.write();
        state.font_environment_generation = Some(u64::MAX);
        state.advance_font_environment();
    }

    pub fn old_face_advance(&self, id: FontId) -> anyhow::Result<Size<f32>> {
        let state = self.backend.0.read();
        let glyph = state
            .glyph_for_char(id, 'A')
            .context("old face glyph unavailable")?;
        state.get_advance(id, glyph)
    }

    pub fn old_face_raster(&self, id: FontId) -> anyhow::Result<(Size<DevicePixels>, Vec<u8>)> {
        let state = self.backend.0.read();
        let glyph = state
            .glyph_for_char(id, 'A')
            .context("old face glyph unavailable")?;
        let params = RenderGlyphParams {
            font_id: id,
            glyph_id: glyph,
            font_size: px(12.),
            subpixel_variant: point(0, 0),
            scale_factor: 1.0,
            is_emoji: false,
        };
        let bounds = state.raster_bounds(&params)?;
        state.rasterize_glyph(&params, bounds)
    }

    pub fn checked_shape_with_retained_id(&self, id: FontId) -> anyhow::Result<LineLayout> {
        self.backend.layout_line_checked(
            "abc",
            px(12.),
            &[FontRun {
                len: 3,
                font_id: id,
            }],
        )
    }

    pub fn refuse_face_at_capacity(&self, id: FontId) -> anyhow::Result<(usize, usize)> {
        let mut state = self.backend.0.write();
        let original = state.fonts.len();
        let source = &state.fonts[id.0];
        let template = FontInfo {
            font_family: source.font_family.clone(),
            font_face: source.font_face.clone(),
            features: source.features.clone(),
            fallbacks: source.fallbacks.clone(),
            is_system_font: source.is_system_font,
        };
        while state.fonts.len() < MAX_RETAINED_FONT_FACES {
            state.fonts.push(FontInfo {
                font_family: template.font_family.clone(),
                font_face: template.font_face.clone(),
                features: template.features.clone(),
                fallbacks: template.fallbacks.clone(),
                is_system_font: template.is_system_font,
            });
        }
        let identifier = get_font_identifier(&template.font_face, &state.components.locale)
            .context("face identity unavailable")?;
        let maps_before = state.font_id_by_identifier.clone();
        let refused = state.admit_font_face(identifier, template).is_err();
        anyhow::ensure!(
            refused
                && state.fonts.len() == MAX_RETAINED_FONT_FACES
                && state.font_id_by_identifier == maps_before,
            "face admission mutated a full registry"
        );
        Ok((original, state.fonts.len()))
    }
}
