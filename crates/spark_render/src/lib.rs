//! spark_render — wgpu core for Spark Studio.
//!
//! Owns the GPU device, surface, and the render passes. Windowing-agnostic:
//! anything that can become a `wgpu::SurfaceTarget` can be rendered to.

pub use wgpu;

mod camera;
mod exec;
mod geom;
mod gpu;
mod light;
mod math;
mod pass;
mod sdf;
mod shapes;

pub use camera::{Camera, Framing};
pub use exec::block_on;
pub use geom::Viewport;
pub use gpu::{Frame, Gpu};
pub use light::{LIGHT_KINDS, Light, LightKind, MAX_LIGHTS};
pub use math::{Mat4, Vec3};
pub use sdf::sd_segment;
pub use pass::{GpuMesh, Layer, MeshData, MeshInstance, Scene, ShapePass, Stage, TextureData};
pub use shapes::{CANVAS_H, CANVAS_W, FIELDS, LIGHT_PICK, STAR_FORMS, Shape, ShapeKind};
