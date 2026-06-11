use crate::{
    DevicePixels, ImagePreparationOutcome, ImageRenderRequestId, ImageResource, ImageResourceId,
    ImageSourceId, PlatformImageResources, PreparedImageUpload, Size,
    SourceBackedImageDiagnosticSnapshot, SourceBackedImageRequestDiagnostic, image_resource_bytes,
};
use collections::{FxHashMap, FxHashSet};

use super::SourceBackedImageRequestStatus;

const DEFAULT_SOURCE_BACKED_IMAGE_RESOURCE_COUNT_LIMIT: usize = 128;
const DEFAULT_SOURCE_BACKED_IMAGE_GPU_BYTE_LIMIT: u64 = 256 * 1024 * 1024;
const DEFAULT_SOURCE_BACKED_IMAGE_PRELOAD_RESOURCE_COUNT_LIMIT: usize = 32;
const DEFAULT_SOURCE_BACKED_IMAGE_PRELOAD_GPU_BYTE_LIMIT: u64 = 64 * 1024 * 1024;
const SOURCE_BACKED_IMAGE_RESOURCE_REUSE_DEVICE_PIXEL_TOLERANCE: i32 = 2;
const MAX_SOURCE_BACKED_IMAGE_DIAGNOSTIC_ITEMS: usize = 64;

#[derive(Default)]
pub(super) struct SourceBackedImageStore {
    requests: FxHashMap<ImageRenderRequestId, SourceBackedImageEntry>,
    latest_live_by_source: FxHashMap<ImageSourceId, ImageResource>,
    known_size_by_source: FxHashMap<ImageSourceId, Size<DevicePixels>>,
    failed_sources: FxHashSet<ImageSourceId>,
    requested_this_frame: FxHashSet<ImageRenderRequestId>,
    preload_requested_this_frame: FxHashSet<ImageRenderRequestId>,
    referenced_this_frame: FxHashSet<ImageResourceId>,
    referenced_gpu_bytes_this_frame: u64,
    preloaded_resource_ids_this_frame: FxHashSet<ImageResourceId>,
    preloaded_gpu_bytes_this_frame: u64,
    preloaded_resource_ids_last_frame: FxHashSet<ImageResourceId>,
    pending_resource_removals: FxHashSet<ImageResourceId>,
    evicted_resource_count: u64,
    preload_evicted_resource_count: u64,
    preload_budget_deferral_count: u64,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct SourceBackedImageBudget {
    max_resource_count: usize,
    max_gpu_bytes: u64,
}

impl SourceBackedImageBudget {
    #[allow(dead_code)]
    pub(super) fn new(max_resource_count: usize, max_gpu_bytes: u64) -> Self {
        Self {
            max_resource_count,
            max_gpu_bytes,
        }
    }

    pub(super) fn preload_default() -> Self {
        Self {
            max_resource_count: DEFAULT_SOURCE_BACKED_IMAGE_PRELOAD_RESOURCE_COUNT_LIMIT,
            max_gpu_bytes: DEFAULT_SOURCE_BACKED_IMAGE_PRELOAD_GPU_BYTE_LIMIT,
        }
    }

    pub(super) fn max_resource_count(self) -> usize {
        self.max_resource_count
    }

    pub(super) fn max_gpu_bytes(self) -> u64 {
        self.max_gpu_bytes
    }
}

impl Default for SourceBackedImageBudget {
    fn default() -> Self {
        Self {
            max_resource_count: DEFAULT_SOURCE_BACKED_IMAGE_RESOURCE_COUNT_LIMIT,
            max_gpu_bytes: DEFAULT_SOURCE_BACKED_IMAGE_GPU_BYTE_LIMIT,
        }
    }
}

struct SourceBackedImageEntry {
    source_id: ImageSourceId,
    requested_size: Size<DevicePixels>,
    state: SourceBackedImageState,
}

enum SourceBackedImageState {
    Loading,
    Ready(PreparedImageUpload),
    Live(ImageResource),
    BudgetDeferred { preload: bool },
    Failed,
}

pub(super) enum SourceBackedPaintCandidate {
    None,
    Loading {
        source_id: ImageSourceId,
    },
    Ready {
        source_id: ImageSourceId,
        upload: PreparedImageUpload,
    },
    Live(ImageResource),
}

impl SourceBackedImageStore {
    pub(super) fn begin_frame(&mut self) {
        self.requested_this_frame.clear();
        self.preload_requested_this_frame.clear();
        self.referenced_this_frame.clear();
        self.referenced_gpu_bytes_this_frame = 0;
        self.preloaded_resource_ids_this_frame.clear();
        self.preloaded_gpu_bytes_this_frame = 0;
    }

