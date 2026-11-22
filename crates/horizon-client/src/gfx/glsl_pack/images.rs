//! A pack's custom images (`image.*`): storage textures its programs write
//! and read, and sample under the declaration's sampler name.
//!
//! Each image is created once at its declared size, or at a factor of the
//! screen size and again when the window resizes (zeroed then). One whose
//! declaration says so is cleared to zero at the start of every frame by a
//! compute dispatch (WebGPU has no texture clear); the others keep their
//! texels across frames.
//!
//! WebGPU checks usages per render pass: a texture written as storage by one
//! pipeline of a pass may not be read (sampled or read-only storage) by
//! another pipeline of the same pass. The gbuffers and shadow passes draw
//! several programs, so such a read gets a stand-in instead (zeros), see
//! [`Images::conflicts`].

use std::collections::{BTreeMap, BTreeSet};

use horizon_glsl_pack::images::{ImageDim, ImageSize, ImageSpec};
use horizon_glsl_pack::plan::StorageFormat;
use horizon_glsl_pack::rewrite::ImageAccess;
use horizon_glsl_pack::{CompiledPack, ImageSource, ProgramKind, TextureSource};

/// The wgpu format and the WGSL name of an image storage format.
pub(super) fn wgpu_format(format: StorageFormat) -> (wgpu::TextureFormat, &'static str) {
    use StorageFormat as S;
    use wgpu::TextureFormat as T;
    match format {
        S::Rgba8Unorm | S::R8Unorm | S::Rg8Unorm => (T::Rgba8Unorm, "rgba8unorm"),
        S::R32Float => (T::R32Float, "r32float"),
        S::Rg32Float => (T::Rg32Float, "rg32float"),
        S::Rgba32Float => (T::Rgba32Float, "rgba32float"),
        S::Rgba8Sint => (T::Rgba8Sint, "rgba8sint"),
        S::Rgba16Sint => (T::Rgba16Sint, "rgba16sint"),
        S::R32Sint | S::R8Sint | S::R16Sint => (T::R32Sint, "r32sint"),
        S::Rg32Sint | S::Rg8Sint | S::Rg16Sint => (T::Rg32Sint, "rg32sint"),
        S::Rgba32Sint => (T::Rgba32Sint, "rgba32sint"),
        S::Rgba8Uint => (T::Rgba8Uint, "rgba8uint"),
        S::Rgba16Uint | S::Rgb10a2Uint => (T::Rgba16Uint, "rgba16uint"),
        S::R32Uint | S::R8Uint | S::R16Uint => (T::R32Uint, "r32uint"),
        S::Rg32Uint | S::Rg8Uint | S::Rg16Uint => (T::Rg32Uint, "rg32uint"),
        S::Rgba32Uint => (T::Rgba32Uint, "rgba32uint"),
        S::Rgba16Float | S::R16Float | S::Rg16Float | S::Rg11b10Ufloat | S::Rgb10a2Unorm => {
            (T::Rgba16Float, "rgba16float")
        }
    }
}

pub(super) fn storage_access(access: ImageAccess) -> wgpu::StorageTextureAccess {
    match access {
        ImageAccess::ReadOnly => wgpu::StorageTextureAccess::ReadOnly,
        ImageAccess::WriteOnly => wgpu::StorageTextureAccess::WriteOnly,
        ImageAccess::ReadWrite => wgpu::StorageTextureAccess::ReadWrite,
    }
}

pub(super) fn view_dimension(dim: ImageDim) -> wgpu::TextureViewDimension {
    match dim {
        ImageDim::D1 => wgpu::TextureViewDimension::D1,
        ImageDim::D2 => wgpu::TextureViewDimension::D2,
        ImageDim::D3 => wgpu::TextureViewDimension::D3,
    }
}

fn texture_dimension(dim: ImageDim) -> wgpu::TextureDimension {
    match dim {
        ImageDim::D1 => wgpu::TextureDimension::D1,
        ImageDim::D2 => wgpu::TextureDimension::D2,
        ImageDim::D3 => wgpu::TextureDimension::D3,
    }
}

/// One image's texture and views.
struct Image {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    extent: [u32; 3],
}

fn create(device: &wgpu::Device, spec: &ImageSpec, extent: [u32; 3]) -> Image {
    let dim = spec.size.dim();
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some(&format!("Shader pack image {}", spec.name)),
        size: wgpu::Extent3d {
            width: extent[0],
            height: extent[1],
            depth_or_array_layers: extent[2],
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: texture_dimension(dim),
        format: wgpu_format(spec.storage()).0,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(view_dimension(dim)),
        ..Default::default()
    });
    Image {
        texture,
        view,
        extent,
    }
}

