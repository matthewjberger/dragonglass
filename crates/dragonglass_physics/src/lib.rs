mod world;

pub use self::world::*;
pub use rapier3d;
pub type Handle = rapier3d::dynamics::RigidBodyHandle;

pub struct RigidBody {
    pub handle: Handle,
}

impl RigidBody {
    pub fn new(handle: Handle) -> Self {
        Self { handle }
    }
}
