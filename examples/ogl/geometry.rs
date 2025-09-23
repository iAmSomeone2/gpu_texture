use glium::glutin::surface::WindowSurface;
use glium::index::PrimitiveType;
use glium::{Display, implement_vertex};

#[repr(C, align(32))]
#[derive(Debug, Default, Copy, Clone)]
pub struct Vertex {
    position: [f32; 3],
    tex_coords: [f32; 2],
}
implement_vertex!(Vertex, position location(0), tex_coords location(1));

pub struct Geometry {
    pub vertex_buffer: glium::VertexBuffer<Vertex>,
    pub index_buffer: glium::IndexBuffer<u16>,
}

impl Geometry {
    pub fn new_plane(display: &Display<WindowSurface>) -> anyhow::Result<Self> {
        let vertices = [
            Vertex {
                position: [-1.0, 1.0, 0.0],
                tex_coords: [0.0, 1.0],
            },
            Vertex {
                position: [1.0, 1.0, 0.0],
                tex_coords: [1.0, 1.0],
            },
            Vertex {
                position: [1.0, -1.0, 0.0],
                tex_coords: [1.0, 0.0],
            },
            Vertex {
                position: [-1.0, -1.0, 0.0],
                tex_coords: [0.0, 0.0],
            },
        ];

        let indices = [0, 1, 2, 0, 2, 3];

        let vertex_buffer = glium::VertexBuffer::immutable(display, &vertices)?;
        let index_buffer =
            glium::IndexBuffer::immutable(display, PrimitiveType::TrianglesList, &indices)?;

        Ok(Self {
            vertex_buffer,
            index_buffer,
        })
    }
}
