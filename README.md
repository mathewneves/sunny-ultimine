# Sunny Ultimine for Sunrise

A native Sunrise mod: hold a key and break, till, strip, flatten or harvest a whole group of connected blocks at once.
Mod id `sunny_ultimine`, game 26.3, side `both` (the server does the work; the client draws the preview and overlay and
sends key state).

## Controls

| Key binding (Options > Controls, category *Sunny Ultimine*) | Default | What it does |
|---|---|---|
| Activate Ultimining | `` ` `` (grave) | Hold while mining or right-clicking to act on the whole group |
| Cycle to Next Shape | `Up` | Next shape |
| Cycle to Previous Shape | `Down` | Previous shape |

While the Ultimine key is held, scrolling the mouse wheel or pressing `Up`/`Down` cycles shapes, and a small menu lists
the shapes around the selected one. When `require_sneak_for_menu` is enabled, sneaking (holding Shift, or Control if the
Ultimine key itself is bound to Shift) is required to show the menu and change shapes; by default it is off, and
`require_ultimine_key_for_cycling` can also drop the need to hold the key.

The overlay shows whether ultimining is active, how many blocks the current shape covers ("Mining 37 blocks"), and why
it can't run when it can't ("Too Hungry", "Not enough XP", "No valid tool", "on cooldown", ...).

## Shapes

| Shape | Selects |
|---|---|
| Shapeless | Every connected matching block (vein mining), up to `max_blocks` |
| Small Tunnel | A 1x1 line going into the clicked face |
| Large Tunnel (3x3) | A 3x3 tunnel going into the clicked face |
| Small Square (3x3) | A 3x3 square on the face you hit |
| Mining Tunnel | A 1-wide diagonal staircase going down into the clicked face (opposite player facing on top/bottom faces) |
| Escape Tunnel | A 1-wide diagonal staircase going up into the clicked face (opposite player facing on top/bottom faces) |

"Matching" means the same block, or a block sharing one of the `merge_tags` (shapeless) / `merge_tags_shaped`
(shaped) tags, so stone variants or all ores can mine as one vein.

## Right-click actions

With the Ultimine key held, right-clicking a block with a tool applies the tool's vanilla block transformation to the
whole group:

- **Axe**: strip logs, scrape copper oxidation, remove wax (`right_click_axe`)
- **Shovel**: flatten eligible blocks into dirt paths (`right_click_shovel`)
- **Hoe**: till eligible blocks into farmland (`right_click_hoe`)
- **Crops** (any item or empty hand): harvest every fully grown crop in the group and replant it from the drops
  (`right_click_harvesting`)

Break drops and XP are merged and spawned at the original block; crop harvest drops are merged and spawned beside the
clicked face.

### Single-crop harvesting

With `single_crop_harvesting` on, right-clicking one fully grown crop *without* the Ultimine key harvests and replants
just that crop. Blocks in `#sunny_ultimine:single_crop_harvesting_blacklist` are skipped.

## Configuration

Both configs are TOML; unknown keys are ignored and out-of-range values are clamped.

### Client (per player) — also in Options > Mods > Sunny Ultimine

| Setting | Default | Meaning |
|---|---|---|
| `require_ultimine_key_for_cycling` | `true` | Only cycle shapes while the Ultimine key is held |
| `shape_feedback_hotbar_message` | `false` | Show the new shape's name above the hotbar |
| `render_outline` | `256` | Most blocks to outline in the preview (0 hides it) |
| `preview_line_alpha` | `45` | Opacity (0-255) of the outline seen through blocks |
| `shape_menu_context_lines` | `2` | Shapes listed above and below the selected one (1-5) |
| `require_sneak_for_menu` | `false` | Only show the menu and change shapes while sneaking |
| `overlay_pos` | `top_left` | `top_left`, `top_center`, `top_right`, `middle_left`, `middle_right`, `bottom_left`, `bottom_center`, `bottom_right` |
| `overlay_inset_x` / `overlay_inset_y` | `2` / `2` | Pixels from the overlay's screen edges (0-1000) |
| `overlay_scale` | `1.0` | Overlay size multiplier (0.25-4.0) |

### Server (per world)

| Section | Setting | Default | Meaning |
|---|---|---|---|
| `[features]` | `right_click_axe`, `right_click_shovel`, `right_click_hoe`, `right_click_harvesting` | `true` | Multi-block right-click actions |
| | `single_crop_harvesting` | `true` | Right-click one grown crop to harvest and replant it |
| `[costs_limits]` | `max_blocks` | `64` | Most blocks per operation (0-32768) |
| | `exhaustion_per_block` | `20.0` | Hunger exhaustion per block |
| | `experience_per_block` | `0.0` | XP points per block |
| | `require_tool` | `false` | Require a damageable tool, item with `minecraft:tool` component, or `#sunny_ultimine:included_tools` item |
| | `require_valid_tool_for_block` | `false` | Only with the correct tool for the block |
| | `ultimine_cooldown` | `0` | Ticks between operations |
| `[misc]` | `merge_tags` | `["minecraft:base_stone_overworld", "minecraft:*_ores", "c:ores/*", "forge:ores/*"]` | Tags that merge in shapeless mode (globs allowed) |
| | `merge_tags_shaped` | `["*"]` | Tags that merge in shaped modes |
| | `prevent_tool_break` | `0` | Stop at this much durability left (0-100) |
| | `cancel_on_block_break_fail` | `false` | Stop the whole operation when one block fails to break |

