//! spark_render — wgpu core for Spark Studio.
//!
//! Owns the GPU device, surface, and the render passes. Windowing-agnostic:
//! anything that can become a `wgpu::SurfaceTarget` can be rendered to.

pub use wgpu;

mod exec;
mod gpu;
mod shapes;

pub use exec::block_on;
pub use gpu::{Frame, Gpu};
pub use shapes::{CANVAS_H, CANVAS_W, Shape, ShapePass};
