//! Shared state, constants, and core/API type conversions for Sunny Ultimine.

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

use mod_api::{BlockFace, ItemStack};
use sunny_ultimine_core::config::{ClientConfig, ServerConfig, SyncedServerConfig};
use sunny_ultimine_core::cooldown::CooldownStore;
use sunny_ultimine_core::outline::Edge;
use sunny_ultimine_core::shapes::{Face, ShapeId};
use sunny_ultimine_core::status::CanUltimineResult;

/// Mod identifier used for config registration, payload channels, attributes, and HUD layers.
pub const MOD_ID: &str = "sunny_ultimine";

/// Client config filename inside `config/`.
pub const CLIENT_CONFIG_FILE: &str = "sunny_ultimine-client.toml";

/// Server config filename inside `config/`.
pub const SERVER_CONFIG_FILE: &str = "sunny_ultimine-server.toml";

/// HUD GUI layer identifier.
pub const OVERLAY_LAYER_ID: &str = "sunny_ultimine:overlay";

/// Converts a Sunrise [`BlockFace`] to a core [`Face`].
pub const fn face_from_block_face(face: BlockFace) -> Face {
    match face {
        BlockFace::Down => Face::Down,
        BlockFace::Up => Face::Up,
        BlockFace::North => Face::North,
        BlockFace::South => Face::South,
        BlockFace::West => Face::West,
        BlockFace::East => Face::East,
        _ => Face::Up,
    }
}

/// Converts a core [`Face`] to a Sunrise [`BlockFace`].
pub const fn block_face_from_face(face: Face) -> BlockFace {
    match face {
        Face::Down => BlockFace::Down,
        Face::Up => BlockFace::Up,
        Face::North => BlockFace::North,
        Face::South => BlockFace::South,
        Face::West => BlockFace::West,
        Face::East => BlockFace::East,
    }
}

/// Converts a unit face normal `[i32; 3]` to a core [`Face`].
pub const fn face_from_normal(normal: [i32; 3]) -> Option<Face> {
    match normal {
        [0, -1, 0] => Some(Face::Down),
        [0, 1, 0] => Some(Face::Up),
        [0, 0, -1] => Some(Face::North),
        [0, 0, 1] => Some(Face::South),
        [-1, 0, 0] => Some(Face::West),
        [1, 0, 0] => Some(Face::East),
        _ => None,
    }
}

/// Converts Sunrise's `entity_direction` string (`"north"`, `"south"`, `"west"`, `"east"`) to [`Face`].
pub fn face_from_entity_dir(dir: &str) -> Face {
    Face::from_str_name(dir).unwrap_or(Face::North)
}

/// Current wall-clock epoch milliseconds for cooldown timestamps (each side uses only its own clock).
pub fn epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Per-player server-side Ultimine state.
#[derive(Clone, Debug)]
pub struct PlayerUltimineData {
    /// Whether the player is currently holding the Ultimine activation key.
    pub pressed: bool,
    /// Selected mining/action shape.
    pub shape: ShapeId,
    /// Fractional XP cost accumulated across multi-block breaks; whole points are deducted on tick.
    pub pending_xp_cost: f64,
    /// Last raycast target block used to compute `cached_blocks`.
    pub cached_pos: Option<[i32; 3]>,
    /// Last raycast hit face used to compute `cached_blocks`.
    pub cached_face: Option<Face>,
    /// Shape active when `cached_blocks` was computed.
    pub cached_shape: Option<ShapeId>,
    /// Cached block positions for the current selection.
    pub cached_blocks: Vec<[i32; 3]>,
}

impl Default for PlayerUltimineData {
    fn default() -> Self {
        Self {
            pressed: false,
            shape: ShapeId::Shapeless,
            pending_xp_cost: 0.0,
            cached_pos: None,
            cached_face: None,
            cached_shape: None,
            cached_blocks: Vec::new(),
        }
    }
}

impl PlayerUltimineData {
    /// Clears the cached target and selected block list.
    pub fn clear_cache(&mut self) {
        self.cached_pos = None;
        self.cached_face = None;
        self.cached_shape = None;
        self.cached_blocks.clear();
    }
}

