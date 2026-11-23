//! The multi-block break: breaks every selected block as the player, collects their drops and
//! experience through `block_drops`, and spawns them merged at the original block.

use std::sync::Arc;

use mod_api::{
    BlockDropsEvent, BlockState, BreakBlockEvent, Flow, HostBlockStates, ItemStack, LevelHost,
    LevelRef, PlayerRef, ServerHost,
};
use sunny_ultimine_core::collector::MergedStacks;
use sunny_ultimine_core::matcher::MatcherKind;
use sunny_ultimine_core::protocol::TimeSyncKind;
use sunny_ultimine_core::shapes::Face;
use sunny_ultimine_core::status::CanUltimineResult;

use crate::attributes::{effective_exhaustion, effective_experience, effective_max_blocks};
use crate::net::{send_shape, send_sync_time};
use crate::server_world::{
    apply_pending_xp, can_ultimine_server, check_block_matches, compute_shape_blocks,
    cooldown_remaining_ms, is_tool_too_damaged, is_valid_tool, main_and_offhand, ray_trace_player,
};
use crate::state::{ActiveBreakSession, ModState, epoch_millis, face_from_block_face};

/// Clears `ModState::active_break` when dropped, so a panic or early return never leaves it set.
struct ActiveBreakGuard<'a>(&'a ModState);

impl ActiveBreakGuard<'_> {
    fn finish(self) -> Option<ActiveBreakSession> {
        let session = self.0.active_break().take();
        std::mem::forget(self);
        session
    }
}

impl Drop for ActiveBreakGuard<'_> {
    fn drop(&mut self) {
        *self.0.active_break() = None;
    }
}

/// Merges stackable drops and re-splits them into max-size stacks, in first-seen order.
fn consolidate(host: &dyn mod_api::Host, drops: Vec<ItemStack>) -> Vec<ItemStack> {
    let mut protos: Vec<ItemStack> = Vec::new();
    let mut merged = MergedStacks::new();
    for stack in drops {
        let idx = match protos.iter().position(|p| p.is_stackable_with(&stack)) {
            Some(i) => i,
            None => {
                protos.push(stack.clone());
                protos.len() - 1
            }
        };
        merged.add(idx, stack.count());
    }
    merged
        .drain_split(|&i| host.max_stack_size(protos[i].item()).unwrap_or(64))
        .into_iter()
        .map(|(i, count)| {
            let mut s = protos[i].clone();
            s.set_count(count).expect("drain_split never yields 0");
            s
        })
        .collect()
}

