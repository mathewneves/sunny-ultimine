//! `/sunny_ultimine` (status, shape, config) and, when `SUNNY_ULTIMINE_DEV=1`, the
//! headless-check `/sunny_ultimine_dev`.

use std::sync::Arc;

use mod_api::{
    Command, CommandArgumentValue as V, CommandInvocation, CommandRegistration, CommandSource,
    ConfigPath, Host, PlayerRef, ServerHost, Text,
};
use sunny_ultimine_core::cooldown::is_on_cooldown;
use sunny_ultimine_core::shapes::ShapeId;

use crate::attributes::{effective_cooldown, effective_max_blocks};
use crate::net;
use crate::state::{ModState, SERVER_CONFIG_FILE, epoch_millis};

const ROOT: &str = "sunny_ultimine";

/// Adds the mod's command usages with bound executors.
pub fn register_commands(state: &Arc<ModState>, commands: &mut Vec<CommandRegistration>) {
    let mut push = |cmd: Command| {
        let st = Arc::clone(state);
        commands.push(cmd.executes(move |host, source, invocation| {
            run_command(host, &st, source, invocation);
            Ok(1)
        }));
    };
    push(Command::new("sunny_ultimine status"));
    push(Command::new("sunny_ultimine shape [shape:word]"));
    push(Command::new("sunny_ultimine config get <key:word>"));
    push(Command::op(
        "sunny_ultimine config set <key:word> <value:text>",
        2,
    ));
    if dev::enabled() {
        for usage in [
            "sunny_ultimine_dev press <player:word> <pressed:boolean>",
            "sunny_ultimine_dev shape <player:word> <shape:word>",
            "sunny_ultimine_dev status <player:word>",
            "sunny_ultimine_dev give <player:word> <item:word> [damage:integer]",
            "sunny_ultimine_dev gamemode <player:word> <mode:word>",
            "sunny_ultimine_dev food <player:word> <level:integer>",
            "sunny_ultimine_dev xp <player:word> <amount:integer>",
            "sunny_ultimine_dev check_fake <player:word> <pos:block_pos>",
            "sunny_ultimine_dev drops <pos:block_pos> <radius:integer>",
            "sunny_ultimine_dev preview <player:word> <pos:block_pos> <face:word>",
            "sunny_ultimine_dev break <player:word> <pos:block_pos> <face:word>",
            "sunny_ultimine_dev right_click <player:word> <pos:block_pos> <face:word>",
        ] {
            push(Command::op(usage, 2));
        }
    }
}

/// Dispatches an `/sunny_ultimine` or `/sunny_ultimine_dev` invocation.
fn run_command(
    host: &mut dyn ServerHost,
    state: &ModState,
    source: &CommandSource,
    command: &CommandInvocation,
) {
    let words: Vec<&str> = command.literals.iter().map(String::as_str).collect();
    match words.split_first() {
        Some((&ROOT, rest)) => run_main(host, state, source, command, rest),
        Some((&dev::ROOT, rest)) if dev::enabled() => {
            dev::run_dev(host, state, source, command, rest)
        }
        _ => {}
    }
}

