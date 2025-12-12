use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time;

use cxx::SharedPtr;
use imgui_winit_support::WinitPlatform;
use wgpu::util::DeviceExt as _;
use winit::dpi::LogicalSize;
use winit::event_loop::ActiveEventLoop;

use crate::ext::ResultExt as _;
use crate::{Box2d, Controls, MapRendererMemberExt as _, Point, Projection, ScreenMapRenderer, ScreenMapRendererBuffer, ScreenMapRendererBuffers, ScreenMapRendererJoinHandle};

pub(crate) struct ImGuiState {
    pub(crate) context: imgui::Context,
    pub(crate) platform: WinitPlatform,
    pub(crate) renderer: imgui_wgpu::Renderer,
    pub(crate) clear_color: wgpu::Color,
    pub(crate) last_frame: time::Instant,
}

impl ImGuiState {
    pub fn new(
        ini_filename: impl Into<Option<PathBuf>>,
        window: Arc<winit::window::Window>,
        hidpi_factor: f64,
        texture_format: wgpu::TextureFormat,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
    ) -> anyhow::Result<ImGuiState> {
        let mut context = imgui::Context::create();
        let mut platform = imgui_winit_support::WinitPlatform::new(&mut context);
        platform.attach_window(
            context.io_mut(),
            &window,
            imgui_winit_support::HiDpiMode::Default
        );

        context.set_ini_filename(ini_filename);

        let font_size = (13.0 * hidpi_factor) as f32;
        context.io_mut().font_global_scale = (1.0 / hidpi_factor) as f32;

        context.fonts().add_font(&[imgui::FontSource::DefaultFontData {
            config: Some(imgui::FontConfig {
                oversample_h: 1,
                pixel_snap_h: true,
                size_pixels: font_size,
                ..Default::default()
            }),
        }]);

        let clear_color = wgpu::Color::WHITE;

        let ren_config = imgui_wgpu::RendererConfig {
            texture_format,
            ..Default::default()
        };

        let renderer = imgui_wgpu::Renderer::new(&mut context, &device, &queue, ren_config);

        Ok(ImGuiState {
            context,
            platform,
            renderer,
            clear_color,
            last_frame: time::Instant::now(),
        })
    }
}

pub(crate) struct UserDataStatic {
    w: u32, h: u32,
    input_projection: SharedPtr<Projection>,
    output_projection: SharedPtr<Projection>,
    units_per_pixel_scale: f32,
}

impl UserDataStatic {
    pub(crate) fn new(controls: &Controls) -> Self {
        Self {
            w: controls.map_width,
            h: controls.map_height,
            input_projection: controls.input_projection(),
            output_projection: controls.output_projection(),
            units_per_pixel_scale: controls.units_per_pixel_scale,
        }
    }
}

unsafe impl Send for UserDataStatic {}
unsafe impl Sync for UserDataStatic {}

pub(crate) struct MapExplorerWindow {
    pub(crate) device: wgpu::Device,
    pub(crate) queue: wgpu::Queue,
    pub(crate) window: Arc<winit::window::Window>,
    // logical_size: LogicalSize<f64>,
    #[allow(unused)]
    pub(crate) surface_desc: wgpu::SurfaceConfiguration,
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) hidpi_factor: f64,
    pub(crate) imgui: ImGuiState,

    pub(crate) controls: Controls,
    pub(crate) map_def_file: PathBuf,
    pub(crate) basepath: PathBuf,
    pub(crate) map_texture: wgpu::Texture,
    pub(crate) map_view: wgpu::TextureView,
    pub(crate) map_sampler: wgpu::Sampler,
    pub(crate) map_bind_group: wgpu::BindGroup,
    pub(crate) map_bind_group_layout: wgpu::BindGroupLayout,
    pub(crate) map_pipeline: wgpu::RenderPipeline,
    pub(crate) map_renderer_join: Option<ScreenMapRendererJoinHandle>,
    pub(crate) ud_sender: mpsc::SyncSender<(f32, f32, Arc<UserDataStatic>)>,
    pub(crate) buffers: Arc<Mutex<ScreenMapRendererBuffers<2, (f32, f32, Arc<UserDataStatic>)>>>,
    pub(crate) curr_buffer: Option<ScreenMapRendererBuffer<2, (f32, f32, Arc<UserDataStatic>)>>,

    // mouse_delta: (f64, f64),
    pub(crate) map_delta: wgpu::Buffer,
    pub(crate) map_delta_bind_group: wgpu::BindGroup,

    pub(crate) mouse_pressed: bool,

    pub(crate) static_user_data: Arc<UserDataStatic>,
}