/// Ultimines from `orig_pos` as `player`. True when at least one block was broken (the
/// original block included), so the caller cancels the vanilla break.
pub fn execute_ultimine_break(
    host: &mut dyn ServerHost,
    state: &ModState,
    player: PlayerRef,
    level: LevelRef,
    orig_pos: [i32; 3],
    orig_state: BlockState,
    face: Face,
) -> bool {
    if state.active_break().is_some() {
        return false;
    }
    if can_ultimine_server(host, state, player, level, orig_pos, orig_state)
        != CanUltimineResult::Allowed
    {
        return false;
    }
    let cfg = state.server().config.clone();
    let max_blocks = effective_max_blocks(host, player, &cfg);
    let (_, blocks) = compute_shape_blocks(host, state, player, level, orig_pos, face, max_blocks);
    if blocks.is_empty() {
        return false;
    }
    let creative = player.is_creative(host).unwrap_or(false);
    let xp_per_block = effective_experience(host, player, &cfg);
    let exhaustion = effective_exhaustion(host, player, &cfg);
    let tag_index = Arc::clone(&state.server().tag_index);
    let selected: Vec<_> = blocks
        .iter()
        .filter_map(|&p| level.block(host, p).map(|b| (p, b)))
        .collect();
    let base_speed = host.block_states().destroy_speed(orig_state).unwrap_or(0.0);
    let had_item = main_and_offhand(host, player).0.is_some();

    *state.active_break() = Some(ActiveBreakSession {
        player_uuid: player.uuid(),
        drops: Vec::new(),
        xp_total: 0,
    });
    let guard = ActiveBreakGuard(state);

    let mut blocks_mined = 0usize;
    for &(pos, expected) in &selected {
        let Some(state1) = level
            .block_state(host, pos)
            .filter(|s| s.block() == expected)
        else {
            continue;
        };
        if !creative {
            let speed1 = host.block_states().destroy_speed(state1).unwrap_or(-1.0);
            let hardness_ok = speed1 >= 0.0
                && (speed1 <= base_speed
                    || check_block_matches(
                        host,
                        &tag_index,
                        MatcherKind::ByTagsShapeless,
                        orig_state,
                        state1,
                    ));
            if !hardness_ok || !is_valid_tool(host, player, level, pos, state1, &cfg) {
                continue;
            }
        }
        if !player.destroy_block(host, pos, false) {
            if cfg.cancel_on_block_break_fail {
                break;
            }
            continue;
        }
        blocks_mined += 1;
        if !creative {
            player.add_exhaustion(host, (exhaustion * 0.005) as f32);
            if player
                .food(host)
                .is_some_and(|f| f.level <= 0 || f.exhaustion / 4.0 > f.saturation + f.level as f32)
            {
                break;
            }
        }
        if had_item {
            let main = main_and_offhand(host, player).0;
            if main.is_none_or(|s| is_tool_too_damaged(host, &s, cfg.prevent_tool_break)) {
                break;
            }
        }
    }

    let session = guard.finish();
    if let Some(session) = session {
        for stack in consolidate(host, session.drops) {
            let _ = level.pop_resource(host, orig_pos, stack);
        }
        if session.xp_total > 0 {
            let [x, y, z] = orig_pos.map(|c| f64::from(c) + 0.5);
            level.award_experience(host, x, y, z, session.xp_total);
        }
    }

    let now = epoch_millis();
    let record = !creative && blocks_mined > 0;
    let shape = {
        let mut srv = state.server();
        if record {
            srv.cooldowns.set(player.uuid(), now);
        }
        let data = srv.players.entry(player.uuid()).or_default();
        if record {
            data.pending_xp_cost += blocks_mined.saturating_sub(1) as f64 * xp_per_block;
        }
        data.clear_cache();
        data.shape
    };
    if record {
        let remaining = cooldown_remaining_ms(host, state, player);
        send_sync_time(host, player, TimeSyncKind::RemainingMs, remaining);
        apply_pending_xp(host, state, player);
    }
    send_shape(host, player, shape, Some(Vec::new()));
    blocks_mined > 0
}

/// `Hooks::break_block`: ultimines instead of the vanilla break while the key is held.
pub fn on_break_block(
    host: &mut dyn ServerHost,
    state: &ModState,
    event: &mut BreakBlockEvent<'_>,
) -> Flow {
    if state.active_break().is_some() {
        return Flow::Continue;
    }
    let Some(player) = host.players().acting_player() else {
        return Flow::Continue;
    };
    if !state
        .server()
        .players
        .get(&player.uuid())
        .is_some_and(|d| d.pressed)
    {
        return Flow::Continue;
    }
    let Some(level) = player.level(host) else {
        return Flow::Continue;
    };
    let face = ray_trace_player(host, player).map_or(Face::Up, |h| face_from_block_face(h.face));
    if execute_ultimine_break(host, state, player, level, event.pos, event.state, face) {
        Flow::Cancel
    } else {
        Flow::Continue
    }
}

/// `Hooks::block_drops`: during an ultimine, takes the breaking player's drops and experience
/// into the session so they spawn merged at the original block.
pub fn on_block_drops(
    _level_host: &dyn LevelHost,
    state: &ModState,
    event: &mut BlockDropsEvent<'_>,
) -> Flow {
    let mut active = state.active_break();
    if let Some(session) = active
        .as_mut()
        .filter(|s| event.breaker == Some(s.player_uuid))
    {
        session.drops.extend(
            event
                .drops
                .iter()
                .filter_map(|d| d.stack().and_then(|s| s.project())),
        );
        event.drops.clear();
        session.xp_total = session.xp_total.saturating_add(event.experience);
        event.experience = 0;
    }
    Flow::Continue
}
