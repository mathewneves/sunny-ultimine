//! Multi-block right-click tool actions and crop harvesting.

use std::sync::atomic::Ordering;

use mod_api::{
    BlockHit, BlockUpdateFlags, EquipmentSlot, Flow, Hand, Host, HostSide, Identifier,
    InteractionResult, ItemStack, LevelRef, PlayerRef, RightClickBlockEvent, ServerHost,
    SoundSource,
};
use sunny_ultimine_core::matcher::MatcherKind;
use sunny_ultimine_core::protocol::TimeSyncKind;
use sunny_ultimine_core::shapes::Face;

use crate::attributes::{effective_experience, effective_max_blocks};
use crate::crops;
use crate::net;
use crate::server_world::{
    compute_shape_blocks, cooldown_remaining_ms, fake_blocked, holds_excluded_tool,
    is_tool_too_damaged, main_and_offhand,
};
use crate::state::{ModState, epoch_millis, face_from_normal};

/// Clears the right-click re-entrancy flag when dropped, on every return path.
struct ReentryGuard<'a>(&'a ModState);

impl Drop for ReentryGuard<'_> {
    fn drop(&mut self) {
        self.0.right_click_reentrant.store(false, Ordering::SeqCst);
    }
}

fn held(host: &dyn Host, player: PlayerRef, hand: Hand) -> Option<ItemStack> {
    let (main, off) = main_and_offhand(host, player);
    match hand {
        Hand::Main => main,
        Hand::Off => off,
    }
}

fn item_in_tag(host: &dyn Host, stack: &ItemStack, tag: &str) -> bool {
    let tags = host.tags();
    Identifier::parse(tag)
        .ok()
        .and_then(|id| tags.key(&id))
        .and_then(|t| tags.is_in_tag(stack.item(), t))
        .unwrap_or(false)
}

/// Applies the held tool's transformation (strip, scrape, unwax, flatten, till) to each of `blocks`
/// as the vanilla tool would, stopping when the tool runs out. Returns the number transformed.
fn transform_blocks(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    hand: Hand,
    blocks: &[[i32; 3]],
    creative: bool,
    prevent_tool_break: u32,
) -> usize {
    let mut did_work = 0;
    for &p in blocks {
        let Some(stack) = held(&*host, player, hand) else {
            break;
        };
        if is_tool_too_damaged(&*host, &stack, prevent_tool_break) {
            break;
        }
        let hit = BlockHit::new(
            [p[0] as f64 + 0.5, p[1] as f64 + 1.0, p[2] as f64 + 0.5],
            [0, 1, 0],
            false,
        );
        let Some(preview) = level.block_transform(&*host, player, hand, &stack, p, hit) else {
            continue;
        };
        if !level
            .set_block(
                host,
                p,
                preview.state,
                BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
            )
            .unwrap_or(false)
        {
            continue;
        }
        let center = [p[0] as f64 + 0.5, p[1] as f64 + 0.5, p[2] as f64 + 0.5];
        if !preview.sound.is_empty() {
            level.play_sound(
                host,
                &preview.sound,
                center,
                SoundSource::Blocks,
                1.0,
                1.0,
                None,
            );
        }
        // The engine names these level events by their particle (`block_particles.rs`).
        match preview.particle.as_str() {
            "wax_off" => {
                level.level_event(host, 3004, p, 0);
            }
            "scrape" => {
                level.level_event(host, 3005, p, 0);
            }
            _ => {}
        }
        if let Some(table) = preview.loot.as_deref() {
            let loot = level
                .roll_loot(host, table, center, Some(&stack), Some(player.entity()))
                .ok()
                .flatten()
                .unwrap_or_default();
            for s in loot {
                let _ = level.pop_resource(host, p, s);
            }
        }
        if !creative
            && preview.durability > 0
            && let Some(living) = host.living(player.entity())
        {
            let slot = match hand {
                Hand::Main => EquipmentSlot::MainHand,
                Hand::Off => EquipmentSlot::OffHand,
            };
            living.hurt_and_break(host, slot, preview.durability as i32);
        }
        did_work += 1;
    }
    did_work
}

