mod gltf;
mod physics;
mod world;

pub use self::{gltf::*, physics::*, world::*};

#[derive(Serialize, Deserialize)]
#[serde(crate = "dragonglass_deps::serde")]
pub struct Hidden;

#[derive(Serialize, Deserialize)]
#[serde(crate = "dragonglass_deps::serde")]
pub struct Name(pub String);