/// Active server multi-block break session used for re-entrancy guarding and drop/XP consolidation.
#[derive(Debug)]
pub struct ActiveBreakSession {
    /// Breaking player's UUID; only drops whose `breaker == Some(player_uuid)` are absorbed.
    pub player_uuid: u128,
    /// Raw dropped stacks collected across all blocks broken in this operation.
    pub drops: Vec<ItemStack>,
    /// Total experience points accumulated across all broken blocks.
    pub xp_total: u32,
}

/// Cached resolved block tags used by [`BlockMatchers`] on the server.
#[derive(Clone, Debug)]
pub struct TagIndex {
    /// All block tag IDs discovered from data packs (e.g., `"minecraft:coal_ores"`).
    pub discovered_block_tags: Vec<String>,
    /// True if `merge_tags` contains `"*"` (match any non-air/non-liquid/non-BE block).
    pub shapeless_any: bool,
    /// Concrete tag IDs resolved from `merge_tags`.
    pub shapeless_tags: Vec<String>,
    /// True if `merge_tags_shaped` contains `"*"`.
    pub shaped_any: bool,
    /// Concrete tag IDs resolved from `merge_tags_shaped`.
    pub shaped_tags: Vec<String>,
    /// True when `#sunny_ultimine:block_whitelist` has no members (meaning all blocks allowed).
    pub whitelist_empty: bool,
    /// True when the tag index needs refreshing against current server config & tags.
    pub dirty: bool,
}

impl Default for TagIndex {
    fn default() -> Self {
        Self {
            discovered_block_tags: Vec::new(),
            shapeless_any: false,
            shapeless_tags: Vec::new(),
            shaped_any: true,
            shaped_tags: Vec::new(),
            whitelist_empty: true,
            dirty: true,
        }
    }
}

/// Server-side mod state.
#[derive(Debug, Default)]
pub struct ServerState {
    /// Server configuration loaded from `sunny_ultimine-server.toml` and modified via `/sunny_ultimine config`.
    pub config: ServerConfig,
    /// Per-player runtime data keyed by UUID.
    pub players: HashMap<u128, PlayerUltimineData>,
    /// Last Ultimine timestamp in epoch milliseconds per player UUID.
    pub cooldowns: CooldownStore<u128>,
    /// Resolved block tag cache for shape matching and whitelist checks.
    pub tag_index: Arc<TagIndex>,
    /// When set, `server_tick` broadcasts updated cooldown & config values to all connected players.
    pub sync_needed: bool,
}

/// A single rendered line in the client HUD overlay panel.
#[derive(Clone, Debug, PartialEq)]
pub struct HudLine {
    /// Left indentation in unscaled GUI pixels.
    pub indent: f32,
    /// Ordered colored text spans `(text, 0x00RRGGBB)`.
    pub segments: Vec<(String, u32)>,
}

/// Client-side Ultimine UI, input, and preview state.
#[derive(Debug)]
pub struct ClientState {
    /// Client settings loaded from `sunny_ultimine-client.toml` and Options > Mods.
    pub config: ClientConfig,
    /// Server settings subset synced from the server.
    pub server_config: SyncedServerConfig,
    /// Whether the Ultimine activation key is currently held down.
    pub ultimine_key_pressed: bool,
    /// Whether Ultimine is currently allowed at the targeted block.
    pub can_ultimine: bool,
    /// Detailed status / refusal reason shown in the HUD panel.
    pub can_ultimine_status: CanUltimineResult,
    /// Active shape mirrored from the server.
    pub shape: ShapeId,
    /// Blocks currently selected for outline rendering (capped to `config.render_outline`).
    pub shape_blocks: Vec<[i32; 3]>,
    /// Total count of blocks selected on the server before outline truncation.
    pub actual_blocks: usize,
    /// Cached merged silhouette edges for `shape_blocks`.
    pub cached_edges: Vec<Edge>,
    /// True when `shape_blocks` or `render_outline` changed and `cached_edges` must be rebuilt.
    pub edges_dirty: bool,
    /// Whether the player has cycled shapes at least once this session (suppresses first-press hint).
    pub has_scrolled_yet: bool,
    /// Millisecond timestamp of last keyboard shape cycle for 125ms debounce.
    pub last_cycle_ms: u64,
    /// Smoothly interpolated HUD panel opacity multiplier in `0.0..=1.0`.
    pub panel_alpha_mult: f32,
    /// Sticky minimum panel width in unscaled GUI pixels (prevents jitter as lines change).
    pub max_panel_width: f32,
    /// Prebuilt HUD panel lines for the current frame.
    pub info_panel_lines: Vec<HudLine>,
    /// Effective cooldown ticks synced from the server (or server config).
    pub cooldown_ticks: u64,
    /// Client epoch ms when the server-reported cooldown ends; `0` means ready.
    pub cooldown_ready_at_ms: u64,
    /// Optional timed hotbar/overlay hint `(message, expires_at_millis)`.
    pub hint_text: Option<(String, u64)>,
}

