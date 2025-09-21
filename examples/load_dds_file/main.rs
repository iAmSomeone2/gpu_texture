use gl::types::{GLint, GLuint};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::Surface;
use gpu_texture::{GPUTexture, TextureData, TextureFormat};
use raw_window_handle::HasWindowHandle;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::mem::offset_of;
use std::num::NonZeroU32;
use std::path::Path;
use std::ptr;
use std::time::{Duration, Instant};
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    raw_window_handle::HasDisplayHandle,
    window::{Window, WindowId},
};

const VERTEX_SHADER: &str = include_str!("shaders/demo.vert.glsl");
const FRAGMENT_SHADER: &str = include_str!("shaders/demo.frag.glsl");

const BC3_DDS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/cd-rw.dds");

const WINDOW_SIZE: u32 = 1280;

/// OpenGL ID
#[derive(Debug, Copy, Clone)]
struct GLId(u32);

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

/// A binding handle for an OpenGL resource. Structs implementing [GlBind] return this after
/// binding their resource.
///
/// Dropping this unbinds the resource.
struct GlBindHandle<'h, B: GlUnbind>(&'h B);

impl<B: GlUnbind> Drop for GlBindHandle<'_, B> {
    fn drop(&mut self) {
        self.0.unbind();
    }
}

trait GlBind<'b>: GlUnbind {
    /// Binds `self` in the current OpenGL context and returns a [GlBindHandle] impl which will automatically
    /// unbind `self` when dropped.
    fn bind(&'_ self) -> GlBindHandle<'_, Self>
    where
        Self: Sized;
}

trait GlUnbind {
    /// Unbinds `self` in the current OpenGL context
    fn unbind(&self);
}

pub struct Buffer<T: Sized> {
    _type: PhantomData<T>,
    id: GLId,
    buf_type: gl::types::GLenum,
    /// Number of items in the buffer
    count: usize,
}

impl<T> GlBind<'_> for Buffer<T> {
    fn bind(&'_ self) -> GlBindHandle<'_, Self> {
        unsafe {
            gl::BindBuffer(self.buf_type, self.id.into());
        }
        GlBindHandle(self)
    }
}

impl<T> GlUnbind for Buffer<T> {
    fn unbind(&self) {
        unsafe {
            gl::BindBuffer(self.buf_type, 0);
        }
    }
}

impl<T> Buffer<T> {
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
        let data_size = std::mem::size_of_val(data);
        // let data: &[u8] = bytemuck::cast_slice(data);
        unsafe {
            gl::NamedBufferStorage(
                buffer.id.into(),
                data_size as gl::types::GLsizeiptr,
                data.as_ptr() as *const _,
                0,
            );
        }

        buffer
    }
}

struct VertexAttributeObject(GLId);

impl GlBind<'_> for VertexAttributeObject {
    fn bind(&'_ self) -> GlBindHandle<'_, Self> {
        unsafe {
            gl::BindVertexArray(self.0.into());
        }
        GlBindHandle(self)
    }
}

impl GlUnbind for VertexAttributeObject {
    fn unbind(&self) {
        unsafe {
            gl::BindVertexArray(0);
        }
    }
}

impl VertexAttributeObject {
    pub fn new() -> Self {
        let mut id = 0u32;
        unsafe {
            gl::GenVertexArrays(1, &mut id);
        }

        Self(id.into())
    }
}

struct CompressedTexture2d(GLId);

impl Drop for CompressedTexture2d {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &(self.0.into()));
        }
    }
}

impl TryFrom<GPUTexture> for CompressedTexture2d {
    type Error = anyhow::Error;

    fn try_from(value: GPUTexture) -> Result<Self, Self::Error> {
        // TODO: Move this logic into the library
        let compression_fmt = match value.format {
            TextureFormat::Compressed(format) => match format {
                gpu_texture::CompressedTextureFormat::BC1 => 0x83F0,
                gpu_texture::CompressedTextureFormat::BC1A => 0x83F1,
                gpu_texture::CompressedTextureFormat::BC2 => 0x83F2,
                gpu_texture::CompressedTextureFormat::BC3 => 0x83F3,
            },
            _ => {
                return Err(anyhow::anyhow!("Only compressed texture formats supported"));
            }
        };

        // TODO: add this as a feature in the library
        // let image_count = value
        //     .mip_maps
        //     .as_ref()
        //     .map(|mips| mips.len() + 1)
        //     .unwrap_or(1);

        // Generate texture and bind it
        let mut tex_id = 0u32;
        unsafe {
            gl::GenTextures(1, &raw mut tex_id);
            gl::BindTexture(gl::TEXTURE_2D, tex_id);
        };

        // Reusable lambda for loading texture data
        let load_texture = |tex_data: &TextureData, level: i32| unsafe {
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);

            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::REPEAT as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::REPEAT as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::CompressedTexImage2D(
                gl::TEXTURE_2D,
                level,
                compression_fmt,
                tex_data.width as i32,
                tex_data.height as i32,
                0,
                tex_data.img_buffer.len() as i32,
                tex_data.img_buffer.as_ptr() as *const _,
            );
        };

