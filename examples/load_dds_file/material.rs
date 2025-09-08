use glium::{Texture2d, texture};

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
}

pub trait Material {}

#[derive(Default)]
pub struct BasicMaterial {
    color: Option<Color>,
    albedo_texture: Option<texture::compressed_texture2d::CompressedTexture2d>,
}

impl Material for BasicMaterial {}

impl BasicMaterial {
    pub fn new() -> Self {
        BasicMaterial {
            color: Some(Color::WHITE),
            ..Default::default()
        }
    }

    pub fn with_albedo_texture(
        mut self,
        texture: texture::compressed_texture2d::CompressedTexture2d,
    ) -> Self {
        self.albedo_texture = Some(texture);
        self.color = None;

        self
    }
}
