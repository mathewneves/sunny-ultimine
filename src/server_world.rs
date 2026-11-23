//! Server-side world reads: raycast, tool and permission checks, block matching, shape
//! computation, and the per-player lifecycle hooks that keep clients in sync.

use std::sync::Arc;

use mod_api::{
    BlockState, ClipHit, Hand, Host, HostBlockStates, ItemStack, LevelRef, PlayerRef, ServerHost,
};
use sunny_ultimine_core::config::SyncedServerConfig;
use sunny_ultimine_core::cooldown::{is_on_cooldown, remaining_ms};
use sunny_ultimine_core::matcher::MatcherKind;
use sunny_ultimine_core::protocol::TimeSyncKind;
use sunny_ultimine_core::shapes::{
    Face, ShapeId, ShapeInput, ShapeQuery, compute_shape_blocks as shape_blocks,
};
use sunny_ultimine_core::status::CanUltimineResult;
use sunny_ultimine_core::xp;

use crate::attributes::{effective_cooldown, effective_experience, effective_max_blocks};
use crate::crops::{bush_type, looks_like_crop};
use crate::net::{send_server_config, send_shape, send_sync_time};
use crate::state::{ModState, TagIndex, epoch_millis, face_from_block_face, face_from_entity_dir};
use crate::tags::{
    BLOCK_WHITELIST, EXCLUDED_BLOCKS, EXCLUDED_TOOLS, EXCLUDED_TOOLS_STRICT, INCLUDED_TOOLS,
    block_in_tag_str, ensure_tag_index, item_in_tag_str,
};
use sunny_ultimine_core::config::ServerConfig;

/// The block the player looks at within their block interaction range, or None on a miss.
pub fn ray_trace_player(host: &dyn Host, player: PlayerRef) -> Option<ClipHit> {
    let mut dist = host
        .living(player.entity())
        .and_then(|l| l.attribute_value(host, "minecraft:block_interaction_range"))
        .unwrap_or(4.5);
    if player.is_creative(host) == Some(true) {
        dist += 0.5;
    }
    player.entity().pick(host, dist).filter(|hit| !hit.miss)
}

/// Whether `stack` has `prevent_tool_break` or fewer uses left (0 disables the check).
pub fn is_tool_too_damaged(host: &dyn Host, stack: &ItemStack, prevent_tool_break: u32) -> bool {
    if prevent_tool_break == 0 {
        return false;
    }
    let Some(max_damage) = host.max_damage(stack.item()).filter(|&m| m > 0) else {
        return false;
    };
    max_damage.saturating_sub(stack.damage()) <= prevent_tool_break
}

/// The player's main- and off-hand stacks.
pub fn main_and_offhand(
    host: &dyn Host,
    player: PlayerRef,
) -> (Option<ItemStack>, Option<ItemStack>) {
    let Some(living) = host.living(player.entity()) else {
        return (None, None);
    };
    (
        living.hand_item(host, Hand::Main),
        living.hand_item(host, Hand::Off),
    )
}

fn is_tool(host: &dyn Host, stack: &ItemStack) -> bool {
    stack
        .components()
        .is_some_and(|c| c.as_nbt().contains_key(&"minecraft:tool".into()))
        || host
            .item_default_component(stack.item(), "minecraft:tool")
            .is_some()
        || host.max_damage(stack.item()).unwrap_or(0) > 0
        || item_in_tag_str(host, stack, INCLUDED_TOOLS)
}

/// The `require_tool` and `require_valid_tool_for_block` rules for breaking `block_state` at `pos`.
pub fn is_valid_tool(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    pos: [i32; 3],
    block_state: BlockState,
    cfg: &ServerConfig,
) -> bool {
    if cfg.require_tool {
        let (main, _) = main_and_offhand(host, player);
        if !main.is_some_and(|s| is_tool(host, &s)) {
            return false;
        }
    }
    !cfg.require_valid_tool_for_block
        || !host
            .block_states()
            .requires_correct_tool(block_state)
            .unwrap_or(false)
        || player
            .has_correct_tool_for_drops(host, level, pos, block_state)
            .unwrap_or(false)
}