/// A right-click on `pos`: a single mature crop harvest without the Ultimine key, or with it the held
/// tool's action (or a crop harvest) over the whole shape.
pub fn execute_right_click(
    host: &mut dyn ServerHost,
    state: &ModState,
    player: PlayerRef,
    level: LevelRef,
    hand: Hand,
    pos: [i32; 3],
    face: Face,
) -> InteractionResult {
    if state.right_click_reentrant.swap(true, Ordering::SeqCst) {
        return InteractionResult::Pass;
    }
    let _guard = ReentryGuard(state);

    let uuid = player.uuid();
    let (pressed, cfg) = {
        let mut srv = state.server();
        let pressed = srv.players.entry(uuid).or_default().pressed;
        (pressed, srv.config.clone())
    };

    if !pressed {
        // Fake players may still harvest single crops when the Ultimine key is not held.
        if crops::try_single_crop_harvest(host, state, player, level, pos, face) {
            if let Some(living) = host.living(player.entity()) {
                living.swing(host, hand, true);
            }
            return InteractionResult::SuccessServer;
        }
        return InteractionResult::Pass;
    }

    let creative = player.is_creative(&*host).unwrap_or(false);
    let starving = player.food(&*host).is_some_and(|f| f.level <= 0);
    if fake_blocked(&*host, state, player)
        || (starving && !creative)
        || holds_excluded_tool(&*host, player)
        || cooldown_remaining_ms(&*host, state, player) > 0
    {
        return InteractionResult::Pass;
    }

    let max_blocks = effective_max_blocks(&*host, player, &cfg);
    let (matcher, blocks) = compute_shape_blocks(host, state, player, level, pos, face, max_blocks);
    if blocks.is_empty() {
        return InteractionResult::Pass;
    }

    let mut did_work = 0;
    let tools = [
        (cfg.right_click_axe, "minecraft:axes"),
        (cfg.right_click_shovel, "minecraft:shovels"),
        (cfg.right_click_hoe, "minecraft:hoes"),
    ];
    for (enabled, tag) in tools {
        if did_work > 0 {
            break;
        }
        let holds = held(&*host, player, hand).is_some_and(|s| item_in_tag(&*host, &s, tag));
        if enabled && holds {
            did_work = transform_blocks(
                host,
                player,
                level,
                hand,
                &blocks,
                creative,
                cfg.prevent_tool_break,
            );
        }
    }
    let orig_is_crop = level
        .block_state(&*host, pos)
        .is_some_and(|s| crops::looks_like_crop(&*host, s));
    if did_work == 0
        && cfg.right_click_harvesting
        && (matcher == MatcherKind::ByCropType || orig_is_crop)
    {
        did_work = crops::harvest_crops_in_shape(host, player, level, pos, face, &blocks);
    }
    if did_work == 0 {
        return InteractionResult::Pass;
    }

    if let Some(living) = host.living(player.entity()) {
        living.swing(host, hand, true);
    }
    if !creative {
        let xp = (did_work - 1) as f64 * effective_experience(&*host, player, &cfg);
        {
            let mut srv = state.server();
            srv.cooldowns.set(uuid, epoch_millis());
            let data = srv.players.entry(uuid).or_default();
            data.pending_xp_cost += xp;
            // The selection changed under the player (stripped logs no longer match logs): recompute it.
            data.clear_cache();
        }
        let remaining = cooldown_remaining_ms(&*host, state, player);
        net::send_sync_time(host, player, TimeSyncKind::RemainingMs, remaining);
        crate::server_world::apply_pending_xp(host, state, player);
    } else {
        state
            .server()
            .players
            .entry(uuid)
            .or_default()
            .clear_cache();
    }
    InteractionResult::SuccessServer
}

/// The `right_click_block` hook: runs [`execute_right_click`] on the server and cancels the click when it did something.
pub fn on_right_click_block(
    host: HostSide<'_>,
    state: &ModState,
    event: &mut RightClickBlockEvent<'_>,
) -> Flow {
    let HostSide::Server(host) = host else {
        return Flow::Continue;
    };
    // `acting_player` also reaches fake players, which `player(uuid)` never returns.
    let uuid = event.click.player;
    let acting = host.players().acting_player().filter(|p| p.uuid() == uuid);
    let Some(player) = acting.or_else(|| host.player(uuid)) else {
        return Flow::Continue;
    };
    let Some(level) = player.level(&*host) else {
        return Flow::Continue;
    };
    let face = face_from_normal(event.click.hit.face).unwrap_or(Face::Up);
    let res = execute_right_click(
        host,
        state,
        player,
        level,
        event.click.hand,
        event.click.pos,
        face,
    );
    if matches!(res, InteractionResult::Pass) {
        return Flow::Continue;
    }
    *event.cancellation_result = res;
    Flow::Cancel
}
