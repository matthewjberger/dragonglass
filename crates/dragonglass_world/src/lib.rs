mod gltf;
mod physics;
mod world;

pub use self::{
    gltf::*,
    legion::{EntityStore, IntoQuery},
    physics::*,
    world::*,
};
pub use legion;

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Hidden;

#[derive(Serialize, Deserialize)]
pub struct Name(pub String);
