use std::collections::VecDeque;
use std::path::Path;
use std::sync::{mpsc, Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use cxx::{SharedPtr, UniquePtr};

use crate::{cairo::*, map_renderer, MapRenderer, MapRendererExt};

struct CairoSurfacesCloser<const BUFFER_SIZE: usize> {
    surfaces: Arc<[*mut cairo_surface_t; BUFFER_SIZE]>
}

impl<const BUFFER_SIZE: usize> Drop for CairoSurfacesCloser<BUFFER_SIZE> {
    fn drop(&mut self) {
        for i in 0..BUFFER_SIZE {
            unsafe { cairo_surface_destroy(self.surfaces[i]); }
        }
    }
}

pub struct MapRendererAndUserData<UserData: 'static + Clone> {
    map_renderer: UniquePtr<MapRenderer>,
    user_data: UserData,
}

impl<UserData: 'static + Clone> MapRendererAndUserData<UserData> {
    pub fn map_renderer(&self) -> &UniquePtr<MapRenderer> {
        &self.map_renderer
    }

    pub fn map_renderer_mut(&mut self) -> &mut UniquePtr<MapRenderer> {
        &mut self.map_renderer
    }

    pub fn user_data(&self) -> &UserData {
        &self.user_data
    }

    pub fn set_user_data(&mut self, user_data: UserData) {
        self.user_data = user_data;
    }
}

// -DMAPNIK_THREADSAGE
unsafe impl<UserData: 'static + Clone> Send for MapRendererAndUserData<UserData> {}

/// Threaded map renderer
pub struct ScreenMapRenderer<const BUFFER_SIZE: usize, UserData: 'static + Clone + Send> {
    surfaces: Arc<[*mut cairo_surface_t; BUFFER_SIZE]>,
    contexts: [SharedPtr<crate::map_renderer::ffi::cairo_t>; BUFFER_SIZE],
    used: [bool; BUFFER_SIZE],
    idx: usize,
    reuse_queue: Arc<Mutex<Vec<usize>>>,
    buffers: Arc<Mutex<ScreenMapRendererBuffers<BUFFER_SIZE, UserData>>>,
    map_renderer_and_user_data: Arc<Mutex<MapRendererAndUserData<UserData>>>,
    on_receive_userdata: Box<dyn Fn(&mut UniquePtr<MapRenderer>, &UserData) -> ()>,
    // map_renderer: Arc<Mutex<UniquePtr<MapRenderer>>>,
    _surfaces_closer: Arc<CairoSurfacesCloser<BUFFER_SIZE>>,
    // user_data: Arc<Mutex<UserData>>,
}

unsafe impl<const BUFFER_SIZE: usize, UserData: 'static + Clone + Send> Send for ScreenMapRenderer<BUFFER_SIZE, UserData> {}

