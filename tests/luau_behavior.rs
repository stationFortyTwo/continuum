use bevy::prelude::*;
use luau_behavior::*;
use std::fs::write;

#[test]
fn behavior_reload_and_override() {
    let dir = tempfile::tempdir().unwrap();
    let script_path = dir.path().join("b.lua");
    write(&script_path, "return { update=function(self,dt) self.x=(self.x or 0)+1 end }").unwrap();

    let mut app = App::new();
    app.add_plugins(MinimalPlugins).add_plugin(LuauBehaviorPlugin);
    let handle;
    let entity;
    {
        let mut manager = app.world.resource_mut::<BehaviorManager>();
        let lua = app.world.resource::<LuaResource>().lua_ref();
        handle = manager.load(lua, script_path.to_str().unwrap());
        entity = app.world.spawn_empty().id();
        manager.attach_instance(lua, app.world_mut(), entity, handle);
    }
    app.update();
    let beh = app.world.entity(entity).get::<LuauBehavior>().unwrap();
    assert_eq!(beh.state.get::<_, i64>("x").unwrap(), 1);

    write(&script_path, "return { update=function(self,dt) self.x=(self.x or 0)+2 end }").unwrap();
    {
        let lua = app.world.resource::<LuaResource>().lua_ref();
        app.world.resource_mut::<BehaviorManager>().reload(lua, handle);
    }
    app.update();
    let beh = app.world.entity(entity).get::<LuauBehavior>().unwrap();
    assert_eq!(beh.state.get::<_, i64>("x").unwrap(), 3);

    {
        let lua = app.world.resource::<LuaResource>().lua_ref();
        app.world.resource_mut::<BehaviorManager>().detach_and_clone(lua, app.world_mut(), entity);
    }
    write(&script_path, "return { update=function(self,dt) self.x=(self.x or 0)+10 end }").unwrap();
    {
        let lua = app.world.resource::<LuaResource>().lua_ref();
        app.world.resource_mut::<BehaviorManager>().reload(lua, handle);
        let other = app.world.spawn_empty().id();
        app.world.resource_mut::<BehaviorManager>().attach_instance(lua, app.world_mut(), other, handle);
        app.update();
        let beh1 = app.world.entity(entity).get::<LuauBehavior>().unwrap();
        let beh2 = app.world.entity(other).get::<LuauBehavior>().unwrap();
        assert_eq!(beh1.state.get::<_, i64>("x").unwrap(), 13);
        assert_eq!(beh2.state.get::<_, i64>("x").unwrap(), 3);
    }
}
