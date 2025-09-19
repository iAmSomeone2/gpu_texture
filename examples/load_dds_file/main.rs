mod geometry;
mod material;
mod mesh;
mod scene;

use glium::backend::Facade;
use glium::buffer::{Buffer, BufferMode, BufferType, Content};
use glium::glutin::surface::WindowSurface;
use glium::texture::{
    CompressedMipmapsOption, CompressedTexture2d, TextureFormat as GLTextureFormat,
};
use glium::winit::event::WindowEvent;
use glium::winit::event_loop::{ActiveEventLoop, ControlFlow};
use glium::winit::window::{Window, WindowButtons, WindowId};
use glium::{CapabilitiesSource, Display, Program, Surface, implement_buffer_content, winit};
use gpu_texture::{GPUTexture, TextureFormat};
use std::collections::HashMap;
use std::path::Path;

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

struct Scene {
    vertex_attrib_array:
    transform_buffer: Buffer<[u8]>,
    textures: Vec<CompressedTexture2d>,
    program: Program,
}

impl Scene {
    fn new_from_compressed_texture<F: ?Sized + Facade>(facade: &F, path: &Path) -> Self {
        // Load texture file with mip maps from the file system
        let gpu_texture = GPUTexture::load_from_file(path).unwrap();

        // Map gpu_texture::TextureFormat to glium::texture::CompressedFormat
        let format: glium::texture::CompressedFormat = match gpu_texture.format {
            TextureFormat::Compressed(format) => match format {
                gpu_texture::CompressedTextureFormat::BC1 => {
                    glium::texture::CompressedFormat::S3tcDxt1NoAlpha
                }
                gpu_texture::CompressedTextureFormat::BC1A => {
                    glium::texture::CompressedFormat::S3tcDxt1Alpha
                }
                gpu_texture::CompressedTextureFormat::BC2 => {
                    glium::texture::CompressedFormat::S3tcDxt3Alpha
                }
                gpu_texture::CompressedTextureFormat::BC3 => {
                    glium::texture::CompressedFormat::S3tcDxt5Alpha
                }
            },
            _ => panic!("Only compressed textures are supported"), // This should be a proper Result in production code
        };

        // To demo the mip maps as separate planes, we will store them as separate glium::texture::CompressedTexture2d instances
        let mut textures = vec![
            CompressedTexture2d::with_compressed_data(
                facade,
                &gpu_texture.main_image.img_buffer,
                gpu_texture.main_image.width,
                gpu_texture.main_image.height,
                format,
                CompressedMipmapsOption::NoMipmap,
            )
            .unwrap(),
        ];

        if let Some(mip_maps) = gpu_texture.mip_maps {
            for mip_map in mip_maps {
                textures.push(
                    CompressedTexture2d::with_compressed_data(
                        facade,
                        &mip_map.img_buffer,
                        mip_map.width,
                        mip_map.height,
                        format,
                        CompressedMipmapsOption::NoMipmap,
                    )
                    .unwrap(),
                );
            }
        }

        // Calculate values for the transforms buffer based on how many total textures are present
        let mut transform_block = TransformBlock {
            count: textures.len() as u32,
            matrix: Vec::with_capacity(textures.len()),
        };
        let tex_count_f = textures.len() as f32;
        let projection_mat =
            glam::Mat4::orthographic_rh_gl(0.0, tex_count_f, tex_count_f, 0.0, 0.01, 100.0); // May need to come back to this to figure out the right values

        let plane_scale = glam::Vec3::splat(1.0f32 / tex_count_f);
        for i in 0..textures.len() {
            let matrix = projection_mat
                * glam::Mat4::from_scale_rotation_translation(
                    plane_scale,
                    glam::Quat::IDENTITY,
                    glam::Vec3::new(i as f32, i as f32, 1.0),
                );
            transform_block.matrix.push(matrix);
        }

        let transform_buffer: Buffer<[u8]> = Buffer::new(
            facade,
            Vec::from(transform_block).as_slice(),
            BufferType::ArrayBuffer,
            BufferMode::Immutable,
        )
        .unwrap();

        let program = Program::from_source(facade, VERTEX_SHADER, FRAGMENT_SHADER, None).unwrap();

        Self {
            textures,
            transform_buffer,
            program,
        }
    }
}

#[derive(Default)]
struct App {
    did_initialize: bool,
    window: Option<Window>,
    display: Option<Display<WindowSurface>>,
    scene: Option<Scene>,
}

impl App {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) {
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

        let scene = Scene::new_from_compressed_texture(&display, Path::new(BC3_DDS_PATH));

        self.window = Some(window);
        self.display = Some(display);
        self.scene = Some(scene);
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
