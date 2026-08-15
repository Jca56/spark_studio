use crate::exec::block_on;

/// The GPU context: device, queue, and the window surface it presents to.
pub struct Gpu {
    surface: wgpu::Surface<'static>,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
}

/// One acquired swapchain frame. Draw to `view`, then call `present`.
pub struct Frame {
    surface: wgpu::SurfaceTexture,
    pub view: wgpu::TextureView,
}

impl Frame {
    pub fn present(self) {
        self.surface.present();
    }
}

impl Gpu {
    pub fn new(target: impl Into<wgpu::SurfaceTarget<'static>>, width: u32, height: u32) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(target)
            .expect("create wgpu surface");
        let adapter = block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        }))
        .expect("no compatible GPU adapter");
        let (device, queue) = block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .expect("request GPU device");
        let config = surface
            .get_default_config(&adapter, width.max(1), height.max(1))
            .expect("surface not supported by adapter");
        surface.configure(&device, &config);
        Self {
            surface,
            device,
            queue,
            config,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    pub fn surface_format(&self) -> wgpu::TextureFormat {
        self.config.format
    }

    pub fn begin_frame(&mut self) -> Option<Frame> {
        let surface = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                return None;
            }
            Err(_) => return None,
        };
        let view = surface
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        Some(Frame { surface, view })
    }
}
