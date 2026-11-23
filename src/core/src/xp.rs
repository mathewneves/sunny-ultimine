//! Vanilla experience math.

/// XP needed to go from `level` to `level + 1` (vanilla `getXpNeededForNextLevel`).
pub fn xp_bar_cap(level: i32) -> i32 {
    if level >= 30 {
        112 + (level - 30) * 9
    } else if level >= 15 {
        37 + (level - 15) * 5
    } else {
        7 + level.max(0) * 2
    }
}

/// Sum of the arithmetic series `a0, a0 + d, ...` of `n` terms.
fn sum(n: i32, a0: i32, d: i32) -> i64 {
    let n = i64::from(n);
    n * (2 * i64::from(a0) + (n - 1) * i64::from(d)) / 2
}

/// Total XP at the start of `level`.
pub fn experience_for_level(level: i32) -> i64 {
    if level <= 0 {
        0
    } else if level <= 15 {
        sum(level, 7, 2)
    } else if level <= 30 {
        315 + sum(level - 15, 37, 5)
    } else {
        1395 + sum(level - 30, 112, 9)
    }
}

/// Level reached with `target_xp` total XP.
// shortcut: linear in the resulting level (~22k steps at i32::MAX XP), switch to a closed form if callers pass larger totals.
pub fn level_for_experience(mut target_xp: i64) -> i32 {
    let mut level = 0;
    loop {
        let cap = i64::from(xp_bar_cap(level));
        if target_xp < cap {
            return level;
        }
        target_xp -= cap;
        level += 1;
    }
}

/// Total XP of a player at `level` with bar `progress` (computed in `f32` like Java).
pub fn player_xp(level: i32, progress: f32) -> i64 {
    (experience_for_level(level) as f32 + progress * xp_bar_cap(level) as f32) as i64
}

/// Player XP state after a change.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XpUpdate {
    /// New `experienceLevel`.
    pub level: i32,
    /// New `experienceProgress` in `[0, 1)`.
    pub progress: f32,
    /// New `totalExperience`.
    pub total_experience: i32,
}

/// Adds `delta` XP (negative to remove) to a player at `level` / `progress`.
/// The new total is clamped to `0..=i32::MAX`.
pub fn apply_xp_delta(level: i32, progress: f32, delta: i64) -> XpUpdate {
    let total = player_xp(level, progress)
        .saturating_add(delta)
        .clamp(0, i64::from(i32::MAX));
    let level = level_for_experience(total);
    let into_level = total - experience_for_level(level);
    XpUpdate {
        level,
        progress: (into_level as f64 / f64::from(xp_bar_cap(level))) as f32,
        total_experience: total as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vanilla_thresholds() {
        for (level, xp) in [
            (0, 0),
            (1, 7),
            (2, 16),
            (15, 315),
            (16, 352),
            (17, 394),
            (30, 1395),
            (31, 1507),
            (32, 1628),
        ] {
            assert_eq!(experience_for_level(level), xp, "level {level}");
            assert_eq!(level_for_experience(xp), level, "xp {xp}");
            if level > 0 {
                assert_eq!(level_for_experience(xp - 1), level - 1, "xp {}", xp - 1);
            }
        }
        assert_eq!(experience_for_level(-5), 0);
        assert_eq!(level_for_experience(-10), 0);
    }

    #[test]
    fn bar_cap_matches_level_deltas() {
        assert_eq!(xp_bar_cap(-3), 7);
        for level in 0..100 {
            assert_eq!(
                i64::from(xp_bar_cap(level)),
                experience_for_level(level + 1) - experience_for_level(level),
                "level {level}"
            );
        }
    }

    #[test]
    fn player_xp_with_progress() {
        assert_eq!(player_xp(0, 0.0), 0);
        assert_eq!(player_xp(15, 0.5), 315 + 18);
        assert_eq!(player_xp(30, 0.0), 1395);
    }

    #[test]
    fn apply_delta_round_trips() {
        let up = apply_xp_delta(0, 0.0, 352);
        assert_eq!(
            up,
            XpUpdate {
                level: 16,
                progress: 0.0,
                total_experience: 352
            }
        );
        let half = apply_xp_delta(16, 0.0, 21);
        assert_eq!((half.level, half.total_experience), (16, 373));
        assert!((half.progress - 0.5).abs() < 1e-6);
        let back = apply_xp_delta(half.level, half.progress, -373);
        assert_eq!(
            back,
            XpUpdate {
                level: 0,
                progress: 0.0,
                total_experience: 0
            }
        );
        assert_eq!(
            apply_xp_delta(5, 0.0, 0),
            XpUpdate {
                level: 5,
                progress: 0.0,
                total_experience: 55
            }
        );
    }

    #[test]
    fn negative_delta_clamps_to_zero() {
        assert_eq!(
            apply_xp_delta(3, 0.25, -1_000_000),
            XpUpdate {
                level: 0,
                progress: 0.0,
                total_experience: 0
            }
        );
    }

    #[test]
    fn huge_delta_saturates() {
        let u = apply_xp_delta(0, 0.0, i64::MAX);
        assert_eq!(u.total_experience, i32::MAX);
        assert!(u.level > 20_000 && (0.0..1.0).contains(&u.progress));
    }
}