impl Default for ClientState {
    fn default() -> Self {
        Self {
            config: ClientConfig::default(),
            server_config: SyncedServerConfig::default(),
            ultimine_key_pressed: false,
            can_ultimine: false,
            can_ultimine_status: CanUltimineResult::NoBlockTargeted,
            shape: ShapeId::Shapeless,
            shape_blocks: Vec::new(),
            actual_blocks: 0,
            cached_edges: Vec::new(),
            edges_dirty: false,
            has_scrolled_yet: false,
            last_cycle_ms: 0,
            panel_alpha_mult: 0.0,
            max_panel_width: 100.0,
            info_panel_lines: Vec::new(),
            cooldown_ticks: 0,
            cooldown_ready_at_ms: 0,
            hint_text: None,
        }
    }
}

impl ClientState {
    /// Clears transient per-world state on disconnect while keeping user config intact.
    pub fn reset_world_state(&mut self) {
        self.server_config = SyncedServerConfig::default();
        self.ultimine_key_pressed = false;
        self.can_ultimine = false;
        self.can_ultimine_status = CanUltimineResult::NoBlockTargeted;
        self.shape = ShapeId::Shapeless;
        self.shape_blocks.clear();
        self.actual_blocks = 0;
        self.cached_edges.clear();
        self.edges_dirty = false;
        self.has_scrolled_yet = false;
        self.last_cycle_ms = 0;
        self.panel_alpha_mult = 0.0;
        self.max_panel_width = 100.0;
        self.info_panel_lines.clear();
        self.cooldown_ticks = 0;
        self.cooldown_ready_at_ms = 0;
        self.hint_text = None;
    }
}

/// Top-level thread-safe state shared across all Sunrise hooks and payload handlers.
#[derive(Default)]
pub struct ModState {
    /// Server-side state (config, players, cooldowns, tag index).
    pub server: Mutex<ServerState>,
    /// Separate lock for the active multi-block break session; never held across host calls.
    pub active_break: Mutex<Option<ActiveBreakSession>>,
    /// Re-entrancy guard for multi-block right-click tool operations.
    pub right_click_reentrant: AtomicBool,
    /// Allows fake players during explicit `/sunny_ultimine_dev` headless server test commands only.
    #[cfg(feature = "dev")]
    pub dev_allow_fake: AtomicBool,
    /// Client-side state (config, input, HUD, outline cache).
    pub client: Mutex<ClientState>,
    /// Inbox populated by the server reload listener with discovered `tags/block` IDs.
    pub discovered_tags_inbox: Arc<Mutex<Option<Vec<String>>>>,
}

impl ModState {
    /// Locks [`Self::server`], recovering cleanly from mutex poisoning.
    pub fn server(&self) -> MutexGuard<'_, ServerState> {
        self.server.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Locks [`Self::client`], recovering cleanly from mutex poisoning.
    pub fn client(&self) -> MutexGuard<'_, ClientState> {
        self.client.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Locks [`Self::active_break`], recovering cleanly from mutex poisoning.
    pub fn active_break(&self) -> MutexGuard<'_, Option<ActiveBreakSession>> {
        self.active_break
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}