    pub(super) fn latest_size(
        &self,
        source_id: ImageSourceId,
        image_resources: &dyn PlatformImageResources,
    ) -> Option<Size<DevicePixels>> {
        if let Some(size) = self.known_size_by_source.get(&source_id) {
            return Some(*size);
        }
        self.latest_live_by_source
            .get(&source_id)
            .filter(|resource| image_resources.contains(resource.id))
            .map(|resource| resource.size)
    }

    pub(super) fn latest_resource(
        &mut self,
        source_id: ImageSourceId,
        image_resources: &dyn PlatformImageResources,
    ) -> Option<ImageResource> {
        let resource = self.latest_live_by_source.get(&source_id).cloned()?;
        if image_resources.contains(resource.id) {
            Some(resource)
        } else {
            self.latest_live_by_source.remove(&source_id);
            None
        }
    }

    pub(super) fn has_failed(&self, source_id: ImageSourceId) -> bool {
        self.failed_sources.contains(&source_id)
    }

    pub(super) fn request_status(
        &self,
        request_id: ImageRenderRequestId,
        image_resources: &dyn PlatformImageResources,
    ) -> SourceBackedImageRequestStatus {
        let Some(entry) = self.requests.get(&request_id) else {
            return SourceBackedImageRequestStatus::Missing;
        };
        match &entry.state {
            SourceBackedImageState::Loading => SourceBackedImageRequestStatus::Loading,
            SourceBackedImageState::Ready(_) => SourceBackedImageRequestStatus::ReadyForUpload,
            SourceBackedImageState::Live(resource) if image_resources.contains(resource.id) => {
                SourceBackedImageRequestStatus::Live
            }
            SourceBackedImageState::Live(_) => SourceBackedImageRequestStatus::Loading,
            SourceBackedImageState::BudgetDeferred { .. } => {
                SourceBackedImageRequestStatus::BudgetDeferred
            }
            SourceBackedImageState::Failed => SourceBackedImageRequestStatus::Failed,
        }
    }

    pub(super) fn request(
        &mut self,
        source_id: ImageSourceId,
        request_id: ImageRenderRequestId,
        requested_size: Size<DevicePixels>,
        image_resources: &dyn PlatformImageResources,
    ) -> bool {
        self.request_source_backed_image(
            source_id,
            request_id,
            requested_size,
            image_resources,
            false,
        )
    }

    pub(super) fn preload(
        &mut self,
        source_id: ImageSourceId,
        request_id: ImageRenderRequestId,
        requested_size: Size<DevicePixels>,
        image_resources: &dyn PlatformImageResources,
    ) -> bool {
        self.request_source_backed_image(
            source_id,
            request_id,
            requested_size,
            image_resources,
            true,
        )
    }

    fn request_source_backed_image(
        &mut self,
        source_id: ImageSourceId,
        request_id: ImageRenderRequestId,
        requested_size: Size<DevicePixels>,
        image_resources: &dyn PlatformImageResources,
        preload: bool,
    ) -> bool {
        if preload {
            self.preload_requested_this_frame.insert(request_id);
        } else {
            self.requested_this_frame.insert(request_id);
        }
        let mut missing_live = None;
        let mut existing_entry_needs_spawn = false;

        if let Some(entry) = self.requests.get_mut(&request_id) {
            entry.requested_size = requested_size;
            match &entry.state {
                SourceBackedImageState::Live(resource)
                    if !image_resources.contains(resource.id) =>
                {
                    missing_live = Some((entry.source_id, resource.id));
                    entry.state = SourceBackedImageState::Loading;
                    existing_entry_needs_spawn = true;
                }
                SourceBackedImageState::BudgetDeferred { .. } => {
                    entry.state = SourceBackedImageState::Loading;
                    existing_entry_needs_spawn = true;
                }
                _ => {}
            }

            if !existing_entry_needs_spawn {
                return false;
            }
        }

        self.failed_sources.remove(&source_id);
        if let Some((source_id, resource_id)) = missing_live {
            self.remove_latest_if_matches(source_id, resource_id);
        }
        if existing_entry_needs_spawn {
            return true;
        }

        if let Some(resource) = self.latest_live_by_source.get(&source_id).cloned() {
            if image_resources.contains(resource.id) {
                if source_backed_resource_matches_request(resource.size, requested_size) {
                    let resource_id = resource.id;
                    self.requests.insert(
                        request_id,
                        SourceBackedImageEntry {
                            source_id,
                            requested_size,
                            state: SourceBackedImageState::Live(resource),
                        },
                    );
                    if !preload {
                        self.remove_live_request_duplicates(request_id, resource_id);
                    }
                    return false;
                }
            } else {
                self.latest_live_by_source.remove(&source_id);
            }
        }

        self.requests.insert(
            request_id,
            SourceBackedImageEntry {
                source_id,
                requested_size,
                state: SourceBackedImageState::Loading,
            },
        );
        true
    }

