//! Native Sunrise trait mod for vein/shape mining, multi-block tool actions,
//! crop harvesting, HUD overlay, and 3D block outline preview).

pub use sunny_ultimine_core;

pub mod attributes;
pub mod client_hud;
pub mod client_input;
pub mod client_outline;
pub mod commands;
pub mod crops;
pub mod net;
pub mod right_click;
pub mod server_break;
pub mod server_world;
pub mod settings;
pub mod state;
pub mod tags;

use std::sync::Arc;

use mod_api::{
    AddServerReloadListenersEvent, BlockDropsEvent, BreakBlockEvent, ClientDisconnectingEvent,
    ClientHost, ClientTickPostEvent, ConfigLoadingEvent, ConfigReloadingEvent,
    ConfigUnloadingEvent, Flow, GuiLayerDrawEvent, Hooks, Host, HostSide, LevelHost,
    MouseScrollingEvent, PlayerLoggedInEvent, PlayerLoggedOutEvent, PlayerTickPostEvent,
    RegisterCommandsEvent, RegisterConfigsEvent, RegisterGuiLayersEvent, Registrar,
    RegistrationContext, RenderLevelStageEvent, RightClickBlockEvent, RunModCommandEvent,
    ServerHost, ServerTickEvent, ServerTickPhase, SettingChangedEvent, TagsUpdatedEvent,
};

use state::ModState;

/// Entry-point struct implementing Sunrise's [`Hooks`] trait for Sunny Ultimine.
#[derive(Default)]
pub struct SunnyUltimineMod {
    state: Arc<ModState>,
}

#[mod_api::sunrise_mod]
impl Hooks for SunnyUltimineMod {
    fn register(&self, r: &mut dyn Registrar, _cx: &RegistrationContext) {
        net::register_payloads(r, &self.state);
        attributes::register_attributes(r);
        client_input::register_key_mappings(r);
        settings::register_settings(r);
    }

    fn register_configs(&self, _host: HostSide<'_>, event: &mut RegisterConfigsEvent<'_>) {
        settings::on_register_configs(event);
    }

    fn config_loading(&self, _host: HostSide<'_>, event: &mut ConfigLoadingEvent<'_>) {
        settings::on_config_loading(&self.state, event);
    }

    fn config_reloading(&self, _host: HostSide<'_>, event: &mut ConfigReloadingEvent<'_>) {
        settings::on_config_reloading(&self.state, event);
    }

    fn config_unloading(&self, _host: &mut dyn ServerHost, event: &mut ConfigUnloadingEvent<'_>) {
        settings::on_config_unloading(&self.state, event);
    }

    fn setting_changed(&self, host: &mut dyn ClientHost, event: &mut SettingChangedEvent<'_>) {
        settings::on_setting_changed(host, &self.state, event);
    }

    fn register_gui_layers(
        &self,
        _context: &RegistrationContext,
        event: &mut RegisterGuiLayersEvent<'_>,
    ) {
        client_hud::register_gui_layers(event);
    }

    fn register_commands(&self, _host: &mut dyn ServerHost, event: &mut RegisterCommandsEvent<'_>) {
        commands::register_commands(event.commands);
    }

    fn run_mod_command(
        &self,
        host: &mut dyn ServerHost,
        event: &mut RunModCommandEvent<'_>,
    ) -> Flow {
        commands::run_command(host, &self.state, event.source, event.command)
    }

    fn add_server_reload_listeners(
        &self,
        _host: &dyn Host,
        event: &mut AddServerReloadListenersEvent<'_>,
    ) {
        tags::register_reload_listener(&self.state, event.listeners);
    }

    fn tags_updated(&self, _host: HostSide<'_>, _event: &mut TagsUpdatedEvent) {
        tags::mark_tags_dirty(&self.state);
    }

    fn player_logged_in(&self, host: &mut dyn ServerHost, event: &mut PlayerLoggedInEvent<'_>) {
        server_world::on_player_logged_in(host, &self.state, event.player);
    }

    fn player_logged_out(&self, _host: &mut dyn ServerHost, event: &mut PlayerLoggedOutEvent<'_>) {
        server_world::on_player_logged_out(&self.state, event.player);
    }

    fn player_tick_post(&self, host: HostSide<'_>, event: &mut PlayerTickPostEvent) {
        if let HostSide::Server(host) = host {
            server_world::on_player_tick_post(host, &self.state, event.player);
        }
    }

    fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
        if matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
            server_world::on_server_tick(host, &self.state);
        }
    }

    fn break_block(&self, host: &mut dyn ServerHost, event: &mut BreakBlockEvent<'_>) -> Flow {
        server_break::on_break_block(host, &self.state, event)
    }

    fn block_drops(&self, level: &dyn LevelHost, event: &mut BlockDropsEvent<'_>) -> Flow {
        server_break::on_block_drops(level, &self.state, event)
    }

    fn right_click_block(&self, host: HostSide<'_>, event: &mut RightClickBlockEvent<'_>) -> Flow {
        right_click::on_right_click_block(host, &self.state, event)
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _event: &mut ClientTickPostEvent) {
        client_input::on_client_tick_post(host, &self.state);
    }

    fn client_disconnecting(&self, _host: &dyn ClientHost, _event: &mut ClientDisconnectingEvent) {
        client_input::on_client_disconnecting(&self.state);
    }

    fn mouse_scrolling(&self, host: &mut dyn ClientHost, event: &mut MouseScrollingEvent) -> Flow {
        client_input::on_mouse_scrolling(host, &self.state, event)
    }

    fn render_level_stage(&self, _host: &dyn ClientHost, event: &mut RenderLevelStageEvent<'_>) {
        client_outline::on_render_level_stage(&self.state, event);
    }

    fn gui_layer_draw(&self, host: &dyn ClientHost, event: &mut GuiLayerDrawEvent<'_>) {
        client_hud::on_gui_layer_draw(host, &self.state, event);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use mod_api::SettingKind;
    use serde_json::Value;
    use sunny_ultimine_core::shapes::ShapeId;
    use sunny_ultimine_core::status::CanUltimineResult;

    use crate::{client_input, settings};

    #[test]
    fn settings_specs_are_valid_and_unique() {
        let specs = settings::setting_specs();
        let mut seen = HashSet::new();
        for spec in &specs {
            spec.check()
                .unwrap_or_else(|e| panic!("invalid setting {}: {e}", spec.key));
            assert!(
                seen.insert(spec.key.as_str()),
                "duplicate setting key {}",
                spec.key
            );
            assert_eq!(
                spec.tooltip.as_deref(),
                Some(format!("sunny_ultimine.setting.{}.tooltip", spec.key).as_str())
            );
        }
    }

    #[test]
    fn every_setting_key_shape_and_status_has_english_translation() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/sunny_ultimine/lang/en_us.json");
        let text = std::fs::read_to_string(&path)
            .expect("assets/sunny_ultimine/lang/en_us.json must exist and be readable");
        let lang: Value = serde_json::from_str(&text).expect("en_us.json must be valid JSON");
        let mut required: Vec<String> = vec![
            "key.category.sunny_ultimine.default".into(),
            format!("key.sunny_ultimine.{}", client_input::KEY_ULTIMINE),
            format!("key.sunny_ultimine.{}", client_input::KEY_NEXT_SHAPE),
            format!("key.sunny_ultimine.{}", client_input::KEY_PREV_SHAPE),
            "sunny_ultimine.info.base".into(),
            "sunny_ultimine.info.active".into(),
            "sunny_ultimine.info.not_active".into(),
            "sunny_ultimine.info.blocks".into(),
            "sunny_ultimine.info.partial_render".into(),
            "sunny_ultimine.change_shape".into(),
            "sunny_ultimine.change_shape.short".into(),
            "sunny_ultimine.change_shape.no_shift".into(),
            "attribute.name.sunny_ultimine.max_blocks_modifier".into(),
            "attribute.name.sunny_ultimine.cooldown_modifier".into(),
            "attribute.name.sunny_ultimine.exhaustion_modifier".into(),
            "attribute.name.sunny_ultimine.experience_modifier".into(),
        ];
        for shape in ShapeId::ALL {
            required.push(shape.translation_key().into());
        }
        for status in [
            CanUltimineResult::Allowed,
            CanUltimineResult::NoBlockTargeted,
            CanUltimineResult::NoFood,
            CanUltimineResult::NoExperience,
            CanUltimineResult::NoTool,
            CanUltimineResult::BlockedTool,
            CanUltimineResult::OnCooldown,
            CanUltimineResult::OtherRestriction,
        ] {
            required.push(status.translation_key().into());
        }
        for spec in settings::setting_specs() {
            required.push(spec.label.clone());
            required.extend(spec.tooltip.clone());
            if let SettingKind::Choice { options, .. } = &spec.kind {
                required.extend(options.iter().cloned());
            }
        }
        let missing: Vec<&String> = required
            .iter()
            .filter(|k| lang.get(k.as_str()).and_then(Value::as_str).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "missing keys in en_us.json: {missing:?}"
        );
    }
}
