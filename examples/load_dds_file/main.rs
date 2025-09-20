use std::ffi::CStr;
use std::marker::PhantomData;

use gl::types::{GLint, GLuint};
use glutin::prelude::*;
use glutin::surface::Surface;
use gpu_texture::{GPUTexture, TextureData, TextureFormat};
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

/// OpenGL ID
#[derive(Debug, Copy, Clone)]
struct GLId(u32);

impl GLId {
    pub fn new(value: u32) -> Self {
        Self(value)
    }

    pub fn value(&self) -> u32 {
        self.0
    }
}

impl From<GLId> for u32 {
    fn from(id: GLId) -> Self {
        id.0
    }
}

impl From<u32> for GLId {
    fn from(value: u32) -> Self {
        GLId(value)
    }
}

/// A binding handle for an OpenGL resource. Structs implementing [GlBind] return an impl of this after
/// binding their resource.
///
/// Dropping this unbinds the resource.
trait GlBindHandle: Drop {}

trait GlBind {
    /// Binds `self` in the current OpenGL context and returns a [GlBindHandle] impl which will automatically
    /// unbind `self` when dropped.
    fn bind(&self) -> impl GlBindHandle;
}

pub struct Buffer<T: Sized> {
    _type: PhantomData<T>,
    id: GLId,
    buf_type: gl::types::GLenum,
    /// Number of items in the buffer
    count: usize,
}

/// [GlBindHandle] for [Buffer] instances
pub struct BufferBindHandle<'h, T>(&'h Buffer<T>);

impl<T> GlBindHandle for BufferBindHandle<'_, T> {}

impl<T> Drop for BufferBindHandle<'_, T> {
    fn drop(&mut self) {
        let buffer = self.0;
        unsafe {
            gl::BindBuffer(buffer.buf_type, buffer.id.into());
        }
    }
}

impl<T> Drop for Buffer<T> {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteBuffers(1, &(self.id.into()));
        }
    }
}

impl<T> GlBind for Buffer<T> {
    fn bind(&self) -> impl GlBindHandle {
        unsafe {
            gl::BindBuffer(self.buf_type, self.id.into());
        }
        BufferBindHandle(self)
    }
}

impl<T: Sized> Buffer<T> {
    fn create(gl_buf_type: gl::types::GLenum) -> Self {
        let mut id = 0u32;
        unsafe {
            gl::CreateBuffers(1, &mut id);
        }
        Self {
            _type: PhantomData,
            id: id.into(),
            buf_type: gl_buf_type,
            count: 0,
        }
    }

    /// Creates a new immutable GPU storage buffer initialized with the contents of `data`
    pub fn new_storage(data: &[T], gl_buf_type: gl::types::GLenum) -> Self {
        let mut buffer = Buffer::<T>::create(gl_buf_type);
        buffer.count = data.len();
        unsafe {
            gl::NamedBufferStorage(
                buffer.id.into(),
                size_of_val(data) as gl::types::GLsizeiptr,
                data.as_ptr() as _,
                0,
            );
        }

        buffer
    }
}

#[repr(C)]
struct InstanceData<const N: usize> {
    pub count: u32,
    pub transform: [glam::Mat4; N],
}

struct VertexAttributeObject {
    id: GLId,
    index_buffer: Buffer<u16>,
}

struct VAOBindHandle<'h>(&'h VertexAttributeObject);

impl GlBindHandle for VAOBindHandle<'_> {}

impl Drop for VAOBindHandle<'_> {
    fn drop(&mut self) {
        unsafe {
            gl::BindVertexArray(0);
        }
    }
}

impl GlBind for VertexAttributeObject {
    fn bind(&self) -> impl GlBindHandle {
        unsafe {
            gl::BindVertexArray(self.id.into());
        }
        VAOBindHandle(self)
    }
}

impl VertexAttributeObject {
    pub fn new() -> Self {
        let mut id = 0u32;
        unsafe {
            gl::GenVertexArrays(1, &mut id);
            gl::BindVertexArray(id);
        }
        let index_buffer = Buffer::new_storage(&[0u16, 1, 2, 2, 1, 3], gl::ELEMENT_ARRAY_BUFFER);

        unsafe {
            let _handle = index_buffer.bind();
            // let handle = transforms_buffer.bind();
            // gl::EnableVertexAttribArray(0);
            // gl::VertexAttribPointer(0, 2, gl::FLOAT, gl::FALSE, 0, std::ptr::null());
            // gl::VertexAttribDivisor(0, 1);
            // drop(handle);
            gl::BindVertexArray(0);
        };

        Self {
            id: id.into(),
            index_buffer,
        }
    }
}

