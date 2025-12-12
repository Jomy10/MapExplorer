mod app;
pub use app::*;
pub(crate) mod window;
pub(crate) mod controls;

// Fix until proper moving is implemented
pub(crate) const TEMP_OFFSET: f32 = 1.5;
