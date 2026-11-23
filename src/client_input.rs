//! Client keybindings, input handling, the client-side "can ultimine" check and HUD line building.

use mod_api::{
    ClientHost, Flow, Hand, HostBlockStates, Identifier, ItemStack, KeyConflictContext,
    KeyRegistration, MouseScrollingEvent, PlayerRef, Registrar,
};
use sunny_ultimine_core::config::{ClientConfig, SyncedServerConfig};
use sunny_ultimine_core::cooldown::is_on_cooldown_until;
use sunny_ultimine_core::shapes::ShapeId;
use sunny_ultimine_core::status::CanUltimineResult;

use crate::net;
use crate::state::{HudLine, MOD_ID, ModState, epoch_millis};

/// Activation key id (hold to ultimine).
pub const KEY_ULTIMINE: &str = "ultimine";
/// Cycle-to-next-shape key id.
pub const KEY_NEXT_SHAPE: &str = "next_shape";
/// Cycle-to-previous-shape key id.
pub const KEY_PREV_SHAPE: &str = "prev_shape";
/// Minimum milliseconds between two keyboard shape cycles.
pub const INPUT_DELAY_MS: u64 = 125;

const HINT_MS: u64 = 3000;
const MENU_INDENT: f32 = 16.0;
const REASON_INDENT: f32 = 8.0;

const C_TEXT: u32 = 0xECEFF4;
const C_COOLDOWN: u32 = 0xBFBF6C;
const C_ACTIVE: u32 = 0xA3BE8C;
const C_INACTIVE: u32 = 0xBF616A;
const C_GRAY: u32 = 0xAAAAAA;
const C_SELECTED: u32 = 0xFFFF55;

/// Registers the three Ultimine key mappings on Options > Controls.
pub fn register_key_mappings(r: &mut dyn Registrar) {
    for (id, key) in [
        (KEY_ULTIMINE, "key.keyboard.grave.accent"),
        (KEY_NEXT_SHAPE, "key.keyboard.up"),
        (KEY_PREV_SHAPE, "key.keyboard.down"),
    ] {
        r.register_key_mapping(
            KeyRegistration::new(id, key)
                .label(&format!("key.sunny_ultimine.{id}"))
                .category("key.category.sunny_ultimine.default")
                .context(KeyConflictContext::InGame),
        );
    }
}

/// Whether the shape menu is "sneaking": always when the config doesn't require it, otherwise
/// Shift held (Control when the Ultimine key itself is bound to a Shift key).
pub fn is_menu_sneaking(host: &dyn ClientHost, cfg: &ClientConfig) -> bool {
    if !cfg.require_sneak_for_menu {
        return true;
    }
    let shift_bound = host.mod_key(MOD_ID, KEY_ULTIMINE).is_some_and(|(key, _)| {
        key == "key.keyboard.left.shift" || key == "key.keyboard.right.shift"
    });
    if shift_bound {
        host.modifiers().control()
    } else {
        host.modifiers().shift()
    }
}

fn tr(host: &dyn ClientHost, key: &str, fallback: &str) -> String {
    host.translate(key).unwrap_or_else(|| fallback.to_owned())
}