struct CompressedTexture2d {
    id: GLId,
}

impl Drop for CompressedTexture2d {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &(self.id.into()));
        }
    }
}

impl CompressedTexture2d {
    fn new_with_separate_mips(gpu_texture: GPUTexture) -> Vec<Self> {
        let compression_fmt = match gpu_texture.format {
            TextureFormat::Compressed(format) => match format {
                gpu_texture::CompressedTextureFormat::BC1 => 0x83F0,
                gpu_texture::CompressedTextureFormat::BC1A => 0x83F1,
                gpu_texture::CompressedTextureFormat::BC2 => 0x83F2,
                gpu_texture::CompressedTextureFormat::BC3 => 0x83F3,
            },
            _ => panic!("Only compressed texture formats supported"),
        };

        let image_count = gpu_texture
            .mip_maps
            .as_ref()
            .map(|mips| mips.len() + 1)
            .unwrap_or(1);

        let mut tex_ids = vec![0u32; image_count];
        unsafe {
            gl::GenTextures(image_count as i32, tex_ids.as_mut_ptr());
        };

        let load_texture = |id: u32, tex_data: &TextureData, compression_fmt: u32| -> Self {
            unsafe {
                gl::BindTexture(gl::TEXTURE_2D, id);
                gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
                gl::CompressedTexImage2D(
                    gl::TEXTURE_2D,
                    0,
                    compression_fmt,
                    tex_data.width as i32,
                    tex_data.height as i32,
                    0,
                    tex_data.img_buffer.len() as i32,
                    tex_data.img_buffer.as_ptr() as *const _,
                );
                gl::BindTexture(gl::TEXTURE_2D, 0);
            }

            Self { id: id.into() }
        };

        let mut textures = Vec::with_capacity(image_count);

        textures.push(load_texture(
            tex_ids[0],
            &gpu_texture.main_image,
            compression_fmt,
        ));

        if let Some(mip_maps) = &gpu_texture.mip_maps {
            for i in 1..image_count {
                textures.push(load_texture(tex_ids[i], &mip_maps[i - 1], compression_fmt));
            }
        }

        textures
    }
}

struct GlProgram {
    id: GLId,
}

impl Drop for GlProgram {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.id.into());
        }
    }
}

impl GlProgram {
    fn new(vertex_src: &str, fragment_src: &str) -> Self {
        let get_info_log = |log_fn: unsafe fn(u32, i32, *mut i32, *mut i8), id: u32| -> String {
            let mut buffer = [0i8; 512];
            let mut log_len = 0i32;
            unsafe {
                log_fn(
                    id,
                    buffer.len() as i32,
                    &raw mut log_len,
                    buffer.as_mut_ptr(),
                );

                let c_str = CStr::from_ptr(buffer.as_ptr());
                c_str.to_string_lossy().into_owned()
            }
        };

        let compile_stage = |src: &str, stage: gl::types::GLenum| -> GLuint {
            unsafe {
                let shader = gl::CreateShader(stage);
                let src_len = src.len() as i32;
                let src_ptr = src.as_ptr() as *const i8;
                gl::ShaderSource(shader, 1, &src_ptr, &raw const src_len);
                gl::CompileShader(shader);

                let mut status = gl::FALSE as GLint;
                gl::GetShaderiv(shader, gl::COMPILE_STATUS, &mut status);
                if status != (gl::TRUE as GLint) {
                    let info_log = get_info_log(gl::GetShaderInfoLog, shader);
                    gl::DeleteShader(shader);
                    panic!("{info_log}");
                }

                shader
            }
        };

        let vertex_shader = compile_stage(vertex_src, gl::VERTEX_SHADER);
        let fragment_shader = compile_stage(fragment_src, gl::FRAGMENT_SHADER);

        unsafe {
            let program_id = gl::CreateProgram();
            gl::AttachShader(program_id, vertex_shader);
            gl::AttachShader(program_id, fragment_shader);
            gl::LinkProgram(program_id);

            gl::DeleteShader(vertex_shader);
            gl::DeleteShader(fragment_shader);

            let mut status = gl::FALSE as GLint;
            gl::GetProgramiv(program_id, gl::LINK_STATUS, &mut status);
            if status != (gl::TRUE as GLint) {
                let info_log = get_info_log(gl::GetProgramInfoLog, program_id);
                gl::DeleteProgram(program_id);
                panic!("{info_log}");
            }

            Self {
                id: program_id.into(),
            }
        }
    }
}

