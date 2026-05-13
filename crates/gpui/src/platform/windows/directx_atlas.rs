use collections::FxHashMap;
use etagere::BucketedAtlasAllocator;
use parking_lot::Mutex;
use windows::Win32::Graphics::{
    Direct3D11::{
        D3D11_BIND_SHADER_RESOURCE, D3D11_BOX, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
        ID3D11Device, ID3D11DeviceContext, ID3D11ShaderResourceView, ID3D11Texture2D,
    },
    Dxgi::Common::*,
};

use crate::{
    AtlasDiagnosticSnapshot, AtlasImageTileDiagnostic, AtlasKey, AtlasKindDiagnostic,
    AtlasTextureId, AtlasTextureKind, AtlasTile, Bounds, DevicePixels, PlatformAtlas, Point, Size,
    platform::AtlasTextureList,
};

pub(crate) struct DirectXAtlas(Mutex<DirectXAtlasState>);

struct DirectXAtlasState {
    device: ID3D11Device,
    device_context: ID3D11DeviceContext,
    monochrome_textures: AtlasTextureList<DirectXAtlasTexture>,
    polychrome_textures: AtlasTextureList<DirectXAtlasTexture>,
    tiles_by_key: FxHashMap<AtlasKey, AtlasTile>,
}

struct DirectXAtlasTexture {
    id: AtlasTextureId,
    size: Size<DevicePixels>,
    bytes_per_pixel: u32,
    allocator: BucketedAtlasAllocator,
    texture: ID3D11Texture2D,
    view: [Option<ID3D11ShaderResourceView>; 1],
    live_atlas_keys: u32,
    cpu_mirror: Vec<u8>,
    dirty_bounds: Option<Bounds<DevicePixels>>,
    upload_calls: u64,
    upload_bytes: u64,
    flush_calls: u64,
    flush_bytes: u64,
}

impl DirectXAtlas {
    pub(crate) fn new(device: &ID3D11Device, device_context: &ID3D11DeviceContext) -> Self {
        DirectXAtlas(Mutex::new(DirectXAtlasState {
            device: device.clone(),
            device_context: device_context.clone(),
            monochrome_textures: Default::default(),
            polychrome_textures: Default::default(),
            tiles_by_key: Default::default(),
        }))
    }

    pub(crate) fn get_texture_view(
        &self,
        id: AtlasTextureId,
    ) -> [Option<ID3D11ShaderResourceView>; 1] {
        let mut lock = self.0.lock();
        let device_context = lock.device_context.clone();
        let tex = lock.texture_mut(id);
        tex.flush(&device_context);
        tex.view.clone()
    }

    pub(crate) fn handle_device_lost(
        &self,
        device: &ID3D11Device,
        device_context: &ID3D11DeviceContext,
    ) {
        let mut lock = self.0.lock();
        lock.device = device.clone();
        lock.device_context = device_context.clone();
        lock.monochrome_textures = AtlasTextureList::default();
        lock.polychrome_textures = AtlasTextureList::default();
        lock.tiles_by_key.clear();
    }
}

impl PlatformAtlas for DirectXAtlas {
    fn get_or_insert_with<'a>(
        &self,
        key: &AtlasKey,
        build: &mut dyn FnMut() -> anyhow::Result<
            Option<(Size<DevicePixels>, std::borrow::Cow<'a, [u8]>)>,
        >,
    ) -> anyhow::Result<Option<AtlasTile>> {
        let mut lock = self.0.lock();
        if let Some(tile) = lock.tiles_by_key.get(key) {
            Ok(Some(tile.clone()))
        } else {
            let Some((size, bytes)) = build()? else {
                return Ok(None);
            };
            let tile = lock
                .allocate(size, key.texture_kind())
                .ok_or_else(|| anyhow::anyhow!("failed to allocate"))?;
            let texture = lock.texture_mut(tile.texture_id);
            texture.upload(tile.bounds, &bytes);
            lock.tiles_by_key.insert(key.clone(), tile.clone());
            Ok(Some(tile))
        }
    }

    fn remove(&self, key: &AtlasKey) {
        let mut lock = self.0.lock();

        let Some(id) = lock.tiles_by_key.remove(key).map(|tile| tile.texture_id) else {
            return;
        };

        let textures = match id.kind {
            AtlasTextureKind::Monochrome => &mut lock.monochrome_textures,
            AtlasTextureKind::Polychrome => &mut lock.polychrome_textures,
        };

        let Some(texture_slot) = textures.textures.get_mut(id.index as usize) else {
            return;
        };

        if let Some(mut texture) = texture_slot.take() {
            texture.decrement_ref_count();
            if texture.is_unreferenced() {
                textures.free_list.push(texture.id.index as usize);
                lock.tiles_by_key.remove(key);
            } else {
                *texture_slot = Some(texture);
            }
        }
    }