        // Load the main image as the top level
        load_texture(&value.main_image, 0);

        if let Some(mip_maps) = value.mip_maps {
            mip_maps.iter().enumerate().for_each(|(level, tex_data)| {
                load_texture(tex_data, (level as i32) + 1);
            });
        }

        // Unbind the target texture to keep things clean.
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, 0);
        }

        Ok(Self(tex_id.into()))
    }
}

struct GlProgram(GLId);

impl Drop for GlProgram {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteProgram(self.0.into());
        }
    }
}

impl GlProgram {
    fn new(vertex_src: &str, fragment_src: &str) -> anyhow::Result<Self> {
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

        let compile_stage = |src: &str, stage: gl::types::GLenum| -> Result<GLuint, String> {
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

                    Err(info_log)
                } else {
                    Ok(shader)
                }
            }
        };

        let vertex_shader =
            compile_stage(vertex_src, gl::VERTEX_SHADER).map_err(anyhow::Error::msg)?;
        let fragment_shader =
            compile_stage(fragment_src, gl::FRAGMENT_SHADER).map_err(|log_msg| {
                unsafe { gl::DeleteShader(vertex_shader) };

                anyhow::Error::msg(log_msg)
            })?;

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

            Ok(Self(program_id.into()))
        }
    }
}

#[repr(C, align(4))]
#[derive(Clone, Copy, Debug)]
struct Vertex {
    _pad: [u8; 4],
    position: glam::Vec3,
    tex_coords: glam::Vec2,
}

impl Vertex {
    const fn new(position: [f32; 3], tex_coords: [f32; 2]) -> Self {
        Self {
            _pad: [0; 4],
            position: glam::Vec3::new(position[0], position[1], position[2]),
            tex_coords: glam::Vec2::new(tex_coords[0], tex_coords[1]),
        }
    }

    fn set_attribute_ptrs() {
        let position_offset: usize = offset_of!(Vertex, position);
        let tex_coords_offset: usize = offset_of!(Vertex, tex_coords);
        let stride: i32 = (size_of::<f32>() as i32) * 6;
        unsafe {
            gl::EnableVertexAttribArray(0);
            gl::VertexAttribPointer(
                0,
                3,
                gl::FLOAT,
                gl::FALSE,
                stride,
                position_offset as *const _,
            );

            gl::EnableVertexAttribArray(1);
            gl::VertexAttribPointer(
                1,
                2,
                gl::FLOAT,
                gl::FALSE,
                stride,
                tex_coords_offset as *const _,
            )
        }
    }
}

struct Geometry {
    vertices: Buffer<Vertex>,
    indices: Option<Buffer<u16>>,
    vao: VertexAttributeObject,
}

impl Geometry {
    fn new_plane() -> Self {
        const VERTICES: [Vertex; 4] = [
            // Front face (lower-left)
            Vertex::new([-1.0, -1.0, 0.0], [0.0, 0.0]),
            // Front face (lower-right)
            Vertex::new([1.0, -1.0, 0.0], [1.0, 0.0]),
            // Front face (upper-left)
            Vertex::new([-1.0, 1.0, 0.0], [0.0, 1.0]),
            // Front face (upper-right)
            Vertex::new([1.0, 1.0, 0.0], [1.0, 1.0]),
        ];

        const INDICES: [u16; 6] = [0, 1, 2, 2, 1, 3];

        let vertices = Buffer::new_storage(&VERTICES, gl::ARRAY_BUFFER);
        let indices = Some(Buffer::new_storage(&INDICES, gl::ELEMENT_ARRAY_BUFFER));

        let vao = Geometry::make_vao(&vertices, &indices);

        Self {
            vertices,
            indices,
            vao,
        }
    }

