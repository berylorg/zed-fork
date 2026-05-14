use crate::{
    BackgroundExecutor, DevicePixels, ImageRenderRequest, ImageRenderRequestId, ImageRenderSource,
    Size, SvgRenderer, SvgSize, Task, size, swap_rgba_pa_to_bgra,
};
use image::{DynamicImage, ImageFormat, ImageReader, RgbaImage, imageops};
use std::{fs, io::Cursor};

use super::img::ImageCacheError;

/// Default upper bound for source pixels decoded by source-backed image preparation.
pub const DEFAULT_MAX_SOURCE_IMAGE_PIXELS: u64 = 64 * 1024 * 1024;

/// Default upper bound for pixels in one prepared image upload buffer.
pub const DEFAULT_MAX_IMAGE_UPLOAD_PIXELS: u64 = 64 * 1024 * 1024;

/// Bounds used while preparing source-backed image upload buffers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImagePreparationLimits {
    max_source_pixels: u64,
    max_upload_pixels: u64,
}

impl Default for ImagePreparationLimits {
    fn default() -> Self {
        Self {
            max_source_pixels: DEFAULT_MAX_SOURCE_IMAGE_PIXELS,
            max_upload_pixels: DEFAULT_MAX_IMAGE_UPLOAD_PIXELS,
        }
    }
}

impl ImagePreparationLimits {
    /// Sets the maximum source pixel count that may be decoded.
    pub fn with_max_source_pixels(mut self, max_source_pixels: u64) -> Self {
        self.max_source_pixels = max_source_pixels;
        self
    }

    /// Sets the maximum prepared upload pixel count.
    pub fn with_max_upload_pixels(mut self, max_upload_pixels: u64) -> Self {
        self.max_upload_pixels = max_upload_pixels;
        self
    }

    /// Returns the maximum source pixel count that may be decoded.
    pub fn max_source_pixels(&self) -> u64 {
        self.max_source_pixels
    }

    /// Returns the maximum prepared upload pixel count.
    pub fn max_upload_pixels(&self) -> u64 {
        self.max_upload_pixels
    }
}

/// A temporary BGRA image buffer prepared for upload.
pub struct PreparedImageUpload {
    request: ImageRenderRequest,
    source_size: Size<DevicePixels>,
    size: Size<DevicePixels>,
    pixels: Vec<u8>,
    #[cfg(feature = "test-support")]
    prepared_on_main_thread: bool,
}

impl PreparedImageUpload {
    /// Returns the render request this upload buffer satisfies.
    pub fn request(&self) -> ImageRenderRequest {
        self.request
    }

    /// Returns the original source frame size in device pixels.
    pub fn source_size(&self) -> Size<DevicePixels> {
        self.source_size
    }

    /// Returns the prepared upload size in device pixels.
    pub fn size(&self) -> Size<DevicePixels> {
        self.size
    }

    /// Returns the prepared BGRA upload bytes.
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// Consumes this upload buffer and returns its BGRA bytes.
    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }

    /// Returns whether preparation ran while the test dispatcher considered itself on the main thread.
    #[cfg(feature = "test-support")]
    pub fn prepared_on_main_thread_for_test(&self) -> bool {
        self.prepared_on_main_thread
    }
}

/// A failed source-backed image preparation result.
#[derive(Clone, Debug)]
pub struct ImagePreparationFailure {
    request: ImageRenderRequest,
    kind: ImagePreparationFailureKind,
}

impl ImagePreparationFailure {
    fn new(request: ImageRenderRequest, kind: ImagePreparationFailureKind) -> Self {
        Self { request, kind }
    }

    /// Returns the render request that failed.
    pub fn request(&self) -> ImageRenderRequest {
        self.request
    }

    /// Returns the failure kind.
    pub fn kind(&self) -> &ImagePreparationFailureKind {
        &self.kind
    }
}

