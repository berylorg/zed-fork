use collections::FxHashMap;
use parking_lot::Mutex;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
        ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D,
    },
    Dxgi::Common::*,
};

use crate::{
    DevicePixels, ImageResource, ImageResourceDiagnostic, ImageResourceDiagnosticSnapshot,
    ImageResourceId, PlatformImageResources, PreparedImageUpload, Size, image_resource_bytes,
};

const MAX_IMAGE_RESOURCE_DIAGNOSTIC_ITEMS: usize = 32;

pub(crate) struct DirectXImageResources(Mutex<DirectXImageResourcesState>);

struct DirectXImageResourcesState {
    device: ID3D11Device,
    device_context: ID3D11DeviceContext,
    resources: FxHashMap<ImageResourceId, DirectXImageResource>,
    upload_count: u64,
    upload_bytes: u64,
}

struct DirectXImageResource {
    metadata: ImageResource,
    _texture: ID3D11Texture2D,
    view: [Option<ID3D11ShaderResourceView>; 1],
}

impl DirectXImageResources {
    pub(crate) fn new(device: &ID3D11Device, device_context: &ID3D11DeviceContext) -> Self {
        Self(Mutex::new(DirectXImageResourcesState {
            device: device.clone(),
            device_context: device_context.clone(),
            resources: FxHashMap::default(),
            upload_count: 0,
            upload_bytes: 0,
        }))
    }

    pub(crate) fn texture_view(
        &self,
        id: ImageResourceId,
    ) -> Option<[Option<ID3D11ShaderResourceView>; 1]> {
        self.0
            .lock()
            .resources
            .get(&id)
            .map(|entry| entry.view.clone())
    }

    pub(crate) fn handle_device_lost(
        &self,
        device: &ID3D11Device,
        device_context: &ID3D11DeviceContext,
    ) {
        let mut state = self.0.lock();
        state.device = device.clone();
        state.device_context = device_context.clone();
        state.resources.clear();
    }
}

impl PlatformImageResources for DirectXImageResources {
    fn upsert(&self, upload: PreparedImageUpload) -> anyhow::Result<ImageResource> {
        self.0.lock().upsert(upload)
    }

    fn contains(&self, id: ImageResourceId) -> bool {
        self.0.lock().resources.contains_key(&id)
    }

    fn remove(&self, id: ImageResourceId) {
        self.0.lock().resources.remove(&id);
    }

    fn clear(&self) {
        self.0.lock().resources.clear();
    }

    fn diagnostic_snapshot(&self) -> ImageResourceDiagnosticSnapshot {
        self.0.lock().diagnostic_snapshot()
    }
}

impl DirectXImageResourcesState {
    fn upsert(&mut self, upload: PreparedImageUpload) -> anyhow::Result<ImageResource> {
        let metadata = ImageResource::from_upload(&upload);
        let pixels = upload.into_pixels();
        let expected_bytes = image_resource_bytes(metadata.size) as usize;
        anyhow::ensure!(
            pixels.len() == expected_bytes,
            "prepared upload byte length did not match resource size"
        );

        let texture = self.create_texture(metadata.size)?;
        let row_pitch = metadata.size.width.0.max(0) as u32 * 4;
        unsafe {
            self.device_context.UpdateSubresource(
                &texture,
                0,
                Some(&D3D11_BOX {
                    left: 0,
                    top: 0,
                    front: 0,
                    right: metadata.size.width.0.max(0) as u32,
                    bottom: metadata.size.height.0.max(0) as u32,
                    back: 1,
                }),
                pixels.as_ptr() as _,
                row_pitch,
                0,
            );
        }

        let view = unsafe {
            let mut view = None;
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut view))?;
            [view]
        };

        self.upload_count = self.upload_count.saturating_add(1);
        self.upload_bytes = self.upload_bytes.saturating_add(pixels.len() as u64);
        self.resources.insert(
            metadata.id,
            DirectXImageResource {
                metadata: metadata.clone(),
                _texture: texture,
                view,
            },
        );
        Ok(metadata)
    }

    fn create_texture(&self, size: Size<DevicePixels>) -> anyhow::Result<ID3D11Texture2D> {
        let texture_desc = D3D11_TEXTURE2D_DESC {
            Width: size.width.0.max(1) as u32,
            Height: size.height.0.max(1) as u32,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };

        let mut texture = None;
        unsafe {
            self.device
                .CreateTexture2D(&texture_desc, None, Some(&mut texture))?;
        }
        Ok(texture.expect("CreateTexture2D returned success without a texture"))
    }

    fn diagnostic_snapshot(&self) -> ImageResourceDiagnosticSnapshot {
        let mut snapshot = ImageResourceDiagnosticSnapshot {
            resource_count: self.resources.len(),
            upload_count: self.upload_count,
            upload_bytes: self.upload_bytes,
            ..Default::default()
        };

        for resource in self.resources.values() {
            snapshot.gpu_bytes_estimate = snapshot
                .gpu_bytes_estimate
                .saturating_add(resource.metadata.gpu_bytes_estimate());
            if snapshot.items.len() < MAX_IMAGE_RESOURCE_DIAGNOSTIC_ITEMS {
                snapshot
                    .items
                    .push(ImageResourceDiagnostic::new(&resource.metadata));
            } else {
                snapshot.truncated = true;
            }
        }

        snapshot
    }
}
