//! Crop detection and right-click harvesting.

use mod_api::{
    BlockState, BlockUpdateFlags, Host, HostBlockStates, ItemStack, LevelRef, PlayerRef, ServerHost,
};
use sunny_ultimine_core::collector::MergedStacks;
use sunny_ultimine_core::matcher::BushType;
use sunny_ultimine_core::shapes::{Face, offset};

use crate::state::ModState;

/// Block tags whose members are plants/crops (`VegetationBlock` subclasses) in vanilla.
const CROP_TAGS: &[&str] = &[
    "minecraft:crops",
    "minecraft:saplings",
    "minecraft:small_flowers",
    "minecraft:tall_flowers",
];

/// Vanilla `VegetationBlock` / `CocoaBlock` blocks not covered by [`CROP_TAGS`].
const CROP_BLOCKS: &[&str] = &[
    "minecraft:cocoa",
    "minecraft:sweet_berry_bush",
    "minecraft:nether_wart",
    "minecraft:short_grass",
    "minecraft:tall_grass",
    "minecraft:fern",
    "minecraft:large_fern",
    "minecraft:dead_bush",
    "minecraft:brown_mushroom",
    "minecraft:red_mushroom",
    "minecraft:crimson_fungus",
    "minecraft:warped_fungus",
    "minecraft:seagrass",
    "minecraft:tall_seagrass",
    "minecraft:torchflower_crop",
    "minecraft:pitcher_crop",
    "minecraft:sunflower",
    "minecraft:lilac",
    "minecraft:rose_bush",
    "minecraft:peony",
    "minecraft:pitcher_plant",
    "minecraft:pink_petals",
    "minecraft:wildflowers",
    "minecraft:leaf_litter",
    "minecraft:bush",
    "minecraft:firefly_bush",
    "minecraft:short_dry_grass",
    "minecraft:tall_dry_grass",
    "minecraft:crimson_roots",
    "minecraft:warped_roots",
    "minecraft:nether_sprouts",
    "minecraft:lily_pad",
    "minecraft:big_dripleaf",
    "minecraft:small_dripleaf",
    "minecraft:azalea",
    "minecraft:flowering_azalea",
    "minecraft:spore_blossom",
    "minecraft:wither_rose",
];

/// Age-based crops outside `#minecraft:crops` that can be harvested.
const EXTRA_AGE_CROPS: &[&str] = &["minecraft:nether_wart", "minecraft:torchflower_crop"];

/// Crops in `#minecraft:crops` or `CROP_BLOCKS` that should not be right-click harvested.
const NON_HARVESTABLE_CROPS: &[&str] = &[
    "minecraft:melon_stem",
    "minecraft:pumpkin_stem",
    "minecraft:attached_melon_stem",
    "minecraft:attached_pumpkin_stem",
    "minecraft:pitcher_crop",
];

const SWEET_BERRY_BUSH: &str = "minecraft:sweet_berry_bush";
const COCOA: &str = "minecraft:cocoa";
const SINGLE_CROP_BLACKLIST: &str = "sunny_ultimine:single_crop_harvesting_blacklist";

fn in_tag(host: &dyn Host, state: BlockState, tag: &str) -> bool {
    host.block_in_tag(state.block(), tag).unwrap_or(false)
}

fn is_liquid(host: &dyn Host, state: BlockState) -> bool {
    host.state_fluid_state(state).is_some_and(|f| !f.is_empty())
        && host.state_outline(state).is_some_and(|o| o.is_empty())
}

/// Whether `state` is a plant (`VegetationBlock` or `CocoaBlock`).
pub fn looks_like_crop(host: &dyn Host, state: BlockState) -> bool {
    if in_tag(host, state, "minecraft:leaves") || is_liquid(host, state) {
        return false;
    }
    let id = state.block().id(host);
    id.is_some_and(|id| CROP_BLOCKS.contains(&id))
        || CROP_TAGS.iter().any(|t| in_tag(host, state, t))
}

/// The plant category of `state`, for the crop matcher.
pub fn bush_type(host: &dyn Host, state: BlockState) -> BushType {
    let id = state.block().id(host);
    if in_tag(host, state, "minecraft:crops") || id.is_some_and(|id| EXTRA_AGE_CROPS.contains(&id))
    {
        BushType::Crop
    } else if in_tag(host, state, "minecraft:saplings") {
        BushType::Sapling
    } else if id == Some(COCOA) {
        BushType::Cocoa
    } else if id == Some(SWEET_BERRY_BUSH) {
        BushType::SweetBerry
    } else {
        BushType::Other
    }
}

