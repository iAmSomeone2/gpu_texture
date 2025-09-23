use crate::geometry::Geometry;
use crate::material::Material;
use glium::Surface;
use std::rc::Rc;
use std::sync::RwLock;

pub struct Mesh {
    geometry: Geometry,
    material: Rc<RwLock<dyn Material>>,
}

impl Mesh {
    pub fn new(geometry: Geometry, material: Rc<RwLock<dyn Material>>) -> Self {
        Self { geometry, material }
    }

    pub fn draw(&self, transform: &glam::Mat4, surface: &mut impl Surface) {}
}