/// The clear of one (format, dimension): a compute pipeline storing zeros.
struct Clear {
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
}

fn clear_pipeline(device: &wgpu::Device, format: StorageFormat, dim: ImageDim) -> Clear {
    let (wgpu_format, name) = wgpu_format(format);
    let ty = match name {
        n if n.ends_with("sint") => "i32",
        n if n.ends_with("uint") => "u32",
        _ => "f32",
    };
    let (d, size, coord, test) = match dim {
        ImageDim::D1 => ("1d", "64, 1, 1", "id.x", "id.x >= textureDimensions(img)"),
        ImageDim::D2 => (
            "2d",
            "8, 8, 1",
            "id.xy",
            "any(id.xy >= textureDimensions(img))",
        ),
        ImageDim::D3 => ("3d", "4, 4, 4", "id", "any(id >= textureDimensions(img))"),
    };
    let wgsl = format!(
        "@group(0) @binding(0) var img: texture_storage_{d}<{name}, write>;\n\
         @compute @workgroup_size({size}) fn main(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
         if ({test}) {{ return; }}\n\
         textureStore(img, {coord}, vec4<{ty}>(0));\n}}\n"
    );
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Shader pack image clear"),
        source: wgpu::ShaderSource::Wgsl(wgsl.into()),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Shader pack image clear"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::COMPUTE,
            ty: wgpu::BindingType::StorageTexture {
                access: wgpu::StorageTextureAccess::WriteOnly,
                format: wgpu_format,
                view_dimension: view_dimension(dim),
            },
            count: None,
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Shader pack image clear"),
        bind_group_layouts: &[Some(&layout)],
        ..Default::default()
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("Shader pack image clear"),
        layout: Some(&pipeline_layout),
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    Clear { pipeline, layout }
}

/// Which pass a program draws in: programs of one pass share a usage scope.
fn pass_of(kind: ProgramKind) -> Option<u8> {
    if kind.is_fullscreen() {
        return None;
    }
    if kind.is_shadow() {
        return Some(0);
    }
    Some(
        if matches!(
            kind,
            ProgramKind::Water
                | ProgramKind::HandWater
                | ProgramKind::EntitiesTranslucent
                | ProgramKind::BlockTranslucent
                | ProgramKind::ParticlesTranslucent
                | ProgramKind::Weather
        ) {
            2
        } else {
            1
        },
    )
}

pub(super) struct Images {
    specs: Vec<ImageSpec>,
    /// By index in `PackPlan::images`; `None` for images no program uses.
    images: Vec<Option<Image>>,
    /// A 1×1(×1) texture of each used image's format, for reads that would
    /// conflict with a write in the same pass.
    stand_ins: Vec<Option<Image>>,
    clears: BTreeMap<(StorageFormat, ImageDim), Clear>,
    /// The screen size the relative images were made for.
    size: [u32; 2],
    /// Per program, the image bindings that read a stand-in.
    pub conflicts: BTreeMap<ProgramKind, BTreeSet<u32>>,
}

