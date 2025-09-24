use anyhow::anyhow;
use gl::types::{GLint, GLuint};
use glutin::display::GetGlDisplay;
use glutin::prelude::*;
use glutin::surface::Surface;
use gpu_texture::{GPUTexture, TextureData};
use raw_window_handle::HasWindowHandle;
use std::ffi::CStr;
use std::marker::PhantomData;
use std::mem::offset_of;
use std::num::NonZeroU32;
use std::path::Path;
use std::ptr;
use std::time::Instant;
use winit::{
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow},
    raw_window_handle::HasDisplayHandle,
    window::{Window, WindowId},
};

const VERTEX_SHADER: &str = include_str!("shaders/demo.vert.glsl");
const FRAGMENT_SHADER: &str = include_str!("shaders/demo.frag.glsl");

const BC3_DDS_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/awesomeface.dds");

const WINDOW_SIZE: u32 = 1280;

/// OpenGL ID
#[derive(Debug, Copy, Clone)]
struct GlId(u32);

impl From<GlId> for u32 {
    fn from(id: GlId) -> Self {
        id.0
    }
}

impl From<u32> for GlId {
    fn from(value: u32) -> Self {
        GlId(value)
    }
}

fn check_gl_error(msg: &str) -> Option<String> {
    match unsafe { gl::GetError() } {
        gl::NO_ERROR => None,
        err => {
            let err_str = match err {
                gl::INVALID_ENUM => "INVALID_ENUM",
                gl::INVALID_VALUE => "INVALID_VALUE",
                gl::INVALID_OPERATION => "INVALID_OPERATION",
                _ => "Unknown error",
            };
            Some(format!("{msg}: {err_str}"))
        }
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
    /// PhantomData marker for type safety
    _type: PhantomData<T>,
    /// OpenGL buffer ID
    id: GlId,
    /// OpenGL buffer type enum
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
    fn create(gl_buf_type: gl::types::GLenum, size: usize) -> Self {
        let mut id = 0u32;
        unsafe {
            gl::CreateBuffers(1, &mut id);
        }
        Self {
            _type: PhantomData,
            id: id.into(),
            buf_type: gl_buf_type,
            count: size / size_of::<T>(),
        }
    }

    /// Creates a new immutable shader storage buffer initialized with the contents of `data`
    pub fn new_immutable_storage(data: T, gl_buf_type: gl::types::GLenum) -> Self {
        let data_size = size_of_val(&data);
        let buffer = Buffer::<T>::create(gl_buf_type, data_size);
        unsafe {
            gl::NamedBufferStorage(
                buffer.id.into(),
                data_size as gl::types::GLsizeiptr,
                &raw const data as *const _,
                0,
            );
        }

        buffer
    }

    pub fn new_uniform_buffer(data: T) -> Self {
        let data_size = size_of_val(&data);
        let buffer = Buffer::<T>::create(gl::UNIFORM_BUFFER, data_size);
        unsafe {
            gl::NamedBufferStorage(
                buffer.id.into(),
                data_size as gl::types::GLsizeiptr,
                &raw const data as *const _,
                gl::DYNAMIC_STORAGE_BIT,
            );
        }
        buffer
    }

    /// Creates a new immutable vertex storage buffer initialized with the contents of `vertices`
    pub fn new_vertex_buffer(vertices: &[T]) -> Self {
        let data_size = size_of_val(vertices);
        let buffer = Buffer::<T>::create(gl::ARRAY_BUFFER, data_size);
        unsafe {
            gl::NamedBufferStorage(
                buffer.id.into(),
                data_size as gl::types::GLsizeiptr,
                vertices.as_ptr() as *const _,
                0,
            );
        }
        buffer
    }

    pub fn write_data(&self, data: &impl Sized, offset: Option<i32>) -> Result<(), String> {
        let offest = offset.unwrap_or(0);
        unsafe {
            gl::NamedBufferSubData(
                self.id.into(),
                offest as gl::types::GLintptr,
                size_of_val(data) as gl::types::GLsizeiptr,
                &raw const data as *const _,
            );
        }
        if let Some(err) = check_gl_error("Buffer::write_data") {
            Err(err)
        } else {
            Ok(())
        }
    }
}

impl Buffer<u16> {
    /// Creates a new immutable index storage buffer initialized with the contents of `indices`
    pub fn new_index_buffer(indices: &[u16]) -> Self {
        let data_size = size_of_val(indices);
        let buffer = Buffer::<u16>::create(gl::ELEMENT_ARRAY_BUFFER, data_size);
        unsafe {
            gl::NamedBufferStorage(
                buffer.id.into(),
                data_size as gl::types::GLsizeiptr,
                indices.as_ptr() as *const _,
                0,
            );
        }

        buffer
    }
}

struct VertexAttributeObject(GlId);

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

struct Texture {
    id: GlId,
    gl_type: gl::types::GLenum,
}

impl Drop for Texture {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteTextures(1, &(self.id.into()));
        }
    }
}