/// Whether `state` is a crop, cocoa pod or sweet berry bush at its highest `age`.
pub fn is_mature_harvestable(host: &dyn Host, state: BlockState) -> bool {
    if state
        .block()
        .id(host)
        .is_some_and(|id| NON_HARVESTABLE_CROPS.contains(&id))
    {
        return false;
    }
    if !matches!(
        bush_type(host, state),
        BushType::Crop | BushType::Cocoa | BushType::SweetBerry
    ) {
        return false;
    }
    let Some(definition) = host.block_states().definition(state.block()) else {
        return false;
    };
    let Some(max) = definition
        .iter()
        .find(|p| p.name == "age")
        .and_then(|p| p.values.iter().filter_map(|v| v.parse::<u32>().ok()).max())
    else {
        return false;
    };
    let states = host.block_states();
    state
        .property(&states, "age")
        .ok()
        .flatten()
        .and_then(|a| a.parse::<u32>().ok())
        == Some(max)
}

/// Harvests every mature crop among `positions`: rolls its drops, takes one seed back for the replant,
/// resets its age, and drops the merged loot beside `orig_pos` on `face`. Returns the number harvested.
pub fn harvest_crops_in_shape(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    orig_pos: [i32; 3],
    face: Face,
    positions: &[[i32; 3]],
) -> usize {
    // Keyed by index into `kinds`: `ItemStack` isn't `Hash`.
    let mut kinds: Vec<ItemStack> = Vec::new();
    let mut merged = MergedStacks::<usize>::new();
    let mut harvested = 0;

    for &pos in positions {
        let Some(block_state) = level.block_state(&*host, pos) else {
            continue;
        };
        if !is_mature_harvestable(&*host, block_state) {
            continue;
        }
        let sweet_berry = block_state.block().id(&*host) == Some(SWEET_BERRY_BUSH);
        let reset = {
            let states = host.block_states();
            let age: u32 = block_state
                .property(&states, "age")
                .ok()
                .flatten()
                .and_then(|a| a.parse().ok())
                .unwrap_or(0);
            let value = if sweet_berry && age >= 1 { "1" } else { "0" };
            block_state.with(&states, "age", value)
        };
        let Ok(new_state) = reset else {
            continue;
        };

        let mut drops = level
            .block_drops(host, pos, None, Some(player.entity()))
            .ok()
            .flatten()
            .unwrap_or_default();
        if !sweet_berry
            && let Some(i) = drops
                .iter()
                .position(|s| host.item_block(s.item()) == Some(block_state.block()))
        {
            let seed = drops.remove(i);
            if let Some(rest) = seed.shrink(1) {
                drops.insert(i, rest);
            }
        }

        if !level
            .set_block(
                host,
                pos,
                new_state,
                BlockUpdateFlags::NEIGHBORS | BlockUpdateFlags::CLIENTS,
            )
            .unwrap_or(false)
        {
            continue;
        }
        harvested += 1;

        for stack in drops {
            let count = stack.count();
            let key = match kinds.iter().position(|k| k.is_stackable_with(&stack)) {
                Some(k) => k,
                None => {
                    kinds.push(stack);
                    kinds.len() - 1
                }
            };
            merged.add(key, count);
        }
    }

    let drop_pos = offset(orig_pos, face.step());
    let split = merged.drain_split(|&k| host.max_stack_size(kinds[k].item()).unwrap_or(64));
    for (k, count) in split {
        let mut stack = kinds[k].clone();
        if stack.set_count(count).is_ok() {
            let _ = level.pop_resource(host, drop_pos, stack);
        }
    }
    harvested
}

/// The single-block harvest of a mature crop on a plain right-click (no Ultimine key).
pub fn try_single_crop_harvest(
    host: &mut dyn ServerHost,
    state: &ModState,
    player: PlayerRef,
    level: LevelRef,
    pos: [i32; 3],
    face: Face,
) -> bool {
    if !state.server().config.single_crop_harvesting {
        return false;
    }
    let Some(block_state) = level.block_state(&*host, pos) else {
        return false;
    };
    if in_tag(&*host, block_state, SINGLE_CROP_BLACKLIST)
        || !is_mature_harvestable(&*host, block_state)
    {
        return false;
    }
    harvest_crops_in_shape(host, player, level, pos, face, &[pos]) > 0
}
