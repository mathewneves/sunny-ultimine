//! Block tag discovery (server reload listener) and the cached [`TagIndex`] used by block matchers.

use std::sync::{Arc, PoisonError};

use mod_api::{
    BlockKey, FileToIdConverter, Host, Identifier, ItemStack, ReloadApply, ReloadResourceError,
    ServerReloadContext, ServerReloadListener, ServerReloadListeners,
};
use sunny_ultimine_core::matcher::{TagMatchSpec, resolve_tag_patterns};

use crate::state::ModState;

/// Block tag: when non-empty, only its members may be ultimined.
pub const BLOCK_WHITELIST: &str = "sunny_ultimine:block_whitelist";
/// Block tag: members are never ultimined.
pub const EXCLUDED_BLOCKS: &str = "sunny_ultimine:excluded_blocks";
/// Item tag: tools that may not be used for Ultimine in the main hand.
pub const EXCLUDED_TOOLS: &str = "sunny_ultimine:excluded_tools";
/// Item tag: tools that block Ultimine in either hand.
pub const EXCLUDED_TOOLS_STRICT: &str = "sunny_ultimine:excluded_tools/strict";
/// Item tag: items counted as tools when `require_tool` is on.
pub const INCLUDED_TOOLS: &str = "sunny_ultimine:included_tools";

/// Vanilla tags seeded into the known set in case the reload listing misses built-in tags.
const WELL_KNOWN_BLOCK_TAGS: &[&str] = &[
    "minecraft:base_stone_overworld",
    "minecraft:base_stone_nether",
    "minecraft:coal_ores",
    "minecraft:iron_ores",
    "minecraft:copper_ores",
    "minecraft:gold_ores",
    "minecraft:redstone_ores",
    "minecraft:emerald_ores",
    "minecraft:lapis_ores",
    "minecraft:diamond_ores",
    "minecraft:logs",
    "minecraft:logs_that_burn",
    "minecraft:leaves",
    "minecraft:planks",
    "minecraft:sand",
    "minecraft:dirt",
    "minecraft:terracotta",
    "minecraft:wool",
];

/// Lists every `data/*/tags/block/**.json` id on each data load.
struct TagDiscoveryListener(Arc<ModState>);

impl ServerReloadListener for TagDiscoveryListener {
    fn prepare(
        &self,
        context: &ServerReloadContext,
    ) -> Result<Box<dyn ReloadApply>, ReloadResourceError> {
        let names: Vec<String> = context.list().map(ToString::to_string).collect();
        let state = Arc::clone(&self.0);
        Ok(<dyn ReloadApply>::new(move || {
            *state
                .discovered_tags_inbox
                .lock()
                .unwrap_or_else(PoisonError::into_inner) = Some(names);
            mark_tags_dirty(&state);
        }))
    }
}

/// `Hooks::add_server_reload_listeners`: registers the block tag discovery listener.
pub fn register_reload_listener(state: &Arc<ModState>, listeners: &mut dyn ServerReloadListeners) {
    let id = Identifier::parse("sunny_ultimine:block_tags").expect("a valid id");
    let files = FileToIdConverter::json("tags/block").expect("a valid directory");
    listeners
        .add_listener(id, files, Arc::new(TagDiscoveryListener(Arc::clone(state))))
        .expect("a free listener name");
}

/// Forces the next [`ensure_tag_index`] to re-resolve (after a reload or a merge-tag config change).
pub fn mark_tags_dirty(state: &ModState) {
    Arc::make_mut(&mut state.server().tag_index).dirty = true;
}

fn split_spec(spec: TagMatchSpec) -> (bool, Vec<String>) {
    match spec {
        TagMatchSpec::AnyBlock => (true, Vec::new()),
        TagMatchSpec::Tags(tags) => (false, tags),
    }
}

/// Re-resolves the merge-tag patterns and whitelist state when the index is dirty.
pub fn ensure_tag_index(host: &dyn Host, state: &ModState) {
    let inbox = state
        .discovered_tags_inbox
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .take();
    let (merge_tags, merge_tags_shaped, mut known) = {
        let mut srv = state.server();
        if let Some(tags) = inbox {
            let index = Arc::make_mut(&mut srv.tag_index);
            index.discovered_block_tags = tags;
            index.dirty = true;
        }
        if !srv.tag_index.dirty {
            return;
        }
        (
            srv.config.merge_tags.clone(),
            srv.config.merge_tags_shaped.clone(),
            srv.tag_index.discovered_block_tags.clone(),
        )
    };
    known.extend(WELL_KNOWN_BLOCK_TAGS.iter().map(|t| (*t).to_owned()));

    let (shapeless_any, shapeless_tags) = split_spec(resolve_tag_patterns(&merge_tags, &known));
    let (shaped_any, shaped_tags) = split_spec(resolve_tag_patterns(&merge_tags_shaped, &known));
    let tags = host.tags();
    let whitelist = Identifier::parse(BLOCK_WHITELIST)
        .ok()
        .and_then(|id| tags.key(&id))
        .and_then(|k| tags.blocks(k));

    let mut srv = state.server();
    let index = Arc::make_mut(&mut srv.tag_index);
    index.shapeless_any = shapeless_any;
    index.shapeless_tags = shapeless_tags;
    index.shaped_any = shaped_any;
    index.shaped_tags = shaped_tags;
    index.whitelist_empty = whitelist.as_ref().is_none_or(Vec::is_empty);
    // Tags not loaded yet answer None: stay dirty so the whitelist is read again later.
    index.dirty = whitelist.is_none();
}

/// Whether `block` is in the block tag `tag_str` (no `#`); false when tags are unavailable.
pub fn block_in_tag_str(host: &dyn Host, block: BlockKey, tag_str: &str) -> bool {
    host.block_in_tag(block, tag_str).unwrap_or(false)
}

/// Whether `stack`'s item is in the item tag `tag_str` (no `#`); false when tags are unavailable.
pub fn item_in_tag_str(host: &dyn Host, stack: &ItemStack, tag_str: &str) -> bool {
    let tags = host.tags();
    Identifier::parse(tag_str)
        .ok()
        .and_then(|id| tags.key(&id))
        .and_then(|k| tags.is_in_tag(stack.item(), k))
        .unwrap_or(false)
}
