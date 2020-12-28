use crate::Handle;
use rapier3d::{
    dynamics::{IntegrationParameters, JointSet, RigidBodySet},
    geometry::{BroadPhase, ColliderSet, NarrowPhase},
    na::Vector3,
    pipeline::{PhysicsPipeline, QueryPipeline},
};

pub struct RigidBody {
    pub handle: Handle,
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
    pub query: QueryPipeline,
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
            query: QueryPipeline::default(),
        }
    }

    pub fn update(&mut self) {
        // We ignore contact events for now.
        let event_handler = ();

        self.query.update(&self.bodies, &self.colliders);

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
