//! Player attributes that modify Ultimine's server config values, and their effective values.

use mod_api::{AttributeTooltip, Host, PlayerRef, Registrar, Sentiment};
use sunny_ultimine_core::config::ServerConfig;

/// Added to `max_blocks`.
pub const ATTR_MAX_BLOCKS: &str = "sunny_ultimine:max_blocks_modifier";
/// Added to `ultimine_cooldown` (ticks).
pub const ATTR_COOLDOWN: &str = "sunny_ultimine:cooldown_modifier";
/// Added to `exhaustion_per_block`.
pub const ATTR_EXHAUSTION: &str = "sunny_ultimine:exhaustion_modifier";
/// Added to `experience_per_block`.
pub const ATTR_EXPERIENCE: &str = "sunny_ultimine:experience_modifier";

/// `(id, min, max, sentiment)` per attribute; all default to 0.
const ATTRIBUTES: [(&str, f64, f64, Sentiment); 4] = [
    (ATTR_MAX_BLOCKS, -32768.0, 32768.0, Sentiment::Positive),
    (
        ATTR_COOLDOWN,
        -1_000_000.0,
        1_000_000.0,
        Sentiment::Negative,
    ),
    (ATTR_EXHAUSTION, -10_000.0, 10_000.0, Sentiment::Negative),
    (ATTR_EXPERIENCE, -20_000.0, 20_000.0, Sentiment::Negative),
];

/// Declares the four modifier attributes and adds them to the player's default set.
pub fn register_attributes(r: &mut dyn Registrar) {
    for (id, min, max, sentiment) in ATTRIBUTES {
        let path = id.strip_prefix("sunny_ultimine:").expect("own namespace");
        // A refusal is recorded by the registrar and refuses the mod load, naming it.
        let _ = r.declare_attribute(
            path,
            0.0,
            min,
            max,
            true,
            sentiment,
            AttributeTooltip::default(),
        );
    }
    let _ = r.modify_entity_attributes(
        "minecraft:player",
        ATTRIBUTES
            .iter()
            .map(|(id, ..)| ((*id).to_owned(), None))
            .collect(),
    );
}

fn attr(host: &dyn Host, player: PlayerRef, id: &str) -> f64 {
    host.living(player.entity())
        .and_then(|l| l.attribute_value(host, id))
        .unwrap_or(0.0)
}

/// `max_blocks` plus the player's modifier, at least 0.
pub fn effective_max_blocks(host: &dyn Host, player: PlayerRef, cfg: &ServerConfig) -> usize {
    (f64::from(cfg.max_blocks) + attr(host, player, ATTR_MAX_BLOCKS))
        .round()
        .max(0.0) as usize
}

/// `ultimine_cooldown` ticks plus the player's modifier, at least 0.
pub fn effective_cooldown(host: &dyn Host, player: PlayerRef, cfg: &ServerConfig) -> u64 {
    (cfg.ultimine_cooldown as f64 + attr(host, player, ATTR_COOLDOWN))
        .round()
        .max(0.0) as u64
}

/// `exhaustion_per_block` plus the player's modifier, at least 0.
pub fn effective_exhaustion(host: &dyn Host, player: PlayerRef, cfg: &ServerConfig) -> f64 {
    (cfg.exhaustion_per_block + attr(host, player, ATTR_EXHAUSTION)).max(0.0)
}

/// `experience_per_block` plus the player's modifier, at least 0.
pub fn effective_experience(host: &dyn Host, player: PlayerRef, cfg: &ServerConfig) -> f64 {
    (cfg.experience_per_block + attr(host, player, ATTR_EXPERIENCE)).max(0.0)
}