/// Per-tick client input, status check and HUD line rebuild.
pub fn on_client_tick_post(host: &mut dyn ClientHost, state: &ModState) {
    if !host.playing() || host.local_player().is_none() {
        let was_pressed = {
            let mut cli = state.client();
            let was = cli.ultimine_key_pressed;
            if was {
                cli.ultimine_key_pressed = false;
                cli.can_ultimine = false;
                clear_preview(&mut cli);
            }
            was
        };
        if was_pressed {
            net::send_key_pressed(host, false);
        }
        return;
    }

    let now = host.millis();
    let pressed = host.is_down(MOD_ID, KEY_ULTIMINE);
    let (was_pressed, cfg, server_cfg, scrolled, last_cycle_ms, cooldown_ready_at_ms) = {
        let cli = state.client();
        (
            cli.ultimine_key_pressed,
            cli.config.clone(),
            cli.server_config,
            cli.has_scrolled_yet,
            cli.last_cycle_ms,
            cli.cooldown_ready_at_ms,
        )
    };

    if pressed != was_pressed {
        net::send_key_pressed(host, pressed);
    }
    let hint = (pressed && !was_pressed && !scrolled).then(|| {
        if cfg.require_sneak_for_menu {
            tr(
                host,
                "sunny_ultimine.change_shape",
                "Hold Shift + (Scroll Mouse or Up/Down keys) to change shape",
            )
        } else {
            tr(
                host,
                "sunny_ultimine.change_shape.no_shift",
                "Scroll Mouse or Up/Down keys to change shape",
            )
        }
    });

    let sneaking = is_menu_sneaking(host, &cfg);
    let mut cycled = false;
    if now.saturating_sub(last_cycle_ms) >= INPUT_DELAY_MS
        && (pressed || !cfg.require_ultimine_key_for_cycling)
        && sneaking
    {
        let dir = if host.is_down(MOD_ID, KEY_NEXT_SHAPE) {
            Some(true)
        } else if host.is_down(MOD_ID, KEY_PREV_SHAPE) {
            Some(false)
        } else {
            None
        };
        if let Some(next) = dir {
            net::send_mode_changed(host, next);
            cycled = true;
        }
    }

    // Status is only shown while the key is held; the fading panel keeps its last lines.
    let status = pressed.then(|| can_ultimine_status(host, &server_cfg, cooldown_ready_at_ms));
    let texts = pressed.then(|| PanelTexts::new(host));

    let mut cli = state.client();
    cli.ultimine_key_pressed = pressed;
    if !pressed && was_pressed {
        clear_preview(&mut cli);
    }
    if let Some(text) = hint {
        cli.hint_text = Some((text, now.saturating_add(HINT_MS)));
    } else if cli
        .hint_text
        .as_ref()
        .is_some_and(|&(_, expires)| now >= expires)
    {
        cli.hint_text = None;
    }
    if cycled {
        cli.last_cycle_ms = now;
        cli.has_scrolled_yet = true;
    }
    if let Some(status) = status {
        cli.can_ultimine_status = status;
    }
    cli.can_ultimine = pressed && cli.can_ultimine_status.is_allowed();
    cli.panel_alpha_mult = if pressed {
        (cli.panel_alpha_mult + 0.1).min(1.0)
    } else {
        (cli.panel_alpha_mult - 0.1).max(0.0)
    };
    if let Some(texts) = texts {
        let lines = build_panel_lines(
            &texts,
            cli.can_ultimine_status,
            cli.can_ultimine,
            cli.shape,
            sneaking,
            cfg.shape_menu_context_lines,
            cli.actual_blocks,
            cli.shape_blocks.len(),
        );
        cli.info_panel_lines = lines;
    }
}

fn clear_preview(cli: &mut crate::state::ClientState) {
    cli.shape_blocks.clear();
    cli.actual_blocks = 0;
    cli.cached_edges.clear();
    cli.edges_dirty = false;
}

/// Cycles the shape with the scroll wheel while the Ultimine key is held and the menu is up.
pub fn on_mouse_scrolling(
    host: &mut dyn ClientHost,
    state: &ModState,
    event: &mut MouseScrollingEvent,
) -> Flow {
    if !host.playing() || (event.scroll[0] == 0.0 && event.scroll[1] == 0.0) {
        return Flow::Continue;
    }
    let (pressed, cfg) = {
        let cli = state.client();
        (cli.ultimine_key_pressed, cli.config.clone())
    };
    if !pressed || !is_menu_sneaking(host, &cfg) {
        return Flow::Continue;
    }
    net::send_mode_changed(host, event.scroll[0] < 0.0 || event.scroll[1] < 0.0);
    state.client().has_scrolled_yet = true;
    Flow::Cancel
}

/// Drops per-world client state when leaving a world.
pub fn on_client_disconnecting(state: &ModState) {
    state.client().reset_world_state();
}