impl<const BUFFER_SIZE: usize, UserData: 'static + Clone + Send> ScreenMapRenderer<BUFFER_SIZE, UserData> {
    pub fn new_from_file(
        w: u32, h: u32,
        map_def_file: impl AsRef<Path>,
        base_path: impl AsRef<Path>,
        user_data: UserData,
        on_receive_userdata: Box<dyn Fn(&mut UniquePtr<MapRenderer>, &UserData) -> ()>,
    ) -> anyhow::Result<(Self, Arc<Mutex<ScreenMapRendererBuffers<BUFFER_SIZE, UserData>>>)> {
        let surfaces: Arc<[*mut cairo_surface_t; BUFFER_SIZE]> = Arc::new(std::array::from_fn(|_| {
            unsafe { cairo_image_surface_create(_cairo_format_CAIRO_FORMAT_ARGB32, w as i32, h as i32) }
        }));
        let surfaces_closer = Arc::new(CairoSurfacesCloser {
            surfaces: surfaces.clone(),
        });

        let contexts: [SharedPtr<crate::map_renderer::ffi::cairo_t>; BUFFER_SIZE] = std::array::from_fn(|i| {
            let surf = surfaces[i];
            let cr: *mut cairo_t = unsafe { cairo_create(surf) };
            let cr_mapnik: *mut crate::map_renderer::ffi::cairo_t = unsafe { std::mem::transmute(cr) };
            unsafe { map_renderer::ffi::make_cairo_shared(cr_mapnik) }
        });

        let reuse_queue = Arc::new(Mutex::new(Vec::new()));
        let buffers = Arc::new(Mutex::new(ScreenMapRendererBuffers::new(
            BUFFER_SIZE,
            unsafe { (cairo_image_surface_get_width(surfaces[0]) as usize) * (cairo_image_surface_get_height(surfaces[0]) as usize) * (cairo_image_surface_get_stride(surfaces[0]) as usize) },
            reuse_queue.clone(),
            surfaces_closer.clone()
        )));
        let ctx1 = contexts[0].clone();
        return Ok((Self {
            surfaces,
            contexts,
            used: [false; BUFFER_SIZE],
            idx: 0,
            reuse_queue,
            buffers: buffers.clone(),
            map_renderer_and_user_data: Arc::new(Mutex::new(MapRendererAndUserData {
                map_renderer: MapRenderer::new_from_file(w, h, map_def_file, ctx1, base_path)?,
                user_data,
            })),
            on_receive_userdata,
            _surfaces_closer: surfaces_closer,
        }, buffers));
    }

    #[deprecated]
    pub fn map_renderer_and_user_data(&self) -> Arc<Mutex<MapRendererAndUserData<UserData>>> {
        self.map_renderer_and_user_data.clone()
    }

    fn try_reuse_buffers(&mut self) -> Result<(), PoisonError<MutexGuard<'_, Vec<usize>>>> {
        match self.reuse_queue.try_lock() {
            Ok(mut queue) => {
                for buffer_idx in queue.iter() {
                    self.used[*buffer_idx] = false;
                }
                queue.clear();
                return Ok(());
            },
            Err(err) => match err {
                std::sync::TryLockError::Poisoned(poison_error) => return Err(poison_error),
                std::sync::TryLockError::WouldBlock => return Ok(()),
            },
        }
    }

    pub fn start(self) -> (ScreenMapRendererJoinHandle, std::sync::mpsc::SyncSender<UserData>) {
        let (quit_sender, quit_receiver) = std::sync::mpsc::channel::<()>();
        let (ud_sender, ud_receiver) = std::sync::mpsc::sync_channel::<UserData>(100);
        return (ScreenMapRendererJoinHandle {
            join: std::thread::spawn(move || {
                let mut ren = self;
                loop {
                    match quit_receiver.try_recv() {
                        Ok(()) => break,
                        Err(err) => match err {
                            mpsc::TryRecvError::Empty => {},
                            mpsc::TryRecvError::Disconnected => return Err(mpsc::TryRecvError::Disconnected.into()),
                        },
                    }

                    loop {
                        match ud_receiver.try_recv() {
                            Ok(ud) => {
                                let mut guard = ren.map_renderer_and_user_data.lock().map_err(|err| anyhow::format_err!("{}", err))?;
                                guard.set_user_data(ud.clone());
                                ren.on_receive_userdata.as_ref()(&mut guard.map_renderer, &ud)
                            },
                            Err(err) => match err{
                                mpsc::TryRecvError::Empty => break,
                                mpsc::TryRecvError::Disconnected => return Err(mpsc::TryRecvError::Disconnected.into()),
                            },
                        }
                    }

                    ren.try_reuse_buffers().map_err(|err| anyhow::format_err!("{}", err))?;

                    if ren.used[ren.idx] {
                        ren.idx = (ren.idx + 1) % BUFFER_SIZE;
                        continue;
                    }

                    let surf = &ren.surfaces[ren.idx];
                    let cr = &ren.contexts[ren.idx];

                    let mut map_renderer_and_ud = ren.map_renderer_and_user_data.lock().map_err(|err| anyhow::format_err!("{}", err))?;
                    let map_renderer = &mut map_renderer_and_ud.map_renderer;
                    map_renderer.pin_mut().set_cairo(cr.clone());
                    map_renderer.pin_mut().render()?;

                    let mut buffers = ren.buffers.lock()
                        .map_err(|err| anyhow::format_err!("{}", err))?;
                    buffers.add_buffer((
                        unsafe { cairo_image_surface_get_data(*surf) },
                        ren.idx,
                        map_renderer_and_ud.user_data.clone()
                        // ren.user_data.lock().map_err(|err| anyhow::format_err!("{}", err))?.clone()
                    ));
                    ren.used[ren.idx] = true;

                    std::thread::sleep(Duration::from_millis(10));
                }
                return Ok(());
            }),
            signal: quit_sender
        }, ud_sender);
    }
}