/// The stable failure states produced by source-backed image preparation.
#[derive(Clone, Debug)]
pub enum ImagePreparationFailureKind {
    /// The source could not be opened or read.
    Unavailable(ImageCacheError),
    /// The source format is not supported by GPUI's image preparation path.
    Unsupported(ImageCacheError),
    /// The source or requested upload exceeded configured limits.
    TooLarge {
        /// The original source frame size when known.
        source_size: Size<DevicePixels>,
        /// The requested render size.
        requested_size: Size<DevicePixels>,
        /// The configured pixel limit that was exceeded.
        max_pixels: u64,
    },
    /// The requested size is empty or invalid.
    InvalidRequestedSize(Size<DevicePixels>),
    /// The render request does not match the supplied source.
    SourceMismatch,
    /// The requested frame is not available for this source.
    FrameUnavailable {
        /// The requested frame index.
        frame_index: usize,
    },
    /// The source was recognized but decoding failed.
    DecodeFailed(ImageCacheError),
}

/// The outcome of preparing one source-backed image request.
pub enum ImagePreparationOutcome {
    /// A BGRA upload buffer is ready.
    Ready(PreparedImageUpload),
    /// Preparation failed with a stable fallback reason.
    Failed(ImagePreparationFailure),
}

impl ImagePreparationOutcome {
    /// Returns the render request represented by this outcome.
    pub fn request(&self) -> ImageRenderRequest {
        match self {
            Self::Ready(upload) => upload.request(),
            Self::Failed(failure) => failure.request(),
        }
    }

    /// Returns the render request identity represented by this outcome.
    pub fn request_id(&self) -> ImageRenderRequestId {
        self.request().id()
    }

    /// Returns this outcome's failure kind, when preparation failed.
    pub fn failure_kind(&self) -> Option<&ImagePreparationFailureKind> {
        match self {
            Self::Ready(_) => None,
            Self::Failed(failure) => Some(failure.kind()),
        }
    }
}

/// Tracks the currently wanted render request for one image presentation slot.
#[derive(Default)]
pub struct ImageRenderRequestSlot {
    wanted_request: Option<ImageRenderRequestId>,
    live_request: Option<ImageRenderRequestId>,
}

impl ImageRenderRequestSlot {
    /// Records the newest request wanted by this slot.
    pub fn request(&mut self, request: ImageRenderRequest) {
        self.wanted_request = Some(request.id());
    }

    /// Clears the slot so later preparation results cannot become live.
    pub fn release(&mut self) {
        self.wanted_request = None;
        self.live_request = None;
    }

    /// Accepts a preparation outcome only when it matches the newest wanted request.
    pub fn accept_ready(
        &mut self,
        outcome: ImagePreparationOutcome,
    ) -> Result<PreparedImageUpload, ImagePreparationOutcome> {
        let request_id = outcome.request_id();
        if self.wanted_request != Some(request_id) {
            return Err(outcome);
        }

        match outcome {
            ImagePreparationOutcome::Ready(upload) => {
                self.live_request = Some(request_id);
                Ok(upload)
            }
            failed @ ImagePreparationOutcome::Failed(_) => Err(failed),
        }
    }

    /// Returns the currently live request, if any.
    pub fn live_request(&self) -> Option<ImageRenderRequestId> {
        self.live_request
    }
}

/// Spawns source-backed image preparation with default limits on the background executor.
pub fn spawn_image_upload_preparation(
    executor: &BackgroundExecutor,
    svg_renderer: SvgRenderer,
    source: ImageRenderSource,
    request: ImageRenderRequest,
) -> Task<ImagePreparationOutcome> {
    spawn_image_upload_preparation_with_limits(
        executor,
        svg_renderer,
        source,
        request,
        ImagePreparationLimits::default(),
    )
}

