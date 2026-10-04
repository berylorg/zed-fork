use super::{ClipboardError, ClipboardLimits, Format, NativeClipboard, add, check};
use crate::{ClipboardItem, Image, ImageFormat};

const RECORD_BYTES: usize = 24;
const VERSION: u32 = 1;

fn format_number(format: ImageFormat) -> Result<u32, ClipboardError> {
    match format {
        ImageFormat::Png => Ok(1),
        ImageFormat::Gif => Ok(2),
        ImageFormat::Jpeg => Ok(3),
        ImageFormat::Svg => Ok(4),
        _ => Err(ClipboardError::Unsupported),
    }
}

fn record_length(
    record: &[u8],
    format: ImageFormat,
    image: &[u8],
) -> Result<usize, ClipboardError> {
    let record = record
        .get(..RECORD_BYTES)
        .ok_or(ClipboardError::Malformed)?;
    let version = u32::from_le_bytes(
        record[0..4]
            .try_into()
            .map_err(|_| ClipboardError::Malformed)?,
    );
    let encoded_format = u32::from_le_bytes(
        record[4..8]
            .try_into()
            .map_err(|_| ClipboardError::Malformed)?,
    );
    let length = u64::from_le_bytes(
        record[8..16]
            .try_into()
            .map_err(|_| ClipboardError::Malformed)?,
    );
    let digest = u64::from_le_bytes(
        record[16..24]
            .try_into()
            .map_err(|_| ClipboardError::Malformed)?,
    );
    if version != VERSION || encoded_format != format_number(format)? {
        return Err(ClipboardError::Malformed);
    }
    let length = usize::try_from(length).map_err(|_| ClipboardError::Malformed)?;
    if length == 0 || length > image.len() {
        return Err(ClipboardError::Malformed);
    }
    // A content consistency check only; this digest grants no application provenance.
    if seahash::hash(&image[..length]) != digest {
        return Err(ClipboardError::Malformed);
    }
    Ok(length)
}

pub(super) fn read<N: NativeClipboard>(
    native: &mut N,
    format: ImageFormat,
    image: &[u8],
    limits: ClipboardLimits,
) -> Result<ClipboardItem, ClipboardError> {
    if image.is_empty() {
        return Err(ClipboardError::Empty);
    }
    let companion = if native.available(Format::ImageMetadata)? {
        let maximum = limits
            .metadata_bytes
            .min(limits.total_bytes.saturating_sub(image.len()));
        let data = native.data(Format::ImageMetadata, maximum)?;
        check(data.as_ref().len(), maximum)?;
        Some(data)
    } else {
        None
    };
    let length = if let Some(companion) = &companion {
        record_length(companion.as_ref(), format, image)?
    } else {
        image.len()
    };
    let image_cost = add(image.len(), length)?;
    check(image_cost, limits.image_bytes)?;
    let companion_cost = companion.as_ref().map_or(0, |data| data.as_ref().len());
    check(companion_cost, limits.metadata_bytes)?;
    check(add(image_cost, companion_cost)?, limits.total_bytes)?;
    native.before_output_allocation()?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| ClipboardError::Allocation)?;
    if bytes.capacity() > length {
        return Err(ClipboardError::OverLimit);
    }
    bytes.extend_from_slice(&image[..length]);
    Ok(ClipboardItem::from(Image::from_bytes(format, bytes)))
}

pub(super) fn prepare<N: NativeClipboard>(
    native: &mut N,
    image: &Image,
    limits: ClipboardLimits,
) -> Result<[(Format, N::Buffer); 2], ClipboardError> {
    let format = format_number(image.format())?;
    let length = image.bytes().len();
    if length == 0 {
        return Err(ClipboardError::Empty);
    }
    let encoded_length = u64::try_from(length).map_err(|_| ClipboardError::OverLimit)?;
    let requested_image_cost = add(length, length)?;
    check(requested_image_cost, limits.image_bytes)?;
    check(RECORD_BYTES, limits.metadata_bytes)?;
    check(add(requested_image_cost, RECORD_BYTES)?, limits.total_bytes)?;
    let mut image_buffer = native.allocate(length)?;
    if image_buffer.as_ref().len() < length {
        return Err(ClipboardError::Allocation);
    }
    let actual_image_cost = add(image_buffer.as_ref().len(), length)?;
    check(actual_image_cost, limits.image_bytes)?;
    check(add(actual_image_cost, RECORD_BYTES)?, limits.total_bytes)?;
    let mut companion = native.allocate(RECORD_BYTES)?;
    if companion.as_ref().len() < RECORD_BYTES {
        return Err(ClipboardError::Allocation);
    }
    check(companion.as_ref().len(), limits.metadata_bytes)?;
    check(
        add(actual_image_cost, companion.as_ref().len())?,
        limits.total_bytes,
    )?;
    image_buffer.as_mut().fill(0);
    image_buffer.as_mut()[..length].copy_from_slice(image.bytes());
    companion.as_mut().fill(0);
    let record = companion.as_mut();
    record[0..4].copy_from_slice(&VERSION.to_le_bytes());
    record[4..8].copy_from_slice(&format.to_le_bytes());
    record[8..16].copy_from_slice(&encoded_length.to_le_bytes());
    record[16..24].copy_from_slice(&seahash::hash(image.bytes()).to_le_bytes());
    Ok([
        (Format::Image(image.format()), image_buffer),
        (Format::ImageMetadata, companion),
    ])
}
