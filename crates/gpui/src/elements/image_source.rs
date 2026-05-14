use crate::{DevicePixels, Size, hash};
use std::{
    fmt,
    hash::{Hash, Hasher},
    path::PathBuf,
    sync::Arc,
};

/// Stable identity for a reloadable or caller-owned image source.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ImageSourceId(u64);

impl ImageSourceId {
    /// Returns this image source identity as an opaque integer.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// Stable identity for one requested rendering of an image source.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ImageRenderRequestId(u64);

impl ImageRenderRequestId {
    /// Returns this render request identity as an opaque integer.
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// A reloadable or caller-owned image source for render-size-aware image requests.
#[derive(Clone)]
pub enum ImageRenderSource {
    /// Image bytes can be reloaded from this filesystem path.
    File(PathBuf),
    /// Image bytes are retained by GPUI through a caller-supplied immutable byte buffer.
    Bytes(Arc<[u8]>),
}

impl ImageRenderSource {
    /// Creates a file-backed image source.
    pub fn file(path: impl Into<PathBuf>) -> Self {
        Self::File(path.into())
    }

    /// Creates an immutable byte-backed image source.
    pub fn bytes(bytes: Arc<[u8]>) -> Self {
        Self::Bytes(bytes)
    }

    /// Returns the stable source identity used for request keys.
    pub fn id(&self) -> ImageSourceId {
        ImageSourceId(hash(self))
    }

    /// Returns whether this source can be reloaded without retaining caller bytes.
    pub fn is_reloadable(&self) -> bool {
        matches!(self, Self::File(_))
    }

    /// Returns the byte count intentionally retained by this source, if any.
    pub fn retained_byte_len(&self) -> Option<usize> {
        match self {
            Self::File(_) => None,
            Self::Bytes(bytes) => Some(bytes.len()),
        }
    }

    /// Builds a render request identity for a concrete image frame and device-pixel size.
    pub fn render_request(
        &self,
        frame_index: usize,
        scale_factor: f32,
        requested_size: Size<DevicePixels>,
    ) -> ImageRenderRequest {
        ImageRenderRequest::new(self.id(), frame_index, scale_factor, requested_size)
    }
}

impl PartialEq for ImageRenderSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(left), Self::File(right)) => left == right,
            (Self::Bytes(left), Self::Bytes(right)) => {
                Arc::ptr_eq(left, right) && left.len() == right.len()
            }
            _ => false,
        }
    }
}

impl Eq for ImageRenderSource {}

impl Hash for ImageRenderSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match self {
            Self::File(path) => {
                0u8.hash(state);
                path.hash(state);
            }
            Self::Bytes(bytes) => {
                1u8.hash(state);
                bytes.as_ptr().hash(state);
                bytes.len().hash(state);
            }
        }
    }
}

impl fmt::Debug for ImageRenderSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::File(path) => f.debug_tuple("File").field(path).finish(),
            Self::Bytes(bytes) => f
                .debug_struct("Bytes")
                .field("ptr", &bytes.as_ptr())
                .field("len", &bytes.len())
                .finish(),
        }
    }
}

/// A render-size-aware request for one image source and frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ImageRenderRequest {
    id: ImageRenderRequestId,
    source_id: ImageSourceId,
    frame_index: usize,
    scale_factor_bits: u32,
    requested_width: DevicePixels,
    requested_height: DevicePixels,
}

impl ImageRenderRequest {
    /// Creates a render request from source identity, frame, scale, and device-pixel size.
    pub fn new(
        source_id: ImageSourceId,
        frame_index: usize,
        scale_factor: f32,
        requested_size: Size<DevicePixels>,
    ) -> Self {
        let scale_factor_bits = scale_factor.to_bits();
        let requested_width = canonical_source_backed_request_dimension(requested_size.width);
        let requested_height = canonical_source_backed_request_dimension(requested_size.height);
        let key = (
            source_id,
            frame_index,
            scale_factor_bits,
            requested_width,
            requested_height,
        );

        Self {
            id: ImageRenderRequestId(hash(&key)),
            source_id,
            frame_index,
            scale_factor_bits,
            requested_width,
            requested_height,
        }
    }

    /// Returns this request identity.
    pub fn id(&self) -> ImageRenderRequestId {
        self.id
    }

    /// Returns the source identity used by this request.
    pub fn source_id(&self) -> ImageSourceId {
        self.source_id
    }

    /// Returns the requested frame index.
    pub fn frame_index(&self) -> usize {
        self.frame_index
    }

    /// Returns the scale factor bits used in this request key.
    pub fn scale_factor_bits(&self) -> u32 {
        self.scale_factor_bits
    }

    /// Returns the scale factor used in this request key.
    pub fn scale_factor(&self) -> f32 {
        f32::from_bits(self.scale_factor_bits)
    }

    /// Returns the requested device-pixel size.
    pub fn requested_size(&self) -> Size<DevicePixels> {
        Size::new(self.requested_width, self.requested_height)
    }
}

fn canonical_source_backed_request_dimension(value: DevicePixels) -> DevicePixels {
    if value.0 <= 0 || value.0 % 2 == 0 {
        return value;
    }

    DevicePixels(value.0.saturating_add(1))
}