    pub(super) fn complete(&mut self, outcome: ImagePreparationOutcome) -> Vec<ImageResourceId> {
        let request_id = outcome.request_id();
        let Some(entry) = self.requests.get_mut(&request_id) else {
            return Vec::new();
        };
        match outcome {
            ImagePreparationOutcome::Ready(upload) => {
                self.failed_sources.remove(&entry.source_id);
                self.known_size_by_source
                    .insert(entry.source_id, upload.size());
                entry.state = SourceBackedImageState::Ready(upload);
                Vec::new()
            }
            ImagePreparationOutcome::Failed(_) => {
                self.failed_sources.insert(entry.source_id);
                let removed = self
                    .latest_live_by_source
                    .remove(&entry.source_id)
                    .map(|resource| resource.id)
                    .into_iter()
                    .collect();
                entry.state = SourceBackedImageState::Failed;
                removed
            }
        }
    }

    pub(super) fn candidate(
        &mut self,
        request_id: ImageRenderRequestId,
        image_resources: &dyn PlatformImageResources,
    ) -> SourceBackedPaintCandidate {
        let Some(entry) = self.requests.get_mut(&request_id) else {
            return SourceBackedPaintCandidate::None;
        };
        match std::mem::replace(&mut entry.state, SourceBackedImageState::Loading) {
            SourceBackedImageState::Ready(upload) => SourceBackedPaintCandidate::Ready {
                source_id: entry.source_id,
                upload,
            },
            SourceBackedImageState::Live(resource) => {
                if image_resources.contains(resource.id) {
                    entry.state = SourceBackedImageState::Live(resource.clone());
                    SourceBackedPaintCandidate::Live(resource)
                } else {
                    self.latest_live_by_source.remove(&entry.source_id);
                    entry.state = SourceBackedImageState::Loading;
                    SourceBackedPaintCandidate::Loading {
                        source_id: entry.source_id,
                    }
                }
            }
            SourceBackedImageState::Loading => {
                entry.state = SourceBackedImageState::Loading;
                SourceBackedPaintCandidate::Loading {
                    source_id: entry.source_id,
                }
            }
            SourceBackedImageState::BudgetDeferred { preload } => {
                entry.state = SourceBackedImageState::BudgetDeferred { preload };
                SourceBackedPaintCandidate::None
            }
            SourceBackedImageState::Failed => {
                entry.state = SourceBackedImageState::Failed;
                SourceBackedPaintCandidate::None
            }
        }
    }

    pub(super) fn mark_live(
        &mut self,
        request_id: ImageRenderRequestId,
        source_id: ImageSourceId,
        resource: ImageResource,
    ) {
        self.failed_sources.remove(&source_id);
        self.known_size_by_source.insert(source_id, resource.size);
        self.latest_live_by_source
            .insert(source_id, resource.clone());
        if let Some(entry) = self.requests.get_mut(&request_id) {
            entry.state = SourceBackedImageState::Live(resource);
        }
    }

    pub(super) fn mark_failed(
        &mut self,
        request_id: ImageRenderRequestId,
        source_id: ImageSourceId,
    ) -> Vec<ImageResourceId> {
        self.failed_sources.insert(source_id);
        let mut removed = Vec::new();
        if let Some(resource) = self.latest_live_by_source.remove(&source_id) {
            removed.push(resource.id);
        }
        if let Some(entry) = self.requests.get_mut(&request_id) {
            if let SourceBackedImageState::Live(resource) =
                std::mem::replace(&mut entry.state, SourceBackedImageState::Failed)
                && !removed.contains(&resource.id)
            {
                removed.push(resource.id);
            }
        }
        removed
    }

