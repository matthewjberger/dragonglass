use anyhow::Result;
use dragonglass_physics::PhysicsWorld;
use dragonglass_world::{
    Camera, Ecs, Light, Mesh, Name, Selection, Skin, Transform, Visibility, World,
};
use legion::{Entity, IntoQuery, Registry};
use serde::{
    de::{self, Deserializer, MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
    Deserialize, Serialize,
};
use std::{fmt, path::Path};

pub struct Universe {
    pub ecs: Ecs,
    pub world: World,
    pub physics_world: PhysicsWorld,
}

impl Serialize for Universe {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut state = serializer.serialize_struct("Universe", 2)?;

        let registry = generate_registry();
        let ecs = self
            .ecs
            .as_serializable(legion::component::<Transform>(), &registry);
        state.serialize_field("ecs", &ecs)?;
        state.serialize_field("physics_world", &self.physics_world)?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for Universe {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        enum Field {
            Ecs,
            World,
            PhysicsWorld,
        }

        impl<'de> Deserialize<'de> for Field {
            fn deserialize<D>(deserializer: D) -> Result<Field, D::Error>
            where
                D: Deserializer<'de>,
            {
                struct FieldVisitor;

                impl<'de> Visitor<'de> for FieldVisitor {
                    type Value = Field;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str("`ecs` or `world` or `physics_world`")
                    }

                    fn visit_str<E>(self, value: &str) -> Result<Field, E>
                    where
                        E: de::Error,
                    {
                        match value {
                            "ecs" => Ok(Field::Ecs),
                            "world" => Ok(Field::World),
                            "physics_world" => Ok(Field::PhysicsWorld),
                            _ => Err(de::Error::unknown_field(value, FIELDS)),
                        }
                    }
                }

                deserializer.deserialize_identifier(FieldVisitor)
            }
        }

        struct UniverseVisitor;

        impl<'de> Visitor<'de> for UniverseVisitor {
            type Value = Universe;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("struct Universe")
            }

            fn visit_seq<V>(self, mut seq: V) -> Result<Universe, V::Error>
            where
                V: SeqAccess<'de>,
            {
                let registry = generate_registry();
                let mut ecs = seq
                    .next_element_seed(registry.as_deserialize())?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let physics_world = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;

                // FIXME legion: When legion allows serializing/deserializing entities outside of the world this can be changed
                let world_entity = <(Entity, &World)>::query()
                    .iter(&mut ecs)
                    .next()
                    .map(|(entity, _)| *entity)
                    .expect("No 'world' entity existed in the ecs when deserializing!");
                let world = ecs
                    .entry(world_entity)
                    .expect("No 'world' entity existed in the ecs when deserializing!")
                    .get_component::<World>()
                    .expect("No 'world' was found in the ecs to deserialize!")
                    .clone();

                Ok(Universe {
                    ecs,
                    world,
                    physics_world,
                })
            }

            fn visit_map<V>(self, mut map: V) -> Result<Universe, V::Error>
            where
                V: MapAccess<'de>,
            {
                let registry = generate_registry();
                let mut ecs = None;
                let mut world = None;
                let mut physics_world = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::Ecs => {
                            if ecs.is_some() {
                                return Err(de::Error::duplicate_field("ecs"));
                            }
                            ecs = Some(map.next_value_seed(registry.as_deserialize())?);
                        }
                        Field::World => {
                            if world.is_some() {
                                return Err(de::Error::duplicate_field("world"));
                            }
                            world = Some(map.next_value()?);
                        }
                        Field::PhysicsWorld => {
                            if physics_world.is_some() {
                                return Err(de::Error::duplicate_field("physics_world"));
                            }
                            physics_world = Some(map.next_value()?);
                        }
                    }
                }
                let ecs = ecs.ok_or_else(|| de::Error::missing_field("ecs"))?;
                let world = world.ok_or_else(|| de::Error::missing_field("world"))?;
                let physics_world =
                    physics_world.ok_or_else(|| de::Error::missing_field("physics_world"))?;
                Ok(Universe {
                    ecs,
                    world,
                    physics_world,
                })
            }
        }

        const FIELDS: &[&str] = &["ecs", "world", "physics_world"];
        deserializer.deserialize_struct("Universe", FIELDS, UniverseVisitor)
    }
}

impl Universe {
    pub fn new() -> Self {
        let mut ecs = Ecs::default();
        let world = World::new(&mut ecs);
        Self {
            ecs,
            world,
            physics_world: PhysicsWorld::new(),
        }
    }

    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<()> {
        // FIXME legion: When legion allows serializing/deserializing entities outside of the world this can be changed
        let world_entity = self.ecs.push((self.world.clone(),));

        let encoded: Vec<u8> = bincode::serialize(&self)?;

        // FIXME legion: When legion allows serializing/deserializing entities outside of the world this can be changed
        self.ecs.remove(world_entity);

        // TODO: write to file
        // TODO: implement a loader
        let decoded: Option<String> = bincode::deserialize(&encoded[..])?;
        log::info!("it worked!");
        Ok(())
    }
}

fn generate_registry() -> Registry<String> {
    let mut registry = Registry::<String>::default();
    registry.register::<Transform>("transform".to_string());
    registry.register::<Name>("name".to_string());
    registry.register::<Camera>("camera".to_string());
    registry.register::<Mesh>("mesh".to_string());
    registry.register::<Skin>("skin".to_string());
    registry.register::<Light>("light".to_string());
    registry.register::<Selection>("selection".to_string());
    registry.register::<Visibility>("visibility".to_string());
    registry
}
