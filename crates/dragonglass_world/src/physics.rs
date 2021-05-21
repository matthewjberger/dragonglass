pub use rapier3d;

use rapier3d::{
    dynamics::{CCDSolver, IntegrationParameters, JointSet, RigidBodySet},
    geometry::{BroadPhase, ColliderSet, NarrowPhase},
    na::Vector3,
    pipeline::{PhysicsPipeline, QueryPipeline},
};
use serde::{Deserialize, Serialize};

pub type Handle = rapier3d::dynamics::RigidBodyHandle;
pub type ColliderHandle = rapier3d::geometry::ColliderHandle;

pub struct RigidBody {
    pub handle: Handle,
    pub colliders: Vec<ColliderHandle>,
}

impl RigidBody {
    pub fn new(handle: Handle) -> Self {
        Self {
            handle,
            colliders: Vec::new(),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct WorldPhysics {
    pub gravity: Vector3<f32>,
    pub integration_parameters: IntegrationParameters,
    pub broad_phase: BroadPhase,
    pub narrow_phase: NarrowPhase,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub joints: JointSet,
    pub query_pipeline: QueryPipeline,
    pub ccd_solver: CCDSolver,
    #[serde(skip)]
    pub pipeline: PhysicsPipeline,
}

impl Default for WorldPhysics {
    fn default() -> Self {
        Self::new()
    }
}

impl WorldPhysics {
    pub fn new() -> Self {
        Self {
            gravity: Vector3::y() * -9.81,
            integration_parameters: IntegrationParameters::default(),
            broad_phase: BroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            joints: JointSet::new(),
            query_pipeline: QueryPipeline::default(),
            ccd_solver: CCDSolver::new(),
            pipeline: PhysicsPipeline::new(),
        }
    }

    pub fn update(&mut self, delta_time: f32) {
        self.integration_parameters.dt = delta_time;

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
            &mut self.ccd_solver,
            &(),
            &event_handler,
        );

        self.query_pipeline.update(&self.bodies, &self.colliders);
    }
}