fn create_map_texture(
    device: &wgpu::Device,
    surface_desc: &wgpu::SurfaceConfiguration,
    map_bind_group_layout: &wgpu::BindGroupLayout,
    w: u32, h: u32,
) -> (
    wgpu::Texture,
    wgpu::TextureView,
    wgpu::Sampler,
    wgpu::BindGroup,
) {
    let map_texture = device.create_texture(&wgpu::TextureDescriptor {
        // size: wgpu::Extent3d { width: w as u32, height: h as u32, depth_or_array_layers: 1 },
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: surface_desc.format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        label: Some("MapTexture"),
        view_formats: &[]
    });
    let map_view = map_texture.create_view(&wgpu::TextureViewDescriptor::default());
    let map_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        address_mode_u: wgpu::AddressMode::ClampToEdge,
        address_mode_v: wgpu::AddressMode::ClampToEdge,
        address_mode_w: wgpu::AddressMode::ClampToEdge,
        mag_filter: wgpu::FilterMode::Linear,
        min_filter: wgpu::FilterMode::Linear,
        mipmap_filter: wgpu::FilterMode::Nearest,
        ..Default::default()
    });

    let map_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Map Bind Group"),
        layout: &map_bind_group_layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(&map_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(&map_sampler),
            }
        ]
    });

    return (map_texture, map_view, map_sampler, map_bind_group);
}

fn create_map_renderer<const N: usize>(
    controls: &Controls,
    map_def_file: impl AsRef<Path>,
    base_path: impl AsRef<Path>,
    static_user_data: Arc<UserDataStatic>,
) -> anyhow::Result<(
    ScreenMapRenderer<N, (f32, f32, Arc<UserDataStatic>)>,
    // ScreenMapRendererJoinHandle,
    // SyncSender<(f32, f32)>,
    Arc<Mutex<ScreenMapRendererBuffers<N, (f32, f32, Arc<UserDataStatic>)>>>
)> {
    // let (map_renderer, buffers) = ScreenMapRenderer::new_from_file(w as u32, h as u32, map_def_file, "./data/build", (controls.center_x, controls.center_y))?;
    // let c = controls.clone();
    // let controls = controls.read().anyhow()?;
    let (map_renderer, buffers) = ScreenMapRenderer::new_from_file(
        controls.map_width, controls.map_height,
        map_def_file, base_path,
        (controls.center_x, controls.center_y, static_user_data),
        Box::new(|map_renderer, ud| {
            let u: &Arc<UserDataStatic> = &ud.2;
            let bbox = Box2d::<f64>::new_centered(
                &Point::<f64>::new(ud.0 as f64, ud.1 as f64),
                u.input_projection.clone(),
                u.output_projection.clone(),
                u.units_per_pixel_scale.into(),
                u.w,
                u.h
            );
            map_renderer.pin_mut()
                .zoom_to_box(&bbox);
        })
    )?;
    #[allow(deprecated)] // TODO
    let map_renderer_and_ud = map_renderer.map_renderer_and_user_data();
    let bbox = controls.create_center_box(controls.map_width, controls.map_height); // TODO: replace with an on_receive user data callback
    map_renderer_and_ud.lock()
        .anyhow()?
        .map_renderer_mut()
        .pin_mut()
        .zoom_to_box(&bbox);

    Ok((
        map_renderer,
        buffers
    ))
}

#[repr(C)]
#[derive(Debug, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct MapDeltaUniform {
    delta: [f32; 2],
}