/// Fake players are refused, except during `/sunny_ultimine_dev` commands when `SUNNY_ULTIMINE_DEV=1`.
pub fn fake_blocked(host: &dyn Host, state: &ModState, player: PlayerRef) -> bool {
    if state
        .dev_allow_fake
        .load(std::sync::atomic::Ordering::Relaxed)
    {
        return false;
    }
    player.is_fake(host).unwrap_or(false)
}

/// `#sunny_ultimine:excluded_tools/strict` in either hand, or `#sunny_ultimine:excluded_tools` in the main hand.
pub fn holds_excluded_tool(host: &dyn Host, player: PlayerRef) -> bool {
    let (main, off) = main_and_offhand(host, player);
    let strict = |s: &Option<ItemStack>| {
        s.as_ref()
            .is_some_and(|s| item_in_tag_str(host, s, EXCLUDED_TOOLS_STRICT))
    };
    strict(&main)
        || strict(&off)
        || main
            .as_ref()
            .is_some_and(|s| item_in_tag_str(host, s, EXCLUDED_TOOLS))
}

/// Milliseconds until `player` may ultimine again; `0` when ready.
pub fn cooldown_remaining_ms(host: &dyn Host, state: &ModState, player: PlayerRef) -> u64 {
    let (cfg, last_used) = {
        let srv = state.server();
        (srv.config.clone(), srv.cooldowns.get(&player.uuid()))
    };
    remaining_ms(
        epoch_millis(),
        last_used,
        effective_cooldown(host, player, &cfg),
    )
}

/// Whether `player` may ultimine `block_state` at `pos` right now, and if not, why.
pub fn can_ultimine_server(
    host: &mut dyn ServerHost,
    state: &ModState,
    player: PlayerRef,
    level: LevelRef,
    pos: [i32; 3],
    block_state: BlockState,
) -> CanUltimineResult {
    if fake_blocked(host, state, player) {
        return CanUltimineResult::OtherRestriction;
    }
    let (cfg, last_used) = {
        let srv = state.server();
        (srv.config.clone(), srv.cooldowns.get(&player.uuid()))
    };
    if is_on_cooldown(
        epoch_millis(),
        last_used,
        effective_cooldown(host, player, &cfg),
    ) {
        return CanUltimineResult::OnCooldown;
    }
    let creative = player.is_creative(host).unwrap_or(false);
    if !creative && player.food(host).is_some_and(|f| f.level <= 0) {
        return CanUltimineResult::NoFood;
    }
    if holds_excluded_tool(host, player) {
        return CanUltimineResult::BlockedTool;
    }
    if !is_valid_tool(host, player, level, pos, block_state, &cfg) {
        return CanUltimineResult::NoTool;
    }
    CanUltimineResult::Allowed
}

fn is_air(host: &dyn Host, state: BlockState) -> bool {
    matches!(
        state.block().id(host).as_str(),
        "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"
    )
}

/// Air, or a `LiquidBlock`: a fluid with no outline (waterlogged blocks keep theirs).
fn is_air_or_liquid(host: &dyn Host, state: BlockState) -> bool {
    is_air(host, state)
        || (host.state_fluid_state(state).is_some_and(|f| !f.is_empty())
            && host.state_outline(state).is_some_and(|o| o.is_empty()))
}

fn tags_match(
    host: &dyn Host,
    any: bool,
    tags: &[String],
    orig: BlockState,
    cand: BlockState,
) -> bool {
    if any {
        return !is_air_or_liquid(host, cand)
            && !host.block_states().has_block_entity(orig).unwrap_or(false)
            && !host.block_states().has_block_entity(cand).unwrap_or(false);
    }
    tags.iter()
        .filter(|t| block_in_tag_str(host, orig.block(), t))
        .any(|t| block_in_tag_str(host, cand.block(), t))
}