/// Spawns source-backed image preparation with explicit limits on the background executor.
pub fn spawn_image_upload_preparation_with_limits(
    executor: &BackgroundExecutor,
    svg_renderer: SvgRenderer,
    source: ImageRenderSource,
    request: ImageRenderRequest,
    limits: ImagePreparationLimits,
) -> Task<ImagePreparationOutcome> {
    let executor_for_task = executor.clone();
    executor.spawn(async move {
        #[cfg(feature = "test-support")]
        let prepared_on_main_thread = executor_for_task.is_main_thread();

        #[cfg(not(feature = "test-support"))]
        let _executor_for_task = executor_for_task;

        prepare_image_upload_blocking(
            svg_renderer,
            source,
            request,
            limits,
            #[cfg(feature = "test-support")]
            prepared_on_main_thread,
        )
    })
}

fn prepare_image_upload_blocking(
    svg_renderer: SvgRenderer,
    source: ImageRenderSource,
    request: ImageRenderRequest,
    limits: ImagePreparationLimits,
    #[cfg(feature = "test-support")] prepared_on_main_thread: bool,
) -> ImagePreparationOutcome {
    if source.id() != request.source_id() {
        return failed(request, ImagePreparationFailureKind::SourceMismatch);
    }

    let requested_size = request.requested_size();
    if requested_size.width.0 <= 0 || requested_size.height.0 <= 0 {
        return failed(
            request,
            ImagePreparationFailureKind::InvalidRequestedSize(requested_size),
        );
    }

    let bytes = match source {
        ImageRenderSource::File(path) => match fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return failed(
                    request,
                    ImagePreparationFailureKind::Unavailable(error.into()),
                );
            }
        },
        ImageRenderSource::Bytes(bytes) => {
            return prepare_bytes(
                svg_renderer,
                bytes.as_ref(),
                request,
                limits,
                #[cfg(feature = "test-support")]
                prepared_on_main_thread,
            );
        }
    };

    prepare_bytes(
        svg_renderer,
        &bytes,
        request,
        limits,
        #[cfg(feature = "test-support")]
        prepared_on_main_thread,
    )
}

fn prepare_bytes(
    svg_renderer: SvgRenderer,
    bytes: &[u8],
    request: ImageRenderRequest,
    limits: ImagePreparationLimits,
    #[cfg(feature = "test-support")] prepared_on_main_thread: bool,
) -> ImagePreparationOutcome {
    match image::guess_format(bytes) {
        Ok(format) => prepare_raster_bytes(
            bytes,
            format,
            request,
            limits,
            #[cfg(feature = "test-support")]
            prepared_on_main_thread,
        ),
        Err(_) => prepare_svg_bytes(
            svg_renderer,
            bytes,
            request,
            limits,
            #[cfg(feature = "test-support")]
            prepared_on_main_thread,
        ),
    }
}

fn prepare_raster_bytes(
    bytes: &[u8],
    format: ImageFormat,
    request: ImageRenderRequest,
    limits: ImagePreparationLimits,
    #[cfg(feature = "test-support")] prepared_on_main_thread: bool,
) -> ImagePreparationOutcome {
    let source_size = match raster_dimensions(bytes, format) {
        Ok(size) => size,
        Err(error) => return failed(request, classify_raster_error(error)),
    };

    if pixel_count(source_size) > limits.max_source_pixels {
        return failed(
            request,
            ImagePreparationFailureKind::TooLarge {
                source_size,
                requested_size: request.requested_size(),
                max_pixels: limits.max_source_pixels,
            },
        );
    }

    let frame = match decode_raster_frame(bytes, format, request.frame_index()) {
        Ok(frame) => frame,
        Err(DecodeFrameError::Image(error)) => {
            return failed(request, classify_decode_error(error));
        }
        Err(DecodeFrameError::FrameUnavailable) => {
            return failed(
                request,
                ImagePreparationFailureKind::FrameUnavailable {
                    frame_index: request.frame_index(),
                },
            );
        }
    };

    let target_size = fit_size(source_size, request.requested_size());
    if pixel_count(target_size) > limits.max_upload_pixels {
        return failed(
            request,
            ImagePreparationFailureKind::TooLarge {
                source_size,
                requested_size: request.requested_size(),
                max_pixels: limits.max_upload_pixels,
            },
        );
    }

    ready(
        request,
        source_size,
        downsample_to_bgra(frame, target_size),
        #[cfg(feature = "test-support")]
        prepared_on_main_thread,
    )
}

