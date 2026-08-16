//! Headless UI-pipeline validation: compiles ui.wgsl without a window.

use spark_render::wgpu;

fn main() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = spark_render::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .expect("no adapter");
    let (device, queue) =
        spark_render::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("no device");
    let _pass = spark_ui::UiPass::new(
        &device,
        &queue,
        wgpu::TextureFormat::Bgra8UnormSrgb,
        &[0, 0, 0, 0],
        1,
    );
    println!("ui pipeline OK");
}
