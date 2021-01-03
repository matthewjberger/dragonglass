mod gltf;
mod world;

use rapier3d::geometry::ColliderHandle;
use serde::{Deserialize, Serialize};

pub use self::{gltf::*, world::*};

#[derive(Serialize, Deserialize)]
pub struct BoxCollider {
    pub handle: ColliderHandle,
    pub visible: bool,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Visibility(pub bool);

impl Visibility {
    pub fn is_visible(&self) -> bool {
        self.0
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Selection(pub bool);

impl Selection {
    pub fn is_selected(&self) -> bool {
        self.0
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct Name(pub String);
