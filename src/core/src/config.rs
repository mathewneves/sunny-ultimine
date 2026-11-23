//! Client and server settings with TOML load/save and `/sunny_ultimine config get|set` support.
//!
//! The TOML reader is a small, lenient subset parser: it understands `[section]` headers,
//! `key = value` lines, `#` comments outside strings, basic (`"..."`, with escapes) and literal
//! (`'...'`) strings, and string arrays that may span lines and end with a trailing comma.
//! Keys are matched by name regardless of section; unknown keys and invalid values are ignored
//! (the default is kept), and numbers are clamped to each field's range.

/// Screen anchor for the overlay panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelPos {
    /// Top-left corner.
    TopLeft,
    /// Top edge, centered horizontally.
    TopCenter,
    /// Top-right corner.
    TopRight,
    /// Left edge, centered vertically.
    MiddleLeft,
    /// Right edge, centered vertically.
    MiddleRight,
    /// Bottom-left corner.
    BottomLeft,
    /// Bottom edge, centered horizontally.
    BottomCenter,
    /// Bottom-right corner.
    BottomRight,
}

impl PanelPos {
    /// Every position, in [`Self::index`] order.
    pub const ALL: [PanelPos; 8] = [
        Self::TopLeft,
        Self::TopCenter,
        Self::TopRight,
        Self::MiddleLeft,
        Self::MiddleRight,
        Self::BottomLeft,
        Self::BottomCenter,
        Self::BottomRight,
    ];

    /// Config name, e.g. `"top_left"`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TopLeft => "top_left",
            Self::TopCenter => "top_center",
            Self::TopRight => "top_right",
            Self::MiddleLeft => "middle_left",
            Self::MiddleRight => "middle_right",
            Self::BottomLeft => "bottom_left",
            Self::BottomCenter => "bottom_center",
            Self::BottomRight => "bottom_right",
        }
    }

    /// Parses a config name produced by [`Self::as_str`].
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|p| p.as_str() == s)
    }

    /// Position in [`Self::ALL`].
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Inverse of [`Self::index`]; wraps around so callers can cycle with `index() + 1`.
    pub fn from_index(idx: usize) -> Self {
        Self::ALL[idx % Self::ALL.len()]
    }

    /// Top-left corner of a `panel_w` x `panel_h` panel at this anchor. Insets push the panel
    /// toward the screen center and are ignored on a centered axis.
    pub fn panel_origin(
        self,
        screen_w: f32,
        screen_h: f32,
        panel_w: f32,
        panel_h: f32,
        inset_x: i32,
        inset_y: i32,
    ) -> [f32; 2] {
        use PanelPos::*;
        let x = match self {
            TopLeft | MiddleLeft | BottomLeft => inset_x as f32,
            TopCenter | BottomCenter => (screen_w - panel_w) / 2.0,
            TopRight | MiddleRight | BottomRight => screen_w - panel_w - inset_x as f32,
        };
        let y = match self {
            TopLeft | TopCenter | TopRight => inset_y as f32,
            MiddleLeft | MiddleRight => (screen_h - panel_h) / 2.0,
            BottomLeft | BottomCenter | BottomRight => screen_h - panel_h - inset_y as f32,
        };
        [x, y]
    }
}

/// Per-player client settings.
#[derive(Clone, Debug, PartialEq)]
pub struct ClientConfig {
    /// Only cycle shapes while the Ultimine key is held.
    pub require_ultimine_key_for_cycling: bool,
    /// Show a hotbar message when the shape changes.
    pub shape_feedback_hotbar_message: bool,
    /// Most blocks to draw the outline for (`0..=i32::MAX`).
    pub render_outline: u32,
    /// Opacity of the dashed preview lines.
    pub preview_line_alpha: u8,
    /// Shapes listed above and below the selected one in the shape menu (`1..=5`).
    pub shape_menu_context_lines: u8,
    /// Only show the shape menu while sneaking.
    pub require_sneak_for_menu: bool,
    /// Where the overlay panel is anchored.
    pub overlay_pos: PanelPos,
    /// Horizontal overlay inset in pixels (`0..=1000`).
    pub overlay_inset_x: i32,
    /// Vertical overlay inset in pixels (`0..=1000`).
    pub overlay_inset_y: i32,
    /// Overlay scale (`0.25..=4.0`).
    pub overlay_scale: f64,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            require_ultimine_key_for_cycling: true,
            shape_feedback_hotbar_message: false,
            render_outline: 256,
            preview_line_alpha: 45,
            shape_menu_context_lines: 2,
            require_sneak_for_menu: false,
            overlay_pos: PanelPos::TopLeft,
            overlay_inset_x: 2,
            overlay_inset_y: 2,
            overlay_scale: 1.0,
        }
    }
}