/// Whether `cand` may join a selection that started at `orig` under `matcher`.
pub fn check_block_matches(
    host: &dyn Host,
    tag_index: &TagIndex,
    matcher: MatcherKind,
    orig: BlockState,
    cand: BlockState,
) -> bool {
    let block = cand.block();
    if !tag_index.whitelist_empty && !block_in_tag_str(host, block, BLOCK_WHITELIST) {
        return false;
    }
    if block_in_tag_str(host, block, EXCLUDED_BLOCKS) {
        return false;
    }
    match matcher {
        MatcherKind::ByBlock => orig.block() == block,
        MatcherKind::ByTagsShapeless => tags_match(
            host,
            tag_index.shapeless_any,
            &tag_index.shapeless_tags,
            orig,
            cand,
        ),
        MatcherKind::ByTagsShaped => tags_match(
            host,
            tag_index.shaped_any,
            &tag_index.shaped_tags,
            orig,
            cand,
        ),
        MatcherKind::ByCropType => {
            looks_like_crop(host, orig)
                && looks_like_crop(host, cand)
                && bush_type(host, orig) == bush_type(host, cand)
        }
    }
}

/// Answers shape membership from the live level; holds no `ModState` lock.
struct WorldQuery<'a> {
    host: &'a dyn Host,
    level: LevelRef,
    tag_index: &'a TagIndex,
    matcher: MatcherKind,
    orig: BlockState,
}

impl ShapeQuery for WorldQuery<'_> {
    fn check(&self, pos: [i32; 3]) -> bool {
        self.level.block_state(self.host, pos).is_some_and(|s| {
            check_block_matches(self.host, self.tag_index, self.matcher, self.orig, s)
        })
    }
}

/// The positions the player's current shape selects from `orig_pos`, capped by `max_blocks`
/// and (outside creative) by how many blocks the player's XP can pay for.
pub fn compute_shape_blocks(
    host: &dyn Host,
    state: &ModState,
    player: PlayerRef,
    level: LevelRef,
    orig_pos: [i32; 3],
    face: Face,
    max_blocks: usize,
) -> (MatcherKind, Vec<[i32; 3]>) {
    ensure_tag_index(host, state);
    let (tag_index, shape, cfg) = {
        let srv = state.server();
        let shape = srv
            .players
            .get(&player.uuid())
            .map(|d| d.shape)
            .unwrap_or(ShapeId::Shapeless);
        (Arc::clone(&srv.tag_index), shape, srv.config.clone())
    };
    let Some(orig) = level
        .block_state(host, orig_pos)
        .filter(|&s| !is_air(host, s))
    else {
        return (MatcherKind::ByBlock, Vec::new());
    };
    let tag_matcher = if shape.is_indeterminate() {
        MatcherKind::ByTagsShapeless
    } else {
        MatcherKind::ByTagsShaped
    };
    let matcher = if check_block_matches(host, &tag_index, tag_matcher, orig, orig) {
        tag_matcher
    } else if looks_like_crop(host, orig) {
        MatcherKind::ByCropType
    } else {
        MatcherKind::ByBlock
    };
    let query = WorldQuery {
        host,
        level,
        tag_index: &tag_index,
        matcher,
        orig,
    };
    let player_facing = face_from_entity_dir(player.entity().direction(host).unwrap_or("north"));
    let mut blocks = shape_blocks(
        shape,
        &ShapeInput {
            orig_pos,
            face,
            player_facing,
            max_blocks,
            crop_like: matcher == MatcherKind::ByCropType,
            query: &query,
        },
    );
    let xp_per_block = effective_experience(host, player, &cfg);
    if xp_per_block > 0.0 && !player.is_creative(host).unwrap_or(false) {
        let total = player
            .experience(host)
            .map_or(0, |e| xp::player_xp(e.level, e.progress));
        blocks.truncate((total as f64 / xp_per_block) as usize);
    }
    (matcher, blocks)
}

/// The player's effective cooldown, remaining cooldown and the synced config, as sent on login and on config changes.
fn sync_player(host: &mut dyn ServerHost, state: &ModState, player: PlayerRef, cfg: &ServerConfig) {
    let cooldown = effective_cooldown(host, player, cfg);
    send_sync_time(host, player, TimeSyncKind::Cooldown, cooldown);
    // Relative time, so client clock skew and relogs show the server's real cooldown.
    let remaining = cooldown_remaining_ms(host, state, player);
    send_sync_time(host, player, TimeSyncKind::RemainingMs, remaining);
    send_server_config(host, player, SyncedServerConfig::from(cfg));
}