fn prepare_svg_bytes(
    svg_renderer: SvgRenderer,
    bytes: &[u8],
    request: ImageRenderRequest,
    limits: ImagePreparationLimits,
    #[cfg(feature = "test-support")] prepared_on_main_thread: bool,
) -> ImagePreparationOutcome {
    if request.frame_index() != 0 {
        return failed(
            request,
            ImagePreparationFailureKind::FrameUnavailable {
                frame_index: request.frame_index(),
            },
        );
    }

    let requested_size = request.requested_size();
    if pixel_count(requested_size) > limits.max_upload_pixels {
        return failed(
            request,
            ImagePreparationFailureKind::TooLarge {
                source_size: requested_size,
                requested_size,
                max_pixels: limits.max_upload_pixels,
            },
        );
    }

    let pixmap = match svg_renderer.render_pixmap(bytes, SvgSize::Size(requested_size)) {
        Ok(pixmap) => pixmap,
        Err(error) => {
            return failed(
                request,
                ImagePreparationFailureKind::Unsupported(error.into()),
            );
        }
    };

    let source_size = size(
        DevicePixels(pixmap.width() as i32),
        DevicePixels(pixmap.height() as i32),
    );
    let mut buffer = RgbaImage::from_raw(pixmap.width(), pixmap.height(), pixmap.take())
        .expect("tiny-skia pixmap dimensions match its backing buffer");
    let target_size = fit_size(source_size, requested_size);

    if target_size != source_size {
        buffer = imageops::resize(
            &buffer,
            target_size.width.0 as u32,
            target_size.height.0 as u32,
            imageops::FilterType::Triangle,
        );
    }

    for pixel in buffer.chunks_exact_mut(4) {
        swap_rgba_pa_to_bgra(pixel);
    }

    ready(
        request,
        source_size,
        PreparedPixels {
            size: target_size,
            pixels: buffer.into_raw(),
        },
        #[cfg(feature = "test-support")]
        prepared_on_main_thread,
    )
}

struct PreparedPixels {
    size: Size<DevicePixels>,
    pixels: Vec<u8>,
}

enum DecodeFrameError {
    Image(image::ImageError),
    FrameUnavailable,
}

fn raster_dimensions(
    bytes: &[u8],
    format: ImageFormat,
) -> Result<Size<DevicePixels>, image::ImageError> {
    let (width, height) = ImageReader::with_format(Cursor::new(bytes), format).into_dimensions()?;
    Ok(size(
        DevicePixels(width as i32),
        DevicePixels(height as i32),
    ))
}