impl Images {
    pub fn new(device: &wgpu::Device, pack: &CompiledPack) -> Result<Self, String> {
        let specs = pack.plan.images.clone();
        let mut used = vec![false; specs.len()];
        // Per pass and image: programs writing it, programs reading it.
        let mut uses: BTreeMap<(u8, usize), (Vec<ProgramKind>, Vec<(ProgramKind, u32)>)> =
            BTreeMap::new();
        for (&kind, program) in &pack.programs {
            for b in &program.images {
                let ImageSource::Custom(i) = b.source;
                used[i] = true;
                if let Some(pass) = pass_of(kind) {
                    let entry = uses.entry((pass, i)).or_default();
                    if b.access == ImageAccess::ReadOnly {
                        entry.1.push((kind, b.binding));
                    } else {
                        entry.0.push(kind);
                    }
                }
            }
            for t in &program.textures {
                if let TextureSource::Image(i) = t.source {
                    used[i] = true;
                    if let Some(pass) = pass_of(kind) {
                        uses.entry((pass, i)).or_default().1.push((kind, t.binding));
                    }
                }
            }
        }
        // Compute dispatches are their own usage scopes: no conflicts.
        for c in pack.computes.values() {
            for b in &c.images {
                let ImageSource::Custom(i) = b.source;
                used[i] = true;
            }
            for t in &c.textures {
                if let TextureSource::Image(i) = t.source {
                    used[i] = true;
                }
            }
        }
        let mut conflicts: BTreeMap<ProgramKind, BTreeSet<u32>> = BTreeMap::new();
        for ((_, i), (writers, readers)) in &uses {
            if writers.is_empty() {
                continue;
            }
            for &(kind, binding) in readers {
                log::warn!(
                    "shader pack image {}: {kind:?} reads it in the pass {:?} writes it in; it reads zeros",
                    specs[*i].name,
                    writers[0]
                );
                conflicts.entry(kind).or_default().insert(binding);
            }
        }
        let limits = device.limits();
        let mut images = Vec::new();
        let mut stand_ins = Vec::new();
        let mut clears = BTreeMap::new();
        for (spec, &used) in specs.iter().zip(&used) {
            if !used {
                images.push(None);
                stand_ins.push(None);
                continue;
            }
            let max = match spec.size.dim() {
                ImageDim::D1 => limits.max_texture_dimension_1d,
                ImageDim::D2 => limits.max_texture_dimension_2d,
                ImageDim::D3 => limits.max_texture_dimension_3d,
            };
            let extent = spec.size.extent(1, 1);
            if extent.iter().any(|&e| e > max) {
                return Err(format!(
                    "image {} is {extent:?} texels, the device allows {max} a side",
                    spec.name
                ));
            }
            let relative = matches!(spec.size, ImageSize::Relative(..));
            images.push((!relative).then(|| create(device, spec, extent)));
            stand_ins.push(Some(create(device, spec, [1, 1, 1])));
            if spec.clear {
                clears
                    .entry((spec.storage(), spec.size.dim()))
                    .or_insert_with(|| clear_pipeline(device, spec.storage(), spec.size.dim()));
            }
        }
        Ok(Self {
            specs,
            images,
            stand_ins,
            clears,
            size: [0, 0],
            conflicts,
        })
    }

    /// (Re)creates the screen-relative images for a screen of `size`.
    pub fn prepare(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.size == size {
            return;
        }
        self.size = size;
        for (i, spec) in self.specs.iter().enumerate() {
            if matches!(spec.size, ImageSize::Relative(..)) && self.stand_ins[i].is_some() {
                let extent = spec.size.extent(size[0], size[1]);
                self.images[i] = Some(create(device, spec, extent));
            }
        }
    }

    /// Zeroes the images declared to be cleared every frame.
    pub fn clear(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder) {
        if self.clears.is_empty() {
            return;
        }
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("Shader pack image clears"),
            timestamp_writes: None,
        });
        for (spec, image) in self.specs.iter().zip(&self.images) {
            let (Some(image), true) = (image, spec.clear) else {
                continue;
            };
            let clear = &self.clears[&(spec.storage(), spec.size.dim())];
            let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Shader pack image clear"),
                layout: &clear.layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&image.view),
                }],
            });
            let [w, h, d] = image.extent;
            let groups = match spec.size.dim() {
                ImageDim::D1 => [w.div_ceil(64), 1, 1],
                ImageDim::D2 => [w.div_ceil(8), h.div_ceil(8), 1],
                ImageDim::D3 => [w.div_ceil(4), h.div_ceil(4), d.div_ceil(4)],
            };
            pass.set_pipeline(&clear.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(groups[0], groups[1], groups[2]);
        }
    }

    /// The view `program` binds at `binding` for image `i`: the image, or
    /// its stand-in when that read conflicts with a write in the same pass.
    pub fn view(
        &self,
        kind: Option<ProgramKind>,
        binding: u32,
        i: usize,
    ) -> Option<&wgpu::TextureView> {
        let conflict = kind
            .and_then(|k| self.conflicts.get(&k))
            .is_some_and(|b| b.contains(&binding));
        let stand_in = self.stand_ins.get(i)?.as_ref();
        let image = if conflict {
            stand_in
        } else {
            self.images.get(i)?.as_ref().or(stand_in)
        };
        image.map(|i| &i.view)
    }

    /// Bytes the images take (for the log).
    pub fn bytes(&self) -> u64 {
        self.images
            .iter()
            .flatten()
            .map(|i| {
                let [w, h, d] = i.extent;
                u64::from(w)
                    * u64::from(h)
                    * u64::from(d)
                    * u64::from(i.texture.format().block_copy_size(None).unwrap_or(4))
            })
            .sum()
    }
}
