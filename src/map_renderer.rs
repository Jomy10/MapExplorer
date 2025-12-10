use std::path::Path;
use std::pin::Pin;
use cxx::memory::SharedPtrTarget;
use cxx::{let_cxx_string, UniquePtr, SharedPtr};

#[cxx::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("MapRenderer.hpp");
        include!("glue.hpp");

        type cairo_t;

        unsafe fn make_cairo_shared(cr: *mut cairo_t) -> SharedPtr<cairo_t>;

        #[cxx_name = "setup_mapnik"]
        fn _setup_mapnik(datasources_dir: Pin<&CxxString>, fonts_dir: Pin<&CxxString>) -> Result<()>;

        type MapRenderer;

        fn new_MapRenderer(w: u32, h: u32, map_def: Pin<&CxxString>, cairo: SharedPtr<cairo_t>, base_path: Pin<&CxxString>) -> Result<UniquePtr<MapRenderer>>;
        fn new_MapRendererFromFile(w: u32, h: u32, map_def_file: Pin<&CxxString>, cairo: SharedPtr<cairo_t>, base_path: Pin<&CxxString>) -> Result<UniquePtr<MapRenderer>>;

        fn resize(self: Pin<&mut MapRenderer>, w: u32, h: u32);

        fn render(self: Pin<&mut MapRenderer>) -> Result<()>;

        // #[cxx_name = "move"]
        // fn move_map(self: Pin<&mut MapRenderer>, x: f64, y: f64);

        fn zoom(self: Pin<&mut MapRenderer>, startx: f64, starty: f64, endx: f64, endy: f64);

        fn set_cairo(self: Pin<&mut MapRenderer>, cr: SharedPtr<cairo_t>);

        #[cxx_name = "zoom_to_box"]
        fn zoom_to_cxx_box(self: Pin<&mut MapRenderer>, bbox: Pin<&box2d_double>);

        type box2d_double;
        fn new_box2d_double(startx: f64, starty: f64, endx: f64, endy: f64) -> SharedPtr<box2d_double>;

        fn box2d_get_startx(b: &box2d_double) -> f64;
        fn box2d_get_starty(b: &box2d_double) -> f64;
        fn box2d_get_endx(b: &box2d_double) -> f64;
        fn box2d_get_endy(b: &box2d_double) -> f64;

        type point_double;
        fn new_point_double(x: f64, y: f64) -> SharedPtr<point_double>;

        #[namespace = "mapnik"]
        #[cxx_name = "projection"]
        type Projection;

        fn new_projection(str: Pin<&CxxString>) -> Result<SharedPtr<Projection>>;

        fn projection_definition(proj: SharedPtr<Projection>) -> UniquePtr<CxxString>;
        // TODO: definition

        fn make_center_box(center: &point_double, projsrc: &Projection, projdst: &Projection, projected_units_per_pixel: f64, screen_w: u32, screen_h: u32) -> SharedPtr<box2d_double>;
    }
}

use ffi::*;

pub trait MapRendererExt {
    fn new(w: u32, h: u32, map_def: &str, cairo: SharedPtr<cairo_t>, base_path: impl AsRef<Path>) -> cxx::core::result::Result<UniquePtr<MapRenderer>, cxx::Exception>;
    fn new_from_file(w: u32, h: u32, map_def: impl AsRef<Path>, cairo: SharedPtr<cairo_t>, base_path: impl AsRef<Path>) -> cxx::core::result::Result<UniquePtr<MapRenderer>, cxx::Exception>;
}

pub trait MapRendererMemberExt {
    fn zoom_to_box(self, bbox: &Box2d<f64>);
}

impl MapRendererExt for MapRenderer {
    fn new(w: u32, h: u32, map_def: &str, cairo: SharedPtr<cairo_t>, base_path: impl AsRef<Path>) -> cxx::core::result::Result<UniquePtr<MapRenderer>, cxx::Exception> {
        let_cxx_string!(map_def_cxx = map_def);
        let_cxx_string!(base_path = base_path.as_ref().as_os_str().as_encoded_bytes());
        new_MapRenderer(w, h, map_def_cxx.as_ref(), cairo, base_path.as_ref())
    }

    fn new_from_file(w: u32, h: u32, map_def: impl AsRef<Path>, cairo: SharedPtr<cairo_t>, base_path: impl AsRef<Path>) -> cxx::core::result::Result<UniquePtr<MapRenderer>, cxx::Exception> {
        let_cxx_string!(map_def_path = map_def.as_ref().as_os_str().as_encoded_bytes());
        let_cxx_string!(base_path = base_path.as_ref().as_os_str().as_encoded_bytes());
        new_MapRendererFromFile(w, h, map_def_path.as_ref(), cairo, base_path.as_ref())
    }
}