fn decode_raster_frame(
    bytes: &[u8],
    format: ImageFormat,
    frame_index: usize,
) -> Result<RgbaImage, DecodeFrameError> {
    match format {
        ImageFormat::Gif => {
            use image::{AnimationDecoder, codecs::gif::GifDecoder};

            let decoder = GifDecoder::new(Cursor::new(bytes)).map_err(DecodeFrameError::Image)?;
            let frame = decoder
                .into_frames()
                .nth(frame_index)
                .ok_or(DecodeFrameError::FrameUnavailable)?
                .map_err(DecodeFrameError::Image)?;
            Ok(frame.into_buffer())
        }
        ImageFormat::WebP => {
            use image::{AnimationDecoder, codecs::webp::WebPDecoder};

            let mut decoder =
                WebPDecoder::new(Cursor::new(bytes)).map_err(DecodeFrameError::Image)?;
            if decoder.has_animation() {
                let _ = decoder.set_background_color(image::Rgba([0, 0, 0, 0]));
                let frame = decoder
                    .into_frames()
                    .nth(frame_index)
                    .ok_or(DecodeFrameError::FrameUnavailable)?
                    .map_err(DecodeFrameError::Image)?;
                Ok(frame.into_buffer())
            } else if frame_index == 0 {
                DynamicImage::from_decoder(decoder)
                    .map(DynamicImage::into_rgba8)
                    .map_err(DecodeFrameError::Image)
            } else {
                Err(DecodeFrameError::FrameUnavailable)
            }
        }
        _ if frame_index == 0 => ImageReader::with_format(Cursor::new(bytes), format)
            .decode()
            .map(DynamicImage::into_rgba8)
            .map_err(DecodeFrameError::Image),
        _ => Err(DecodeFrameError::FrameUnavailable),
    }
}

fn downsample_to_bgra(mut frame: RgbaImage, target_size: Size<DevicePixels>) -> PreparedPixels {
    let current_size = size(
        DevicePixels(frame.width() as i32),
        DevicePixels(frame.height() as i32),
    );

    if target_size != current_size {
        frame = imageops::resize(
            &frame,
            target_size.width.0 as u32,
            target_size.height.0 as u32,
            imageops::FilterType::Triangle,
        );
    }

    for pixel in frame.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }

    PreparedPixels {
        size: target_size,
        pixels: frame.into_raw(),
    }
}

fn fit_size(
    source_size: Size<DevicePixels>,
    requested_size: Size<DevicePixels>,
) -> Size<DevicePixels> {
    let source_width = source_size.width.0.max(1) as u32;
    let source_height = source_size.height.0.max(1) as u32;
    let requested_width = requested_size.width.0.max(1) as u32;
    let requested_height = requested_size.height.0.max(1) as u32;

    if source_width <= requested_width && source_height <= requested_height {
        return source_size;
    }

    let scale = (requested_width as f32 / source_width as f32)
        .min(requested_height as f32 / source_height as f32)
        .min(1.0);
    let width = ((source_width as f32 * scale).round() as u32)
        .max(1)
        .min(requested_width);
    let height = ((source_height as f32 * scale).round() as u32)
        .max(1)
        .min(requested_height);

    size(DevicePixels(width as i32), DevicePixels(height as i32))
}

fn pixel_count(size: Size<DevicePixels>) -> u64 {
    (size.width.0.max(0) as u64).saturating_mul(size.height.0.max(0) as u64)
}

fn ready(
    request: ImageRenderRequest,
    source_size: Size<DevicePixels>,
    prepared: PreparedPixels,
    #[cfg(feature = "test-support")] prepared_on_main_thread: bool,
) -> ImagePreparationOutcome {
    ImagePreparationOutcome::Ready(PreparedImageUpload {
        request,
        source_size,
        size: prepared.size,
        pixels: prepared.pixels,
        #[cfg(feature = "test-support")]
        prepared_on_main_thread,
    })
}

fn failed(
    request: ImageRenderRequest,
    kind: ImagePreparationFailureKind,
) -> ImagePreparationOutcome {
    ImagePreparationOutcome::Failed(ImagePreparationFailure::new(request, kind))
}

fn classify_raster_error(error: image::ImageError) -> ImagePreparationFailureKind {
    match error {
        image::ImageError::Unsupported(_) => ImagePreparationFailureKind::Unsupported(error.into()),
        _ => ImagePreparationFailureKind::DecodeFailed(error.into()),
    }
}

fn classify_decode_error(error: image::ImageError) -> ImagePreparationFailureKind {
    match error {
        image::ImageError::Unsupported(_) => ImagePreparationFailureKind::Unsupported(error.into()),
        _ => ImagePreparationFailureKind::DecodeFailed(error.into()),
    }
}
