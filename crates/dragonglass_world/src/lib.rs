mod gltf;
mod world;

use ncollide3d::pipeline::CollisionObjectSlabHandle;

pub use self::{gltf::*, world::*};

// TODO: Move collision code to separate module and remove ncollide3d from world module
pub struct Collider {
    pub handle: CollisionObjectSlabHandle,
}

pub struct Selected;
pub struct Hidden;
pub struct ColliderVisible;
pub struct Name(pub String);
