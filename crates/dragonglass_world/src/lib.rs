mod gltf;
mod physics;
mod registry;
mod scenegraph;
mod texture;
mod world;

pub use self::{
    gltf::*,
    legion::{EntityStore, IntoQuery},
    physics::*,
    registry::*,
    scenegraph::*,
    texture::*,
    world::*,
};
pub use legion;
pub use petgraph;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Hidden;

#[derive(Serialize, Deserialize)]
pub struct Name(pub String);
