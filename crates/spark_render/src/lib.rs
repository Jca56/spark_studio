//! spark_render — wgpu core for Spark Studio.
//!
//! Owns the GPU device, surface, and the render passes. Windowing-agnostic:
//! anything that can become a `wgpu::SurfaceTarget` can be rendered to.

pub use wgpu;

mod exec;
mod geom;
mod gpu;
mod pass;
mod sdf;
mod shapes;

pub use exec::block_on;
pub use geom::Viewport;
pub use gpu::{Frame, Gpu};
pub use pass::ShapePass;
pub use shapes::{CANVAS_H, CANVAS_W, FIELDS, STAR_FORMS, Shape, ShapeKind};