    pub(super) fn mark_budget_deferred(
        &mut self,
        request_id: ImageRenderRequestId,
        source_id: ImageSourceId,
        preload: bool,
    ) -> Vec<ImageResourceId> {
        self.failed_sources.remove(&source_id);
        if preload {
            self.preload_budget_deferral_count =
                self.preload_budget_deferral_count.saturating_add(1);
        }
        let mut removed = Vec::new();
        let mut live_resource_id = None;
        if let Some(entry) = self.requests.get_mut(&request_id) {
            if let SourceBackedImageState::Live(resource) = std::mem::replace(
                &mut entry.state,
                SourceBackedImageState::BudgetDeferred { preload },
            ) {
                live_resource_id = Some(resource.id);
                removed.push(resource.id);
            }
        }
        if let Some(resource_id) = live_resource_id {
            self.remove_latest_if_matches(source_id, resource_id);
        }
        removed
    }

    pub(super) fn can_paint(
        &self,
        resource: &ImageResource,
        budget: SourceBackedImageBudget,
    ) -> bool {
        if self.referenced_this_frame.contains(&resource.id) {
            return true;
        }
        let resource_bytes = resource.gpu_bytes_estimate();
        if budget.max_resource_count == 0 || resource_bytes > budget.max_gpu_bytes {
            return false;
        }
        self.referenced_this_frame.len() < budget.max_resource_count
            && self
                .referenced_gpu_bytes_this_frame
                .saturating_add(resource_bytes)
                <= budget.max_gpu_bytes
    }

    fn can_preload(
        &self,
        resource: &ImageResource,
        budget: SourceBackedImageBudget,
        preload_budget: SourceBackedImageBudget,
    ) -> bool {
        if self.referenced_this_frame.contains(&resource.id)
            || self
                .preloaded_resource_ids_this_frame
                .contains(&resource.id)
        {
            return true;
        }

        let resource_bytes = resource.gpu_bytes_estimate();
        if budget.max_resource_count == 0
            || preload_budget.max_resource_count == 0
            || resource_bytes > budget.max_gpu_bytes
            || resource_bytes > preload_budget.max_gpu_bytes
        {
            return false;
        }

        let total_resource_count = self
            .referenced_this_frame
            .len()
            .saturating_add(self.preloaded_resource_ids_this_frame.len());
        total_resource_count < budget.max_resource_count
            && self.preloaded_resource_ids_this_frame.len() < preload_budget.max_resource_count
            && self
                .referenced_gpu_bytes_this_frame
                .saturating_add(self.preloaded_gpu_bytes_this_frame)
                .saturating_add(resource_bytes)
                <= budget.max_gpu_bytes
            && self
                .preloaded_gpu_bytes_this_frame
                .saturating_add(resource_bytes)
                <= preload_budget.max_gpu_bytes
    }

    pub(super) fn mark_referenced(&mut self, resource: &ImageResource) {
        if self.referenced_this_frame.insert(resource.id) {
            self.referenced_gpu_bytes_this_frame = self
                .referenced_gpu_bytes_this_frame
                .saturating_add(resource.gpu_bytes_estimate());
        }
    }

    pub(super) fn mark_referenced_resource_ids(
        &mut self,
        resource_ids: impl IntoIterator<Item = ImageResourceId>,
    ) {
        for resource_id in resource_ids {
            self.mark_referenced_resource_id(resource_id);
        }
    }

    fn mark_preloaded(&mut self, resource: &ImageResource) {
        if self.preloaded_resource_ids_this_frame.insert(resource.id) {
            self.preloaded_gpu_bytes_this_frame = self
                .preloaded_gpu_bytes_this_frame
                .saturating_add(resource.gpu_bytes_estimate());
        }
    }

