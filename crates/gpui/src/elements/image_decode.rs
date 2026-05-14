use crate::{RenderImage, SMOOTH_SVG_SCALE_FACTOR, SvgRenderer, SvgSize, swap_rgba_pa_to_bgra};
use image::{
    AnimationDecoder, DynamicImage, Frame, ImageBuffer, ImageFormat, Rgba,
    codecs::{gif::GifDecoder, webp::WebPDecoder},
};
use smallvec::SmallVec;
use std::io::Cursor;

use super::img::ImageCacheError;

/// Decodes encoded image bytes into the current CPU-side render image representation.
pub(crate) fn decode_image_bytes(
    bytes: &[u8],
    svg_renderer: SvgRenderer,
) -> std::result::Result<RenderImage, ImageCacheError> {
    let data = if let Ok(format) = image::guess_format(bytes) {
        match format {
            ImageFormat::Gif => {
                let decoder = GifDecoder::new(Cursor::new(bytes))?;
                let mut frames = SmallVec::new();

                for frame in decoder.into_frames() {
                    let mut frame = frame?;
                    // Convert from RGBA to BGRA.
                    for pixel in frame.buffer_mut().chunks_exact_mut(4) {
                        pixel.swap(0, 2);
                    }
                    frames.push(frame);
                }

                RenderImage::new(frames)
            }
            ImageFormat::WebP => {
                let mut decoder = WebPDecoder::new(Cursor::new(bytes))?;

                if decoder.has_animation() {
                    let _ = decoder.set_background_color(Rgba([0, 0, 0, 0]));
                    let mut frames = SmallVec::new();

                    for frame in decoder.into_frames() {
                        let mut frame = frame?;
                        // Convert from RGBA to BGRA.
                        for pixel in frame.buffer_mut().chunks_exact_mut(4) {
                            pixel.swap(0, 2);
                        }
                        frames.push(frame);
                    }

                    RenderImage::new(frames)
                } else {
                    let mut data = DynamicImage::from_decoder(decoder)?.into_rgba8();

                    // Convert from RGBA to BGRA.
                    for pixel in data.chunks_exact_mut(4) {
                        pixel.swap(0, 2);
                    }

                    RenderImage::new(SmallVec::from_elem(Frame::new(data), 1))
                }
            }
            _ => {
                let mut data = image::load_from_memory_with_format(bytes, format)?.into_rgba8();

                // Convert from RGBA to BGRA.
                for pixel in data.chunks_exact_mut(4) {
                    pixel.swap(0, 2);
                }

                RenderImage::new(SmallVec::from_elem(Frame::new(data), 1))
            }
        }
    } else {
        let pixmap =
            svg_renderer.render_pixmap(bytes, SvgSize::ScaleFactor(SMOOTH_SVG_SCALE_FACTOR))?;

        let mut buffer =
            ImageBuffer::from_raw(pixmap.width(), pixmap.height(), pixmap.take()).unwrap();

        for pixel in buffer.chunks_exact_mut(4) {
            swap_rgba_pa_to_bgra(pixel);
        }

        let mut image = RenderImage::new(SmallVec::from_elem(Frame::new(buffer), 1));
        image.scale_factor = SMOOTH_SVG_SCALE_FACTOR;
        image
    };

    Ok(data)
}
