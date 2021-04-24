mod gltf;
mod world;

pub use self::{
    gltf::*,
    legion::{EntityStore, IntoQuery},
    world::*,
};
pub use legion;

pub struct Hidden;
pub struct Name(pub String);