    pub(super) fn service_preloads(
        &mut self,
        image_resources: &dyn PlatformImageResources,
        budget: SourceBackedImageBudget,
        preload_budget: SourceBackedImageBudget,
    ) -> Vec<ImageResourceId> {
        let mut removed = Vec::new();
        let mut request_ids = self
            .preload_requested_this_frame
            .iter()
            .copied()
            .collect::<Vec<_>>();
        request_ids.sort_unstable_by_key(|request_id| request_id.as_u64());

        for request_id in request_ids {
            match self.candidate(request_id, image_resources) {
                SourceBackedPaintCandidate::Ready { source_id, upload } => {
                    let pending_resource = ImageResource::from_upload(&upload);
                    if !self.can_preload(&pending_resource, budget, preload_budget) {
                        removed.extend(self.mark_budget_deferred(request_id, source_id, true));
                        continue;
                    }
                    let resource = match image_resources.upsert(upload) {
                        Ok(resource) => resource,
                        Err(_) => {
                            removed.extend(self.mark_failed(request_id, source_id));
                            continue;
                        }
                    };
                    self.mark_live(request_id, source_id, resource.clone());
                    self.mark_preloaded(&resource);
                }
                SourceBackedPaintCandidate::Live(resource) => {
                    if self.referenced_this_frame.contains(&resource.id) {
                        continue;
                    }
                    if self.can_preload(&resource, budget, preload_budget) {
                        self.mark_preloaded(&resource);
                    } else {
                        removed.extend(self.mark_budget_deferred(
                            request_id,
                            resource.source_id,
                            true,
                        ));
                    }
                }
                SourceBackedPaintCandidate::Loading { .. } | SourceBackedPaintCandidate::None => {}
            }
        }

        removed
    }

    pub(super) fn sync_final_scene_references(
        &mut self,
        resource_ids: impl IntoIterator<Item = ImageResourceId>,
    ) -> FxHashSet<ImageResourceId> {
        let resource_ids = resource_ids.into_iter().collect::<FxHashSet<_>>();
        self.referenced_this_frame.clear();
        self.referenced_gpu_bytes_this_frame = 0;
        for resource_id in &resource_ids {
            self.mark_referenced_resource_id(*resource_id);
        }
        resource_ids
    }

    pub(super) fn finish_frame(
        &mut self,
        final_scene_resource_ids: impl IntoIterator<Item = ImageResourceId>,
    ) {
        let final_scene_resource_ids = self.sync_final_scene_references(final_scene_resource_ids);
        for resource_id in &final_scene_resource_ids {
            self.pending_resource_removals.remove(resource_id);
        }
        for resource_id in &self.preloaded_resource_ids_this_frame {
            self.pending_resource_removals.remove(resource_id);
        }

        let mut removed = FxHashSet::default();
        let requested_this_frame = &self.requested_this_frame;
        let preload_requested_this_frame = &self.preload_requested_this_frame;
        let referenced_this_frame = &final_scene_resource_ids;
        let preloaded_this_frame = &self.preloaded_resource_ids_this_frame;

        self.requests.retain(|request_id, entry| {
            let requested = requested_this_frame.contains(request_id)
                || preload_requested_this_frame.contains(request_id);
            match &entry.state {
                SourceBackedImageState::Live(resource) => {
                    let referenced = referenced_this_frame.contains(&resource.id);
                    let preloaded = preloaded_this_frame.contains(&resource.id);
                    if !referenced && !preloaded {
                        removed.insert(resource.id);
                    }
                    referenced || preloaded
                }
                SourceBackedImageState::Ready(_)
                | SourceBackedImageState::Loading
                | SourceBackedImageState::BudgetDeferred { .. }
                | SourceBackedImageState::Failed => requested,
            }
        });

        let live_resources = self
            .requests
            .values()
            .filter_map(|entry| match &entry.state {
                SourceBackedImageState::Live(resource) => Some(resource.id),
                _ => None,
            })
            .collect::<FxHashSet<_>>();
        self.latest_live_by_source.retain(|_, resource| {
            let keep = live_resources.contains(&resource.id);
            if !keep {
                removed.insert(resource.id);
            }
            keep
        });

        let retained_sources = self
            .requests
            .values()
            .map(|entry| entry.source_id)
            .chain(self.latest_live_by_source.keys().copied())
            .collect::<FxHashSet<_>>();
        self.known_size_by_source
            .retain(|source_id, _| retained_sources.contains(source_id));
        self.failed_sources
            .retain(|source_id| retained_sources.contains(source_id));

        let preload_removed = removed
            .iter()
            .copied()
            .filter(|resource_id| self.preloaded_resource_ids_last_frame.contains(resource_id))
            .collect::<Vec<_>>();
        self.queue_preload_removal_ids(preload_removed);
        self.queue_removal_ids(removed);
        self.preloaded_resource_ids_last_frame = self.preloaded_resource_ids_this_frame.clone();
    }