The client-relevant part (tool requirements, XP cost, cooldown) is synced to players on join and on change.

## Commands

```
/sunny_ultimine status                       print your current shape, key state, effective limits and cooldown
/sunny_ultimine shape [shape]                print or set your active shape
/sunny_ultimine config get <setting>         print a server setting
/sunny_ultimine config set <setting> <value> change a server setting (operator level 2, saved and synced to players)
```

## Data pack tags

| Tag | Default | Purpose |
|---|---|---|
| `#sunny_ultimine:block_whitelist` | empty | If non-empty, only these blocks can be ultimined |
| `#sunny_ultimine:excluded_blocks` | empty | Never ultimined |
| `#sunny_ultimine:single_crop_harvesting_blacklist` | empty | Crops single-crop harvesting skips |
| `#sunny_ultimine:excluded_tools` (item) | `#sunny_ultimine:excluded_tools/strict` | Items that can't ultimine in the main hand |
| `#sunny_ultimine:excluded_tools/strict` (item) | empty | Items that can't ultimine in either the main hand or off-hand |
| `#sunny_ultimine:included_tools` (item) | empty | Additional items that count as tools when `require_tool` is enabled |

Add entries with `"replace": false` in your own data pack at `data/sunny_ultimine/tags/...`.

## Player attributes

Per-player modifiers (e.g. from gear or `/attribute`) adjust the server settings:

| Attribute | Effect |
|---|---|
| `sunny_ultimine:max_blocks_modifier` | Added to `max_blocks` |
| `sunny_ultimine:cooldown_modifier` | Added to `ultimine_cooldown` (ticks) |
| `sunny_ultimine:exhaustion_modifier` | Added to `exhaustion_per_block` |
| `sunny_ultimine:experience_modifier` | Added to `experience_per_block` |

## Building and testing

The scripts in `tools/` find their tools through these environment variables:

| Variable | Default | Used for |
|---|---|---|
| `SUNRISE_LAUNCHER` | `sunrise-launcher` on `PATH` | Building into an instance and running servers |
| `CARGO_SUNRISE_MOD` | `cargo-sunrise-mod` on `PATH`, else built from `SUNRISE_SRC` | `tools/content-check.sh` |
| `SUNRISE_SRC` | `$SUNRISE_CACHE_DIR/src` | A Sunrise checkout at the `sunrise` pin in `sunrise-mod.toml` |
| `SUNRISE_CACHE_DIR` | `${XDG_CACHE_HOME:-~/.cache}/sunrise` | Scratch space, under `sunny-ultimine/` |
| `SUNRISE_DATA` | `${XDG_DATA_HOME:-~/.local/share}/sunrise/launcher` | The launcher's instances and versions |
| `SUNNY_ULTIMINE_INSTANCE` | `sunny-ultimine` | The launcher instance to build into and run |

```sh
(cd src/core && cargo test)               # pure game-free logic
tools/devcheck.sh                         # cargo check against the pinned SDK (also: test, clippy, doc)
tools/devcheck.sh . clippy -- -D warnings
tools/content-check.sh                    # validate assets/ and data/ with cargo sunrise-mod build + check
tools/build.sh                            # native-lock (if no Cargo.lock) + native-build into the instance
tools/server-check.sh -f tools/scenarios/<name>.txt   # headless server scenario with expect lines
tools/server-check.sh "say hi"            # or ad-hoc console commands
tools/client.sh start                     # offscreen, muted single-player client driven by sunrise-ctl
tools/client.sh screenshot …              # any sunrise-ctl command; tools/client.sh help lists them
tools/client.sh stop
```

### Dev commands

`/sunny_ultimine_dev` (press keys, set shapes, give items, preview, break and right-click for a named player) is
disabled by default and only registered when `SUNNY_ULTIMINE_DEV=1` is set in the server environment (and requires op
level 2). `tools/server-check.sh` sets `SUNNY_ULTIMINE_DEV=1` automatically for the scenarios in `tools/scenarios/`.

Scenario files hold one console command per line, `wait` (hold 1 s), `pace <seconds>`, `#` comments and
`expect <extended regex>` lines matched against the server log.

## License

MIT, see [LICENSE](LICENSE).
