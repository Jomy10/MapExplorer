use std::fs;
use std::time::{self, Instant};
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};

use cxx::{let_cxx_string, SharedPtr};
use imgui::Condition;
use imgui_winit_support::WinitPlatform;
use log::*;
use log4rs::append::console::ConsoleAppender;
use log4rs::config::{Appender, Logger};
use log4rs::encode::pattern::PatternEncoder;
use map_explorer::ext::ResultExt;
use map_explorer::ffi::{new_Pipe, new_PipeInputStream, new_PipeOutputStream, ostream};
use map_explorer::*;
use regex::Regex;
use wgpu::util::DeviceExt;
use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;

// Fix until proper moving is implemented
const TEMP_OFFSET: f32 = 1.5;

struct ImGuiState {
    context: imgui::Context,
    platform: WinitPlatform,
    renderer: imgui_wgpu::Renderer,
    clear_color: wgpu::Color,
    last_frame: time::Instant,
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

struct UserDataStatic {
    w: u32, h: u32,
    input_projection: SharedPtr<Projection>,
    output_projection: SharedPtr<Projection>,
    units_per_pixel_scale: f32,
}

impl UserDataStatic {
    fn new(controls: &Controls) -> Self {
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

struct MapExplorerWindow {
    device: wgpu::Device,
    queue: wgpu::Queue,
    window: Arc<winit::window::Window>,
    // logical_size: LogicalSize<f64>,
    #[allow(unused)]
    surface_desc: wgpu::SurfaceConfiguration,
    surface: wgpu::Surface<'static>,
    hidpi_factor: f64,
    imgui: ImGuiState,

    controls: Controls,
    map_def_file: PathBuf,
    basepath: PathBuf,
    map_texture: wgpu::Texture,
    map_view: wgpu::TextureView,
    map_sampler: wgpu::Sampler,
    map_bind_group: wgpu::BindGroup,
    map_bind_group_layout: wgpu::BindGroupLayout,
    map_pipeline: wgpu::RenderPipeline,
    map_renderer_join: Option<ScreenMapRendererJoinHandle>,
    ud_sender: mpsc::SyncSender<(f32, f32, Arc<UserDataStatic>)>,
    buffers: Arc<Mutex<ScreenMapRendererBuffers<2, (f32, f32, Arc<UserDataStatic>)>>>,
    curr_buffer: Option<ScreenMapRendererBuffer<2, (f32, f32, Arc<UserDataStatic>)>>,

    // mouse_delta: (f64, f64),
    map_delta: wgpu::Buffer,
    map_delta_bind_group: wgpu::BindGroup,

    mouse_pressed: bool,

    static_user_data: Arc<UserDataStatic>,
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
    async fn new(
        w: usize, h: usize,
        event_loop: &ActiveEventLoop,
        map_def_file: impl AsRef<Path>, basepath: impl AsRef<Path>,
        inifilename: impl AsRef<Path>
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

        let controls = Controls::default();
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

    fn update_buffer(&mut self) -> anyhow::Result<()> {
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
                        ((self.controls.center_x - ud.0) / (self.controls.map_width as f32)) / (self.hidpi_factor as f32 * TEMP_OFFSET),
                        (-1. * (self.controls.center_y - ud.1) / (self.controls.map_height as f32)) / (self.hidpi_factor as f32 * TEMP_OFFSET)
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
    fn resize_map(&mut self, w: u32, h: u32) -> Result<(), ResizeMapResult> {
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

        Ok(())
    }
}

#[derive(Debug)]
enum ResizeMapResult {
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

struct MapExplorer {
    window: Option<MapExplorerWindow>,
    w: usize,
    h: usize,
    map_def_file: PathBuf,
    basepath: PathBuf,
    inifile: PathBuf,
}

impl MapExplorer {
    fn new(w: usize, h: usize, map_def_file: impl Into<PathBuf>, basepath: impl Into<PathBuf>, inifile: impl Into<PathBuf>) -> anyhow::Result<MapExplorer> {
        Ok(MapExplorer {
            window: None,
            w, h,
            map_def_file: map_def_file.into(),
            basepath: basepath.into(),
            inifile: inifile.into(),
        })
    }
}

impl ApplicationHandler for MapExplorer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // TODO: handle unwrap
        self.window = Some(pollster::block_on(MapExplorerWindow::new(self.w, self.h, event_loop, &self.map_def_file, &self.basepath, &self.inifile)).unwrap());
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: winit::event::WindowEvent,
    ) {
        let window = self.window.as_mut().unwrap();

        {
            let imgui = &mut window.imgui;

            imgui.platform.handle_event::<()>(
                imgui.context.io_mut(),
                &window.window,
                &winit::event::Event::WindowEvent { window_id, event: event.clone() });
        }

        match &event {
            WindowEvent::Resized(new_size) => {
                window.resize_map(new_size.width, new_size.height).unwrap();
            }
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                let frame = match window.surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(err) => {
                        eprintln!("Dropped frame: {err:?}");
                        return;
                    }
                };

                // Map
                window.update_buffer().unwrap();

                // UI
                #[allow(unused_mut)] // TODO (see next TODO)
                let mut resize_window: Option<(u32, u32)> = None;
                let imgui = &mut window.imgui;
                let now = Instant::now();
                imgui.context
                    .io_mut()
                    .update_delta_time(now - imgui.last_frame);
                imgui.last_frame = now;

                imgui.platform
                    .prepare_frame(imgui.context.io_mut(), &window.window)
                    .expect("Failed to prepare frame");

                let ui = imgui.context.frame();

                {
                    let win = ui.window("Controls");
                    win
                        .size([300.0, 100.0], Condition::FirstUseEver)
                        .build(|| {
                            let mut changed = false;
                            ui.input_float("x", &mut window.controls.center_x).build();
                            ui.input_float("y", &mut window.controls.center_y).build();
                            changed |= ui.input_float("units per pixel", &mut window.controls.units_per_pixel_scale).build();
                            // TODO:
                            // let mut scale = (window.controls.map_width as f32) / (window.window.inner_size().width as f32);
                            // changed |= ui.input_float("render scale", &mut scale).build();
                            // if scale != (window.controls.map_width as f32) / (window.window.inner_size().width as f32) {
                            //     resize_window = Some((((window.window.inner_size().width as f32) * scale) as u32, ((window.window.inner_size().height as f32) * scale) as u32));
                            // }
                            match window.controls.updating_input_projection(|srs| {
                                ui.input_text("input projection", srs).build()
                            }) {
                                Ok(c) => changed |= c,
                                Err(err) => error!("{}", err),
                            }

                            match window.controls.updating_output_projection(|srs| {
                                ui.input_text("output projection", srs).build()
                            }) {
                                Ok(c) => changed |= c,
                                Err(err) => error!("{}", err),
                            }

                            if changed {
                                window.static_user_data = Arc::new(UserDataStatic::new(&window.controls));
                            }
                        });
                }

                // Finish rendering
                let mut encoder: wgpu::CommandEncoder = window.device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                imgui.platform.prepare_render(ui, &window.window);

                let view = frame.texture
                    .create_view(&wgpu::TextureViewDescriptor::default());

                let mut rpass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: None,
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(imgui.clear_color),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });

                // map
                rpass.set_pipeline(&window.map_pipeline);
                rpass.set_bind_group(0, &window.map_bind_group, &[]);
                rpass.set_bind_group(1, &window.map_delta_bind_group, &[]);
                rpass.draw(0..6, 0..1);

                // ui
                let draw_data = imgui.context.render();
                if draw_data.draw_lists_count() == 0 {
                    error!("ImGui returned empty draw list");
                } else {
                    imgui.renderer
                        .render(
                            draw_data,
                            &window.queue,
                            &window.device,
                            &mut rpass
                        ).expect("Rendering failed");
                }

                drop(rpass);

                window.queue.submit(Some(encoder.finish()));

                frame.present();

                if let Some(new_size) = resize_window {
                    match window.resize_map(new_size.0, new_size.1) {
                        Ok(()) => {},
                        Err(err) => match err {
                            ResizeMapResult::Size0 | ResizeMapResult::SizeTooBig => warn!("{:?}", err),
                            ResizeMapResult::Error(error) => panic!("{}", error),
                        },
                    }
                }
            },
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                let imgui = &mut window.imgui;
                window.hidpi_factor = *scale_factor;
                let io = imgui.context.io_mut();
                io.display_framebuffer_scale = [*scale_factor as f32, *scale_factor as f32];
                io.font_global_scale = (1.0 / *scale_factor) as f32;
            },
            WindowEvent::MouseInput { state, button, .. } if *button == MouseButton::Left => {
                match state {
                    winit::event::ElementState::Pressed if !unsafe { imgui_sys::igIsWindowHovered(imgui_sys::ImGuiHoveredFlags_AnyWindow as i32) }
                        => window.mouse_pressed = true,
                    winit::event::ElementState::Released => window.mouse_pressed = false,
                    _ => {}
                }
            }
            _ => {},
        }
    }

    fn user_event(&mut self, _: &ActiveEventLoop, event: ()) {
        let window = self.window.as_mut().unwrap();
        let imgui = &mut window.imgui;
        imgui.platform.handle_event::<()>(
            imgui.context.io_mut(),
            &window.window,
            &winit::event::Event::UserEvent(event),
        );
    }

    fn device_event(
        &mut self,
        _: &ActiveEventLoop,
        device_id: winit::event::DeviceId,
        event: winit::event::DeviceEvent,
    ) {
        let window = self.window.as_mut().unwrap();

        match &event {
            &winit::event::DeviceEvent::MouseMotion { delta } if window.mouse_pressed == true => {
                window.controls.center_x += (delta.0 as f32) * window.controls.units_per_pixel_scale * TEMP_OFFSET;
                window.controls.center_y += (delta.1 as f32) * window.controls.units_per_pixel_scale * TEMP_OFFSET;
                window.ud_sender.send((window.controls.center_x, window.controls.center_y, window.static_user_data.clone())).unwrap();
            },
            _ => {}
        }

        let imgui = &mut window.imgui;
        imgui.platform.handle_event::<()>(
            imgui.context.io_mut(),
            &window.window,
            &winit::event::Event::DeviceEvent { device_id, event },
        );
    }

    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        let window = self.window.as_mut().unwrap();
        let imgui = &mut window.imgui;
        window.window.request_redraw();
        imgui.platform.handle_event::<()>(
            imgui.context.io_mut(),
            &window.window,
            &winit::event::Event::AboutToWait
        );
    }
}

