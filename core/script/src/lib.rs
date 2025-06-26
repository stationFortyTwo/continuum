#![cfg_attr(docsrs, feature(doc_auto_cfg))]
#![forbid(unsafe_code)]
#![no_std]

#[cfg(feature = "std")]
extern crate std;
extern crate alloc;

use alloc::{string::String, sync::Arc, vec::Vec};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use bevy_app::{App, Plugin, PostUpdate, Update};
use bevy_ecs::{component::Component, event::Event, prelude::*, system::Resource};
use bevy_input::keyboard::KeyCode;
use bevy_input::Input;
use bevy_time::Time;
use bevy_utils::tracing::{error, info};
use crossbeam_channel::{unbounded, Receiver};
use mlua::{Lua, Function, Table};
use notify::{RecommendedWatcher, RecursiveMode, Watcher, EventKind};
use luau_behavior_macros::lua_api;

/// Events emitted by sample api functions
#[derive(Event)]
pub struct MoveToEvent {
    pub entity: Entity,
    pub x: f32,
    pub y: f32,
}

#[derive(Event)]
pub struct AttackEvent {
    pub entity: Entity,
    pub target: Entity,
}

#[derive(Event)]
pub struct GetStateEvent {
    pub entity: Entity,
}

#[derive(Default)]
struct LuaEventQueue {
    move_to: Vec<MoveToEvent>,
    attack: Vec<AttackEvent>,
    get_state: Vec<GetStateEvent>,
}

/// Global Lua resource
#[derive(Resource)]
pub struct LuaResource {
    pub lua: Lua,
    events: Arc<Mutex<LuaEventQueue>>, // shared with Lua functions
}

impl LuaResource {
    pub fn lua_ref(&self) -> &Lua { &self.lua }
}

impl Default for LuaResource {
    fn default() -> Self {
        let lua = Lua::new();
        let events = Arc::new(Mutex::new(LuaEventQueue::default()));
        // register api functions
        lua.context(|ctx| {
            EVENTS.with(|c| *c.borrow_mut() = Some(events.clone()));
            register_move_to(ctx).unwrap();
            register_attack(ctx).unwrap();
            register_get_state(ctx).unwrap();
        });
        Self { lua, events }
    }
}

/// Behavior handle identifying a loaded script
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct BehaviorHandle(usize);

struct BehaviorData {
    path: PathBuf,
    bytecode: Arc<Vec<u8>>,
    watcher: RecommendedWatcher,
}

#[derive(Resource)]
pub struct BehaviorManager {
    handles: Vec<BehaviorData>,
    rx: Receiver<(usize, Vec<u8>)>,
    tx: crossbeam_channel::Sender<(usize, Vec<u8>)>,
}

impl BehaviorManager {
    pub fn new() -> Self {
        let (tx, rx) = unbounded();
        Self { handles: Vec::new(), rx, tx }
    }

    pub fn load(&mut self, lua: &Lua, path: &str) -> BehaviorHandle {
        let idx = self.handles.len();
        let src = std::fs::read_to_string(path).expect("read script");
        let func: Function = lua.load(&src).into_function().unwrap();
        let bytecode = Arc::new(func.dump(false));
        let tx = self.tx.clone();
        let path_buf = PathBuf::from(path);
        let mut watcher: RecommendedWatcher = RecommendedWatcher::new(move |res| {
            if let Ok(event) = res {
                if matches!(event.kind, EventKind::Modify(_)) {
                    if let Ok(src) = std::fs::read_to_string(&path_buf) {
                        if let Ok(func) = lua.load(&src).into_function() {
                            let bytes = func.dump(false);
                            let _ = tx.send((idx, bytes));
                        }
                    }
                }
            }
        }).unwrap();
        watcher.watch(Path::new(path), RecursiveMode::NonRecursive).unwrap();
        self.handles.push(BehaviorData { path: path.into(), bytecode, watcher });
        BehaviorHandle(idx)
    }

    pub fn reload(&mut self, lua: &Lua, handle: BehaviorHandle) {
        if let Some(data) = self.handles.get_mut(handle.0) {
            if let Ok(src) = std::fs::read_to_string(&data.path) {
                if let Ok(func) = lua.load(&src).into_function() {
                    let new = Arc::new(func.dump(false));
                    data.bytecode = new.clone();
                    let bytes = (*new).clone();
                    let _ = self.tx.send((handle.0, bytes));
                }
            }
        }
    }

    pub fn attach_instance(&self, lua: &Lua, world: &mut World, entity: Entity, handle: BehaviorHandle) {
        let data = &self.handles[handle.0];
        let func: Function = lua.load(&*data.bytecode).into_function().unwrap();
        let table: Table = func.call(()).unwrap();
        world.entity_mut(entity).insert(LuauBehavior {
            handle,
            bytecode: data.bytecode.clone(),
            state: table,
        });
    }

