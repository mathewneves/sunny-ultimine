//! Block-matching primitives: glob patterns for tag config entries and matcher kinds.

use std::collections::HashSet;

/// A glob pattern: `*` matches any run of characters (including none), `?` any
/// single character, everything else literally.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GlobPattern {
    chars: Vec<char>,
}

impl GlobPattern {
    /// Parses `pattern`. Every string is a valid pattern.
    pub fn parse(pattern: &str) -> Self {
        Self {
            chars: pattern.chars().collect(),
        }
    }

    /// `true` if the whole of `text` matches. Greedy with single-star backtracking,
    /// so worst case is O(pattern * text), never exponential.
    pub fn matches(&self, text: &str) -> bool {
        let p = &self.chars;
        let t: Vec<char> = text.chars().collect();
        let (mut pi, mut ti) = (0, 0);
        // Position of the last `*` seen and the text index it is currently matched up to.
        let mut star: Option<(usize, usize)> = None;
        while ti < t.len() {
            if p.get(pi) == Some(&'*') {
                star = Some((pi, ti));
                pi += 1;
            } else if p.get(pi).is_some_and(|&c| c == '?' || c == t[ti]) {
                pi += 1;
                ti += 1;
            } else if let Some((sp, st)) = star {
                star = Some((sp, st + 1));
                pi = sp + 1;
                ti = st + 1;
            } else {
                return false;
            }
        }
        p[pi..].iter().all(|&c| c == '*')
    }
}

/// Which blocks a tag-based matcher accepts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagMatchSpec {
    /// A bare `*` pattern: every block matches.
    AnyBlock,
    /// Blocks sharing any of these tags with the original block.
    Tags(Vec<String>),
}

/// Expands config tag patterns into concrete tag names.
///
/// Literal patterns are kept as-is; patterns containing `*` or `?` are matched
/// against `known_tags`. Empty patterns are ignored. Output is deduplicated, in
/// first-seen order.
pub fn resolve_tag_patterns(patterns: &[String], known_tags: &[String]) -> TagMatchSpec {
    if patterns.iter().any(|p| p == "*") {
        return TagMatchSpec::AnyBlock;
    }
    let mut seen = HashSet::new();
    let mut tags = Vec::new();
    for pattern in patterns.iter().filter(|p| !p.is_empty()) {
        if pattern.contains(['*', '?']) {
            let glob = GlobPattern::parse(pattern);
            for tag in known_tags.iter().filter(|t| glob.matches(t)) {
                if seen.insert(tag.as_str()) {
                    tags.push(tag.clone());
                }
            }
        } else if seen.insert(pattern.as_str()) {
            tags.push(pattern.clone());
        }
    }
    TagMatchSpec::Tags(tags)
}

/// Plant category used by the crop matcher.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BushType {
    /// Not a recognised plant.
    Other,
    /// Age-based crop (wheat, carrots, ...).
    Crop,
    /// Sapling.
    Sapling,
    /// Cocoa pod.
    Cocoa,
    /// Sweet berry bush.
    SweetBerry,
}

/// How candidate blocks are compared with the original block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatcherKind {
    /// Same block.
    ByBlock,
    /// Shared merge tag, for the shapeless shape.
    ByTagsShapeless,
    /// Shared merge tag, for fixed shapes.
    ByTagsShaped,
    /// Same plant category.
    ByCropType,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(p: &str, t: &str) -> bool {
        GlobPattern::parse(p).matches(t)
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn glob_literals_and_empty() {
        assert!(m("", ""));
        assert!(!m("", "a"));
        assert!(!m("a", ""));
        assert!(m("minecraft:logs", "minecraft:logs"));
        assert!(!m("minecraft:logs", "minecraft:log"));
        assert!(!m("minecraft:log", "minecraft:logs"));
    }

    #[test]
    fn glob_wildcards() {
        assert!(m("*", ""));
        assert!(m("*", "anything"));
        assert!(m("**", "x"));
        assert!(m("c:ores/*", "c:ores/iron"));
        assert!(m("c:ores/*", "c:ores/"));
        assert!(!m("c:ores/*", "c:ores"));
        assert!(!m("c:ores/*", "forge:ores/iron"));
        assert!(m("minecraft:*_ores", "minecraft:iron_ores"));
        assert!(!m("minecraft:*_ores", "minecraft:iron_ore"));
        assert!(m("*:logs", "minecraft:logs"));
        assert!(m("a?c", "abc"));
        assert!(!m("a?c", "ac"));
        assert!(m("?", "é"));
        assert!(m("*a*b*c*", "xxaxxbxxcxx"));
        assert!(!m("*a*b*c*", "xxaxxcxxbxx"));
        assert!(m("a*b?d", "aXXbcd"));
        // Pathological case would explode with naive recursion.
        let text = "a".repeat(200);
        assert!(!m(&format!("{}b", "*a".repeat(30)), &text));
    }

    #[test]
    fn resolve_any_block() {
        let spec = resolve_tag_patterns(&strings(&["c:ores/*", "*"]), &[]);
        assert_eq!(spec, TagMatchSpec::AnyBlock);
    }

    #[test]
    fn resolve_literals_and_globs_dedup_in_order() {
        let known = strings(&[
            "c:ores/iron",
            "minecraft:logs",
            "c:ores/gold",
            "c:storage_blocks/iron",
        ]);
        let spec = resolve_tag_patterns(
            &strings(&[
                "minecraft:logs",
                "",
                "c:ores/*",
                "c:ores/gold",
                "minecraft:not_known",
                "*:logs",
            ]),
            &known,
        );
        assert_eq!(
            spec,
            TagMatchSpec::Tags(strings(&[
                "minecraft:logs",
                "c:ores/iron",
                "c:ores/gold",
                "minecraft:not_known"
            ]))
        );
        assert_eq!(
            resolve_tag_patterns(&strings(&["x:?"]), &known),
            TagMatchSpec::Tags(vec![])
        );
    }
}