    fn diagnostic_snapshot(&self) -> AtlasDiagnosticSnapshot {
        self.0.lock().diagnostic_snapshot()
    }
}

impl DirectXAtlasState {
    fn diagnostic_snapshot(&self) -> AtlasDiagnosticSnapshot {
        const MAX_IMAGE_TILE_DIAGNOSTIC_ITEMS: usize = 64;

        let mut snapshot = AtlasDiagnosticSnapshot {
            tile_count: self.tiles_by_key.len(),
            ..Default::default()
        };
        for (key, tile) in &self.tiles_by_key {
            match key {
                AtlasKey::Glyph(_) => snapshot.glyph_tiles += 1,
                AtlasKey::Svg(_) => snapshot.svg_tiles += 1,
                AtlasKey::Image(params) => {
                    snapshot.image_tiles += 1;
                    let tile_bytes_estimate = bounds_bytes(tile.bounds, 4);
                    snapshot.image_tile_bytes_estimate = snapshot
                        .image_tile_bytes_estimate
                        .saturating_add(tile_bytes_estimate);
                    if snapshot.image_tile_items.len() < MAX_IMAGE_TILE_DIAGNOSTIC_ITEMS {
                        snapshot.image_tile_items.push(AtlasImageTileDiagnostic {
                            render_image_id: params.image_id.0,
                            frame_index: params.frame_index,
                            width: tile.bounds.size.width.0.max(0) as u32,
                            height: tile.bounds.size.height.0.max(0) as u32,
                            tile_bytes_estimate,
                        });
                    } else {
                        snapshot.image_tile_items_truncated = true;
                    }
                }
            }
        }
        snapshot
            .kinds
            .push(self.texture_list_diagnostic("monochrome", &self.monochrome_textures));
        snapshot
            .kinds
            .push(self.texture_list_diagnostic("polychrome", &self.polychrome_textures));
        snapshot
    }

    fn texture_list_diagnostic(
        &self,
        kind: &str,
        textures: &AtlasTextureList<DirectXAtlasTexture>,
    ) -> AtlasKindDiagnostic {
        let mut diagnostic = AtlasKindDiagnostic {
            kind: kind.to_string(),
            texture_count: 0,
            free_texture_slots: textures.free_list.len(),
            live_key_count: 0,
            gpu_texture_bytes_estimate: 0,
            cpu_mirror_bytes: 0,
            dirty_texture_count: 0,
            dirty_bytes_estimate: 0,
            upload_calls: 0,
            upload_bytes: 0,
            flush_calls: 0,
            flush_bytes: 0,
        };
        for texture in textures.textures.iter().flatten() {
            diagnostic.texture_count += 1;
            diagnostic.live_key_count = diagnostic
                .live_key_count
                .saturating_add(texture.live_atlas_keys as u64);
            diagnostic.gpu_texture_bytes_estimate = diagnostic
                .gpu_texture_bytes_estimate
                .saturating_add(texture.texture_bytes());
            diagnostic.cpu_mirror_bytes = diagnostic
                .cpu_mirror_bytes
                .saturating_add(texture.cpu_mirror.len() as u64);
            if let Some(dirty_bounds) = texture.dirty_bounds {
                diagnostic.dirty_texture_count += 1;
                diagnostic.dirty_bytes_estimate = diagnostic
                    .dirty_bytes_estimate
                    .saturating_add(texture.bounds_bytes(dirty_bounds));
            }
            diagnostic.upload_calls = diagnostic.upload_calls.saturating_add(texture.upload_calls);
            diagnostic.upload_bytes = diagnostic.upload_bytes.saturating_add(texture.upload_bytes);
            diagnostic.flush_calls = diagnostic.flush_calls.saturating_add(texture.flush_calls);
            diagnostic.flush_bytes = diagnostic.flush_bytes.saturating_add(texture.flush_bytes);
        }
        diagnostic
    }

    fn allocate(
        &mut self,
        size: Size<DevicePixels>,
        texture_kind: AtlasTextureKind,
    ) -> Option<AtlasTile> {
        {
            let textures = match texture_kind {
                AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
                AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
            };

            if let Some(tile) = textures
                .iter_mut()
                .rev()
                .find_map(|texture| texture.allocate(size))
            {
                return Some(tile);
            }
        }

        let texture = self.push_texture(size, texture_kind)?;
        texture.allocate(size)
    }

