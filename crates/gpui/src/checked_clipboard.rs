use std::hash::{Hash, Hasher};

use crate::{ClipboardEntry, ClipboardItem, ImageFormat};

#[path = "checked_clipboard/encoded_image.rs"]
mod encoded_image;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClipboardLimits {
    pub total_bytes: usize,
    pub text_bytes: usize,
    pub metadata_bytes: usize,
    pub image_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardRepresentation {
    Text,
    Metadata,
    TextHash,
    Image,
    ImageMetadata,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardError {
    Unavailable,
    InvalidLimits,
    Empty,
    NoValue,
    Unsupported,
    Malformed,
    OverLimit,
    Ownership,
    Allocation,
    Read,
    Write(ClipboardRepresentation),
    Close,
    SnapshotChanged,
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "checked clipboard operation failed: {self:?}")
    }
}

impl std::error::Error for ClipboardError {}

#[derive(Debug)]
pub struct CheckedClipboardSnapshot {
    pub item: ClipboardItem,
    pub sequence: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Format {
    Text,
    Metadata,
    TextHash,
    Image(ImageFormat),
    ImageMetadata,
}

impl Format {
    pub(crate) fn representation(self) -> ClipboardRepresentation {
        match self {
            Self::Text => ClipboardRepresentation::Text,
            Self::Metadata => ClipboardRepresentation::Metadata,
            Self::TextHash => ClipboardRepresentation::TextHash,
            Self::Image(_) => ClipboardRepresentation::Image,
            Self::ImageMetadata => ClipboardRepresentation::ImageMetadata,
        }
    }
}

pub(crate) trait NativeClipboard {
    type Data: AsRef<[u8]>;
    type Buffer: AsRef<[u8]> + AsMut<[u8]>;

    fn open(&mut self) -> Result<(), ClipboardError>;
    fn close(&mut self) -> Result<(), ClipboardError>;
    fn sequence(&self) -> Result<u32, ClipboardError>;
    fn first_supported(&mut self) -> Result<Format, ClipboardError>;
    fn available(&mut self, format: Format) -> Result<bool, ClipboardError>;
    fn data(&mut self, format: Format, maximum: usize) -> Result<Self::Data, ClipboardError>;
    fn allocate(&mut self, bytes: usize) -> Result<Self::Buffer, ClipboardError>;
    fn empty(&mut self) -> Result<(), ClipboardError>;
    fn publish(&mut self, format: Format, buffer: Self::Buffer) -> Result<(), ClipboardError>;
    fn before_output_allocation(&mut self) -> Result<(), ClipboardError> {
        Ok(())
    }
}

struct Session<'a, N: NativeClipboard> {
    native: &'a mut N,
    open: bool,
}

impl<'a, N: NativeClipboard> Session<'a, N> {
    fn new(native: &'a mut N) -> Result<Self, ClipboardError> {
        native.open()?;
        Ok(Self { native, open: true })
    }

    fn finish<T>(mut self, result: Result<T, ClipboardError>) -> Result<T, ClipboardError> {
        let close = self.native.close();
        // Drop makes one final release attempt if the explicit close failed.
        self.open = close.is_err();
        close?;
        result
    }
}

impl<N: NativeClipboard> Drop for Session<'_, N> {
    fn drop(&mut self) {
        if self.open {
            let _ = self.native.close();
        }
    }
}

impl ClipboardLimits {
    pub(crate) fn validate(self) -> Result<Self, ClipboardError> {
        if [
            self.total_bytes,
            self.text_bytes,
            self.metadata_bytes,
            self.image_bytes,
        ]
        .contains(&0)
        {
            Err(ClipboardError::InvalidLimits)
        } else {
            Ok(self)
        }
    }
}

fn add(a: usize, b: usize) -> Result<usize, ClipboardError> {
    a.checked_add(b).ok_or(ClipboardError::OverLimit)
}

fn check(bytes: usize, ceiling: usize) -> Result<(), ClipboardError> {
    if bytes > ceiling || bytes > isize::MAX as usize {
        Err(ClipboardError::OverLimit)
    } else {
        Ok(())
    }
}

