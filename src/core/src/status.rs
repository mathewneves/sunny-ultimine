//! Result of checking whether a player may ultimine right now.

/// Outcome of the "can this player ultimine?" check, as a status code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CanUltimineResult {
    /// Ultimining is allowed.
    Allowed,
    /// The player is not looking at a block.
    NoBlockTargeted,
    /// The player does not have enough food.
    NoFood,
    /// The player does not have enough experience.
    NoExperience,
    /// A tool is required but none is held.
    NoTool,
    /// The held tool is on the deny list.
    BlockedTool,
    /// Ultimine was used too recently.
    OnCooldown,
    /// Some other restriction (claims, permissions, ...) blocks the action.
    OtherRestriction,
}

impl CanUltimineResult {
    /// Whether this result permits ultimining.
    pub const fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed)
    }

    /// Translation key shown in the overlay. `Allowed` maps to the "no valid block" message,
    /// which is what the overlay shows when the check passed but the shape found nothing.
    pub const fn translation_key(self) -> &'static str {
        match self {
            Self::Allowed => "sunny_ultimine.info.no_valid_block",
            Self::NoBlockTargeted => "sunny_ultimine.info.no_block_targeted",
            Self::NoFood => "sunny_ultimine.info.no_food",
            Self::NoExperience => "sunny_ultimine.info.no_experience",
            Self::NoTool => "sunny_ultimine.info.no_tool",
            Self::BlockedTool => "sunny_ultimine.info.denied_tool",
            Self::OnCooldown => "sunny_ultimine.info.cooldown",
            Self::OtherRestriction => "sunny_ultimine.info.other_restriction",
        }
    }

    /// English text for [`Self::translation_key`], matching `en_us.json`.
    pub const fn fallback_english(self) -> &'static str {
        match self {
            Self::Allowed => "Not a valid block",
            Self::NoBlockTargeted => "No block targeted",
            Self::NoFood => "Too Hungry",
            Self::NoExperience => "Not enough XP",
            Self::NoTool => "No valid tool",
            Self::BlockedTool => "Tool is not allowed",
            Self::OnCooldown => "on cooldown",
            Self::OtherRestriction => "Other Restriction",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::CanUltimineResult::{self, *};

    const ALL: [CanUltimineResult; 8] = [
        Allowed,
        NoBlockTargeted,
        NoFood,
        NoExperience,
        NoTool,
        BlockedTool,
        OnCooldown,
        OtherRestriction,
    ];

    #[test]
    fn only_allowed_is_allowed() {
        for r in ALL {
            assert_eq!(r.is_allowed(), r == Allowed);
        }
    }

    #[test]
    fn keys_and_text_are_distinct_and_nonempty() {
        let keys: std::collections::HashSet<_> = ALL.iter().map(|r| r.translation_key()).collect();
        assert_eq!(keys.len(), ALL.len());
        for r in ALL {
            assert!(r.translation_key().starts_with("sunny_ultimine.info."));
            assert!(!r.fallback_english().is_empty());
        }
        assert_eq!(
            BlockedTool.translation_key(),
            "sunny_ultimine.info.denied_tool"
        );
        assert_eq!(OnCooldown.translation_key(), "sunny_ultimine.info.cooldown");
    }
}
