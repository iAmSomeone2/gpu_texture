use glutin::prelude::*;
use gpu_texture::{GPUTexture, TextureFormat};
use raw_window_handle::HasWindowHandle;
use std::num::NonZeroU32;
use std::path::Path;
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    raw_window_handle::HasDisplayHandle,
    window::{Window, WindowId},
};

const VERTEX_SHADER: &str = include_str!("shaders/demo.vert.glsl");
const FRAGMENT_SHADER: &str = include_str!("shaders/demo.frag.glsl");

const BC3_DDS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/cd-rw.dds");

struct TransformBlock {
    count: u32,
    matrix: Vec<glam::Mat4>,
}

impl TransformBlock {
    pub fn compute_size(&self) -> usize {
        size_of::<u32>() + (self.matrix.len() * size_of::<glam::Mat4>())
    }
}

impl From<TransformBlock> for Vec<u8> {
    fn from(t: TransformBlock) -> Self {
        let mut bytes = vec![0u8; t.compute_size()];

        bytes[..4].copy_from_slice(&t.count.to_le_bytes());
        bytes[4..].copy_from_slice(bytemuck::cast_slice(&t.matrix));

        bytes
    }
}

// struct Scene {
//     vertex_attrib_array:
//     transform_buffer: Buffer<[u8]>,
//     textures: Vec<CompressedTexture2d>,
//     program: Program,
// }

// impl Scene {
//     fn new_from_compressed_texture<F: ?Sized + Facade>(facade: &F, path: &Path) -> Self {
//         // Load texture file with mip maps from the file system
//         let gpu_texture = GPUTexture::load_from_file(path).unwrap();
//
//         // Map gpu_texture::TextureFormat to glium::texture::CompressedFormat
//         let format: glium::texture::CompressedFormat = match gpu_texture.format {
//             TextureFormat::Compressed(format) => match format {
//                 gpu_texture::CompressedTextureFormat::BC1 => {
//                     glium::texture::CompressedFormat::S3tcDxt1NoAlpha
//                 }
//                 gpu_texture::CompressedTextureFormat::BC1A => {
//                     glium::texture::CompressedFormat::S3tcDxt1Alpha
//                 }
//                 gpu_texture::CompressedTextureFormat::BC2 => {
//                     glium::texture::CompressedFormat::S3tcDxt3Alpha
//                 }
//                 gpu_texture::CompressedTextureFormat::BC3 => {
//                     glium::texture::CompressedFormat::S3tcDxt5Alpha
//                 }
//             },
//             _ => panic!("Only compressed textures are supported"), // This should be a proper Result in production code
//         };
//
//         // To demo the mip maps as separate planes, we will store them as separate glium::texture::CompressedTexture2d instances
//         let mut textures = vec![
//             CompressedTexture2d::with_compressed_data(
//                 facade,
//                 &gpu_texture.main_image.img_buffer,
//                 gpu_texture.main_image.width,
//                 gpu_texture.main_image.height,
//                 format,
//                 CompressedMipmapsOption::NoMipmap,
//             )
//             .unwrap(),
//         ];
//
//         if let Some(mip_maps) = gpu_texture.mip_maps {
//             for mip_map in mip_maps {
//                 textures.push(
//                     CompressedTexture2d::with_compressed_data(
//                         facade,
//                         &mip_map.img_buffer,
//                         mip_map.width,
//                         mip_map.height,
//                         format,
//                         CompressedMipmapsOption::NoMipmap,
//                     )
//                     .unwrap(),
//                 );
//             }
//         }
//
//         // Calculate values for the transforms buffer based on how many total textures are present
//         let mut transform_block = TransformBlock {
//             count: textures.len() as u32,
//             matrix: Vec::with_capacity(textures.len()),
//         };
//         let tex_count_f = textures.len() as f32;
//         let projection_mat =
//             glam::Mat4::orthographic_rh_gl(0.0, tex_count_f, tex_count_f, 0.0, 0.01, 100.0); // May need to come back to this to figure out the right values
//
//         let plane_scale = glam::Vec3::splat(1.0f32 / tex_count_f);
//         for i in 0..textures.len() {
//             let matrix = projection_mat
//                 * glam::Mat4::from_scale_rotation_translation(
//                     plane_scale,
//                     glam::Quat::IDENTITY,
//                     glam::Vec3::new(i as f32, i as f32, 1.0),
//                 );
//             transform_block.matrix.push(matrix);
//         }
//
//         let transform_buffer: Buffer<[u8]> = Buffer::new(
//             facade,
//             Vec::from(transform_block).as_slice(),
//             BufferType::ArrayBuffer,
//             BufferMode::Immutable,
//         )
//         .unwrap();
//
//         let program = Program::from_source(facade, VERTEX_SHADER, FRAGMENT_SHADER, None).unwrap();
//
//         Self {
//             textures,
//             transform_buffer,
//             program,
//         }
//     }
// }

