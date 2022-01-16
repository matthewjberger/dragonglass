pub use rapier3d;

use rapier3d::{
    dynamics::{CCDSolver, IntegrationParameters, RigidBodySet},
    geometry::{BroadPhase, ColliderSet, NarrowPhase},
    na::Vector3,
    pipeline::{PhysicsPipeline, QueryPipeline},
    prelude::{ImpulseJointSet, IslandManager, MultibodyJointSet, RigidBodyHandle},
};
use serde::{Deserialize, Serialize};
pub type Handle = rapier3d::dynamics::RigidBodyHandle;
pub type ColliderHandle = rapier3d::geometry::ColliderHandle;

#[derive(Debug, Serialize, Deserialize)]
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
    pub islands: IslandManager,
    pub bodies: RigidBodySet,
    pub colliders: ColliderSet,
    pub impulse_joints: ImpulseJointSet,
    #[serde(skip, default = "MultibodyJointSet::new")]
    pub multibody_joints: MultibodyJointSet,
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
            islands: IslandManager::new(),
            bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            query_pipeline: QueryPipeline::default(),
            ccd_solver: CCDSolver::new(),
            pipeline: PhysicsPipeline::new(),
        }
    }

    pub fn remove_rigid_body(&mut self, handle: RigidBodyHandle) {
        self.bodies.remove(
            handle,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
        );
    }

    pub fn set_gravity(&mut self, gravity: Vector3<f32>) {
        self.gravity = gravity;
    }

    pub fn update(&mut self, delta_time: f32) {
        self.integration_parameters.dt = delta_time;

        // We ignore contact events for now.
        let event_handler = ();

        self.pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.islands,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            &(),
            &event_handler,
        );

        self.query_pipeline
            .update(&self.islands, &self.bodies, &self.colliders);
    }
}
