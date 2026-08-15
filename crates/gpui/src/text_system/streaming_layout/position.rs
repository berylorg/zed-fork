use super::StreamingLayoutError;

/// Stable fixed-size identity for one source-zero-width object.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct StreamingObjectId(pub u128);

/// Stable fixed-size order key for one object at its UTF-8 anchor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StreamingObjectOrder(pub u128);

/// One edge of an exact adjacent-object gap witness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StreamingObjectEdge {
    /// No objects exist at this byte offset.
    NoObject,
    /// The gap is before the first object at this byte offset.
    BeforeAll,
    /// An immediately adjacent object.
    Object {
        /// Opaque object identity.
        id: StreamingObjectId,
        /// Object order key at this anchor.
        order: StreamingObjectOrder,
    },
    /// The gap is after the last object at this byte offset.
    AfterAll,
}

/// Constant-size exact witness for one gap between adjacent zero-width objects.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamingObjectGap {
    /// Immediately preceding object or explicit outer edge.
    pub preceding: StreamingObjectEdge,
    /// Immediately following object or explicit outer edge.
    pub following: StreamingObjectEdge,
}

impl StreamingObjectGap {
    /// The sole position at an anchor with no zero-width objects.
    pub const fn no_objects() -> Self {
        Self {
            preceding: StreamingObjectEdge::NoObject,
            following: StreamingObjectEdge::NoObject,
        }
    }

    /// The gap before the first named object.
    pub const fn before(id: StreamingObjectId, order: StreamingObjectOrder) -> Self {
        Self {
            preceding: StreamingObjectEdge::BeforeAll,
            following: StreamingObjectEdge::Object { id, order },
        }
    }

    /// The gap between two stated adjacent objects.
    pub const fn between(
        preceding_id: StreamingObjectId,
        preceding_order: StreamingObjectOrder,
        following_id: StreamingObjectId,
        following_order: StreamingObjectOrder,
    ) -> Self {
        Self {
            preceding: StreamingObjectEdge::Object {
                id: preceding_id,
                order: preceding_order,
            },
            following: StreamingObjectEdge::Object {
                id: following_id,
                order: following_order,
            },
        }
    }

    /// The gap after the last named object.
    pub const fn after(id: StreamingObjectId, order: StreamingObjectOrder) -> Self {
        Self {
            preceding: StreamingObjectEdge::Object { id, order },
            following: StreamingObjectEdge::AfterAll,
        }
    }

    pub(super) fn validate(self) -> Result<(), StreamingLayoutError> {
        use StreamingObjectEdge::*;
        match (self.preceding, self.following) {
            (NoObject, NoObject) | (BeforeAll, Object { .. }) | (Object { .. }, AfterAll) => Ok(()),
            (
                Object {
                    id: preceding_id,
                    order: preceding,
                },
                Object {
                    id: following_id,
                    order: following,
                },
            ) if preceding_id != following_id && preceding < following => Ok(()),
            _ => Err(StreamingLayoutError::InvalidPosition),
        }
    }

    pub(super) fn is_terminal(self) -> bool {
        matches!(
            (self.preceding, self.following),
            (StreamingObjectEdge::NoObject, StreamingObjectEdge::NoObject)
                | (_, StreamingObjectEdge::AfterAll)
        )
    }

    pub(super) fn is_source_range_end(self) -> bool {
        matches!(
            (self.preceding, self.following),
            (StreamingObjectEdge::NoObject, StreamingObjectEdge::NoObject)
                | (StreamingObjectEdge::BeforeAll, _)
        )
    }
}

/// Canonical position in the ordered text-and-object stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamingLayoutPosition {
    /// Exact logical UTF-8 byte offset.
    pub byte_offset: u64,
    /// Exact adjacent-object gap at that offset.
    pub gap: StreamingObjectGap,
}

impl StreamingLayoutPosition {
    /// Creates a position at an anchor with no zero-width objects.
    pub const fn at(byte_offset: u64) -> Self {
        Self {
            byte_offset,
            gap: StreamingObjectGap::no_objects(),
        }
    }

    /// Creates a position with an explicit adjacent-object gap.
    pub const fn with_gap(byte_offset: u64, gap: StreamingObjectGap) -> Self {
        Self { byte_offset, gap }
    }

    pub(super) fn validate(self) -> Result<(), StreamingLayoutError> {
        self.gap.validate()
    }
}