fn item_tag(host: &dyn ClientHost, stack: Option<&ItemStack>, tag: &str) -> bool {
    let Some(stack) = stack else { return false };
    let tags = host.tags();
    Identifier::parse(tag)
        .ok()
        .and_then(|id| tags.key(&id))
        .and_then(|key| tags.is_in_tag(stack.item(), key))
        .unwrap_or(false)
}

fn is_blocked_tool(
    host: &dyn ClientHost,
    main: Option<&ItemStack>,
    off: Option<&ItemStack>,
) -> bool {
    item_tag(host, main, "sunny_ultimine:excluded_tools/strict")
        || item_tag(host, off, "sunny_ultimine:excluded_tools/strict")
        || item_tag(host, main, "sunny_ultimine:excluded_tools")
}

fn is_valid_client_tool(host: &dyn ClientHost, stack: &ItemStack) -> bool {
    stack
        .components()
        .is_some_and(|c| c.as_nbt().contains_key(&"minecraft:tool".into()))
        || host
            .item_default_component(stack.item(), "minecraft:tool")
            .is_some()
        || host.max_damage(stack.item()).unwrap_or(0) > 0
        || item_tag(host, Some(stack), "sunny_ultimine:included_tools")
}

/// The local player's Ultimine status, mirroring the server's checks with synced rules.
fn can_ultimine_status(
    host: &dyn ClientHost,
    server_cfg: &SyncedServerConfig,
    cooldown_ready_at_ms: u64,
) -> CanUltimineResult {
    let Some(player) = host.local_player() else {
        return CanUltimineResult::NoBlockTargeted;
    };
    // `cooldown_ready_at_ms` is on the client's epoch clock, not the game's `millis()`.
    if is_on_cooldown_until(epoch_millis(), cooldown_ready_at_ms) {
        return CanUltimineResult::OnCooldown;
    }
    let creative = player.is_creative(host).unwrap_or(false);
    if !creative && player.food(host).is_some_and(|f| f.level <= 0) {
        return CanUltimineResult::NoFood;
    }
    if !creative
        && server_cfg.experience_per_block > 0.0
        && player.experience(host).is_some_and(|xp| {
            (sunny_ultimine_core::xp::player_xp(xp.level, xp.progress) as f64)
                < server_cfg.experience_per_block
        })
    {
        return CanUltimineResult::NoExperience;
    }
    let Some(living) = host.living(player.entity()) else {
        return CanUltimineResult::NoBlockTargeted;
    };
    let main = living.hand_item(host, Hand::Main);
    let off = living.hand_item(host, Hand::Off);
    if is_blocked_tool(host, main.as_ref(), off.as_ref()) {
        return CanUltimineResult::BlockedTool;
    }
    let reach = living
        .attribute_value(host, "minecraft:block_interaction_range")
        .unwrap_or(4.5)
        + if creative { 0.5 } else { 0.0 };
    let Some(hit) = host.raycast(reach as f32) else {
        return CanUltimineResult::NoBlockTargeted;
    };
    if server_cfg.require_tool && !main.as_ref().is_some_and(|s| is_valid_client_tool(host, s)) {
        return CanUltimineResult::NoTool;
    }
    if server_cfg.require_valid_tool_for_block
        && !has_correct_tool(host, player, main.as_ref(), hit)
    {
        return CanUltimineResult::NoTool;
    }
    CanUltimineResult::Allowed
}

/// Client-side `Player.hasCorrectToolForDrops`: the block needs no tool, or the stack is right for it.
fn has_correct_tool(
    host: &dyn ClientHost,
    player: PlayerRef,
    main: Option<&ItemStack>,
    hit: mod_api::Hit,
) -> bool {
    let pos = [hit.block_x, hit.block_y, hit.block_z];
    let Some(block) = player
        .entity()
        .level(host)
        .and_then(|l| l.block_state(host, pos))
    else {
        return true;
    };
    let facts = host.block_states();
    if !facts.requires_correct_tool(block).unwrap_or(false) {
        return true;
    }
    main.is_some_and(|s| facts.is_correct_tool_for_drops(s, block).unwrap_or(false))
}