/// `(section, key, comment)` for each client field, in file order.
const CLIENT_FIELDS: &[(&str, &str, &str)] = &[
    (
        "general",
        "require_ultimine_key_for_cycling",
        "Only cycle shapes while the Ultimine key is held. Default: true",
    ),
    (
        "general",
        "shape_feedback_hotbar_message",
        "Show a hotbar message when the shape changes. Default: false",
    ),
    (
        "general",
        "shape_menu_context_lines",
        "Shapes listed above and below the selected one in the shape menu. Range: 1 ~ 5, default: 2",
    ),
    (
        "general",
        "require_sneak_for_menu",
        "Only show the shape menu while sneaking. Default: false",
    ),
    (
        "rendering",
        "render_outline",
        "Most blocks to draw the outline for; high values can lag. Range: 0 ~ 2147483647, default: 256",
    ),
    (
        "rendering",
        "preview_line_alpha",
        "Opacity of the dashed preview lines. Range: 0 ~ 255, default: 45",
    ),
    (
        "overlay",
        "overlay_pos",
        "Overlay anchor: top_left, top_center, top_right, middle_left, middle_right, bottom_left, bottom_center or bottom_right. Default: \"top_left\"",
    ),
    (
        "overlay",
        "overlay_inset_x",
        "Horizontal inset from the anchored edge, in pixels. Range: 0 ~ 1000, default: 2",
    ),
    (
        "overlay",
        "overlay_inset_y",
        "Vertical inset from the anchored edge, in pixels. Range: 0 ~ 1000, default: 2",
    ),
    (
        "overlay",
        "overlay_scale",
        "Overlay scale. Range: 0.25 ~ 4.0, default: 1.0",
    ),
];

impl ClientConfig {
    /// Serializes to commented TOML that [`Self::from_toml`] reads back.
    pub fn to_toml(&self) -> String {
        write_toml(CLIENT_FIELDS, |key| self.get_field(key))
    }

    /// Reads TOML over the defaults; see the module docs for what is accepted.
    pub fn from_toml(text: &str) -> Self {
        let mut cfg = Self::default();
        for (key, value) in toml_entries(text) {
            // Invalid or unknown entries keep the default.
            let _ = cfg.set_field(&key, &value);
        }
        cfg
    }

    fn get_field(&self, key: &str) -> Option<String> {
        Some(match key {
            "require_ultimine_key_for_cycling" => self.require_ultimine_key_for_cycling.to_string(),
            "shape_feedback_hotbar_message" => self.shape_feedback_hotbar_message.to_string(),
            "render_outline" => self.render_outline.to_string(),
            "preview_line_alpha" => self.preview_line_alpha.to_string(),
            "shape_menu_context_lines" => self.shape_menu_context_lines.to_string(),
            "require_sneak_for_menu" => self.require_sneak_for_menu.to_string(),
            "overlay_pos" => quote(self.overlay_pos.as_str()),
            "overlay_inset_x" => self.overlay_inset_x.to_string(),
            "overlay_inset_y" => self.overlay_inset_y.to_string(),
            "overlay_scale" => format_float(self.overlay_scale),
            _ => return None,
        })
    }

    fn set_field(&mut self, key: &str, raw: &str) -> Result<(), String> {
        match key {
            "require_ultimine_key_for_cycling" => {
                self.require_ultimine_key_for_cycling = parse_bool(raw)?
            }
            "shape_feedback_hotbar_message" => {
                self.shape_feedback_hotbar_message = parse_bool(raw)?
            }
            "render_outline" => self.render_outline = parse_int(raw, 0, i32::MAX.into())? as u32,
            "preview_line_alpha" => self.preview_line_alpha = parse_int(raw, 0, 255)? as u8,
            "shape_menu_context_lines" => {
                self.shape_menu_context_lines = parse_int(raw, 1, 5)? as u8
            }
            "require_sneak_for_menu" => self.require_sneak_for_menu = parse_bool(raw)?,
            "overlay_pos" => {
                let name = parse_text(raw)?;
                self.overlay_pos =
                    PanelPos::parse(&name).ok_or_else(|| format!("unknown position {name:?}"))?;
            }
            "overlay_inset_x" => self.overlay_inset_x = parse_int(raw, 0, 1000)? as i32,
            "overlay_inset_y" => self.overlay_inset_y = parse_int(raw, 0, 1000)? as i32,
            "overlay_scale" => self.overlay_scale = parse_float(raw, 0.25, 4.0)?,
            _ => return Err(format!("unknown setting {key:?}")),
        }
        Ok(())
    }
}

