//! Options > Mods settings and the client/server TOML config files.

use std::sync::Arc;

use mod_api::{
    ClientHost, Config, ConfigKind, ConfigLoadingEvent, ConfigPath, ConfigReloadingEvent,
    ConfigUnloadingEvent, RegisterConfigsEvent, Registrar, SettingChangedEvent, SettingSpec,
};
use sunny_ultimine_core::config::{ClientConfig, PanelPos, ServerConfig};

use crate::state::{CLIENT_CONFIG_FILE, MOD_ID, ModState, SERVER_CONFIG_FILE};

/// How a setting is drawn in Options > Mods; its default is `ClientConfig::default()`'s value.
#[derive(Clone, Copy)]
enum Kind {
    Toggle,
    /// One of [`PanelPos::ALL`].
    Position,
    /// min, max, step.
    Range(f64, f64, f64),
}

use Kind::{Position, Range, Toggle};

/// Every client setting, in page order. Each key is the `sunny_ultimine-client.toml` key it changes.
const SETTINGS: &[(&str, Kind)] = &[
    ("require_ultimine_key_for_cycling", Toggle),
    ("shape_feedback_hotbar_message", Toggle),
    ("render_outline", Range(0.0, 4096.0, 16.0)),
    ("preview_line_alpha", Range(0.0, 255.0, 1.0)),
    ("shape_menu_context_lines", Range(1.0, 5.0, 1.0)),
    ("require_sneak_for_menu", Toggle),
    ("overlay_pos", Position),
    ("overlay_inset_x", Range(0.0, 1000.0, 1.0)),
    ("overlay_inset_y", Range(0.0, 1000.0, 1.0)),
    ("overlay_scale", Range(0.25, 4.0, 0.25)),
];

fn label(key: &str) -> String {
    format!("sunny_ultimine.setting.{key}")
}

/// `key`'s value in `cfg`, as the setting widget holds it.
fn toggle_value(cfg: &ClientConfig, key: &str) -> Option<bool> {
    Some(match key {
        "require_ultimine_key_for_cycling" => cfg.require_ultimine_key_for_cycling,
        "shape_feedback_hotbar_message" => cfg.shape_feedback_hotbar_message,
        "require_sneak_for_menu" => cfg.require_sneak_for_menu,
        _ => return None,
    })
}

fn range_value(cfg: &ClientConfig, key: &str) -> Option<f64> {
    Some(match key {
        "render_outline" => f64::from(cfg.render_outline),
        "preview_line_alpha" => f64::from(cfg.preview_line_alpha),
        "shape_menu_context_lines" => f64::from(cfg.shape_menu_context_lines),
        "overlay_inset_x" => f64::from(cfg.overlay_inset_x),
        "overlay_inset_y" => f64::from(cfg.overlay_inset_y),
        "overlay_scale" => cfg.overlay_scale,
        _ => return None,
    })
}

/// The Options > Mods page, defaulting to `ClientConfig::default()`.
pub fn setting_specs() -> Vec<SettingSpec> {
    let d = ClientConfig::default();
    SETTINGS
        .iter()
        .map(|&(key, kind)| {
            let spec = match kind {
                Toggle => {
                    SettingSpec::toggle(key, label(key), toggle_value(&d, key).expect("toggle key"))
                }
                Position => SettingSpec::choice(
                    key,
                    label(key),
                    PanelPos::ALL
                        .iter()
                        .map(|p| label(&format!("{key}.{}", p.as_str()))),
                    d.overlay_pos.index(),
                ),
                Range(min, max, step) => SettingSpec::range(
                    key,
                    label(key),
                    min,
                    max,
                    step,
                    range_value(&d, key).expect("range key"),
                ),
            };
            spec.tooltip(format!("{}.tooltip", label(key)))
        })
        .collect()
}

/// Registers every client setting; a refused spec is a bug in [`SETTINGS`].
pub fn register_settings(r: &mut dyn Registrar) {
    for spec in setting_specs() {
        let key = spec.key.clone();
        r.register_setting(spec)
            .unwrap_or_else(|e| panic!("sunny_ultimine setting {key}: {e}"));
    }
}

/// Registers `sunny_ultimine-client.toml` and `sunny_ultimine-server.toml`.
pub fn on_register_configs(event: &mut RegisterConfigsEvent<'_>) {
    event.configs.push(
        Config::new(
            MOD_ID,
            ConfigKind::Client,
            &ClientConfig::default().to_toml(),
        )
        .file(CLIENT_CONFIG_FILE),
    );
    event.configs.push(
        Config::new(
            MOD_ID,
            ConfigKind::Server,
            &ServerConfig::default().to_toml(),
        )
        .file(SERVER_CONFIG_FILE),
    );
}