    fn make_vao(vertices: &Buffer<Vertex>, indices: &Option<Buffer<u16>>) -> VertexAttributeObject {
        let vao = VertexAttributeObject::new();
        let vao_handle = vao.bind();
        let _vert_handle = vertices.bind();
        Vertex::set_attribute_ptrs();
        let _index_handle = indices.as_ref().map(|i| i.bind());
        drop(vao_handle);

        vao
    }

    fn draw(&self) {
        let _vao_handle = self.vao.bind();
        unsafe {
            if let Some(indices) = self.indices.as_ref() {
                gl::DrawElements(
                    gl::TRIANGLES,
                    indices.count as i32,
                    gl::UNSIGNED_SHORT,
                    ptr::null(),
                );
            } else {
                gl::DrawArrays(gl::TRIANGLES, 0, self.vertices.count as i32);
            }
        }
    }
}

struct Material {
    program: GlProgram,
    texture: CompressedTexture2d,
}

impl Material {
    fn new(gpu_texture: GPUTexture) -> anyhow::Result<Self> {
        Ok(Self {
            program: GlProgram::new(VERTEX_SHADER, FRAGMENT_SHADER)?,
            texture: CompressedTexture2d::try_from(gpu_texture)?,
        })
    }

    fn apply(&self) {
        unsafe {
            gl::UseProgram(self.program.0.into());

            // gl::ActiveTexture(gl::TEXTURE0);
            gl::BindTexture(gl::TEXTURE_2D, self.texture.0.into());
        }
    }
}

struct Mesh {
    geometry: Geometry,
    material: Material,
}

impl Mesh {
    fn new_textured_plane(texture: GPUTexture) -> anyhow::Result<Self> {
        let material = Material::new(texture)?;
        Ok(Self {
            geometry: Geometry::new_plane(),
            material,
        })
    }

    fn draw<P: Into<glam::Vec3> + Copy>(&self, projection: &glam::Mat4, position: &P) {
        self.material.apply();

        let transform = glam::Mat4::from_translation((*position).into());

        unsafe {
            let projection_ptr = &raw const (*projection);
            let transform_ptr = &raw const transform;

            gl::UniformMatrix4fv(2, 1, gl::FALSE, projection_ptr as *const _);
            gl::UniformMatrix4fv(3, 1, gl::FALSE, transform_ptr as *const _);
        }

        self.geometry.draw();
    }
}

struct DemoScene {
    projection: glam::Mat4,
    mesh: Mesh,
    mesh_position: glam::Vec3A,
    start_instant: Option<Instant>,
    near_plane: f32,
    far_plane: f32,
}

impl DemoScene {
    fn new(texture_path: &Path) -> anyhow::Result<Self> {
        let near_plane = 0.01;
        let far_plane = 100.0;

        // Load texture file from the file system
        let gpu_texture = GPUTexture::load_from_file(texture_path)?;
        let projection_mat = glam::Mat4::perspective_rh_gl(70.0, 1.0, near_plane, far_plane);

        let mesh = Mesh::new_textured_plane(gpu_texture)?;

        Ok(Self {
            projection: projection_mat,
            mesh,
            mesh_position: glam::Vec3A::ZERO,
            start_instant: None,
            near_plane,
            far_plane,
        })
    }

    fn draw(&mut self) {
        let half_far_plane = self.far_plane * 0.5;
        let elapsed = if let Some(instant) = self.start_instant {
            instant.elapsed().as_secs_f32()
        } else {
            self.start_instant = Some(Instant::now());
            0.0
        };

        let mesh_z =
            (f32::cos(elapsed) * half_far_plane * 0.5) - (half_far_plane - self.near_plane);
        self.mesh_position.z = mesh_z;

        self.mesh.draw(&self.projection, &self.mesh_position);
    }
}

#[derive(Default)]
struct App {
    did_initialize: bool,
    window: Option<Window>,
    display: Option<glutin::display::Display>,
    scene: Option<DemoScene>,
    frame_buffer: Option<Surface<glutin::surface::WindowSurface>>,
    gl_ctx: Option<glutin::context::PossiblyCurrentContext>,
}

