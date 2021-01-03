use anyhow::Result;
use dragonglass_physics::PhysicsWorld;
use dragonglass_world::{Ecs, Transform, World};
use legion::Registry;
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
        let mut state = serializer.serialize_struct("Universe", 3)?;

        let registry = generate_registry();
        let ecs = self
            .ecs
            .as_serializable(legion::component::<Transform>(), &registry);
        state.serialize_field("ecs", &ecs)?;

        state.serialize_field("world", &self.world)?;
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
                let ecs = seq
                    .next_element_seed(registry.as_deserialize())?
                    .ok_or_else(|| de::Error::invalid_length(0, &self))?;
                let world = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(1, &self))?;
                let physics_world = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(2, &self))?;
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

    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let encoded: Vec<u8> = bincode::serialize(&self)?;
        // TODO: write to file
        // TODO: implement a loader
        let decoded: Option<String> = bincode::deserialize(&encoded[..])?;
        Ok(())
    }
}

fn generate_registry() -> Registry<String> {
    let mut registry = Registry::<String>::default();
    registry.register::<Transform>("transform".to_string());
    registry
}