/// Server-wide settings.
#[derive(Clone, Debug, PartialEq)]
pub struct ServerConfig {
    /// Strip, scrape and unwax many blocks at once with an axe.
    pub right_click_axe: bool,
    /// Flatten many blocks into paths at once with a shovel.
    pub right_click_shovel: bool,
    /// Till many blocks into farmland at once with a hoe.
    pub right_click_hoe: bool,
    /// Harvest and replant many grown crops at once.
    pub right_click_harvesting: bool,
    /// Right-click one grown crop to harvest and replant it without the Ultimine key.
    pub single_crop_harvesting: bool,
    /// Most blocks one operation can affect (`0..=32768`).
    pub max_blocks: u32,
    /// Hunger exhaustion added per block (`0.0..=10000.0`).
    pub exhaustion_per_block: f64,
    /// Experience points taken per block (`0.0..=20000.0`).
    pub experience_per_block: f64,
    /// Only ultimine while holding a damageable tool.
    pub require_tool: bool,
    /// Only ultimine when the held tool is correct for the block.
    pub require_valid_tool_for_block: bool,
    /// Ticks to wait between operations.
    pub ultimine_cooldown: u64,
    /// Block tags treated as one block in shapeless mode (`*` wildcards allowed).
    pub merge_tags: Vec<String>,
    /// Block tags treated as one block in shaped modes (`*` wildcards allowed).
    pub merge_tags_shaped: Vec<String>,
    /// Stop when the tool has this much durability left; `0` lets it break (`0..=100`).
    pub prevent_tool_break: u32,
    /// Stop the whole operation as soon as one block fails to break.
    pub cancel_on_block_break_fail: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            right_click_axe: true,
            right_click_shovel: true,
            right_click_hoe: true,
            right_click_harvesting: true,
            single_crop_harvesting: true,
            max_blocks: 64,
            exhaustion_per_block: 20.0,
            experience_per_block: 0.0,
            require_tool: false,
            require_valid_tool_for_block: false,
            ultimine_cooldown: 0,
            merge_tags: [
                "minecraft:base_stone_overworld",
                "minecraft:*_ores",
                "c:ores/*",
                "forge:ores/*",
            ]
            .map(String::from)
            .into(),
            merge_tags_shaped: vec!["*".to_owned()],
            prevent_tool_break: 0,
            cancel_on_block_break_fail: false,
        }
    }
}

/// `(section, key, comment)` for each server field, in file order.
const SERVER_FIELDS: &[(&str, &str, &str)] = &[
    (
        "features",
        "right_click_axe",
        "Strip, scrape and unwax many blocks at once with an axe while holding the Ultimine key. Default: true",
    ),
    (
        "features",
        "right_click_shovel",
        "Flatten many blocks into paths at once with a shovel while holding the Ultimine key. Default: true",
    ),
    (
        "features",
        "right_click_hoe",
        "Till many blocks into farmland at once with a hoe while holding the Ultimine key. Default: true",
    ),
    (
        "features",
        "right_click_harvesting",
        "Harvest and replant many grown crops at once while holding the Ultimine key. Default: true",
    ),
    (
        "features",
        "single_crop_harvesting",
        "Right-click one grown crop to harvest and replant it without the Ultimine key. Default: true",
    ),
    (
        "costs_limits",
        "max_blocks",
        "Most blocks one ultimine operation can affect. Range: 0 ~ 32768, default: 64",
    ),
    (
        "costs_limits",
        "exhaustion_per_block",
        "Hunger exhaustion added for each block mined. Range: 0.0 ~ 10000.0, default: 20.0",
    ),
    (
        "costs_limits",
        "experience_per_block",
        "Experience points taken for each block mined. Range: 0.0 ~ 20000.0, default: 0.0",
    ),
    (
        "costs_limits",
        "require_tool",
        "Only ultimine while holding a damageable tool or an item in the sunny_ultimine:included_tools tag. Default: false",
    ),
    (
        "costs_limits",
        "require_valid_tool_for_block",
        "Only ultimine when the held tool is the correct tool for the block. Default: false",
    ),
    (
        "costs_limits",
        "ultimine_cooldown",
        "Ticks to wait after an ultimine operation before the next. Default: 0",
    ),
    (
        "misc",
        "merge_tags",
        "Block tags treated as the same block in shapeless mode; '*' is a wildcard.",
    ),
    (
        "misc",
        "merge_tags_shaped",
        "Block tags treated as the same block in shaped modes; '*' is a wildcard.",
    ),
    (
        "misc",
        "prevent_tool_break",
        "Stop mining when the tool has this much durability left; 0 lets it break. Range: 0 ~ 100, default: 0",
    ),
    (
        "misc",
        "cancel_on_block_break_fail",
        "Stop the whole operation as soon as one block fails to break. Default: false",
    ),
];