impl Texture {
    #[inline(always)]
    fn create(gl_texture_type: gl::types::GLenum) -> anyhow::Result<Self> {
        let mut id = [0u32];
        unsafe {
            gl::CreateTextures(gl_texture_type, 1, id.as_mut_ptr());
        }
        if let Some(err) = check_gl_error("gl::CreateTextures") {
            return Err(anyhow!(err));
        }
        Ok(Self {
            id: id[0].into(),
            gl_type: gl_texture_type,
        })
    }

    pub fn create_1d_from_f32_data(data: &[f32], repeat: bool) -> anyhow::Result<Self> {
        let texture = Texture::create(gl::TEXTURE_1D)?;

        let clamp_mode = if repeat {
            gl::REPEAT
        } else {
            gl::CLAMP_TO_EDGE
        } as i32;

        unsafe {
            gl::TextureStorage1D(
                texture.id.into(),
                1,
                gl::R32F,
                data.len() as gl::types::GLsizei,
            );
            if let Some(err) = check_gl_error("gl::TextureStorage1D") {
                return Err(anyhow!(err));
            }

            gl::TextureParameteri(texture.id.into(), gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
            gl::TextureParameteri(texture.id.into(), gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TextureParameteri(texture.id.into(), gl::TEXTURE_WRAP_S, clamp_mode);
            gl::TextureSubImage1D(
                texture.id.into(),
                0,
                0,
                data.len() as gl::types::GLsizei,
                gl::RED,
                gl::FLOAT,
                data.as_ptr() as *const _,
            );
            if let Some(err) = check_gl_error("gl::TextureSubImage1D") {
                return Err(anyhow!(err));
            }
        }

        Ok(texture)
    }

    /// Creates a new 2D texture on the GPU from a GPUTexture object using the DSA paradigm from OpenGL 4.5+
    pub fn create_2d_from_gpu_texture(mut input_texture: GPUTexture) -> anyhow::Result<Self> {
        let texture = Texture::create(gl::TEXTURE_2D)?;

        let format = input_texture.format.try_into_ogl_enum()?;
        let filter_type = if input_texture.mip_maps.is_some() {
            gl::LINEAR_MIPMAP_LINEAR
        } else {
            gl::LINEAR
        } as i32;

        let is_compressed = matches!(
            input_texture.format,
            gpu_texture::TextureFormat::Compressed(..)
        );

        let upload_mipmap = |mipmap: TextureData, level: usize| unsafe {
            if is_compressed {
                gl::CompressedTextureSubImage2D(
                    texture.id.into(),
                    level as i32,
                    0,
                    0,
                    mipmap.width() as i32,
                    mipmap.height() as i32,
                    format,
                    mipmap.img_buffer.len() as i32,
                    mipmap.img_buffer.as_ptr() as *const _,
                );
                if let Some(err) = check_gl_error("gl::CompressedTextureSubImage2D") {
                    panic!("{err}")
                }
            } else {
                gl::TextureSubImage2D(
                    texture.id.into(),
                    level as i32,
                    0,
                    0,
                    mipmap.width() as i32,
                    mipmap.height() as i32,
                    gl::RGBA,
                    gl::UNSIGNED_BYTE,
                    mipmap.img_buffer.as_ptr() as *const _,
                );
                if let Some(err) = check_gl_error("gl::TextureSubImage2D") {
                    panic!("{err}")
                }
            };
        };

        unsafe {
            gl::TextureStorage2D(
                texture.id.into(),
                input_texture.texture_count() as i32,
                format,
                input_texture.main_image.width() as i32,
                input_texture.main_image.height() as i32,
            );
            gl::TextureParameteri(texture.id.into(), gl::TEXTURE_MIN_FILTER, filter_type);
            gl::TextureParameteri(texture.id.into(), gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
            gl::TextureParameteri(texture.id.into(), gl::TEXTURE_WRAP_S, gl::REPEAT as i32);
            gl::TextureParameteri(texture.id.into(), gl::TEXTURE_WRAP_T, gl::REPEAT as i32);
        }

        // Upload top-level image
        upload_mipmap(input_texture.main_image, 0);
        // Upload mipmaps
        if let Some(mut mip_maps) = input_texture.mip_maps.take() {
            mip_maps.drain(..).enumerate().for_each(|(i, mip_map)| {
                upload_mipmap(mip_map, i + 1);
            });
        }

        Ok(texture)
    }

    /// Binds the texture to the given [GlProgram] using DSA
    pub fn dsa_bind(&self, gl_program: &GlProgram, tex_unit: u32, bind_loc: u32) {
        unsafe {
            gl::BindTextureUnit(tex_unit, self.id.into());
            gl::ProgramUniform1i(gl_program.0.into(), bind_loc as i32, tex_unit as i32);
        }
    }
}

struct GlProgram(GlId);

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
                return Err(anyhow::Error::msg(info_log));
            }

            Ok(Self(program_id.into()))
        }
    }
}

