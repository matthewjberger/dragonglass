mod gltf;
mod world;

pub use self::{gltf::*, world::*};

pub struct Hidden;
pub struct Name(pub String);