#[derive(Default)]
struct App {
    did_initialize: bool,
    window: Option<Window>,
    display: Option<glutin::display::Display>,
    display_config: Option<glutin::config::Config>,
    frame_buffer: Option<glutin::surface::Surface<glutin::surface::WindowSurface>>,
    gl_ctx: Option<glutin::context::PossiblyCurrentContext>,
}

impl App {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) {
        let window_attribs = winit::window::WindowAttributes::default()
            .with_title("Example: Load DDS File")
            .with_inner_size(winit::dpi::LogicalSize::new(800.0, 800.0))
            .with_resizable(false)
            .with_enabled_buttons(
                winit::window::WindowButtons::CLOSE | winit::window::WindowButtons::MINIMIZE,
            );
        let window = event_loop.create_window(window_attribs).unwrap();

        let display = unsafe {
            glutin::display::Display::new(
                window.display_handle().unwrap().as_raw(),
                glutin::display::DisplayApiPreference::Egl,
            )
        }
        .unwrap();

        let display_configs: Vec<glutin::config::Config> = unsafe {
            let config_template = glutin::config::ConfigTemplateBuilder::new()
                .compatible_with_native_window(window.window_handle().unwrap().as_raw())
                .with_api(glutin::config::Api::OPENGL)
                .with_transparency(false)
                .with_buffer_type(glutin::config::ColorBufferType::Rgb {
                    r_size: 8,
                    g_size: 8,
                    b_size: 8,
                })
                .prefer_hardware_accelerated(Some(true))
                .build();

            display
                .find_configs(config_template)
                .unwrap()
                .collect::<Vec<_>>()
        };

        let context = unsafe {
            let context_attribs = glutin::context::ContextAttributesBuilder::new()
                .with_context_api(glutin::context::ContextApi::OpenGl(Some(
                    glutin::context::Version::new(4, 1),
                )))
                .with_debug(true)
                .with_profile(glutin::context::GlProfile::Core)
                .build(Some(window.window_handle().unwrap().as_raw()));

            display
                .create_context(&display_configs[0], &context_attribs)
                .unwrap()
        };

        let frame_buffer = unsafe {
            let surface_attribs =
                glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                    .with_srgb(Some(true))
                    .with_single_buffer(false)
                    .build(
                        window.window_handle().unwrap().as_raw(),
                        NonZeroU32::new(800).unwrap_unchecked(),
                        NonZeroU32::new(800).unwrap_unchecked(),
                    );

            display
                .create_window_surface(&display_configs[0], &surface_attribs)
                .unwrap()
        };

        let context = context.make_current(&frame_buffer).unwrap();

        gl::load_with(|s| {
            let c_s = ::std::ffi::CString::new(s).unwrap();
            display.get_proc_address(&c_s) as *const _
        });

        unsafe {
            gl::Viewport(0, 0, 800, 800);
            gl::ClearColor(1.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            frame_buffer.swap_buffers(&context).unwrap();
        }

        // let scene = Scene::new_from_compressed_texture(&display, Path::new(BC3_DDS_PATH));

        self.window = Some(window);
        self.display = Some(display);
        self.frame_buffer = Some(frame_buffer);
        self.gl_ctx = Some(context);
        // self.scene = Some(scene);
        self.did_initialize = true;
    }
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.did_initialize {
            self.initialize(event_loop);
        }
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

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);
            self.frame_buffer
                .as_mut()
                .unwrap()
                .swap_buffers(self.gl_ctx.as_ref().unwrap())
                .unwrap();
        }
    }
}

pub fn main() -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    event_loop.run_app(&mut App::default())?;
    Ok(())
}