fn apply_config_text(state: &ModState, mod_id: &str, file: &str, text: &str) {
    if mod_id != MOD_ID {
        return;
    }
    if file == CLIENT_CONFIG_FILE {
        let mut cli = state.client();
        cli.config = ClientConfig::from_toml(text);
        cli.edges_dirty = true;
    } else if file == SERVER_CONFIG_FILE {
        let mut srv = state.server();
        srv.config = ServerConfig::from_toml(text);
        Arc::make_mut(&mut srv.tag_index).dirty = true;
        srv.sync_needed = true;
    }
}

/// A config file was read.
pub fn on_config_loading(state: &ModState, event: &ConfigLoadingEvent<'_>) {
    apply_config_text(
        state,
        &event.config.mod_id,
        &event.config.file,
        &event.config.text,
    );
}

/// A config file changed on disk.
pub fn on_config_reloading(state: &ModState, event: &ConfigReloadingEvent<'_>) {
    apply_config_text(
        state,
        &event.config.mod_id,
        &event.config.file,
        &event.config.text,
    );
}

/// The server config is dropped as its server stops.
pub fn on_config_unloading(state: &ModState, event: &ConfigUnloadingEvent<'_>) {
    if event.config.mod_id != MOD_ID || event.config.file != SERVER_CONFIG_FILE {
        return;
    }
    let mut srv = state.server();
    srv.config = ServerConfig::default();
    Arc::make_mut(&mut srv.tag_index).dirty = true;
    srv.players.clear();
    srv.cooldowns.clear();
    srv.sync_needed = false;
}

/// Sets `key` in `cfg` from the value Sunrise keeps; false when the key or value is unknown.
fn apply_setting(host: &dyn ClientHost, cfg: &mut ClientConfig, key: &str) -> bool {
    let Some(&(_, kind)) = SETTINGS.iter().find(|(k, _)| *k == key) else {
        return false;
    };
    match kind {
        Toggle => {
            let Some(v) = host.setting_bool(key) else {
                return false;
            };
            match key {
                "require_ultimine_key_for_cycling" => cfg.require_ultimine_key_for_cycling = v,
                "shape_feedback_hotbar_message" => cfg.shape_feedback_hotbar_message = v,
                "require_sneak_for_menu" => cfg.require_sneak_for_menu = v,
                _ => return false,
            }
        }
        Position => {
            let Some(i) = host.setting_choice(key) else {
                return false;
            };
            cfg.overlay_pos = PanelPos::from_index(i as usize);
        }
        Range(min, max, _) => {
            let Some(v) = host.setting_f64(key) else {
                return false;
            };
            let v = v.clamp(min, max);
            let n = v.round();
            match key {
                "render_outline" => cfg.render_outline = n as u32,
                "preview_line_alpha" => cfg.preview_line_alpha = n as u8,
                "shape_menu_context_lines" => cfg.shape_menu_context_lines = n as u8,
                "overlay_inset_x" => cfg.overlay_inset_x = n as i32,
                "overlay_inset_y" => cfg.overlay_inset_y = n as i32,
                "overlay_scale" => cfg.overlay_scale = (v * 4.0).round() / 4.0,
                _ => return false,
            }
        }
    }
    true
}

/// The player changed a setting on the Options > Mods page: apply it and save the client TOML.
pub fn on_setting_changed(
    host: &mut dyn ClientHost,
    state: &ModState,
    event: &SettingChangedEvent<'_>,
) {
    if event.mod_id != MOD_ID {
        return;
    }
    // Read the setting before locking; the lock is never held across host calls.
    let mut cfg = state.client().config.clone();
    if !apply_setting(host, &mut cfg, event.key) {
        return;
    }
    let text = cfg.to_toml();
    {
        let mut cli = state.client();
        cli.config = cfg;
        if event.key == "render_outline" {
            cli.edges_dirty = true;
        }
    }
    let path =
        ConfigPath::new(CLIENT_CONFIG_FILE).expect("client config file name is a valid path");
    if let Err(e) = host.config_files().write(&path, text.as_bytes()) {
        eprintln!("[sunny_ultimine] writing {CLIENT_CONFIG_FILE}: {e:?}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specs_pass_registrar_checks() {
        let specs = setting_specs();
        assert_eq!(specs.len(), 10);
        for spec in &specs {
            spec.check().unwrap_or_else(|e| panic!("{}: {e}", spec.key));
        }
    }

    #[test]
    fn every_setting_round_trips_its_default() {
        let d = ClientConfig::default();
        for &(key, kind) in SETTINGS {
            match kind {
                Toggle => assert!(toggle_value(&d, key).is_some(), "{key}"),
                Range(..) => assert!(range_value(&d, key).is_some(), "{key}"),
                Position => assert_eq!(key, "overlay_pos"),
            }
        }
    }
}