fn word<'a>(command: &'a CommandInvocation, name: &str) -> Option<&'a str> {
    match command.argument(name) {
        Some(V::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn source_player(source: &CommandSource) -> Option<PlayerRef> {
    match source {
        CommandSource::Player { player, .. } => Some(*player),
        _ => None,
    }
}

/// Tells a player source the line; the console gets it in the log.
fn reply(host: &mut dyn ServerHost, source: &CommandSource, line: &str) {
    match source_player(source) {
        Some(player) => {
            let _ = player.tell(host, Text::literal(line));
        }
        None => eprintln!("[sunny_ultimine] {line}"),
    }
}

fn save_config(host: &dyn Host, toml: &str) -> Result<(), String> {
    let path = ConfigPath::new(SERVER_CONFIG_FILE).map_err(|e| format!("{e:?}"))?;
    host.config_files()
        .write(&path, toml.as_bytes())
        .map_err(|e| format!("{e:?}"))
}

fn run_main(
    host: &mut dyn ServerHost,
    state: &ModState,
    source: &CommandSource,
    command: &CommandInvocation,
    words: &[&str],
) {
    let line = match words {
        ["config", "get"] => {
            let key = word(command, "key").unwrap_or_default();
            match state.server().config.get_field(key) {
                Some(value) => format!("{key} = {value}"),
                None => format!("Unknown setting {key:?}"),
            }
        }
        ["config", "set"] => {
            let key = word(command, "key").unwrap_or_default();
            let value = word(command, "value").unwrap_or_default();
            let result = {
                let mut srv = state.server();
                let set = srv.config.set_field(key, value);
                set.map(|()| {
                    Arc::make_mut(&mut srv.tag_index).dirty = true;
                    srv.sync_needed = true;
                    (
                        srv.config.get_field(key).unwrap_or_default(),
                        srv.config.to_toml(),
                    )
                })
            };
            match result {
                Ok((new_value, toml)) => match save_config(&*host, &toml) {
                    Ok(()) => format!("{key} = {new_value}"),
                    Err(e) => format!("{key} = {new_value} (not saved: {e})"),
                },
                Err(e) => format!("Could not set {key}: {e}"),
            }
        }
        ["status"] | ["shape"] => {
            let Some(player) = source_player(source) else {
                return reply(host, source, "Only players can use this command");
            };
            if words == ["status"] {
                status_line(host, state, player)
            } else if let Some(name) = word(command, "shape") {
                match set_shape(host, state, player, name) {
                    Some(shape) => format!("Shape set to {}", shape.english_name()),
                    None => format!("Unknown shape {name:?}. Shapes: {}", shape_list()),
                }
            } else {
                let shape = state
                    .server()
                    .players
                    .entry(player.uuid())
                    .or_default()
                    .shape;
                format!(
                    "Shape: {} ({}). Shapes: {}",
                    shape.english_name(),
                    shape.short_name(),
                    shape_list()
                )
            }
        }
        _ => return,
    };
    reply(host, source, &line);
}

fn shape_list() -> String {
    ShapeId::ALL.map(ShapeId::short_name).join(", ")
}

/// Sets `player`'s shape by name and tells their client; None for an unknown name.
fn set_shape(
    host: &mut dyn ServerHost,
    state: &ModState,
    player: PlayerRef,
    name: &str,
) -> Option<ShapeId> {
    let shape = ShapeId::from_name(name)?;
    {
        let mut srv = state.server();
        let data = srv.players.entry(player.uuid()).or_default();
        data.shape = shape;
        data.clear_cache();
    }
    net::send_shape(host, player, shape, None);
    Some(shape)
}

fn status_line(host: &mut dyn ServerHost, state: &ModState, player: PlayerRef) -> String {
    let uuid = player.uuid();
    let (pressed, shape, last_used, cfg) = {
        let mut srv = state.server();
        let data = srv.players.entry(uuid).or_default().clone();
        (
            data.pressed,
            data.shape,
            srv.cooldowns.get(&uuid),
            srv.config.clone(),
        )
    };
    let cooldown = effective_cooldown(&*host, player, &cfg);
    format!(
        "Sunny Ultimine: shape={} pressed={pressed} max_blocks={} cooldown_ticks={cooldown} on_cooldown={}",
        shape.short_name(),
        effective_max_blocks(&*host, player, &cfg),
        is_on_cooldown(epoch_millis(), last_used, cooldown),
    )
}

// --- dev commands (`SUNNY_ULTIMINE_DEV=1`): the headless check has no client to press keys or click with ---------
mod dev {
    use std::collections::BTreeMap;
    use std::sync::atomic::Ordering;

    use mod_api::{GameMode, Hand, Identifier, ItemStack, LevelRef};
    use sunny_ultimine_core::shapes::Face;
    use sunny_ultimine_core::xp;

    use super::*;
    use crate::right_click::execute_right_click;
    use crate::server_break::execute_ultimine_break;
    use crate::server_world::{can_ultimine_server, compute_shape_blocks, main_and_offhand};

    pub(super) const ROOT: &str = "sunny_ultimine_dev";

    /// Dev console commands exist only with `SUNNY_ULTIMINE_DEV=1` (`tools/server-check.sh` sets it).
    pub(super) fn enabled() -> bool {
        std::env::var("SUNNY_ULTIMINE_DEV").as_deref() == Ok("1")
    }

    /// A stable fake-player UUID for `name`.
    fn fake_uuid(name: &str) -> u128 {
        let hash = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
            (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
        });
        (0x4654_4255_4c54_494d_u128 << 64) | u128::from(hash)
    }

    /// The online player named `name`, else the fake player of that name (created on first use).
    // shortcut: fakes live in the overworld only; take the command's dimension if a check needs another.
    fn dev_player(host: &mut dyn ServerHost, name: &str) -> Option<(PlayerRef, bool)> {
        if let Some(player) = host.players().player_by_name(name) {
            return Some((player, false));
        }
        let level = host.level("minecraft:overworld")?;
        host.fake_player(level, fake_uuid(name), name)
            .map(|p| (p, true))
    }

    /// Level for `player`: real players use `player.level`; fake players outside a turn fall back to the overworld.
    fn dev_level(host: &dyn ServerHost, player: PlayerRef, fake: bool) -> Option<LevelRef> {
        player
            .level(host)
            .or_else(|| fake.then(|| host.level("minecraft:overworld")).flatten())
    }

    fn item_name(host: &dyn Host, stack: Option<&ItemStack>) -> String {
        stack.map_or_else(
            || "empty".to_owned(),
            |s| {
                format!(
                    "{}x{}@{}",
                    s.item().id(host).unwrap_or("unknown"),
                    s.count(),
                    s.damage()
                )
            },
        )
    }

    struct DevFakeGuard<'a>(&'a ModState);

    impl<'a> DevFakeGuard<'a> {
        fn enter(state: &'a ModState) -> Self {
            state.dev_allow_fake.store(true, Ordering::Relaxed);
            Self(state)
        }
    }

    impl Drop for DevFakeGuard<'_> {
        fn drop(&mut self) {
            self.0.dev_allow_fake.store(false, Ordering::Relaxed);
        }
    }

    fn integer(command: &CommandInvocation, name: &str) -> Option<i32> {
        match command.argument(name) {
            Some(V::Integer(n)) => Some(*n),
            _ => None,
        }
    }

    fn run_dev_drops(
        host: &mut dyn ServerHost,
        source: &CommandSource,
        command: &CommandInvocation,
    ) {
        let Some(V::BlockPos(pos)) = command.argument("pos") else {
            return;
        };
        let pos = *pos;
        let r = integer(command, "radius").unwrap_or(2).clamp(0, 16) as f64;
        let min = [pos[0] as f64 - r, pos[1] as f64 - r, pos[2] as f64 - r];
        let max = [
            pos[0] as f64 + 1.0 + r,
            pos[1] as f64 + 1.0 + r,
            pos[2] as f64 + 1.0 + r,
        ];
        let mut items: BTreeMap<String, u32> = BTreeMap::new();
        let mut item_entities = 0usize;
        if let Some(level) = host.level("minecraft:overworld") {
            let _ = level.visit_entities_in(&*host, min, max, &mut |entity| {
                if let Some(ie) = host.item_entity(entity)
                    && let Some(stack) = ie.with_stack(&*host, |s| s.project()).flatten()
                    && stack.count() > 0
                {
                    item_entities += 1;
                    let id = stack.item().id(&*host).unwrap_or("unknown").to_owned();
                    *items.entry(id).or_default() += stack.count();
                }
                std::ops::ControlFlow::Continue(())
            });
        }
        let orbs = host.experience_orbs("minecraft:overworld", min, max);
        let xp_orbs = orbs.len();
        let xp_total: i64 = orbs
            .iter()
            .map(|o| i64::from(o.value) * i64::from(o.count))
            .sum();
        let list: Vec<String> = items
            .into_iter()
            .map(|(id, count)| format!("{id}x{count}"))
            .collect();
        let line = format!(
            "DEV_DROPS pos={} {} {} item_entities={item_entities} items={} xp_orbs={xp_orbs} xp_total={xp_total}",
            pos[0],
            pos[1],
            pos[2],
            if list.is_empty() {
                "none".to_owned()
            } else {
                list.join(",")
            }
        );
        dev_log(host, source, &line);
    }

    pub(super) fn run_dev(
        host: &mut dyn ServerHost,
        state: &ModState,
        source: &CommandSource,
        command: &CommandInvocation,
        words: &[&str],
    ) {
        if words == ["drops"] {
            run_dev_drops(host, source, command);
            return;
        }
        let name = word(command, "player").unwrap_or_default();
        let Some((player, fake)) = dev_player(host, name) else {
            return dev_log(host, source, &format!("DEV_ERROR no player {name:?}"));
        };
        let uuid = player.uuid();
        let line = match words {
            ["press"] => {
                let pressed = matches!(command.argument("pressed"), Some(V::Boolean(true)));
                {
                    let mut srv = state.server();
                    let data = srv.players.entry(uuid).or_default();
                    data.pressed = pressed;
                    data.clear_cache();
                }
                format!("DEV_PRESS player={name} fake={fake} pressed={pressed}")
            }
            ["shape"] => {
                let shape = word(command, "shape").unwrap_or_default();
                match set_shape(host, state, player, shape) {
                    Some(s) => format!("DEV_SHAPE player={name} shape={}", s.short_name()),
                    None => format!("DEV_SHAPE player={name} error=unknown_shape {shape}"),
                }
            }
            ["give"] => {
                let item_raw = word(command, "item").unwrap_or_default();
                let damage = integer(command, "damage").unwrap_or(0).max(0) as u32;
                let slot = player.selected_slot(&*host).unwrap_or(0) as u32;
                let stack = if item_raw == "air" || item_raw == "empty" {
                    None
                } else {
                    let id_str = if item_raw.contains(':') {
                        item_raw.to_owned()
                    } else {
                        format!("minecraft:{item_raw}")
                    };
                    let Some(mut s) = Identifier::parse(&id_str)
                        .ok()
                        .and_then(|id| host.item_key(&id))
                        .and_then(|k| ItemStack::new(k, 1))
                    else {
                        return dev_log(host, source, &format!("DEV_ERROR bad item {item_raw:?}"));
                    };
                    if damage > 0 {
                        s.set_damage(damage);
                    }
                    Some(s)
                };
                match player.set_inventory_slot(host, slot, stack) {
                    Ok(()) => {
                        format!(
                            "DEV_GIVE player={name} slot={slot} item={item_raw} damage={damage}"
                        )
                    }
                    Err(e) => format!("DEV_ERROR give {e:?}"),
                }
            }
            ["gamemode"] => {
                let mode_str = word(command, "mode").unwrap_or_default();
                let mode = match mode_str {
                    "survival" => GameMode::Survival,
                    "creative" => GameMode::Creative,
                    "adventure" => GameMode::Adventure,
                    "spectator" => GameMode::Spectator,
                    _ => {
                        return dev_log(
                            host,
                            source,
                            &format!("DEV_ERROR bad gamemode {mode_str:?}"),
                        );
                    }
                };
                let ok = player.set_game_mode(host, mode);
                format!("DEV_GAMEMODE player={name} mode={mode_str} ok={ok}")
            }
            ["food"] => {
                let level = integer(command, "level").unwrap_or(20);
                player.set_food_level(host, level);
                player.set_saturation(host, 0.0);
                player.set_exhaustion(host, 0.0);
                format!("DEV_FOOD player={name} food={level}")
            }
            ["xp"] => {
                let amount = i64::from(integer(command, "amount").unwrap_or(0).max(0));
                let up = xp::apply_xp_delta(0, 0.0, amount);
                let into_level =
                    i64::from(up.total_experience) - xp::experience_for_level(up.level);
                player.set_experience_levels(host, up.level);
                player.set_experience_points(host, into_level as i32);
                player.set_total_experience(host, up.total_experience);
                format!("DEV_XP player={name} xp={}", up.total_experience)
            }
            ["check_fake"] => {
                let Some(V::BlockPos(pos)) = command.argument("pos") else {
                    return;
                };
                let pos = *pos;
                let Some(level) = dev_level(&*host, player, fake) else {
                    return dev_log(
                        host,
                        source,
                        &format!("DEV_ERROR player {name} has no level"),
                    );
                };
                let can = level
                    .block_state(&*host, pos)
                    .map(|s| can_ultimine_server(host, state, player, level, pos, s));
                format!("DEV_FAKE_CHECK player={name} can={can:?}")
            }
            ["status"] => {
                let (pressed, shape, last_used, cfg) = {
                    let mut srv = state.server();
                    let data = srv.players.entry(uuid).or_default().clone();
                    (
                        data.pressed,
                        data.shape,
                        srv.cooldowns.get(&uuid),
                        srv.config.clone(),
                    )
                };
                let (main, off) = main_and_offhand(&*host, player);
                let cooldown = effective_cooldown(&*host, player, &cfg);
                let total_xp = player
                    .experience(&*host)
                    .map_or(0, |e| xp::player_xp(e.level, e.progress));
                let settings: Vec<String> = cfg
                    .to_toml()
                    .lines()
                    .map(str::trim)
                    .filter(|l| !l.starts_with('#') && l.contains('='))
                    .map(str::to_owned)
                    .collect();
                format!(
                    "DEV_STATUS player={name} fake={fake} pressed={pressed} shape={} cooldown_ticks={cooldown} on_cooldown={} food={} xp={total_xp} creative={} main={} off={} max_blocks={} config=[{}]",
                    shape.short_name(),
                    is_on_cooldown(epoch_millis(), last_used, cooldown),
                    player.food(&*host).map_or(-1, |f| f.level),
                    player.is_creative(&*host).unwrap_or(false),
                    item_name(&*host, main.as_ref()),
                    item_name(&*host, off.as_ref()),
                    effective_max_blocks(&*host, player, &cfg),
                    settings.join("; "),
                )
            }
            [verb @ ("preview" | "break" | "right_click")] => {
                let Some(V::BlockPos(pos)) = command.argument("pos") else {
                    return;
                };
                let pos = *pos;
                let face_name = word(command, "face").unwrap_or_default();
                let Some(face) = Face::from_str_name(face_name) else {
                    return dev_log(host, source, &format!("DEV_ERROR bad face {face_name:?}"));
                };
                let Some(level) = dev_level(&*host, player, fake) else {
                    return dev_log(
                        host,
                        source,
                        &format!("DEV_ERROR player {name} has no level"),
                    );
                };
                let at = format!("{} {} {}", pos[0], pos[1], pos[2]);
                let _dev_guard = DevFakeGuard::enter(state);
                match *verb {
                    "preview" => {
                        let cfg = state.server().config.clone();
                        let max_blocks = effective_max_blocks(&*host, player, &cfg);
                        let can = level
                            .block_state(&*host, pos)
                            .map(|s| can_ultimine_server(host, state, player, level, pos, s));
                        let (matcher, blocks) =
                            compute_shape_blocks(host, state, player, level, pos, face, max_blocks);
                        let list: Vec<String> = blocks
                            .iter()
                            .map(|b| format!("{},{},{}", b[0], b[1], b[2]))
                            .collect();
                        format!(
                            "DEV_PREVIEW player={name} pos={at} face={face_name} can={can:?} matcher={matcher:?} count={} blocks={}",
                            blocks.len(),
                            list.join(";")
                        )
                    }
                    "break" => match level.block_state(&*host, pos) {
                        None => {
                            format!("DEV_BREAK ok=false player={name} pos={at} reason=unloaded")
                        }
                        Some(block_state) => {
                            let block = block_state
                                .block()
                                .id(&*host)
                                .unwrap_or("unknown")
                                .to_owned();
                            let can =
                                can_ultimine_server(host, state, player, level, pos, block_state);
                            let ok = execute_ultimine_break(
                                host,
                                state,
                                player,
                                level,
                                pos,
                                block_state,
                                face,
                            );
                            format!(
                                "DEV_BREAK ok={ok} player={name} pos={at} block={block} can={can:?}"
                            )
                        }
                    },
                    _ => {
                        let result =
                            execute_right_click(host, state, player, level, Hand::Main, pos, face);
                        format!(
                            "DEV_RIGHT_CLICK result={result:?} player={name} pos={at} face={face_name}"
                        )
                    }
                }
            }
            _ => return,
        };
        dev_log(host, source, &line);
    }

    /// Dev lines always reach the server log (the headless check's `expect` lines read it), and a player source too.
    fn dev_log(host: &mut dyn ServerHost, source: &CommandSource, line: &str) {
        eprintln!("[sunny_ultimine] {line}");
        if let Some(player) = source_player(source) {
            let _ = player.tell(host, Text::literal(line));
        }
    }
}
