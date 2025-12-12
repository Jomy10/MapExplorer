use std::path::PathBuf;
use std::sync::Arc;
use std::time;

use winit::application::ApplicationHandler;
use winit::event::{MouseButton, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use log::*;

use crate::file_watcher::FileWatcher;

use super::window::*;

pub struct MapExplorer {
    window: Option<MapExplorerWindow>,
    w: usize,
    h: usize,
    map_def_file: PathBuf,
    map_def_watcher: FileWatcher,
    basepath: PathBuf,
    inifile: PathBuf,
    cachefile: PathBuf,
}

impl MapExplorer {
    pub fn new(
        w: usize, h: usize,
        map_def_file: impl Into<PathBuf>, basepath: impl Into<PathBuf>,
        inifile: impl Into<PathBuf>,
        cachefile: impl Into<PathBuf>
    ) -> anyhow::Result<MapExplorer> {
        let map_def_file = map_def_file.into();
        let map_def_watcher = FileWatcher::new(&map_def_file)?;
        Ok(MapExplorer {
            window: None,
            w, h,
            map_def_file,
            map_def_watcher,
            basepath: basepath.into(),
            inifile: inifile.into(),
            cachefile: cachefile.into(),
        })
    }
}

impl ApplicationHandler for MapExplorer {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // TODO: handle unwrap
        self.window = Some(pollster::block_on(MapExplorerWindow::new(self.w, self.h, event_loop, &self.map_def_file, &self.basepath, &self.inifile, &self.cachefile)).unwrap());
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
                if self.map_def_watcher.changed().unwrap() {
                    match window.reload_map() {
                        Ok(()) => {},
                        Err(err) => error!("{}", err),
                    }
                }

                let mut should_reload = false;

                let frame = match window.surface.get_current_texture() {
                    Ok(frame) => frame,
                    Err(err) => {
                        warn!("Dropped frame: {err:?}");
                        return;
                    }
                };

                // Map
                window.update_buffer().unwrap();

                // UI
                #[allow(unused_mut)] // TODO (see next TODO)
                let mut resize_window: Option<(u32, u32)> = None;
                let imgui = &mut window.imgui;
                let now = time::Instant::now();
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
                        .size([300.0, 100.0], imgui::Condition::FirstUseEver)
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

                            should_reload = ui.button("reload");

                            if changed {
                                window.static_user_data = Arc::new(UserDataStatic::new(&window.controls));
                                window.ud_sender.send((window.controls.center_x, window.controls.center_y, window.static_user_data.clone())).unwrap();
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

                if should_reload {
                    match window.reload_map() {
                        Ok(()) => {},
                        Err(err) => error!("{}", err),
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
                window.controls.center_x += (delta.0 as f32) * window.controls.units_per_pixel_scale * super::TEMP_OFFSET;
                window.controls.center_y += (delta.1 as f32) * window.controls.units_per_pixel_scale * super::TEMP_OFFSET;
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

impl Drop for MapExplorer {
    fn drop(&mut self) {
        match self.window.as_ref().unwrap().controls
            .write(&self.cachefile)
        {
            Ok(()) => {},
            Err(err) => error!("Couldn't write cache to {}: {}", self.cachefile.display(), err),
        }
    }
}