fn main() -> anyhow::Result<()> {
    // Set up logging
    let stdout_appender = ConsoleAppender::builder()
        .encoder(Box::new(PatternEncoder::new("{h({l})} {d(%H:%M:%S)} [{t}] {m}\n")))
        .build();
    let config = log4rs::Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(stdout_appender)))
        .logger(Logger::builder().build("map_explorer", LevelFilter::Trace))
        .build(log4rs::config::Root::builder()
            .appender("stdout")
            .build(LevelFilter::Info)
        )?;
    _ = log4rs::init_config(config).unwrap();

    // Parse args
    let mut args = std::env::args();
    let progname = args.next().unwrap(); // always present
    let mapfile = args.next().unwrap_or("map.xml".to_string());
    let basepath = args.next().unwrap_or(".".to_string());

    if args.next().is_some() {
        return Err(anyhow::format_err!("Invalid argument.\nUsage: {} [mapnik stylesheet path] [basepath]", progname));
    }

    let projdirs = directories::ProjectDirs::from("be", "jonaseveraert", "MapExplorer").unwrap();
    let cache_dir = projdirs.cache_dir();
    if !cache_dir.exists() {
        fs::create_dir(cache_dir)?;
    }

    let inifile = cache_dir.join("MapExplorer.ini");
    let cachefile = cache_dir.join("cache.json");

    info!("inifile: {}", inifile.display());
    info!("cachefile: {}", cachefile.display());

    // TODO: logging
    let mut pipe = new_Pipe()?;
    let pipeout = new_PipeOutputStream(pipe.clone())?;
    let pipein = new_PipeInputStream(pipe.clone())?;
    let pipein = UniqueSendPtr { ptr: pipein };

    let os: *mut ostream = unsafe { std::mem::transmute(pipeout.as_mut_ptr()) };
    unsafe { map_explorer::set_logging(os); }
    map_explorer::ffi::clog_redirect();

    _ = std::thread::spawn(move || -> anyhow::Result<()> {
        let pipein = pipein;
        let mut pipein = pipein.ptr;
        let_cxx_string!(cxxbuf = "");
        let mapnik_log_regex = Regex::new(r"Mapnik LOG> \d+-\d+-\d+ \d+:\d+:\d+: ")?;
        loop {
            let input = pipein.pin_mut();
            _ = map_explorer::ffi::getline(unsafe { std::mem::transmute(input) }, cxxbuf.as_mut())?;

            if cxxbuf.len() == 0 { continue } // TODO: can close?

            let str = cxxbuf.to_string_lossy();
            if str.as_ref().starts_with("Mapnik LOG>") {
                let str = mapnik_log_regex.replace(str.as_ref(), "");
                if str.contains("error") {
                    error!(target: "Mapnik", "{}", str);
                } else {
                    info!(target: "Mapnik", "{}", str);
                }
            } else {
                info!(target: "CxxMapExplorer", "{}", str);
            }
        }
        // return Ok(());
    });

    setup_mapnik(&mapnik_config::input_plugins_dir()?, &mapnik_config::fonts_dir()?)?;

    let w = 800;
    let h = 600;

    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(winit::event_loop::ControlFlow::Poll);
    let mut app = MapExplorer::new(w, h, mapfile, basepath, inifile)?;
    event_loop.run_app(&mut app)?;

    map_explorer::ffi::restore_clog();
    unsafe { map_explorer::ffi::close_pipe(pipe.pin_mut_unchecked())? };

    Ok(())
}