struct Scene {
    instance_count: u32,
    instance_data: Buffer<glam::Mat4>,
    vao: VertexAttributeObject,
    textures: Vec<CompressedTexture2d>,
    program: GlProgram,
}

const PLANE_VERTICES: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [-1.0, 1.0], [1.0, 1.0]];

impl Scene {
    fn new(texture_path: &Path) -> Self {
        // Load texture file with mip maps from the file system
        let gpu_texture = GPUTexture::load_from_file(texture_path).unwrap();

        let textures = CompressedTexture2d::new_with_separate_mips(gpu_texture);

        let program = GlProgram::new(VERTEX_SHADER, FRAGMENT_SHADER);

        let tex_count_f = textures.len() as f32;
        let mut transforms = Vec::with_capacity(textures.len());
        let projection_mat =
            glam::Mat4::orthographic_rh_gl(0.0, tex_count_f, tex_count_f, 0.0, -1.0, 1.0); // May need to come back to this to figure out the right values

        // let plane_scale = glam::Vec3::splat((1.0f32 / tex_count_f) - 0.01);
        for i in 0..textures.len() {
            let matrix = projection_mat
                * glam::Mat4::from_scale_rotation_translation(
                    glam::Vec3::new(1.0, 1.0, 1.0),
                    glam::Quat::IDENTITY,
                    glam::Vec3::new(i as f32, i as f32, 0.0),
                );
            transforms.push(matrix);
        }

        let instance_data_buf = Buffer::new_storage(&transforms, gl::SHADER_STORAGE_BUFFER);

        Self {
            instance_count: textures.len() as u32,
            instance_data: instance_data_buf,
            textures,
            program,
            vao: VertexAttributeObject::new(),
        }
    }

    fn draw(&self) {
        unsafe {
            gl::UseProgram(self.program.id.into());
            gl::Uniform1ui(0, self.instance_count);
            gl::BindBufferBase(gl::SHADER_STORAGE_BUFFER, 0, self.instance_data.id.into());

            gl::BindVertexArray(self.vao.id.into());
            gl::DrawElementsInstanced(gl::TRIANGLES, 6, gl::UNSIGNED_SHORT, std::ptr::null(), 11);
            gl::BindVertexArray(0);
        }
    }
}

#[derive(Default)]
struct App {
    did_initialize: bool,
    window: Option<Window>,
    display: Option<glutin::display::Display>,
    scene: Option<Scene>,
    frame_buffer: Option<Surface<glutin::surface::WindowSurface>>,
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
                    glutin::context::Version::new(4, 5),
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

        let gl_extensions = unsafe {
            let mut num_extensions = 0;
            gl::GetIntegerv(gl::NUM_EXTENSIONS, &mut num_extensions);

            let mut extensions = Vec::with_capacity(num_extensions as usize);
            for i in (0..num_extensions as u32) {
                let ext_name = gl::GetStringi(gl::EXTENSIONS, i);
                extensions.push(
                    CStr::from_ptr(ext_name as *const _)
                        .to_str()
                        .unwrap()
                        .to_owned(),
                );
            }
            extensions
        };

        println!("OpenGL extensions: {:#?}", gl_extensions);

        unsafe {
            gl::Enable(gl::ALPHA | gl::CULL_FACE);
            gl::Viewport(0, 0, 800, 800);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            frame_buffer.swap_buffers(&context).unwrap();
        }

        let scene = Scene::new(Path::new(BC3_DDS_PATH));

        self.window = Some(window);
        self.display = Some(display);
        self.frame_buffer = Some(frame_buffer);
        self.gl_ctx = Some(context);
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

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);

            if let Some(scene) = self.scene.as_ref() {
                scene.draw();
            }

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
