//! spark_render — wgpu core for Spark Studio.
//!
//! Owns the GPU device, surface, and (soon) the offscreen HDR targets and
//! post-fx chain. Windowing-agnostic: anything that can become a
//! `wgpu::SurfaceTarget` can be rendered to.

pub use wgpu;

mod exec;
mod gpu;

pub use exec::block_on;
pub use gpu::Gpu;