#[repr(C, align(4))]
#[derive(Clone, Copy, Debug)]
struct Vertex {
    /// Padding to align with OpenGL's 4-byte boundary
    _pad: [u8; 4],
    /// 3D position
    position: glam::Vec3,
    /// 2D texture coordinates
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
            Vertex::new([-1.0, -1.0, 0.0], [0.0, 1.0]),
            // Front face (lower-right)
            Vertex::new([1.0, -1.0, 0.0], [1.0, 1.0]),
            // Front face (upper-left)
            Vertex::new([-1.0, 1.0, 0.0], [0.0, 0.0]),
            // Front face (upper-right)
            Vertex::new([1.0, 1.0, 0.0], [1.0, 0.0]),
        ];

        const INDICES: [u16; 6] = [0, 1, 2, 2, 1, 3];

        let vertices = Buffer::new_vertex_buffer(&VERTICES);
        let indices = Some(Buffer::new_index_buffer(&INDICES));

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
    _texture: Texture,
}

impl Material {
    fn new(color_texture: GPUTexture) -> anyhow::Result<Self> {
        let program = GlProgram::new(VERTEX_SHADER, FRAGMENT_SHADER)?;
        let texture = Texture::create_2d_from_gpu_texture(color_texture)?;
        texture.dsa_bind(&program, 0, 4);

        Ok(Self {
            program,
            _texture: texture,
        })
    }

