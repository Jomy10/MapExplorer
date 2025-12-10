use cxx::SharedPtr;

use crate::{Box2d, Point, Projection, ProjectionExt};

#[derive(Debug)]
pub struct Controls {
    // would prefer f64 for these
    pub center_x: f32,
    pub center_y: f32,
    pub units_per_pixel_scale: f32,
    pub map_width: u32,
    pub map_height: u32,
    input_projection_srs: String,
    output_projection_srs: String,
    input_projection: SharedPtr<Projection>,
    output_projection: SharedPtr<Projection>,

    cache_input_projection_srs: String,
    cache_output_projection_srs: String,
}

unsafe impl Send for Controls {}
unsafe impl Sync for Controls {}

impl Controls {
    pub fn input_projection(&self) -> SharedPtr<Projection> {
        self.input_projection.clone()
    }

    pub fn output_projection(&self) -> SharedPtr<Projection> {
        self.output_projection.clone()
    }

    pub fn set_input_projection(&mut self, projection: impl Into<String>) -> anyhow::Result<()> {
        let srs: String = projection.into();
        self.input_projection = Projection::new(&srs)?;
        self.input_projection_srs = srs.clone();
        self.input_projection_srs = srs;
        Ok(())
    }

    pub fn set_output_projection(&mut self, projection: impl Into<String>) -> anyhow::Result<()> {
        let srs: String = projection.into();
        self.output_projection = Projection::new(&srs)?;
        self.output_projection_srs = srs.clone();
        self.cache_output_projection_srs = srs;
        Ok(())
    }

    pub fn updating_input_projection<R>(&mut self, cb: impl Fn(&mut String) -> R) -> anyhow::Result<R> {
        let ret = cb(&mut self.input_projection_srs);
        if self.input_projection_srs != self.cache_input_projection_srs {
            self.input_projection = Projection::new(&self.input_projection_srs)?;
            self.cache_input_projection_srs = self.input_projection_srs.clone();
        }
        Ok(ret)
    }

    pub fn updating_output_projection<R>(&mut self, cb: impl Fn(&mut String) -> R) -> anyhow::Result<R> {
        let ret = cb(&mut self.output_projection_srs);
        if self.output_projection_srs != self.cache_output_projection_srs {
            self.output_projection = Projection::new(&self.output_projection_srs)?;
            self.cache_output_projection_srs = self.output_projection_srs.clone();
        }
        Ok(ret)
    }

    pub fn create_center_box(&self, w: u32, h: u32) -> Box2d<f64> {
        let center = Point::<f64>::new(self.center_x.into(), self.center_y.into());
        return Box2d::<f64>::new_centered(
            &center,
            self.input_projection.clone(),
            self.output_projection.clone(),
            self.units_per_pixel_scale.into(),
            w, h
        );
    }
}

impl Default for Controls {
    fn default() -> Self {
        let isrs = "epsg:3812".to_string();
        let osrs = "+proj=merc +lat_ts=0 +lon_0=0 +x_0=0 +y_0=0 +R=6371000 +units=m +no_defs +type=crs".to_string();

        Self {
            center_x: 549000.00,
            center_y: 713900.00,
            units_per_pixel_scale: 10.0,
            map_width: 800,
            map_height: 600,
            input_projection_srs: isrs.clone(),
            output_projection_srs: osrs.clone(),
            input_projection: Projection::new(&isrs).unwrap(),
            output_projection: Projection::new(&osrs).unwrap(),
            cache_input_projection_srs: isrs,
            cache_output_projection_srs: osrs,
        }
    }
}