impl<'a> MapRendererMemberExt for Pin<&'a mut MapRenderer> {
    fn zoom_to_box(self, bbox: &Box2d<f64>) {
        let mut bbox = bbox.as_cxx();
        let pin = unsafe { bbox.pin_mut_unchecked() };
        MapRenderer::zoom_to_cxx_box(self, pin.as_ref());
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Box2d<T: Clone + Copy> {
    pub startx: T,
    pub starty: T,
    pub endx: T,
    pub endy: T,
}

impl<T: CXXBox2dCapable> Box2d<T> {
    fn as_cxx(&self) -> SharedPtr<T::Box2dType> {
        T::create_cxx_box(self.startx, self.starty, self.endx, self.endy)
    }
}

impl Box2d<f64> {
    pub fn new_centered_cxx(
        center: &Point<f64>,
        projsrc: SharedPtr<Projection>,
        projdst: SharedPtr<Projection>,
        projected_units_per_pixel: f64,
        screen_w: u32, screen_h: u32
    ) -> SharedPtr<box2d_double> {
        return make_center_box(center.as_cxx().as_ref().unwrap(), projsrc.as_ref().unwrap(), projdst.as_ref().unwrap(), projected_units_per_pixel, screen_w, screen_h);
    }

    pub fn new_centered(
        center: &Point<f64>,
        projsrc: SharedPtr<Projection>,
        projdst: SharedPtr<Projection>,
        projected_units_per_pixel: f64,
        screen_w: u32, screen_h: u32
    ) -> Box2d<f64> {
        let bbox = Self::new_centered_cxx(center, projsrc, projdst, projected_units_per_pixel, screen_w, screen_h);
        let b = bbox.as_ref().unwrap();
        return Self {
            startx: box2d_get_startx(b),
            starty: box2d_get_starty(b),
            endx: box2d_get_endx(b),
            endy: box2d_get_endy(b),
        }
    }
}

pub trait CXXBox2dCapable: Clone + Copy {
    type Box2dType: SharedPtrTarget;
    fn create_cxx_box(startx: Self, starty: Self, endx: Self, endy: Self) -> SharedPtr<Self::Box2dType>;
}

impl CXXBox2dCapable for f64 {
    type Box2dType = box2d_double;

    fn create_cxx_box(startx: Self, starty: Self, endx: Self, endy: Self) -> SharedPtr<Self::Box2dType> {
        return new_box2d_double(startx, starty, endx, endy);
    }
}

pub fn setup_mapnik(datasources_dir: &str, fonts_dir: &str) -> cxx::core::result::Result<(), cxx::Exception> {
    let_cxx_string!(datasources_dir = datasources_dir);
    let_cxx_string!(fonts_dir = fonts_dir);
    _setup_mapnik(datasources_dir.as_ref(), fonts_dir.as_ref())
}

pub struct Point<T: Copy + Clone> {
    pub x: T,
    pub y: T
}

impl<T: Copy + Clone> Point<T> {
    pub fn new(x: T, y: T) -> Point<T> {
        return Point { x, y };
    }
}

impl<T: CXXPointCapable> Point<T> {
    fn as_cxx(&self) -> SharedPtr<T::PointType> {
        T::create_cxx_point(self.x, self.y)
    }
}

pub trait CXXPointCapable: Clone + Copy {
    type PointType: SharedPtrTarget;

    fn create_cxx_point(x: Self, y: Self) -> SharedPtr<Self::PointType>;
}

impl CXXPointCapable for f64 {
    type PointType = point_double;

    fn create_cxx_point(x: Self, y: Self) -> SharedPtr<Self::PointType> {
        return new_point_double(x, y);
    }
}

pub trait ProjectionExt {
    fn new(str: &str) -> cxx::core::result::Result<SharedPtr<Projection>, cxx::Exception>;
}

impl ProjectionExt for Projection {
    fn new(str: &str) -> cxx::core::result::Result<SharedPtr<Projection>, cxx::Exception> {
        let_cxx_string!(srs = str);
        return new_projection(srs.as_ref());
    }
}

pub trait ProjectionMemberExt {
    fn definition(&self) -> String;
}

impl ProjectionMemberExt for SharedPtr<Projection> {
    fn definition(&self) -> String {
        projection_definition(self.clone()).to_string()
    }
}

impl std::fmt::Debug for Projection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Projection { .. }")
        // f.debug_struct("Projection").field("srs", self.definition()).finish()
    }
}

pub use ffi::{MapRenderer, Projection};