/// Translated strings for the HUD panel, read before the client lock is taken.
struct PanelTexts {
    base: String,
    cooldown: String,
    active: String,
    not_active: String,
    reasons: Vec<(CanUltimineResult, String)>,
    shapes: Vec<String>,
    hold_shift: String,
    blocks: String,
    partial: String,
}

impl PanelTexts {
    fn new(host: &dyn ClientHost) -> Self {
        use CanUltimineResult::*;
        Self {
            base: tr(host, "sunny_ultimine.info.base", "Sunny Ultimine is %s"),
            cooldown: tr(
                host,
                OnCooldown.translation_key(),
                OnCooldown.fallback_english(),
            ),
            active: tr(host, "sunny_ultimine.info.active", "active"),
            not_active: tr(host, "sunny_ultimine.info.not_active", "not active"),
            reasons: [
                Allowed,
                NoBlockTargeted,
                NoFood,
                NoExperience,
                NoTool,
                BlockedTool,
                OnCooldown,
                OtherRestriction,
            ]
            .into_iter()
            .map(|r| (r, tr(host, r.translation_key(), r.fallback_english())))
            .collect(),
            shapes: ShapeId::ALL
                .iter()
                .map(|s| tr(host, s.translation_key(), s.english_name()))
                .collect(),
            hold_shift: tr(
                host,
                "sunny_ultimine.change_shape.short",
                "Hold Shift for menu",
            ),
            blocks: tr(host, "sunny_ultimine.info.blocks", "Mining %d blocks"),
            partial: tr(host, "sunny_ultimine.info.partial_render", "%d rendered"),
        }
    }

    fn reason(&self, r: CanUltimineResult) -> String {
        self.reasons
            .iter()
            .find(|(k, _)| *k == r)
            .map_or_else(|| r.fallback_english().to_owned(), |(_, t)| t.clone())
    }
}

fn line(indent: f32, segments: Vec<(String, u32)>) -> HudLine {
    HudLine { indent, segments }
}

fn blank() -> HudLine {
    line(0.0, Vec::new())
}

fn format_count(template: &str, count: usize) -> String {
    let n = count.to_string();
    template.replace("%d", &n).replace("%s", &n)
}