impl ServerConfig {
    /// Serializes to commented TOML that [`Self::from_toml`] reads back.
    pub fn to_toml(&self) -> String {
        write_toml(SERVER_FIELDS, |key| self.get_field(key))
    }

    /// Reads TOML over the defaults; see the module docs for what is accepted.
    pub fn from_toml(text: &str) -> Self {
        let mut cfg = Self::default();
        for (key, value) in toml_entries(text) {
            // Invalid or unknown entries keep the default.
            let _ = cfg.set_field(&key, &value);
        }
        cfg
    }

    /// The TOML-formatted value of `key`, or `None` for an unknown key.
    pub fn get_field(&self, key: &str) -> Option<String> {
        Some(match key {
            "right_click_axe" => self.right_click_axe.to_string(),
            "right_click_shovel" => self.right_click_shovel.to_string(),
            "right_click_hoe" => self.right_click_hoe.to_string(),
            "right_click_harvesting" => self.right_click_harvesting.to_string(),
            "single_crop_harvesting" => self.single_crop_harvesting.to_string(),
            "max_blocks" => self.max_blocks.to_string(),
            "exhaustion_per_block" => format_float(self.exhaustion_per_block),
            "experience_per_block" => format_float(self.experience_per_block),
            "require_tool" => self.require_tool.to_string(),
            "require_valid_tool_for_block" => self.require_valid_tool_for_block.to_string(),
            "ultimine_cooldown" => self.ultimine_cooldown.to_string(),
            "merge_tags" => format_list(&self.merge_tags),
            "merge_tags_shaped" => format_list(&self.merge_tags_shaped),
            "prevent_tool_break" => self.prevent_tool_break.to_string(),
            "cancel_on_block_break_fail" => self.cancel_on_block_break_fail.to_string(),
            _ => return None,
        })
    }

    /// Sets `key` from `raw_value`. Numbers are clamped to the field's range. Tag lists accept a
    /// TOML string array, a single quoted string, or a bare comma-separated list.
    pub fn set_field(&mut self, key: &str, raw_value: &str) -> Result<(), String> {
        let raw = raw_value;
        match key {
            "right_click_axe" => self.right_click_axe = parse_bool(raw)?,
            "right_click_shovel" => self.right_click_shovel = parse_bool(raw)?,
            "right_click_hoe" => self.right_click_hoe = parse_bool(raw)?,
            "right_click_harvesting" => self.right_click_harvesting = parse_bool(raw)?,
            "single_crop_harvesting" => self.single_crop_harvesting = parse_bool(raw)?,
            "max_blocks" => self.max_blocks = parse_int(raw, 0, 32768)? as u32,
            "exhaustion_per_block" => self.exhaustion_per_block = parse_float(raw, 0.0, 10000.0)?,
            "experience_per_block" => self.experience_per_block = parse_float(raw, 0.0, 20000.0)?,
            "require_tool" => self.require_tool = parse_bool(raw)?,
            "require_valid_tool_for_block" => self.require_valid_tool_for_block = parse_bool(raw)?,
            "ultimine_cooldown" => {
                self.ultimine_cooldown = parse_int(raw, 0, u64::MAX.into())? as u64
            }
            "merge_tags" => self.merge_tags = parse_string_list(raw)?,
            "merge_tags_shaped" => self.merge_tags_shaped = parse_string_list(raw)?,
            "prevent_tool_break" => self.prevent_tool_break = parse_int(raw, 0, 100)? as u32,
            "cancel_on_block_break_fail" => self.cancel_on_block_break_fail = parse_bool(raw)?,
            _ => return Err(format!("unknown setting {key:?}")),
        }
        Ok(())
    }
}

