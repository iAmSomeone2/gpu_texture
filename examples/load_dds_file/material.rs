use crate::geometry::Geometry;
use glium::backend::Facade;
use glium::framebuffer::{ColorAttachment, ToColorAttachment};
use glium::texture::CompressedTexture2d;
use glium::uniforms::{
    MagnifySamplerFilter, MinifySamplerFilter, SamplerBehavior, SamplerWrapFunction, UniformValue,
    Uniforms,
};
use glium::{Display, Program, ProgramCreationError, Surface, Texture2d, texture};
use std::rc::Rc;
use std::sync::RwLock;

#[derive(Debug, Copy, Clone)]
/// 8 bits/cc ARGB color
pub struct Color {
    pub alpha: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Default for Color {
    fn default() -> Self {
        Color::BLACK
    }
}

impl Color {
    pub const BLACK: Color = Color {
        alpha: 0xff,
        red: 0,
        green: 0,
        blue: 0,
    };

    pub const WHITE: Color = Color {
        alpha: 0xff,
        red: 0xff,
        green: 0xff,
        blue: 0xff,
    };

    pub const RED: Color = Color {
        alpha: 0xff,
        red: 0xff,
        green: 0,
        blue: 0,
    };
    pub const GREEN: Color = Color {
        alpha: 0xff,
        red: 0,
        green: 0xff,
        blue: 0,
    };
    pub const BLUE: Color = Color {
        alpha: 0xff,
        red: 0,
        green: 0,
        blue: 0xff,
    };

    pub const MAGENTA: Color = Color {
        alpha: 0xff,
        red: 0xff,
        green: 0,
        blue: 0xff,
    };
}

impl From<Color> for [f32; 4] {
    fn from(color: Color) -> [f32; 4] {
        let scale_component = |component: u8| component as f32 / (u8::MAX as f32);

        [
            scale_component(color.red),
            scale_component(color.green),
            scale_component(color.blue),
            scale_component(color.alpha),
        ]
    }
}

pub trait Material {
    fn apply(&self, geometry: &Geometry, transform: &glam::Mat4, surface: &mut impl Surface)
    where
        Self: Sized;
}

pub trait MaterialBuilder: Default {
    const FRAG_SHADER_SRC: &'static str;

    const VERT_SHADER_SRC: &'static str;

    fn build<F: ?Sized + Facade>(
        self,
        facade: &F,
    ) -> Result<Rc<RwLock<dyn Material>>, ProgramCreationError>;
}

#[derive(Default)]
pub struct BasicMaterialBuilder {
    color: Option<Color>,
    texture: Option<Texture2d>,
}

impl BasicMaterialBuilder {
    /// Sets the material color and ensures that the texture is unset
    pub fn with_color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self.texture = None;

        self
    }

    /// Sets the texture and ensures that the color is unset
    pub fn with_texture(mut self, texture: Texture2d) -> Self {
        self.texture = Some(texture);
        self.color = None;

        self
    }
}

impl MaterialBuilder for BasicMaterialBuilder {
    const FRAG_SHADER_SRC: &'static str = include_str!("shaders/basic_mat.frag.glsl");
    const VERT_SHADER_SRC: &'static str = include_str!("shaders/basic.vert.glsl");

    fn build<F: ?Sized + Facade>(
        self,
        facade: &F,
    ) -> Result<Rc<RwLock<dyn Material>>, ProgramCreationError> {
        let program =
            Program::from_source(facade, Self::VERT_SHADER_SRC, Self::FRAG_SHADER_SRC, None)?;

        let color = if let Some(color) = self.color {
            color
        } else {
            Color::MAGENTA
        };

        Ok(Rc::new(RwLock::new(BasicMaterial {
            program,
            color,
            texture: self.texture,
        })))
    }
}

pub struct BasicMaterial {
    program: Program,
    color: Color,
    texture: Option<Texture2d>,
}

pub struct BasicMaterialUniforms<'u> {
    color: &'u Color,
    texture: Option<&'u Texture2d>,
    transform: &'u glam::Mat4,
}

impl Uniforms for BasicMaterialUniforms<'_> {
    fn visit_values<'a, F: FnMut(&str, UniformValue<'a>)>(&'a self, mut visit_value: F) {
        const SAMPLER_BEHAVIOR: SamplerBehavior = SamplerBehavior {
            wrap_function: (
                SamplerWrapFunction::Clamp,
                SamplerWrapFunction::Clamp,
                SamplerWrapFunction::Clamp,
            ),
            minify_filter: MinifySamplerFilter::LinearMipmapLinear,
            magnify_filter: MagnifySamplerFilter::Linear,
            max_anisotropy: 16,
            depth_texture_comparison: None,
        };

        if let Some(texture) = self.texture {
            visit_value("u_useTexture", UniformValue::Bool(true));
            visit_value(
                "u_texture",
                UniformValue::Texture2d(texture, Some(SAMPLER_BEHAVIOR)),
            );
        } else {
            visit_value("u_useTexture", UniformValue::Bool(false));
        }

        visit_value("u_color", UniformValue::Vec4((*self.color).into()));
        visit_value(
            "u_transformMatrix",
            UniformValue::Mat4(self.transform.to_cols_array_2d()),
        );
    }
}

impl Material for BasicMaterial {
    fn apply(&self, geometry: &Geometry, transform: &glam::Mat4, surface: &mut impl Surface)
    where
        Self: Sized,
    {
        let uniforms = BasicMaterialUniforms {
            color: &self.color,
            texture: self.texture.as_ref(),
            transform,
        };

        surface
            .draw(
                &geometry.vertex_buffer,
                &geometry.index_buffer,
                &self.program,
                &uniforms,
                &Default::default(),
            )
            .unwrap();
    }
}