fn text_hash(text: &str) -> u64 {
    let mut hasher = seahash::SeaHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn native_text_length(text: &str) -> Result<usize, ClipboardError> {
    if text.contains('\0') {
        return Err(ClipboardError::Malformed);
    }
    add(text.encode_utf16().count(), 1)?
        .checked_mul(2)
        .ok_or(ClipboardError::OverLimit)
}

fn inspect_text(bytes: &[u8]) -> Result<(usize, usize), ClipboardError> {
    if bytes.is_empty() || bytes.len() % 2 != 0 {
        return Err(ClipboardError::Malformed);
    }
    let units = bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]));
    let length = units
        .clone()
        .position(|unit| unit == 0)
        .ok_or(ClipboardError::Malformed)?;
    let mut utf8 = 0;
    for ch in char::decode_utf16(units.take(length)) {
        utf8 = add(utf8, ch.map_err(|_| ClipboardError::Malformed)?.len_utf8())?;
    }
    Ok((length, utf8))
}

fn copy_text(bytes: &[u8], units: usize, capacity: usize) -> Result<String, ClipboardError> {
    let mut text = String::new();
    text.try_reserve_exact(capacity)
        .map_err(|_| ClipboardError::Allocation)?;
    if text.capacity() > capacity {
        return Err(ClipboardError::OverLimit);
    }
    for ch in char::decode_utf16(
        bytes
            .chunks_exact(2)
            .take(units)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]])),
    ) {
        text.push(ch.map_err(|_| ClipboardError::Malformed)?);
    }
    Ok(text)
}

pub(crate) fn read<N: NativeClipboard>(
    native: &mut N,
    limits: ClipboardLimits,
) -> Result<CheckedClipboardSnapshot, ClipboardError> {
    let limits = limits.validate()?;
    let session = Session::new(native)?;
    let result = read_snapshot(session.native, limits);
    session.finish(result)
}

fn read_snapshot<N: NativeClipboard>(
    native: &mut N,
    limits: ClipboardLimits,
) -> Result<CheckedClipboardSnapshot, ClipboardError> {
    let sequence = native.sequence()?;
    let format = native.first_supported()?;
    let ceiling = match format {
        Format::Text => limits.text_bytes,
        Format::Image(_) => limits.image_bytes,
        _ => return Err(ClipboardError::Unsupported),
    };
    let data = native.data(format, ceiling.min(limits.total_bytes))?;
    check(data.as_ref().len(), ceiling.min(limits.total_bytes))?;
    let item = match format {
        Format::Text => {
            let (units, capacity) = inspect_text(data.as_ref())?;
            if units == 0 {
                return Err(ClipboardError::Empty);
            }
            let text_cost = add(data.as_ref().len(), capacity)?;
            check(text_cost, limits.text_bytes.min(limits.total_bytes))?;
            let has_metadata = native.available(Format::Metadata)?;
            let has_hash = native.available(Format::TextHash)?;
            if has_metadata != has_hash {
                return Err(ClipboardError::Malformed);
            }
            let metadata = if has_metadata {
                let metadata = native.data(
                    Format::Metadata,
                    limits.metadata_bytes.min(limits.total_bytes),
                )?;
                let hash = native.data(Format::TextHash, 8)?;
                if hash.as_ref().len() != 8 {
                    return Err(ClipboardError::Malformed);
                }
                check(
                    metadata.as_ref().len(),
                    limits.metadata_bytes.min(limits.total_bytes),
                )?;
                let (metadata_units, metadata_capacity) = inspect_text(metadata.as_ref())?;
                let metadata_cost = add(metadata.as_ref().len(), metadata_capacity)?;
                check(metadata_cost, limits.metadata_bytes)?;
                check(add(add(text_cost, metadata_cost)?, 8)?, limits.total_bytes)?;
                Some((
                    metadata,
                    metadata_units,
                    metadata_capacity,
                    u64::from_ne_bytes(
                        hash.as_ref()
                            .try_into()
                            .map_err(|_| ClipboardError::Malformed)?,
                    ),
                ))
            } else {
                None
            };
            native.before_output_allocation()?;
            let text = copy_text(data.as_ref(), units, capacity)?;
            if let Some((metadata, units, capacity, expected_hash)) = metadata {
                if text_hash(&text) != expected_hash {
                    return Err(ClipboardError::Malformed);
                }
                ClipboardItem::new_string_with_metadata(
                    text,
                    copy_text(metadata.as_ref(), units, capacity)?,
                )
            } else {
                ClipboardItem::new_string(text)
            }
        }
        Format::Image(format) => encoded_image::read(native, format, data.as_ref(), limits)?,
        _ => return Err(ClipboardError::Unsupported),
    };
    if native.sequence()? != sequence {
        return Err(ClipboardError::SnapshotChanged);
    }
    Ok(CheckedClipboardSnapshot { item, sequence })
}