/// The subset of [`ServerConfig`] the client needs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyncedServerConfig {
    /// See [`ServerConfig::require_tool`].
    pub require_tool: bool,
    /// See [`ServerConfig::require_valid_tool_for_block`].
    pub require_valid_tool_for_block: bool,
    /// See [`ServerConfig::experience_per_block`].
    pub experience_per_block: f64,
    /// See [`ServerConfig::ultimine_cooldown`].
    pub ultimine_cooldown: u64,
}

impl Default for SyncedServerConfig {
    fn default() -> Self {
        Self::from(&ServerConfig::default())
    }
}

impl From<&ServerConfig> for SyncedServerConfig {
    fn from(c: &ServerConfig) -> Self {
        Self {
            require_tool: c.require_tool,
            require_valid_tool_for_block: c.require_valid_tool_for_block,
            experience_per_block: c.experience_per_block,
            ultimine_cooldown: c.ultimine_cooldown,
        }
    }
}

// ---- TOML writing ----

fn write_toml(fields: &[(&str, &str, &str)], get: impl Fn(&str) -> Option<String>) -> String {
    let mut out = String::new();
    let mut section = "";
    for &(sec, key, comment) in fields {
        if sec != section {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&format!("[{sec}]\n"));
            section = sec;
        }
        let value = get(key).expect("field table key missing from get_field");
        out.push_str(&format!("# {comment}\n{key} = {value}\n"));
    }
    out
}

/// TOML basic string with the required escapes.
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn format_list(items: &[String]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| quote(s)).collect();
    format!("[{}]", quoted.join(", "))
}

/// `{:?}` always keeps a `.0` or exponent, so the value reads back as a float.
fn format_float(v: f64) -> String {
    format!("{v:?}")
}

// ---- TOML reading ----

/// Splits TOML into `(key, raw value)` pairs, joining multi-line arrays and dropping comments,
/// section headers and malformed lines. An array left unclosed at end of input is dropped.
fn toml_entries(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut pending: Option<(String, String, i32)> = None;
    for line in text.lines() {
        let (code, depth) = scan_line(line);
        let code = code.trim();
        if let Some((key, mut value, d)) = pending.take() {
            value.push(' ');
            value.push_str(code);
            let d = d + depth;
            if d > 0 {
                pending = Some((key, value, d));
            } else {
                out.push((key, value));
            }
            continue;
        }
        // Blank lines and `[section]` headers carry no entries.
        if code.is_empty() || code.starts_with('[') {
            continue;
        }
        let Some((key, value)) = code.split_once('=') else {
            continue;
        };
        let key = key
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .to_owned();
        let value = value.trim().to_owned();
        if depth > 0 {
            pending = Some((key, value, depth));
        } else {
            out.push((key, value));
        }
    }
    out
}

/// Returns the line without its `#` comment and its net `[`/`]` depth, both ignoring string
/// contents.
fn scan_line(line: &str) -> (&str, i32) {
    let mut depth = 0;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in line.char_indices() {
        match quote {
            Some('"') if escaped => escaped = false,
            Some('"') if c == '\\' => escaped = true,
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None => match c {
                '#' => return (&line[..i], depth),
                '"' | '\'' => quote = Some(c),
                '[' => depth += 1,
                ']' => depth -= 1,
                _ => {}
            },
        }
    }
    (line, depth)
}

fn parse_bool(raw: &str) -> Result<bool, String> {
    match raw.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("expected true or false, got {other:?}")),
    }
}

/// Parses an integer (TOML `_` separators and `+` allowed) and clamps it to `lo..=hi`.
fn parse_int(raw: &str, lo: i128, hi: i128) -> Result<i128, String> {
    let s = raw.trim().replace('_', "");
    let v: i128 = s
        .parse()
        .map_err(|_| format!("expected an integer, got {:?}", raw.trim()))?;
    Ok(v.clamp(lo, hi))
}

