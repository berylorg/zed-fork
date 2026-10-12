use super::{Font, FontId};
use crate::Result;
use collections::FxHashMap;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(feature = "test-support")]
mod test_support;
#[cfg(feature = "test-support")]
pub use test_support::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct FontEnvironmentKey {
    owner: u64,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FontEnvironmentError {
    Changed,
    FaceCapacityExceeded,
}

impl std::fmt::Display for FontEnvironmentError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(self, formatter)
    }
}

impl std::error::Error for FontEnvironmentError {}

impl FontEnvironmentKey {
    pub(super) fn new(owner: u64, generation: u64) -> Self {
        Self { owner, generation }
    }
}

pub(super) fn allocate_owner() -> Option<u64> {
    static NEXT_OWNER: AtomicU64 = AtomicU64::new(1);
    NEXT_OWNER
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |owner| {
            owner.checked_add(1)
        })
        .ok()
}

#[derive(Default)]
pub(super) struct FontResolutionCache {
    pub(super) generation: Option<u64>,
    pub(super) fonts: FxHashMap<Font, Result<FontId>>,
}