    fn apply(&self) {
        unsafe {
            gl::UseProgram(self.program.0.into());
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

    fn draw(&self) {
        self.material.apply();
        self.geometry.draw();
    }
}

#[repr(C)]
struct AnimationLUT<const N: usize> {
    count: u32,
    lut: [f32; N],
}

impl<const N: usize> AnimationLUT<N> {
    fn generate(min: f32, max: f32) -> Self {
        let step_size = (std::f32::consts::PI * 2.0) / N as f32;

        let mut lut = [0.0; N];
        let range = max - min;
        let midpoint = (max + min) * 0.5;
        let scale = range * 0.5;

        for (i, item) in lut.iter_mut().enumerate() {
            let t = i as f32 * step_size;
            *item = (-f32::cos(t) * scale) + midpoint;
        }

        Self {
            count: N as u32,
            lut,
        }
    }
}

struct DemoScene<const N: usize> {
    mesh: Mesh,
    /// Animation playback rate in frames per second
    anim_rate: f32,
    _anim_lut_tex: Texture,
    uniform_buf: Buffer<glam::Mat4>,
    start_instant: Option<Instant>,
}

impl<const N: usize> DemoScene<N> {
    fn new(texture_path: &Path) -> anyhow::Result<Self> {
        let near_plane = 0.01;
        let far_plane = 100.0;

        // Load texture file from the file system
        let gpu_texture = GPUTexture::load_from_file(texture_path)?;
        let projection_mat = glam::Mat4::perspective_rh_gl(70.0, 1.0, near_plane, far_plane);
        let uniform_buf = Buffer::new_uniform_buffer(projection_mat);

        let mesh = Mesh::new_textured_plane(gpu_texture)?;

        let anim_lut = AnimationLUT::<N>::generate(near_plane + 2.0, far_plane - 1.0);
        let anim_lut_tex = Texture::create_1d_from_f32_data(&anim_lut.lut, true)?;
        anim_lut_tex.dsa_bind(&mesh.material.program, 1, 3);

        let anim_rate = N as f32 / 5.0; // Frames / second

        Ok(Self {
            uniform_buf,
            mesh,
            start_instant: None,
            _anim_lut_tex: anim_lut_tex,
            anim_rate,
        })
    }

    fn calc_anim_sample_point(&mut self) -> f32 {
        // Total number of seconds elapsed since animation started
        let elapsed = if let Some(instant) = self.start_instant {
            instant.elapsed().as_secs_f32()
        } else {
            self.start_instant = Some(Instant::now());
            0.0
        };

        let frame_count = (elapsed * self.anim_rate).round() as usize;

        let anim_sample_point: f32 = (frame_count % N) as f32 / N as f32;

        debug_assert!(
            (0.0..=1.0).contains(&anim_sample_point),
            "Animation sample point out of range: {}",
            anim_sample_point
        );
        anim_sample_point
    }

    fn draw(&mut self) {
        let anim_sample_point = self.calc_anim_sample_point();

        unsafe {
            gl::ProgramUniform1f(self.mesh.material.program.0.into(), 2, anim_sample_point);
            gl::BindBufferBase(gl::UNIFORM_BUFFER, 0, self.uniform_buf.id.into());
            // gl::BindBufferBase(gl::SHADER_STORAGE_BUFFER, 1, self.anim_lut_buf.id.into());
        }

        self.mesh.draw();
    }
}

struct Renderer {
    frame_buffer: Surface<glutin::surface::WindowSurface>,
    gl_ctx: glutin::context::NotCurrentContext,
}

impl Renderer {
    fn new(
        gl_ctx: glutin::context::NotCurrentContext,
        frame_buffer: Surface<glutin::surface::WindowSurface>,
    ) -> Self {
        Self {
            frame_buffer,
            gl_ctx,
        }
    }

    fn run(
        self,
        shutdown_receiver: std::sync::mpsc::Receiver<()>,
    ) -> (
        std::thread::JoinHandle<()>,
        std::sync::mpsc::Receiver<anyhow::Error>,
    ) {
        let (error_sender, error_receiver) = std::sync::mpsc::channel();
        let thread_handle = std::thread::spawn(move || {
            let err_handler = |err: anyhow::Error| match error_sender.send(err) {
                Ok(_) => (),
                Err(_) => std::process::exit(1),
            };

            let gl_ctx = match self.gl_ctx.make_current(&self.frame_buffer) {
                Ok(gl_ctx) => gl_ctx,
                Err(err) => return err_handler(anyhow::Error::new(err)),
            };

            if !gl_ctx.is_current() {
                err_handler(anyhow!("GL context is not current!"))
            }

            gl::load_with(|s| {
                let c_s = ::std::ffi::CString::new(s).unwrap();
                gl_ctx.display().get_proc_address(&c_s)
            });

            if let Err(err) = self.frame_buffer.set_swap_interval(
                &gl_ctx,
                glutin::surface::SwapInterval::Wait(NonZeroU32::new(1).unwrap()),
            ) {
                return err_handler(anyhow::Error::new(err));
            }

            let mut scene = match DemoScene::<500>::new(Path::new(BC3_DDS_PATH)) {
                Ok(scene) => scene,
                Err(err) => return err_handler(err),
            };

            // Enable alpha blending and clear the framebuffer.
            unsafe {
                gl::Enable(gl::BLEND);
                gl::BlendFunc(gl::SRC_ALPHA, gl::ONE_MINUS_SRC_ALPHA);

                gl::Viewport(0, 0, WINDOW_SIZE as i32, WINDOW_SIZE as i32);
                gl::ClearColor(0.0, 0.0, 0.0, 1.0);
                gl::Clear(gl::COLOR_BUFFER_BIT);

                if let Err(err) = self.frame_buffer.swap_buffers(&gl_ctx) {
                    return err_handler(anyhow::Error::new(err));
                }
            }

            'render: loop {
                if shutdown_receiver.try_recv().is_ok() {
                    println!("Shutting down render thread...");
                    break 'render;
                }

                unsafe {
                    if !gl_ctx.is_current() {
                        eprintln!("OpenGL context is not current! Skipping frame...");
                        continue 'render;
                    }
                    gl::Clear(gl::COLOR_BUFFER_BIT);

                    scene.draw();

                    self.frame_buffer
                        .swap_buffers(&gl_ctx)
                        .expect("Failed to swap buffers");
                }
            }
        });

        (thread_handle, error_receiver)
    }
}

#[derive(Default)]
struct App {
    did_initialize: bool,
    window: Option<Window>,
    display: Option<glutin::display::Display>,
    shutdown_sender: Option<std::sync::mpsc::Sender<()>>,
    renderer_handle: Option<(
        std::thread::JoinHandle<()>,
        std::sync::mpsc::Receiver<anyhow::Error>,
    )>,
}

impl App {
    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> anyhow::Result<()> {
        let window = {
            let window_attribs = winit::window::WindowAttributes::default()
                .with_title("Example: OpenGL")
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

        let renderer = Renderer::new(context, frame_buffer);
        let (shutdown_sender, shutdown_receiver) = std::sync::mpsc::channel();

        // Set all initialized values in `self`
        self.window = Some(window);
        self.display = Some(display);
        self.shutdown_sender = Some(shutdown_sender);
        self.renderer_handle = Some(renderer.run(shutdown_receiver));
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
        let shutdown = || {
            println!("Shutdown event received...");
            let _ = self.shutdown_sender.as_ref().unwrap().send(());
            event_loop.exit();
        };

        if let Some((_, render_err_recv)) = &self.renderer_handle {
            if let Ok(render_error) = render_err_recv.try_recv() {
                eprintln!("Render error: {render_error}");
                shutdown();
            }
        };

        match event {
            WindowEvent::CloseRequested | WindowEvent::Destroyed => {
                shutdown();
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.physical_key
                    == winit::keyboard::PhysicalKey::Code(winit::keyboard::KeyCode::Escape)
                {
                    shutdown();
                }
            }
            _ => {}
        }
    }
}

pub fn main() -> anyhow::Result<()> {
    let event_loop = winit::event_loop::EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Wait);

    let mut app = App::default();

    event_loop.run_app(&mut app)?;
    app.renderer_handle.take().map(|handle| handle.0.join());
    Ok(())
}