/// Parses a finite float and clamps it to `lo..=hi`.
fn parse_float(raw: &str, lo: f64, hi: f64) -> Result<f64, String> {
    let s = raw.trim().replace('_', "");
    match s.parse::<f64>() {
        Ok(v) if v.is_finite() => Ok(v.clamp(lo, hi)),
        _ => Err(format!("expected a finite number, got {:?}", raw.trim())),
    }
}

/// A quoted TOML string, or bare text.
fn parse_text(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    if !raw.starts_with(['"', '\'']) {
        return Ok(raw.to_owned());
    }
    let (s, rest) = parse_quoted(raw)?;
    if rest.trim().is_empty() {
        Ok(s)
    } else {
        Err(format!("unexpected text after string: {rest:?}"))
    }
}

/// A TOML string array (trailing comma allowed), a single string, or a bare comma-separated list.
fn parse_string_list(raw: &str) -> Result<Vec<String>, String> {
    let raw = raw.trim();
    let Some(mut rest) = raw.strip_prefix('[') else {
        if raw.starts_with(['"', '\'']) {
            return Ok(vec![parse_text(raw)?]);
        }
        return Ok(raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect());
    };
    let mut out = Vec::new();
    loop {
        rest = rest.trim_start();
        if let Some(after) = rest.strip_prefix(']') {
            return if after.trim().is_empty() {
                Ok(out)
            } else {
                Err(format!("unexpected text after array: {after:?}"))
            };
        }
        let (s, after) = parse_quoted(rest)?;
        out.push(s);
        rest = after.trim_start();
        if let Some(after) = rest.strip_prefix(',') {
            rest = after;
        } else if !rest.starts_with(']') {
            return Err("expected ',' or ']' in array".to_owned());
        }
    }
}

