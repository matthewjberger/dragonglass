mod gltf;
mod world;

use ncollide3d::pipeline::CollisionObjectSlabHandle;
use serde::{Deserialize, Serialize};

pub use self::{gltf::*, world::*};

#[derive(Serialize, Deserialize)]
// TODO: Move collision code to separate module and remove ncollide3d from world module
pub struct BoxCollider {
    pub handle: CollisionObjectSlabHandle,
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
