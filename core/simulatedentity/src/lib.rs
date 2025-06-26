#![allow(unsafe_code)]

use bevy_app::{App, Plugin, PostUpdate};
use bevy_ecs::{archetype::ArchetypeId, prelude::*};
use bevy_math::{Vec3, Vec4};
use bevy_time::Time;

pub use simulatedentity_macro::DODConvertible;

pub mod dod;
use dod::{apply_dod_back, convert_chunk_to_dod, move_simd, DODStorage};

#[derive(Component, Clone, Copy, DODConvertible)]
#[repr(C)]
pub struct Position {
    pub value: Vec4,
}

impl From<Vec3> for Position {
    fn from(v: Vec3) -> Self {
        Self { value: v.extend(0.0) }
    }
}

#[derive(Component, Clone, Copy, DODConvertible)]
#[repr(C)]
pub struct Velocity {
    pub value: Vec4,
}

impl From<Vec3> for Velocity {
    fn from(v: Vec3) -> Self {
        Self { value: v.extend(0.0) }
    }
}

#[derive(SystemSet, Debug, Hash, PartialEq, Eq, Clone)]
pub struct UpdateSimulatedEntities;

pub trait SimulatedEntityCommands {
    fn spawn_simulated_entity(&mut self, bundle: impl Bundle) -> Entity;
}

impl<'w, 's> SimulatedEntityCommands for Commands<'w, 's> {
    fn spawn_simulated_entity(&mut self, bundle: impl Bundle) -> Entity {
        self.spawn(bundle).id()
    }
}

#[derive(Default, Resource)]
struct DODStore(pub std::collections::HashMap<ArchetypeId, DODStorage>);

fn update_simulated_entities(world: &mut World) {
    let pos_id = world.component_id::<Position>().unwrap();
    let vel_id = world.component_id::<Velocity>().unwrap();
    let ids: Vec<ArchetypeId> = world
        .archetypes()
        .iter()
        .filter(|a| a.len() as usize >= 1000 && a.contains(pos_id) && a.contains(vel_id))
        .map(|a| a.id())
        .collect();
    let dt = world.resource::<Time>().delta_secs();
    world.resource_scope(|world, mut store: Mut<DODStore>| {
        for id in &ids {
            let arch = world.archetypes().get(*id).unwrap();
            let entry = store
                .0
                .entry(*id)
                .or_insert_with(|| convert_chunk_to_dod(arch));
            move_simd(entry, dt);
        }
        store.0.retain(|&id, _| {
            world
                .archetypes()
                .get(id)
                .map_or(false, |a| a.len() as usize >= 1000)
        });
    });
}

fn apply_back_system(world: &mut World) {
    let ids: Vec<ArchetypeId> = world.resource::<DODStore>().0.keys().copied().collect();
    for id in ids {
        world.resource_scope(|world, mut store: Mut<DODStore>| {
            if let Some(dod) = store.0.get(&id) {
                apply_dod_back(world, dod, id);
            }
        });
    }
}

#[derive(Default)]
pub struct SimulatedEntityPlugin;

impl Plugin for SimulatedEntityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DODStore>()
            .add_systems(
                PostUpdate,
                update_simulated_entities.in_set(UpdateSimulatedEntities),
            )
            .add_systems(PostUpdate, apply_back_system.after(UpdateSimulatedEntities));
    }
}