impl App {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let window = {
            let window_attribs = winit::window::WindowAttributes::default()
                .with_title("Example: Load DDS File")
                .with_inner_size(winit::dpi::LogicalSize::new(WINDOW_SIZE, WINDOW_SIZE))
                .with_resizable(false)
                .with_enabled_buttons(
                    winit::window::WindowButtons::CLOSE | winit::window::WindowButtons::MINIMIZE,
                );

            event_loop.create_window(window_attribs)?
        };

        let display = unsafe {
            glutin::display::Display::new(
                window.display_handle()?.as_raw(),
                glutin::display::DisplayApiPreference::Egl,
            )
        }?;

        let display_config: glutin::config::Config = unsafe {
            let config_template = glutin::config::ConfigTemplateBuilder::new()
                .compatible_with_native_window(window.window_handle()?.as_raw())
                .with_api(glutin::config::Api::OPENGL)
                .with_transparency(false)
                .prefer_hardware_accelerated(Some(true))
                .build();

            display
                .find_configs(config_template)?
                .next()
                .ok_or(anyhow::anyhow!("No valid display configs found"))?
        };

        let context = unsafe {
            let context_attribs = glutin::context::ContextAttributesBuilder::new()
                .with_context_api(glutin::context::ContextApi::OpenGl(Some(
                    glutin::context::Version::new(4, 5),
                )))
                .with_debug(true)
                .with_profile(glutin::context::GlProfile::Core)
                .build(Some(window.window_handle()?.as_raw()));

            display.create_context(&display_config, &context_attribs)?
        };

        let frame_buffer = unsafe {
            let surface_attribs =
                glutin::surface::SurfaceAttributesBuilder::<glutin::surface::WindowSurface>::new()
                    .with_srgb(Some(true))
                    .with_single_buffer(false)
                    .build(
                        window.window_handle()?.as_raw(),
                        NonZeroU32::new(WINDOW_SIZE).unwrap_unchecked(),
                        NonZeroU32::new(WINDOW_SIZE).unwrap_unchecked(),
                    );

            display.create_window_surface(&display_config, &surface_attribs)?
        };

        let context = context.make_current(&frame_buffer)?;
        gl::load_with(|s| {
            let c_s = ::std::ffi::CString::new(s).unwrap();
            context.display().get_proc_address(&c_s)
        });
        frame_buffer.set_swap_interval(&context, glutin::surface::SwapInterval::DontWait)?;

        // let gl_extensions = unsafe {
        //     let mut num_extensions = 0;
        //     gl::GetIntegerv(gl::NUM_EXTENSIONS, &mut num_extensions);
        //
        //     let mut extensions = Vec::with_capacity(num_extensions as usize);
        //     for i in (0..num_extensions as u32) {
        //         let ext_name = gl::GetStringi(gl::EXTENSIONS, i);
        //         extensions.push(CStr::from_ptr(ext_name as *const _).to_str()?.to_owned());
        //     }
        //     extensions
        // };
        //
        // println!("OpenGL extensions: {:#?}", gl_extensions);

        // Enable alpha blending and clear the framebuffer.
        unsafe {
            gl::Enable(gl::BLEND);
            gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

            gl::Viewport(0, 0, WINDOW_SIZE as i32, WINDOW_SIZE as i32);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);

            frame_buffer.swap_buffers(&context)?;
        }

        let scene = DemoScene::new(Path::new(BC3_DDS_PATH))?;

        // Set all initialized values in `self`
        self.window = Some(window);
        self.display = Some(display);
        self.frame_buffer = Some(frame_buffer);
        self.gl_ctx = Some(context);
        self.scene = Some(scene);
        self.did_initialize = true;

        Ok(())
    }
}

impl winit::application::ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if !self.did_initialize {
            self.initialize(event_loop).expect("Failed to initialize");
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if event == WindowEvent::CloseRequested {
            event_loop.exit();
        }

        unsafe {
            gl::Clear(gl::COLOR_BUFFER_BIT);

            if let Some(scene) = self.scene.as_mut() {
                scene.draw();
            }

            let context = self.gl_ctx.as_ref().expect("GL Context not initialized");

            if context.is_current() {
                self.frame_buffer
                    .as_mut()
                    .expect("Frame buffer not initialized")
                    .swap_buffers(context)
                    .expect("Failed to swap buffers");
            } else {
                eprintln!("OpenGL context is not currently open");
            }
        }
    }
}

pub fn main() -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::wait_duration(Duration::from_millis(1000 / 60)));

    event_loop.run_app(&mut App::default())?;
    Ok(())
}