/// Parses one leading basic or literal string, returning it and the remaining input.
fn parse_quoted(s: &str) -> Result<(String, &str), String> {
    if let Some(body) = s.strip_prefix('\'') {
        let end = body.find('\'').ok_or("unterminated string")?;
        return Ok((body[..end].to_owned(), &body[end + 1..]));
    }
    let body = s
        .strip_prefix('"')
        .ok_or_else(|| format!("expected a string, got {s:?}"))?;
    let mut out = String::new();
    let mut chars = body.char_indices();
    while let Some((i, c)) = chars.next() {
        match c {
            '"' => return Ok((out, &body[i + 1..])),
            '\\' => {
                let (_, e) = chars.next().ok_or("unterminated string")?;
                out.push(match e {
                    'b' => '\u{8}',
                    't' => '\t',
                    'n' => '\n',
                    'f' => '\u{c}',
                    'r' => '\r',
                    '"' => '"',
                    '\\' => '\\',
                    'u' | 'U' => {
                        let len = if e == 'u' { 4 } else { 8 };
                        let hex: String = chars.by_ref().take(len).map(|(_, c)| c).collect();
                        u32::from_str_radix(&hex, 16)
                            .ok()
                            .filter(|_| hex.len() == len)
                            .and_then(char::from_u32)
                            .ok_or_else(|| format!("invalid unicode escape \\{e}{hex}"))?
                    }
                    other => return Err(format!("invalid escape \\{other}")),
                });
            }
            c => out.push(c),
        }
    }
    Err("unterminated string".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_pos_names_and_indices_roundtrip() {
        for (i, p) in PanelPos::ALL.into_iter().enumerate() {
            assert_eq!(p.index(), i);
            assert_eq!(PanelPos::from_index(i), p);
            assert_eq!(PanelPos::parse(p.as_str()), Some(p));
        }
        assert_eq!(PanelPos::from_index(8), PanelPos::TopLeft);
        assert_eq!(PanelPos::parse("TopLeft"), None);
    }

    #[test]
    fn panel_origin_insets_toward_center() {
        let o = |p: PanelPos| p.panel_origin(200.0, 100.0, 50.0, 20.0, 3, 4);
        assert_eq!(o(PanelPos::TopLeft), [3.0, 4.0]);
        assert_eq!(o(PanelPos::TopCenter), [75.0, 4.0]);
        assert_eq!(o(PanelPos::TopRight), [147.0, 4.0]);
        assert_eq!(o(PanelPos::MiddleLeft), [3.0, 40.0]);
        assert_eq!(o(PanelPos::MiddleRight), [147.0, 40.0]);
        assert_eq!(o(PanelPos::BottomLeft), [3.0, 76.0]);
        assert_eq!(o(PanelPos::BottomCenter), [75.0, 76.0]);
        assert_eq!(o(PanelPos::BottomRight), [147.0, 76.0]);
    }

    #[test]
    fn client_defaults_roundtrip() {
        let def = ClientConfig::default();
        let text = def.to_toml();
        assert!(text.starts_with("[general]\n"));
        assert!(text.contains("\n[rendering]\n") && text.contains("\n[overlay]\n"));
        assert_eq!(ClientConfig::from_toml(&text), def);
        assert_eq!(ClientConfig::from_toml(""), def);
    }

    #[test]
    fn client_custom_roundtrip() {
        let cfg = ClientConfig {
            require_ultimine_key_for_cycling: false,
            shape_feedback_hotbar_message: true,
            render_outline: 1000,
            preview_line_alpha: 200,
            shape_menu_context_lines: 5,
            require_sneak_for_menu: true,
            overlay_pos: PanelPos::BottomCenter,
            overlay_inset_x: 10,
            overlay_inset_y: 999,
            overlay_scale: 0.75,
        };
        assert_eq!(ClientConfig::from_toml(&cfg.to_toml()), cfg);
    }

    #[test]
    fn client_clamps_and_ignores_garbage() {
        let cfg = ClientConfig::from_toml(
            "render_outline = -5\npreview_line_alpha = 9999\nshape_menu_context_lines = 0\n\
             overlay_inset_x = 5000\noverlay_scale = 100\noverlay_pos = 'middle_right' # c\n\
             unknown = 1\nrequire_sneak_for_menu = yes\noverlay_inset_y = nan\n",
        );
        assert_eq!(cfg.render_outline, 0);
        assert_eq!(cfg.preview_line_alpha, 255);
        assert_eq!(cfg.shape_menu_context_lines, 1);
        assert_eq!(cfg.overlay_inset_x, 1000);
        assert_eq!(cfg.overlay_scale, 4.0);
        assert_eq!(cfg.overlay_pos, PanelPos::MiddleRight);
        assert!(!cfg.require_sneak_for_menu);
        assert_eq!(cfg.overlay_inset_y, 2);
        let big =
            ClientConfig::from_toml("render_outline = 99999999999\noverlay_pos = \"nowhere\"");
        assert_eq!(big.render_outline, i32::MAX as u32);
        assert_eq!(big.overlay_pos, PanelPos::TopLeft);
    }

    #[test]
    fn server_defaults_roundtrip() {
        let def = ServerConfig::default();
        let text = def.to_toml();
        for section in ["[features]", "[costs_limits]", "[misc]"] {
            assert!(text.contains(section), "{section} missing");
        }
        assert_eq!(ServerConfig::from_toml(&text), def);
        assert_eq!(ServerConfig::from_toml(""), def);
    }

    #[test]
    fn every_listed_field_gets_and_sets() {
        let mut cfg = ServerConfig::default();
        for &(_, key, _) in SERVER_FIELDS {
            let v = cfg.get_field(key).unwrap();
            cfg.set_field(key, &v).unwrap();
        }
        assert_eq!(cfg, ServerConfig::default());
        let mut client = ClientConfig::default();
        for &(_, key, _) in CLIENT_FIELDS {
            let v = client.get_field(key).unwrap();
            client.set_field(key, &v).unwrap();
        }
        assert_eq!(client, ClientConfig::default());
        assert_eq!(cfg.get_field("nope"), None);
        assert!(cfg.set_field("nope", "1").is_err());
    }

    #[test]
    fn server_custom_roundtrip_with_tricky_strings() {
        let cfg = ServerConfig {
            right_click_axe: false,
            max_blocks: 32768,
            exhaustion_per_block: 0.5,
            experience_per_block: 1e-3,
            require_tool: true,
            ultimine_cooldown: u64::MAX,
            merge_tags: vec![
                "a\"b".into(),
                "back\\slash".into(),
                "has # hash".into(),
                "[brackets],".into(),
                "tab\tnl\n\u{1}é".into(),
            ],
            merge_tags_shaped: Vec::new(),
            prevent_tool_break: 100,
            cancel_on_block_break_fail: true,
            ..ServerConfig::default()
        };
        assert_eq!(ServerConfig::from_toml(&cfg.to_toml()), cfg);
    }

    #[test]
    fn server_parses_multiline_arrays_comments_and_escapes() {
        let text = r#"
# header comment
[features]
right_click_hoe = false   # trailing comment

[misc]
merge_tags = [
    "minecraft:logs",      # first
    'c:ores/*',
    "quote\"d # not a comment",
    "\u00e9\U0001F600",
]
merge_tags_shaped = ["x", "y",]
[costs_limits]
"max_blocks" = 1_000
exhaustion_per_block = 2
ultimine_cooldown = -3
"#;
        let cfg = ServerConfig::from_toml(text);
        assert!(!cfg.right_click_hoe);
        assert_eq!(
            cfg.merge_tags,
            [
                "minecraft:logs",
                "c:ores/*",
                "quote\"d # not a comment",
                "é😀"
            ]
            .map(String::from)
        );
        assert_eq!(cfg.merge_tags_shaped, ["x", "y"].map(String::from));
        assert_eq!(cfg.max_blocks, 1000);
        assert_eq!(cfg.exhaustion_per_block, 2.0);
        assert_eq!(cfg.ultimine_cooldown, 0);
    }

    #[test]
    fn server_rejects_malformed_values_keeping_defaults() {
        let def = ServerConfig::default();
        let cfg = ServerConfig::from_toml("merge_tags = [\"unterminated\nmax_blocks = 5\n");
        // The unclosed array swallows the rest of the file and is dropped at EOF.
        assert_eq!(cfg, def);
        let cfg = ServerConfig::from_toml(
            "merge_tags = [\"a\" \"b\"]\nmerge_tags_shaped = [\"\\q\"]\nexhaustion_per_block = inf\n\
             max_blocks = 1.5\nexperience_per_block = -4\n= 3\nnot a line\n",
        );
        assert_eq!(cfg.merge_tags, def.merge_tags);
        assert_eq!(cfg.merge_tags_shaped, def.merge_tags_shaped);
        assert_eq!(cfg.exhaustion_per_block, 20.0);
        assert_eq!(cfg.max_blocks, 64);
        assert_eq!(cfg.experience_per_block, 0.0);
    }

    #[test]
    fn set_field_for_commands() {
        let mut cfg = ServerConfig::default();
        cfg.set_field("max_blocks", "100000").unwrap();
        assert_eq!(cfg.get_field("max_blocks").as_deref(), Some("32768"));
        cfg.set_field("merge_tags", "minecraft:logs, c:ores/*")
            .unwrap();
        assert_eq!(
            cfg.merge_tags,
            ["minecraft:logs", "c:ores/*"].map(String::from)
        );
        cfg.set_field("merge_tags", "\"single\"").unwrap();
        assert_eq!(cfg.merge_tags, ["single"].map(String::from));
        cfg.set_field("merge_tags_shaped", "[]").unwrap();
        assert!(cfg.merge_tags_shaped.is_empty());
        assert_eq!(cfg.get_field("merge_tags_shaped").as_deref(), Some("[]"));
        assert_eq!(
            cfg.get_field("exhaustion_per_block").as_deref(),
            Some("20.0")
        );
        assert!(cfg.set_field("require_tool", "1").is_err());
        assert!(cfg.set_field("ultimine_cooldown", "abc").is_err());
        cfg.set_field("ultimine_cooldown", "99999999999999999999999")
            .unwrap();
        assert_eq!(cfg.ultimine_cooldown, u64::MAX);
    }

    #[test]
    fn synced_config_copies_fields() {
        let cfg = ServerConfig {
            require_tool: true,
            require_valid_tool_for_block: true,
            experience_per_block: 3.0,
            ultimine_cooldown: 20,
            ..ServerConfig::default()
        };
        let s = SyncedServerConfig::from(&cfg);
        assert!(s.require_tool && s.require_valid_tool_for_block);
        assert_eq!(s.experience_per_block, 3.0);
        assert_eq!(s.ultimine_cooldown, 20);
        assert_eq!(
            SyncedServerConfig::default(),
            SyncedServerConfig::from(&ServerConfig::default())
        );
    }
}