fn fill_text(buffer: &mut [u8], text: &str) {
    for (pair, unit) in buffer
        .chunks_exact_mut(2)
        .zip(text.encode_utf16().chain(Some(0)))
    {
        pair.copy_from_slice(&unit.to_le_bytes());
    }
}

pub(crate) fn write<N: NativeClipboard>(
    native: &mut N,
    item: &ClipboardItem,
    limits: ClipboardLimits,
) -> Result<(), ClipboardError> {
    let limits = limits.validate()?;
    let mut buffers = [None, None, None];
    match item.entries() {
        [ClipboardEntry::String(text)] => {
            if text.text().is_empty() {
                return Err(ClipboardError::Empty);
            }
            let text_length = native_text_length(text.text())?;
            let text_cost = add(text_length, text.text().len())?;
            check(text_cost, limits.text_bytes.min(limits.total_bytes))?;
            let metadata = item
                .metadata()
                .map(|text| native_text_length(text).map(|bytes| (text, bytes)))
                .transpose()?;
            let metadata_cost = metadata
                .map(|(text, bytes)| add(bytes, text.len()))
                .transpose()?
                .unwrap_or(0);
            check(metadata_cost, limits.metadata_bytes)?;
            check(
                add(
                    add(text_cost, metadata_cost)?,
                    if metadata.is_some() { 8 } else { 0 },
                )?,
                limits.total_bytes,
            )?;
            let mut buffer = native.allocate(text_length)?;
            let mut actual_cost = add(buffer.as_ref().len(), text.text().len())?;
            check(actual_cost, limits.text_bytes.min(limits.total_bytes))?;
            check(
                add(
                    add(actual_cost, metadata_cost)?,
                    if metadata.is_some() { 8 } else { 0 },
                )?,
                limits.total_bytes,
            )?;
            if buffer.as_ref().len() < text_length {
                return Err(ClipboardError::Allocation);
            }
            fill_text(buffer.as_mut(), text.text());
            buffers[0] = Some((Format::Text, buffer));
            if let Some((metadata, bytes)) = metadata {
                let mut buffer = native.allocate(bytes)?;
                let cost = add(buffer.as_ref().len(), metadata.len())?;
                check(cost, limits.metadata_bytes)?;
                actual_cost = add(actual_cost, cost)?;
                check(add(actual_cost, 8)?, limits.total_bytes)?;
                if buffer.as_ref().len() < bytes {
                    return Err(ClipboardError::Allocation);
                }
                fill_text(buffer.as_mut(), metadata);
                buffers[1] = Some((Format::Metadata, buffer));
                let mut buffer = native.allocate(8)?;
                check(buffer.as_ref().len(), 8)?;
                if buffer.as_ref().len() != 8 {
                    return Err(ClipboardError::Allocation);
                }
                buffer
                    .as_mut()
                    .copy_from_slice(&text_hash(text.text()).to_ne_bytes());
                actual_cost = add(actual_cost, buffer.as_ref().len())?;
                buffers[2] = Some((Format::TextHash, buffer));
            }
            check(actual_cost, limits.total_bytes)?;
        }
        [ClipboardEntry::Image(image)] => {
            let [image, companion] = encoded_image::prepare(native, image, limits)?;
            buffers[0] = Some(image);
            buffers[1] = Some(companion);
        }
        [] => return Err(ClipboardError::Empty),
        _ => return Err(ClipboardError::Unsupported),
    }
    let session = Session::new(native)?;
    let result = (|| {
        session.native.empty()?;
        for (format, buffer) in buffers.into_iter().flatten() {
            session.native.publish(format, buffer)?;
        }
        Ok(())
    })();
    session.finish(result)
}