pub struct ScreenMapRendererJoinHandle {
    join: std::thread::JoinHandle<anyhow::Result<()>>,
    signal: mpsc::Sender<()>,
}

impl ScreenMapRendererJoinHandle {
    pub fn join(self) -> anyhow::Result<()> {
        self.signal.send(())?;
        self.join.join().map_err(|err| anyhow::format_err!("{:?}", err))?
    }
}

pub struct ScreenMapRendererBuffers<const BUFFER_SIZE: usize, UserData: 'static + Clone> {
    buffers: VecDeque<(*const u8, usize, UserData)>,
    len: usize,
    reuse_queue: Arc<Mutex<Vec<usize>>>,
    surfaces_closer: Arc<CairoSurfacesCloser<BUFFER_SIZE>>
}

impl<const BUFFER_SIZE: usize, UserData: 'static + Clone> ScreenMapRendererBuffers<BUFFER_SIZE, UserData> {
    fn new(cap: usize, len: usize, reuse_queue: Arc<Mutex<Vec<usize>>>, surfaces_closer: Arc<CairoSurfacesCloser<BUFFER_SIZE>>) -> Self {
        Self {
            buffers: VecDeque::with_capacity(cap),
            len: len,
            reuse_queue,
            surfaces_closer
        }
    }

    fn add_buffer(&mut self, buffer: (*const u8, usize, UserData)) {
        self.buffers.push_back(buffer);
    }

    /// Try to get a buffer
    pub fn get_buffer<'a>(&'a mut self) -> Option<ScreenMapRendererBuffer<BUFFER_SIZE, UserData>> {
        if let Some(buffer) = self.buffers.pop_front() {
            return Some(ScreenMapRendererBuffer {
                buffer: buffer.0,
                buffer_len: self.len,
                index: buffer.1,
                reuse_queue: self.reuse_queue.clone(),
                _cairo_surfaces_closer: self.surfaces_closer.clone(),
                user_data: buffer.2
            });
        } else {
            return None;
        }
    }
}

pub struct ScreenMapRendererBuffer<const BUFFER_SIZE: usize, UserData: 'static + Clone> {
    buffer: *const u8,
    buffer_len: usize,
    index: usize,
    reuse_queue: Arc<Mutex<Vec<usize>>>,
    /// Prevents the need for lifetimes in this struct
    _cairo_surfaces_closer: Arc<CairoSurfacesCloser<BUFFER_SIZE>>,
    user_data: UserData
}

impl<const BUFFER_SIZE: usize, UserData: 'static + Clone> ScreenMapRendererBuffer<BUFFER_SIZE, UserData> {
    pub fn buffer(&self) -> &[u8] {
        // self.buffer
        unsafe { std::slice::from_raw_parts(self.buffer, self.buffer_len) }
    }

    pub fn user_data(&self) -> &UserData {
        &self.user_data
    }
}

impl<const BUFFER_SIZE: usize, UserData: 'static + Clone> Drop for ScreenMapRendererBuffer<BUFFER_SIZE, UserData> {
    fn drop(&mut self) {
        self.reuse_queue.lock().unwrap().push(self.index);
    }
}
