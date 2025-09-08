use crate::geometry::Geometry;
use crate::material::Material;

pub struct Mesh<'m> {
    geometry: Geometry,
    material: &'m dyn Material,
}

impl<'m> Mesh<'m> {
    pub fn new(geometry: Geometry, material: &'m impl Material) -> Self {
        Self { geometry, material }
    }
}