    pub fn detach_and_clone(&self, lua: &Lua, world: &mut World, entity: Entity) {
        if let Some(mut b) = world.get_mut::<LuauBehavior>(entity) {
            let bytes = (*b.bytecode).clone();
            b.bytecode = Arc::new(bytes.clone());
            let func: Function = lua.load(&bytes).into_function().unwrap();
            b.state = func.call(()).unwrap();
        }
    }
}

#[derive(Component)]
pub struct LuauBehavior {
    handle: BehaviorHandle,
    bytecode: Arc<Vec<u8>>,
    state: Table<'static>,
}

#[derive(Event)]
struct BehaviorReloadEvent {
    handle: BehaviorHandle,
    old: Arc<Vec<u8>>,
    new: Arc<Vec<u8>>,
}

fn process_fs_events(mut manager: ResMut<BehaviorManager>, mut ev: EventWriter<BehaviorReloadEvent>) {
    for (id, bytes) in manager.rx.try_iter() {
        if let Some(data) = manager.handles.get_mut(id) {
            let old = data.bytecode.clone();
            let new = Arc::new(bytes);
            data.bytecode = new.clone();
            ev.send(BehaviorReloadEvent { handle: BehaviorHandle(id), old, new });
        }
    }
}

fn apply_reload(
    mut q: Query<&mut LuauBehavior>,
    lua_res: Res<LuaResource>,
    mut events: EventReader<BehaviorReloadEvent>,
) {
    let lua = &lua_res.lua;
    for ev in events.read() {
        for mut beh in q.iter_mut() {
            if beh.handle == ev.handle && Arc::ptr_eq(&beh.bytecode, &ev.old) {
                beh.bytecode = ev.new.clone();
                let func: Function = lua.load(&*beh.bytecode).into_function().unwrap();
                beh.state = func.call(()).unwrap();
            }
        }
    }
}

fn run_lua_behaviors(
    lua_res: Res<LuaResource>,
    time: Res<Time>,
    mut move_writer: EventWriter<MoveToEvent>,
    mut attack_writer: EventWriter<AttackEvent>,
    mut get_state_writer: EventWriter<GetStateEvent>,
    mut query: Query<&mut LuauBehavior>,
) {
    let lua = &lua_res.lua;
    for mut beh in query.iter_mut() {
        let dt = time.delta_seconds_f64();
        lua.context(|ctx| {
            if let Ok(update) = beh.state.get::<_, Function>("update") {
                if let Err(e) = update.call::<_, ()>((beh.state.clone(), dt)) {
                    error!("lua error: {e}");
                }
            }
        });
    }
    let mut queue = lua_res.events.lock().unwrap();
    for ev in queue.move_to.drain(..) { move_writer.send(ev); }
    for ev in queue.attack.drain(..) { attack_writer.send(ev); }
    for ev in queue.get_state.drain(..) { get_state_writer.send(ev); }
}

fn developer_detach_helper(
    input: Res<Input<KeyCode>>, 
    mut manager: ResMut<BehaviorManager>,
    lua_res: Res<LuaResource>,
    mut commands: Commands,
    query: Query<Entity, With<LuauBehavior>>,
) {
    if input.just_pressed(KeyCode::R) {
        if let Some(entity) = query.iter().next() {
            let world = commands.as_mut().world_mut();
            let lua = &lua_res.lua;
            manager.detach_and_clone(lua, world, entity);
        }
    }
}

pub struct LuauBehaviorPlugin;

impl Plugin for LuauBehaviorPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(LuaResource::default());
        app.insert_resource(BehaviorManager::new())
            .add_event::<MoveToEvent>()
            .add_event::<AttackEvent>()
            .add_event::<GetStateEvent>()
            .add_event::<BehaviorReloadEvent>()
            .add_systems(Update, (process_fs_events, developer_detach_helper))
            .add_systems(PostUpdate, (apply_reload, run_lua_behaviors));
    }
}

pub use {
    LuauBehaviorPlugin, BehaviorManager, BehaviorHandle, LuauBehavior, LuaResource,
    MoveToEvent, AttackEvent, GetStateEvent,
};

// Sample API functions

use core::cell::RefCell;
thread_local! {
    static EVENTS: RefCell<Option<Arc<Mutex<LuaEventQueue>>>> = const { RefCell::new(None) };
}

fn with_events(f: impl FnOnce(&mut LuaEventQueue)) {
    EVENTS.with(|cell| {
        if let Some(ref rc) = *cell.borrow() {
            if let Ok(mut q) = rc.lock() {
                f(&mut q);
            }
        }
    });
}

#[lua_api]
pub fn move_to(entity: u64, x: f32, y: f32) {
    with_events(|q| q.move_to.push(MoveToEvent { entity: Entity::from_bits(entity), x, y }));
}

#[lua_api]
pub fn attack(entity: u64, target: u64) {
    with_events(|q| q.attack.push(AttackEvent { entity: Entity::from_bits(entity), target: Entity::from_bits(target) }));
}

#[lua_api]
pub fn get_state(entity: u64) {
    with_events(|q| q.get_state.push(GetStateEvent { entity: Entity::from_bits(entity) }));
}

