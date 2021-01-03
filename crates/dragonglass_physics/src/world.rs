use crate::Handle;
use rapier3d::{
    dynamics::{IntegrationParameters, JointSet, RigidBodySet},
    geometry::{BroadPhase, ColliderSet, NarrowPhase},
    na::Vector3,
    pipeline::{PhysicsPipeline, QueryPipeline},
};
use serde::{ser::SerializeStruct, Deserialize, Serialize};

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

impl Serialize for PhysicsWorld {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        // The pipeline itself does not need to be serialized
        let mut state = serializer.serialize_struct("PhysicsWorld", 8)?;
        state.serialize_field("gravity", &self.gravity)?;
        state.serialize_field("integration_parameters", &self.integration_parameters)?;
        state.serialize_field("broad_phase", &self.broad_phase)?;
        state.serialize_field("narrow_phase", &self.narrow_phase)?;
        state.serialize_field("bodies", &self.bodies)?;
        state.serialize_field("colliders", &self.colliders)?;
        state.serialize_field("joints", &self.joints)?;
        state.serialize_field("query", &self.query)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for PhysicsWorld {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        todo!()
    }
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