/// `Hooks::player_logged_in`: sends the cooldown length, remaining cooldown and synced config to the joining player.
pub fn on_player_logged_in(host: &mut dyn ServerHost, state: &ModState, uuid: u128) {
    let Some(player) = host.player(uuid) else {
        return;
    };
    let cfg = state.server().config.clone();
    sync_player(host, state, player, &cfg);
}

/// `Hooks::player_logged_out`: forgets the player's runtime data.
pub fn on_player_logged_out(state: &ModState, uuid: u128) {
    state.server().players.remove(&uuid);
}

/// Deducts whole points of accumulated XP cost for `player`.
pub fn apply_pending_xp(host: &mut dyn ServerHost, state: &ModState, player: PlayerRef) {
    let uuid = player.uuid();
    let pending = state
        .server()
        .players
        .get(&uuid)
        .map_or(0.0, |d| d.pending_xp_cost);
    if pending >= 1.0 {
        let Some(exp) = player.experience(host) else {
            return;
        };
        let whole = pending.floor();
        if let Some(d) = state.server().players.get_mut(&uuid) {
            d.pending_xp_cost -= whole;
        }
        let up = xp::apply_xp_delta(exp.level, exp.progress, -(whole as i64));
        let into_level = i64::from(up.total_experience) - xp::experience_for_level(up.level);
        player.set_experience_levels(host, up.level);
        player.set_experience_points(host, into_level as i32);
        player.set_total_experience(host, up.total_experience);
    }
}

/// `Hooks::player_tick_post` (server side): refreshes the selection preview while the key is
/// held, and deducts whole points of accumulated XP cost.
pub fn on_player_tick_post(host: &mut dyn ServerHost, state: &ModState, player: PlayerRef) {
    let uuid = player.uuid();
    let Some((pressed, shape, cached_pos, cached_face, cached_shape, has_cache)) =
        state.server().players.get(&uuid).map(|d| {
            (
                d.pressed,
                d.shape,
                d.cached_pos,
                d.cached_face,
                d.cached_shape,
                d.cached_pos.is_some() || !d.cached_blocks.is_empty(),
            )
        })
    else {
        return;
    };

    if pressed {
        match ray_trace_player(host, player) {
            Some(hit) => {
                let face = face_from_block_face(hit.face);
                let changed = cached_pos != Some(hit.block)
                    || cached_face != Some(face)
                    || cached_shape != Some(shape);
                if let (true, Some(level)) = (changed, player.level(host)) {
                    let cfg = state.server().config.clone();
                    let max_blocks = effective_max_blocks(host, player, &cfg);
                    let (_, blocks) = compute_shape_blocks(
                        host, state, player, level, hit.block, face, max_blocks,
                    );
                    if let Some(d) = state.server().players.get_mut(&uuid) {
                        d.cached_pos = Some(hit.block);
                        d.cached_face = Some(face);
                        d.cached_shape = Some(shape);
                        d.cached_blocks.clone_from(&blocks);
                    }
                    send_shape(host, player, shape, Some(blocks));
                }
            }
            None if has_cache => {
                if let Some(d) = state.server().players.get_mut(&uuid) {
                    d.clear_cache();
                }
                send_shape(host, player, shape, Some(Vec::new()));
            }
            None => {}
        }
    }

    apply_pending_xp(host, state, player);
}

/// `Hooks::server_tick`: after a config change, re-syncs cooldown and config to every player.
pub fn on_server_tick(host: &mut dyn ServerHost, state: &ModState) {
    let cfg = {
        let mut srv = state.server();
        if !srv.sync_needed {
            return;
        }
        srv.sync_needed = false;
        srv.config.clone()
    };
    for player in host.players().server_players() {
        sync_player(host, state, player, &cfg);
    }
}
