use crate::Handle;
use nalgebra_glm as glm;
use rapier3d::{
    dynamics::{IntegrationParameters, JointSet, RigidBodySet},
    geometry::{BroadPhase, ColliderSet, NarrowPhase},
    math::Isometry,
    na::{Translation3, UnitQuaternion, Vector3},
    pipeline::PhysicsPipeline,
};

pub struct RigidBody {
    pub handle: Handle,
    pub translation: glm::Vec3,
    pub rotation: glm::Quat,
}

impl RigidBody {
    pub fn new(handle: Handle) -> Self {
        Self {
            handle,
            translation: glm::Vec3::identity(),
            rotation: glm::Quat::identity(),
        }
    }

    pub fn as_isometry(&self) -> Isometry<f32> {
        Isometry::from_parts(
            Translation3::from(self.translation),
            UnitQuaternion::from_quaternion(self.rotation),
        )
    }
}

pub struct PhysicsWorld {
    pub gravity: Vector3<f32>,
    pub integration_parameters: IntegrationParameters,
    pub pipeline: PhysicsPipeline,
    pub broad_phase: BroadPhase,
    pub narrow_phase: NarrowPhase,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub joints: JointSet,
}

impl Default for PhysicsWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl PhysicsWorld {
    pub fn new() -> Self {
        Self {
            gravity: Vector3::y() * -9.81,
            integration_parameters: IntegrationParameters::default(),
            pipeline: PhysicsPipeline::new(),
            broad_phase: BroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            joints: JointSet::new(),
        }
    }

    pub fn step(&mut self) {
        // We ignore contact events for now.
        let event_handler = ();

        self.pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.joints,
            None,
            None,
            &event_handler,
        );
    }
}
