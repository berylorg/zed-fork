use super::{
    Pixels, Point, StreamingLayoutComponent, StreamingLayoutError, StreamingLayoutMetric, point,
};
use crate::px;

pub(crate) fn validate_positive(
    value: Pixels,
    metric: StreamingLayoutMetric,
) -> Result<(), StreamingLayoutError> {
    let value = f32::from(value);
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(StreamingLayoutError::InvalidMetric(metric))
    }
}

pub(super) fn validate_point(
    value: Point<Pixels>,
    metric: StreamingLayoutMetric,
) -> Result<(), StreamingLayoutError> {
    let x = f32::from(value.x);
    let y = f32::from(value.y);
    if x.is_finite() && y.is_finite() {
        Ok(())
    } else {
        Err(StreamingLayoutError::InvalidMetric(metric))
    }
}

pub(crate) fn checked_point_add(
    left: Point<Pixels>,
    right: Point<Pixels>,
    component: StreamingLayoutComponent,
) -> Result<Point<Pixels>, StreamingLayoutError> {
    Ok(point(
        checked_pixel_add_signed(left.x, right.x, component)?,
        checked_pixel_add_signed(left.y, right.y, component)?,
    ))
}

pub(super) fn checked_point_sub(
    left: Point<Pixels>,
    right: Point<Pixels>,
    component: StreamingLayoutComponent,
) -> Result<Point<Pixels>, StreamingLayoutError> {
    Ok(point(
        checked_pixel_sub_signed(left.x, right.x, component)?,
        checked_pixel_sub_signed(left.y, right.y, component)?,
    ))
}

fn checked_pixel_add_signed(
    left: Pixels,
    right: Pixels,
    component: StreamingLayoutComponent,
) -> Result<Pixels, StreamingLayoutError> {
    let value = f32::from(left) + f32::from(right);
    if value.is_finite() {
        Ok(px(value))
    } else {
        Err(StreamingLayoutError::Overflow(component))
    }
}

fn checked_pixel_sub_signed(
    left: Pixels,
    right: Pixels,
    component: StreamingLayoutComponent,
) -> Result<Pixels, StreamingLayoutError> {
    let value = f32::from(left) - f32::from(right);
    if value.is_finite() {
        Ok(px(value))
    } else {
        Err(StreamingLayoutError::Overflow(component))
    }
}

pub(crate) fn validate_nonnegative(
    value: Pixels,
    metric: StreamingLayoutMetric,
) -> Result<(), StreamingLayoutError> {
    let value = f32::from(value);
    if value.is_finite() && value >= 0.0 {
        Ok(())
    } else {
        Err(StreamingLayoutError::InvalidMetric(metric))
    }
}

pub(super) fn validate_finite(
    value: Pixels,
    metric: StreamingLayoutMetric,
) -> Result<(), StreamingLayoutError> {
    if f32::from(value).is_finite() {
        Ok(())
    } else {
        Err(StreamingLayoutError::InvalidMetric(metric))
    }
}

pub(crate) fn checked_pixel_add(
    left: Pixels,
    right: Pixels,
    component: StreamingLayoutComponent,
) -> Result<Pixels, StreamingLayoutError> {
    checked_nonnegative_pixel(f32::from(left) + f32::from(right), component)
}

pub(super) fn checked_pixel_sub(
    left: Pixels,
    right: Pixels,
    component: StreamingLayoutComponent,
) -> Result<Pixels, StreamingLayoutError> {
    checked_nonnegative_pixel(f32::from(left) - f32::from(right), component)
}

pub(crate) fn checked_pixel_mul_usize(
    value: Pixels,
    count: usize,
    component: StreamingLayoutComponent,
) -> Result<Pixels, StreamingLayoutError> {
    // Every integer through 2^24 is represented exactly by f32. Larger placement counts are
    // rejected instead of being rounded into a different visual-line coordinate.
    if count > 16_777_216 {
        return Err(StreamingLayoutError::Overflow(component));
    }
    checked_nonnegative_pixel(f32::from(value) * count as f32, component)
}

pub(super) fn checked_pixel_mul_f32(
    value: Pixels,
    factor: f32,
    component: StreamingLayoutComponent,
) -> Result<Pixels, StreamingLayoutError> {
    checked_nonnegative_pixel(f32::from(value) * factor, component)
}

fn checked_nonnegative_pixel(
    value: f32,
    component: StreamingLayoutComponent,
) -> Result<Pixels, StreamingLayoutError> {
    if value.is_finite() && value >= 0.0 {
        Ok(px(value))
    } else {
        Err(StreamingLayoutError::Overflow(component))
    }
}

pub(crate) fn checked_add(
    left: usize,
    right: usize,
    component: StreamingLayoutComponent,
) -> Result<usize, StreamingLayoutError> {
    left.checked_add(right)
        .ok_or(StreamingLayoutError::Overflow(component))
}

pub(super) fn checked_mul(
    left: usize,
    right: usize,
    component: StreamingLayoutComponent,
) -> Result<usize, StreamingLayoutError> {
    left.checked_mul(right)
        .ok_or(StreamingLayoutError::Overflow(component))
}

pub(super) fn checked_sum<const N: usize>(
    values: [usize; N],
    component: StreamingLayoutComponent,
) -> Result<usize, StreamingLayoutError> {
    values
        .into_iter()
        .try_fold(0usize, |total, value| checked_add(total, value, component))
}

pub(super) fn usize_to_u64(
    value: usize,
    component: StreamingLayoutComponent,
) -> Result<u64, StreamingLayoutError> {
    u64::try_from(value).map_err(|_| StreamingLayoutError::Overflow(component))
}

pub(super) fn usize_to_u32(
    value: usize,
    component: StreamingLayoutComponent,
) -> Result<u32, StreamingLayoutError> {
    u32::try_from(value).map_err(|_| StreamingLayoutError::Overflow(component))
}

pub(super) fn checked_u64_add(
    left: u64,
    right: u64,
    component: StreamingLayoutComponent,
) -> Result<u64, StreamingLayoutError> {
    left.checked_add(right)
        .ok_or(StreamingLayoutError::Overflow(component))
}
