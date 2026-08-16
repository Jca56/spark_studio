//! Headless pipeline validation: builds the shape pass (and thus compiles
//! the WGSL) without a window, so shader errors surface in CI/terminal
//! instead of at app launch.

fn main() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = spark_render::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .expect("no adapter");
    let (device, _queue) =
        spark_render::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("no device");
    let _pass = spark_render::ShapePass::new(&device, wgpu::TextureFormat::Bgra8UnormSrgb);
    println!("shape pipeline OK");
}
