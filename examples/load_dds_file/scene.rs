use crate::mesh::Mesh;
use glam::Mat4;
use std::collections::HashSet;
use std::hash::Hash;

pub struct World {
    /// Number of units used to represent the bounds of each axis (X, Y, Z)
    size: f32,
}

impl World {
    pub const fn new(size: f32) -> Self {
        Self { size }
    }
}

pub struct Camera {
    projection_matrix: Mat4,
}

impl Camera {
    pub fn new_orthographic(width: f32, height: f32, z_near: f32, z_far: f32) -> Self {
        Self {
            projection_matrix: Mat4::orthographic_rh_gl(0.0, width, 0.0, height, z_near, z_far),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Rect2D {
    pub x_left: f32,
    pub x_right: f32,
    pub y_bottom: f32,
    pub y_top: f32,
}

pub trait Drawable {
    fn is_visible(&self, view_frustum: &Rect2D) -> bool;
}

pub struct Node3D {
    id: usize,
    parent_idx: Option<usize>,
    children: HashSet<usize>,
    transform: Mat4,
    pub drawable: Option<Box<dyn Drawable>>,
}

impl Hash for Node3D {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Node3D {
    fn new(id: usize, parent_idx: Option<usize>, drawable: Option<Box<dyn Drawable>>) -> Self {
        Self {
            id,
            parent_idx,
            children: HashSet::new(),
            transform: Mat4::IDENTITY,
            drawable,
        }
    }

    fn add_child(&mut self, child_idx: usize) {
        self.children.insert(child_idx);
    }

    fn remove_child(&mut self, child_idx: usize) {
        self.children.remove(&child_idx);
    }
}

pub struct Scene {
    world: World,
    camera: Camera,
    nodes: HashSet<Node3D>,
    root_node: Node3D,
}

impl Scene {
    pub fn new(width: f32, height: f32) -> Self {
        let root_node = Node3D::new(0, None, None);
    }
}