    fn push_texture(
        &mut self,
        min_size: Size<DevicePixels>,
        kind: AtlasTextureKind,
    ) -> Option<&mut DirectXAtlasTexture> {
        const DEFAULT_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(1024),
            height: DevicePixels(1024),
        };
        // Max texture size for DirectX. See:
        // https://learn.microsoft.com/en-us/windows/win32/direct3d11/overviews-direct3d-11-resources-limits
        const MAX_ATLAS_SIZE: Size<DevicePixels> = Size {
            width: DevicePixels(16384),
            height: DevicePixels(16384),
        };
        let size = min_size.min(&MAX_ATLAS_SIZE).max(&DEFAULT_ATLAS_SIZE);
        let pixel_format;
        let bind_flag;
        let bytes_per_pixel;
        match kind {
            AtlasTextureKind::Monochrome => {
                pixel_format = DXGI_FORMAT_R8_UNORM;
                bind_flag = D3D11_BIND_SHADER_RESOURCE;
                bytes_per_pixel = 1;
            }
            AtlasTextureKind::Polychrome => {
                pixel_format = DXGI_FORMAT_B8G8R8A8_UNORM;
                bind_flag = D3D11_BIND_SHADER_RESOURCE;
                bytes_per_pixel = 4;
            }
        }
        let texture_desc = D3D11_TEXTURE2D_DESC {
            Width: size.width.0 as u32,
            Height: size.height.0 as u32,
            MipLevels: 1,
            ArraySize: 1,
            Format: pixel_format,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: bind_flag.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture: Option<ID3D11Texture2D> = None;
        unsafe {
            // This only returns None if the device is lost, which we will recreate later.
            // So it's ok to return None here.
            self.device
                .CreateTexture2D(&texture_desc, None, Some(&mut texture))
                .ok()?;
        }
        let texture = texture.unwrap();

        let texture_list = match kind {
            AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
        };
        let index = texture_list.free_list.pop();
        let view = unsafe {
            let mut view = None;
            self.device
                .CreateShaderResourceView(&texture, None, Some(&mut view))
                .ok()?;
            [view]
        };
        let atlas_texture = DirectXAtlasTexture {
            id: AtlasTextureId {
                index: index.unwrap_or(texture_list.textures.len()) as u32,
                kind,
            },
            size,
            bytes_per_pixel,
            allocator: etagere::BucketedAtlasAllocator::new(size.into()),
            texture,
            view,
            live_atlas_keys: 0,
            cpu_mirror: vec![
                0;
                size.width.0.max(0) as usize
                    * size.height.0.max(0) as usize
                    * bytes_per_pixel as usize
            ],
            dirty_bounds: None,
            upload_calls: 0,
            upload_bytes: 0,
            flush_calls: 0,
            flush_bytes: 0,
        };
        if let Some(ix) = index {
            texture_list.textures[ix] = Some(atlas_texture);
            texture_list.textures.get_mut(ix).unwrap().as_mut()
        } else {
            texture_list.textures.push(Some(atlas_texture));
            texture_list.textures.last_mut().unwrap().as_mut()
        }
    }

    fn texture_mut(&mut self, id: AtlasTextureId) -> &mut DirectXAtlasTexture {
        let textures = match id.kind {
            crate::AtlasTextureKind::Monochrome => &mut self.monochrome_textures,
            crate::AtlasTextureKind::Polychrome => &mut self.polychrome_textures,
        };
        textures.textures[id.index as usize].as_mut().unwrap()
    }
}

impl DirectXAtlasTexture {
    fn allocate(&mut self, size: Size<DevicePixels>) -> Option<AtlasTile> {
        let allocation = self.allocator.allocate(size.into())?;
        let tile = AtlasTile {
            texture_id: self.id,
            tile_id: allocation.id.into(),
            bounds: Bounds {
                origin: allocation.rectangle.min.into(),
                size,
            },
            padding: 0,
        };
        self.live_atlas_keys += 1;
        Some(tile)
    }

