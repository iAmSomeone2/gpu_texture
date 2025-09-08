use glium::glutin::surface::WindowSurface;
use glium::texture::TextureFormat as GLTextureFormat;
use glium::winit::event::WindowEvent;
use glium::winit::event_loop::{ActiveEventLoop, ControlFlow};
use glium::winit::window::{Window, WindowButtons, WindowId};
use glium::{CapabilitiesSource, Display, Surface, winit};
use std::collections::HashMap;

#[derive(Default, Debug)]
struct App {
    did_initialize: bool,
    window: Option<Window>,
    display: Option<Display<WindowSurface>>,
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.did_initialize {
            return;
        }

        let (window, display) = glium::backend::glutin::SimpleWindowBuilder::new()
            .with_title("Example: Load DDS File")
            .with_inner_size(800, 800)
            .build(event_loop);
        window.set_enabled_buttons(WindowButtons::CLOSE | WindowButtons::MINIMIZE);
        window.set_resizable(false);

        println!("GL Version: {}", display.get_opengl_version_string());
        let supported_compressed_fmts = display
            .get_capabilities()
            .internal_formats_textures
            .iter()
            .filter(|(tex_fmt, _)| matches!(tex_fmt, GLTextureFormat::CompressedFormat(..)))
            .collect::<HashMap<_, _>>();
        println!(
            "Supported compressed texture formats: {:#?}",
            supported_compressed_fmts
        );

        self.window = Some(window);
        self.display = Some(display);
        self.did_initialize = true;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if event == WindowEvent::CloseRequested {
            event_loop.exit();
        }

        let mut frame = self.display.as_ref().unwrap().draw();
        frame.clear_color(0.0, 0.0, 0.0, 1.0);
        frame.finish().unwrap();
    }
}

pub fn main() -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.run_app(&mut App::default())?;
    Ok(())
}
