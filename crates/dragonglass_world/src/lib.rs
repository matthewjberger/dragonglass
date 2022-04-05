mod animation;
mod camera;
mod gltf;
mod physics;
mod registry;
mod scenegraph;
mod texture;
mod transform;
mod world;

pub use self::{
    animation::*,
    camera::*,
    gltf::*,
    legion::{EntityStore, IntoQuery},
    physics::*,
    registry::*,
    scenegraph::*,
    texture::*,
    transform::*,
    world::*,
};
pub use legion;
pub use petgraph;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Hidden;

#[derive(Serialize, Deserialize)]
pub struct Name(pub String);