    fn upload(&mut self, bounds: Bounds<DevicePixels>, bytes: &[u8]) {
        let Some(bounds) = self.clamp_bounds(bounds) else {
            return;
        };

        let bytes_per_pixel = self.bytes_per_pixel as usize;
        let texture_width = self.size.width.0.max(0) as usize;
        let source_width = bounds.size.width.0.max(0) as usize;
        let source_height = bounds.size.height.0.max(0) as usize;
        let source_pitch = source_width * bytes_per_pixel;
        let destination_pitch = texture_width * bytes_per_pixel;
        let destination_x = bounds.left().0.max(0) as usize;
        let destination_y = bounds.top().0.max(0) as usize;

        if source_pitch == 0 || source_height == 0 {
            return;
        }

        for row in 0..source_height {
            let source_start = row * source_pitch;
            let source_end = source_start + source_pitch;
            if source_end > bytes.len() {
                return;
            }

            let destination_start =
                (destination_y + row) * destination_pitch + destination_x * bytes_per_pixel;
            let destination_end = destination_start + source_pitch;
            if destination_end > self.cpu_mirror.len() {
                return;
            }

            self.cpu_mirror[destination_start..destination_end]
                .copy_from_slice(&bytes[source_start..source_end]);
        }

        self.dirty_bounds = Some(match self.dirty_bounds {
            Some(dirty_bounds) => dirty_bounds.union(&bounds),
            None => bounds,
        });
        self.upload_calls = self.upload_calls.saturating_add(1);
        self.upload_bytes = self.upload_bytes.saturating_add(bounds_bytes(
            Bounds {
                origin: Point {
                    x: DevicePixels(0),
                    y: DevicePixels(0),
                },
                size: Size {
                    width: DevicePixels(source_width as i32),
                    height: DevicePixels(source_height as i32),
                },
            },
            self.bytes_per_pixel,
        ));
    }

    fn flush(&mut self, device_context: &ID3D11DeviceContext) {
        let Some(bounds) = self.dirty_bounds.take() else {
            return;
        };
        let Some(bounds) = self.clamp_bounds(bounds) else {
            return;
        };

        let bytes_per_pixel = self.bytes_per_pixel as usize;
        let texture_width = self.size.width.0.max(0) as usize;
        let row_pitch = texture_width * bytes_per_pixel;
        let source_offset = bounds.top().0.max(0) as usize * row_pitch
            + bounds.left().0.max(0) as usize * bytes_per_pixel;
        let Some(source) = self.cpu_mirror.get(source_offset..) else {
            return;
        };
        let flush_bytes = self.bounds_bytes(bounds);

        unsafe {
            device_context.UpdateSubresource(
                &self.texture,
                0,
                Some(&D3D11_BOX {
                    left: bounds.left().0 as u32,
                    top: bounds.top().0 as u32,
                    front: 0,
                    right: bounds.right().0 as u32,
                    bottom: bounds.bottom().0 as u32,
                    back: 1,
                }),
                source.as_ptr() as _,
                row_pitch as u32,
                0,
            );
        }
        self.flush_calls = self.flush_calls.saturating_add(1);
        self.flush_bytes = self.flush_bytes.saturating_add(flush_bytes);
    }

    fn clamp_bounds(&self, bounds: Bounds<DevicePixels>) -> Option<Bounds<DevicePixels>> {
        let left = bounds.left().0.max(0).min(self.size.width.0);
        let top = bounds.top().0.max(0).min(self.size.height.0);
        let right = bounds.right().0.max(left).min(self.size.width.0);
        let bottom = bounds.bottom().0.max(top).min(self.size.height.0);

        if right <= left || bottom <= top {
            return None;
        }

        Some(Bounds {
            origin: Point {
                x: DevicePixels(left),
                y: DevicePixels(top),
            },
            size: Size {
                width: DevicePixels(right - left),
                height: DevicePixels(bottom - top),
            },
        })
    }

    fn decrement_ref_count(&mut self) {
        self.live_atlas_keys -= 1;
    }

    fn is_unreferenced(&mut self) -> bool {
        self.live_atlas_keys == 0
    }

    fn texture_bytes(&self) -> u64 {
        bounds_bytes(
            Bounds {
                origin: Point {
                    x: DevicePixels(0),
                    y: DevicePixels(0),
                },
                size: self.size,
            },
            self.bytes_per_pixel,
        )
    }

    fn bounds_bytes(&self, bounds: Bounds<DevicePixels>) -> u64 {
        bounds_bytes(bounds, self.bytes_per_pixel)
    }
}

fn bounds_bytes(bounds: Bounds<DevicePixels>, bytes_per_pixel: u32) -> u64 {
    let width = bounds.size.width.0.max(0) as u64;
    let height = bounds.size.height.0.max(0) as u64;
    width
        .saturating_mul(height)
        .saturating_mul(bytes_per_pixel as u64)
}

impl From<Size<DevicePixels>> for etagere::Size {
    fn from(size: Size<DevicePixels>) -> Self {
        etagere::Size::new(size.width.into(), size.height.into())
    }
}

impl From<etagere::Point> for Point<DevicePixels> {
    fn from(value: etagere::Point) -> Self {
        Point {
            x: DevicePixels::from(value.x),
            y: DevicePixels::from(value.y),
        }
    }
}