    fn remove_latest_if_matches(&mut self, source_id: ImageSourceId, resource_id: ImageResourceId) {
        if self
            .latest_live_by_source
            .get(&source_id)
            .is_some_and(|resource| resource.id == resource_id)
        {
            self.latest_live_by_source.remove(&source_id);
        }
    }

    fn remove_live_request_duplicates(
        &mut self,
        retained_request_id: ImageRenderRequestId,
        resource_id: ImageResourceId,
    ) {
        self.requests.retain(|request_id, entry| {
            *request_id == retained_request_id
                || !matches!(
                    &entry.state,
                    SourceBackedImageState::Live(resource) if resource.id == resource_id
                )
        });
    }

    pub(super) fn queue_removals(
        &mut self,
        resource_ids: impl IntoIterator<Item = ImageResourceId>,
    ) {
        self.queue_removal_ids(resource_ids);
    }

    pub(super) fn drain_pending_removals(&mut self) -> Vec<ImageResourceId> {
        self.pending_resource_removals.drain().collect()
    }

    pub(super) fn diagnostic_snapshot(
        &self,
        preload_budget: SourceBackedImageBudget,
    ) -> SourceBackedImageDiagnosticSnapshot {
        let mut snapshot = SourceBackedImageDiagnosticSnapshot {
            request_count: self.requests.len(),
            known_source_count: self.known_size_by_source.len(),
            failed_source_count: self.failed_sources.len(),
            requested_this_frame_count: self.requested_this_frame.len(),
            preload_request_count: self.preload_requested_this_frame.len(),
            painted_resource_count: self.referenced_this_frame.len(),
            final_scene_resource_count: self.referenced_this_frame.len(),
            preload_live_count: self.preloaded_resource_ids_this_frame.len(),
            preload_gpu_bytes_estimate: self.preloaded_gpu_bytes_this_frame,
            preload_budget_deferral_count: self.preload_budget_deferral_count,
            preload_max_resource_count: preload_budget.max_resource_count(),
            preload_max_gpu_bytes: preload_budget.max_gpu_bytes(),
            pending_resource_removal_count: self.pending_resource_removals.len(),
            evicted_resource_count: self.evicted_resource_count,
            preload_evicted_resource_count: self.preload_evicted_resource_count,
            ..Default::default()
        };

        for (request_id, entry) in &self.requests {
            let mut item = SourceBackedImageRequestDiagnostic {
                request_id: request_id.as_u64(),
                source_id: entry.source_id.as_u64(),
                state: "loading".to_string(),
                requested_width: entry.requested_size.width.0.max(0) as u32,
                requested_height: entry.requested_size.height.0.max(0) as u32,
                resource_id: None,
                gpu_bytes_estimate: None,
                decoded_cpu_bytes_estimate: None,
                retention_kind: "loading".to_string(),
            };
            let preload_requested = self.preload_requested_this_frame.contains(request_id);

            match &entry.state {
                SourceBackedImageState::Loading => {
                    snapshot.pending_decode_count = snapshot.pending_decode_count.saturating_add(1);
                    if preload_requested {
                        snapshot.preload_pending_decode_count =
                            snapshot.preload_pending_decode_count.saturating_add(1);
                    }
                }
                SourceBackedImageState::Ready(upload) => {
                    let decoded_bytes = upload.pixels().len() as u64;
                    snapshot.pending_upload_count = snapshot.pending_upload_count.saturating_add(1);
                    snapshot.pending_upload_decoded_cpu_bytes_estimate = snapshot
                        .pending_upload_decoded_cpu_bytes_estimate
                        .saturating_add(decoded_bytes);
                    if preload_requested {
                        snapshot.preload_pending_upload_count =
                            snapshot.preload_pending_upload_count.saturating_add(1);
                    }
                    item.state = "ready".to_string();
                    item.retention_kind = if preload_requested {
                        "preload".to_string()
                    } else {
                        "loading".to_string()
                    };
                    item.gpu_bytes_estimate = Some(image_resource_bytes(upload.size()));
                    item.decoded_cpu_bytes_estimate = Some(decoded_bytes);
                }
                SourceBackedImageState::Live(resource) => {
                    snapshot.live_count = snapshot.live_count.saturating_add(1);
                    snapshot.live_gpu_bytes_estimate = snapshot
                        .live_gpu_bytes_estimate
                        .saturating_add(resource.gpu_bytes_estimate());
                    item.state = "live".to_string();
                    item.retention_kind = match (
                        self.referenced_this_frame.contains(&resource.id),
                        self.preloaded_resource_ids_this_frame
                            .contains(&resource.id),
                    ) {
                        (true, true) => "both",
                        (true, false) => "final_scene",
                        (false, true) => "preload",
                        (false, false) => "live",
                    }
                    .to_string();
                    item.resource_id = Some(resource.id.as_u64());
                    item.gpu_bytes_estimate = Some(resource.gpu_bytes_estimate());
                }
                SourceBackedImageState::BudgetDeferred { preload } => {
                    snapshot.budget_deferred_count =
                        snapshot.budget_deferred_count.saturating_add(1);
                    if *preload {
                        snapshot.preload_budget_deferred_count =
                            snapshot.preload_budget_deferred_count.saturating_add(1);
                    }
                    item.state = "budget_deferred".to_string();
                    item.retention_kind =
                        if *preload { "preload" } else { "final_scene" }.to_string();
                }
                SourceBackedImageState::Failed => {
                    snapshot.failed_count = snapshot.failed_count.saturating_add(1);
                    item.state = "failed".to_string();
                    item.retention_kind = if preload_requested {
                        "preload".to_string()
                    } else {
                        "failed".to_string()
                    };
                }
            }

            if snapshot.items.len() < MAX_SOURCE_BACKED_IMAGE_DIAGNOSTIC_ITEMS {
                snapshot.items.push(item);
            } else {
                snapshot.truncated = true;
            }
        }

        snapshot
    }