/// Builds the HUD panel lines: status, reason, shape menu and block count.
#[allow(clippy::too_many_arguments)]
fn build_panel_lines(
    t: &PanelTexts,
    status: CanUltimineResult,
    can_ultimine: bool,
    shape: ShapeId,
    showing_menu: bool,
    context_lines: u8,
    actual_blocks: usize,
    rendered_blocks: usize,
) -> Vec<HudLine> {
    let on_cooldown = status == CanUltimineResult::OnCooldown;
    let (word, color) = if on_cooldown {
        (&t.cooldown, C_COOLDOWN)
    } else if can_ultimine {
        (&t.active, C_ACTIVE)
    } else {
        (&t.not_active, C_INACTIVE)
    };
    let (before, after) = t.base.split_once("%s").unwrap_or((&t.base, ""));
    let mut segments = vec![(before.to_owned(), C_TEXT), (word.clone(), color)];
    if !after.is_empty() {
        segments.push((after.to_owned(), C_TEXT));
    }
    let mut lines = vec![line(0.0, segments)];
    if !can_ultimine && !on_cooldown {
        lines.push(line(REASON_INDENT, vec![(t.reason(status), C_INACTIVE)]));
    }

    let n = ShapeId::ALL.len();
    let idx = shape.index();
    if showing_menu {
        if can_ultimine {
            lines.push(blank());
        }
        let context = ((n - 1) / 2).min(usize::from(context_lines));
        for i in (1..=context).rev() {
            lines.push(line(
                MENU_INDENT,
                vec![(t.shapes[(idx + n - i) % n].clone(), C_GRAY)],
            ));
        }
        lines.push(line(MENU_INDENT, vec![(t.shapes[idx].clone(), C_SELECTED)]));
        for i in 1..=context {
            lines.push(line(
                MENU_INDENT,
                vec![(t.shapes[(idx + i) % n].clone(), C_GRAY)],
            ));
        }
    } else {
        lines.push(line(0.0, vec![(t.hold_shift.clone(), C_GRAY)]));
        lines.push(blank());
        lines.push(line(0.0, vec![(t.shapes[idx].clone(), C_SELECTED)]));
    }

    if can_ultimine && actual_blocks > 0 {
        lines.push(blank());
        let mut text = format_count(&t.blocks, actual_blocks);
        if actual_blocks > rendered_blocks {
            text.push_str(", ");
            text.push_str(&format_count(&t.partial, rendered_blocks));
        }
        lines.push(line(0.0, vec![(text, C_ACTIVE)]));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts() -> PanelTexts {
        PanelTexts {
            base: "Sunny Ultimine is %s".into(),
            cooldown: "on cooldown".into(),
            active: "active".into(),
            not_active: "not active".into(),
            reasons: Vec::new(),
            shapes: ShapeId::ALL
                .iter()
                .map(|s| s.english_name().to_owned())
                .collect(),
            hold_shift: "Hold Shift for menu".into(),
            blocks: "Mining %d blocks".into(),
            partial: "%d rendered".into(),
        }
    }

    fn text(l: &HudLine) -> String {
        l.segments.iter().map(|(s, _)| s.as_str()).collect()
    }

    #[test]
    fn active_menu_layout() {
        let shape = ShapeId::ALL[0];
        let lines = build_panel_lines(
            &texts(),
            CanUltimineResult::Allowed,
            true,
            shape,
            true,
            5,
            10,
            4,
        );
        assert_eq!(text(&lines[0]), "Sunny Ultimine is active");
        assert!(lines[1].segments.is_empty());
        // Context capped at (6 - 1) / 2 = 2 above and below the selection.
        let menu: Vec<_> = lines.iter().filter(|l| l.indent == MENU_INDENT).collect();
        assert_eq!(menu.len(), 5);
        assert_eq!(
            menu[2].segments[0],
            (shape.english_name().to_owned(), C_SELECTED)
        );
        assert_eq!(
            text(menu[1]),
            ShapeId::ALL[ShapeId::ALL.len() - 1].english_name()
        );
        assert_eq!(text(lines.last().unwrap()), "Mining 10 blocks, 4 rendered");
    }

    #[test]
    fn inactive_without_menu() {
        let lines = build_panel_lines(
            &texts(),
            CanUltimineResult::NoFood,
            false,
            ShapeId::ALL[1],
            false,
            2,
            0,
            0,
        );
        assert_eq!(text(&lines[0]), "Sunny Ultimine is not active");
        assert_eq!(lines[1].indent, REASON_INDENT);
        assert_eq!(text(&lines[1]), "Too Hungry");
        assert_eq!(text(&lines[2]), "Hold Shift for menu");
        assert_eq!(text(&lines[4]), ShapeId::ALL[1].english_name());
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn cooldown_has_no_reason_line() {
        let lines = build_panel_lines(
            &texts(),
            CanUltimineResult::OnCooldown,
            false,
            ShapeId::ALL[0],
            true,
            1,
            0,
            0,
        );
        assert_eq!(text(&lines[0]), "Sunny Ultimine is on cooldown");
        assert_eq!(lines.iter().filter(|l| l.indent == MENU_INDENT).count(), 3);
        assert_eq!(lines.len(), 4);
    }

    #[test]
    fn supports_s_count_placeholders() {
        let mut t = texts();
        t.blocks = "Mining %s blocks".into();
        t.partial = "%s rendered".into();
        let lines = build_panel_lines(
            &t,
            CanUltimineResult::Allowed,
            true,
            ShapeId::ALL[0],
            false,
            2,
            12,
            5,
        );
        assert_eq!(text(lines.last().unwrap()), "Mining 12 blocks, 5 rendered");
    }
}