impl MapExplorerWindow {
    pub(crate) async fn new(
        w: usize, h: usize,
        event_loop: &ActiveEventLoop,
        map_def_file: impl AsRef<Path>, basepath: impl AsRef<Path>,
        inifilename: impl AsRef<Path>,
        cachefile: impl AsRef<Path>
    ) -> anyhow::Result<Self> {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        let window = {
            let size = LogicalSize::new(w as f64, h as f64);
            let attributes = winit::window::Window::default_attributes()
                .with_inner_size(winit::dpi::Size::Logical(size))
                .with_title("MapExplorer");
            Arc::new(event_loop.create_window(attributes)?)
        };
        window.set_resizable(false);

        let size = window.inner_size();
        let hidpi_factor = window.scale_factor();
        let surface = instance.create_surface(window.clone())?;

        let adapter = instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance, // TODO: LowPower?
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }).await?;

        let (device, queue) = adapter.request_device(&wgpu::DeviceDescriptor::default()).await?;

        let surface_desc = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            width: size.width,
            height: size.height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![wgpu::TextureFormat::Bgra8Unorm],
        };

        surface.configure(&device, &surface_desc);

        let imgui = ImGuiState::new(
            Some(inifilename.as_ref().to_path_buf()),
            window.clone(),
            hidpi_factor,
            surface_desc.format,
            &device,
            &queue
        )?;

        let controls = if cachefile.as_ref().exists() {
            Controls::read(cachefile)?
        } else {
            Controls::default()
        };
        let static_user_data = Arc::new(UserDataStatic::new(&controls));

        let (
            map_renderer,
            buffers
        ) = create_map_renderer(&controls, map_def_file.as_ref(), basepath.as_ref(), static_user_data.clone())?;
        let (join, ud_sender) = map_renderer.start();

        let map_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Map Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float {
                            filterable: true
                        },
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let (
            map_texture,
            map_view,
            map_sampler,
            map_bind_group
        ) = create_map_texture(&device, &surface_desc, &map_bind_group_layout, controls.map_width, controls.map_height);

        let map_delta = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Map Delta Uniform Buffer"),
            contents: bytemuck::cast_slice(&[MapDeltaUniform { delta: [0., 0.] }]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let map_delta_bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Map Delta Bind Group Layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }
            ],
        });
        let map_delta_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Map Delta Bind Group"),
            layout: &map_delta_bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: map_delta.as_entire_binding(),
                }
            ],
        });

        let map_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Map Pipeline Layout"),
            bind_group_layouts: &[
                &map_bind_group_layout,
                &map_delta_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });
        let map_shader = device.create_shader_module(wgpu::include_wgsl!("map_shader.wgsl"));
        let map_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Render Pipeline"),
            layout: Some(&map_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &map_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &map_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_desc.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                polygon_mode: wgpu::PolygonMode::Fill,
                unclipped_depth: false,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
            cache: None,
        });

        Ok(Self {
            device,
            queue,
            window,
            surface_desc,
            surface,
            hidpi_factor,
            imgui,

            controls,
            map_def_file: map_def_file.as_ref().to_path_buf(),
            basepath: basepath.as_ref().to_path_buf(),
            map_texture,
            map_view,
            map_sampler,
            map_bind_group_layout,
            map_bind_group,
            map_pipeline,
            map_renderer_join: Some(join),
            ud_sender,
            buffers,
            curr_buffer: None,

            map_delta,
            map_delta_bind_group,

            mouse_pressed: false,
            static_user_data
        })
    }

    pub(crate) fn update_buffer(&mut self) -> anyhow::Result<()> {
        match self.buffers.try_lock() {
            Ok(mut buffers) => {
                if let Some(buffer) = buffers.get_buffer() {
                    self.curr_buffer = Some(buffer);
                }

                if let Some(buffer) = &self.curr_buffer {
                    self.queue.write_texture(
                        wgpu::TexelCopyTextureInfo {
                            texture: &self.map_texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d::ZERO,
                            aspect: wgpu::TextureAspect::All,
                        },
                        buffer.buffer(),
                        wgpu::TexelCopyBufferLayout {
                            offset: 0,
                            bytes_per_row: Some(self.controls.map_width * 4),
                            rows_per_image: Some(self.controls.map_height)
                        },
                        wgpu::Extent3d {
                            width: self.controls.map_width,
                            height: self.controls.map_height,
                            depth_or_array_layers: 1
                        }
                    );

                    let ud = buffer.user_data();
                    // TODO: proj_transform
                    let delta = [
                        ((self.controls.center_x - ud.0) / (self.controls.map_width as f32)) / (self.hidpi_factor as f32 * super::TEMP_OFFSET),
                        (-1. * (self.controls.center_y - ud.1) / (self.controls.map_height as f32)) / (self.hidpi_factor as f32 * super::TEMP_OFFSET)
                    ];
                    self.queue.write_buffer(&self.map_delta, 0, bytemuck::cast_slice(&[MapDeltaUniform { delta }]));
                }
            },
            Err(err) => match err {
                std::sync::TryLockError::Poisoned(poison_error) => return Err(anyhow::format_err!("{}", poison_error)),
                std::sync::TryLockError::WouldBlock => {},
            },
        }

        Ok(())
    }

    // TODO: very buggy
    pub(crate) fn resize_map(&mut self, w: u32, h: u32) -> Result<(), ResizeMapResult> {
        if w == 0 || h == 0 {
            return Err(ResizeMapResult::Size0);
        }

        if let Some(handle) = self.map_renderer_join.take() {
            handle.join()?;
        }

        let (
            map_renderer,
            buffers
        ) = create_map_renderer(&self.controls, &self.map_def_file, &self.basepath, self.static_user_data.clone())?;
        self.buffers = buffers;
        self.curr_buffer = None;

        let limits = self.device.limits();
        if w > limits.max_texture_dimension_2d || h > limits.max_texture_dimension_2d {
            return Err(ResizeMapResult::SizeTooBig);
        }

        self.controls.map_width = w; // TODO: restrict pub access to map_width
        self.controls.map_height = h;
        self.static_user_data = Arc::new(UserDataStatic::new(&self.controls));

        let (
            map_texture,
            map_view,
            map_sampler,
            map_bind_group
        ) = create_map_texture(&self.device, &self.surface_desc, &self.map_bind_group_layout, self.controls.map_width, self.controls.map_height);
        self.map_texture = map_texture;
        self.map_view = map_view;
        self.map_sampler = map_sampler;
        self.map_bind_group = map_bind_group;

        let (join, ud_sender) = map_renderer.start();

        self.map_renderer_join = Some(join);
        self.ud_sender = ud_sender;
        self.ud_sender.send((self.controls.center_x, self.controls.center_y, self.static_user_data.clone())).map_err(|err| anyhow::format_err!("{}", err))?;

        Ok(())
    }
}

#[derive(Debug)]
pub(crate) enum ResizeMapResult {
    Size0,
    SizeTooBig,
    Error(anyhow::Error),
}

impl From<anyhow::Error> for ResizeMapResult {
    fn from(value: anyhow::Error) -> Self {
        Self::Error(value)
    }
}

impl Drop for MapExplorerWindow {
    fn drop(&mut self) {
        if let Some(handle) = self.map_renderer_join.take() {
            handle.join().unwrap();
        }
    }
}