    fn queue_removal_ids(&mut self, resource_ids: impl IntoIterator<Item = ImageResourceId>) {
        for resource_id in resource_ids {
            if self.pending_resource_removals.insert(resource_id) {
                self.evicted_resource_count = self.evicted_resource_count.saturating_add(1);
            }
        }
    }

    fn queue_preload_removal_ids(
        &mut self,
        resource_ids: impl IntoIterator<Item = ImageResourceId>,
    ) {
        for resource_id in resource_ids {
            if !self.pending_resource_removals.contains(&resource_id) {
                self.preload_evicted_resource_count =
                    self.preload_evicted_resource_count.saturating_add(1);
            }
        }
    }

    fn mark_referenced_resource_id(&mut self, resource_id: ImageResourceId) {
        if self.referenced_this_frame.contains(&resource_id) {
            return;
        }
        let Some(resource_bytes) = self
            .resource_for_id(resource_id)
            .map(|resource| resource.gpu_bytes_estimate())
        else {
            return;
        };
        if self.referenced_this_frame.insert(resource_id) {
            self.referenced_gpu_bytes_this_frame = self
                .referenced_gpu_bytes_this_frame
                .saturating_add(resource_bytes);
        }
    }

    fn resource_for_id(&self, resource_id: ImageResourceId) -> Option<&ImageResource> {
        self.requests
            .values()
            .find_map(|entry| match &entry.state {
                SourceBackedImageState::Live(resource) if resource.id == resource_id => {
                    Some(resource)
                }
                _ => None,
            })
            .or_else(|| {
                self.latest_live_by_source
                    .values()
                    .find(|resource| resource.id == resource_id)
            })
    }
}

fn source_backed_resource_matches_request(
    resource_size: Size<DevicePixels>,
    requested_size: Size<DevicePixels>,
) -> bool {
    device_pixel_delta_within_tolerance(resource_size.width, requested_size.width)
        && device_pixel_delta_within_tolerance(resource_size.height, requested_size.height)
}

fn device_pixel_delta_within_tolerance(a: DevicePixels, b: DevicePixels) -> bool {
    a.0.abs_diff(b.0) <= SOURCE_BACKED_IMAGE_RESOURCE_REUSE_DEVICE_PIXEL_TOLERANCE as u32
}
