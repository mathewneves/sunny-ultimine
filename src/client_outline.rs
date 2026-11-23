//! World outline of the blocks the current Ultimine selection will mine.

use mod_api::{RenderLevelStage, RenderLevelStageEvent, WorldDepth, WorldLine};
use sunny_ultimine_core::outline::extract_merged_edges;

use crate::state::ModState;

/// Outward inflation of each block box, as Ultimine's `ShapeMerger` uses.
const INFLATE: f64 = 0.005;

/// Draws the merged outline: solid depth-tested lines plus faint see-through lines.
pub fn on_render_level_stage(state: &ModState, event: &mut RenderLevelStageEvent<'_>) {
    if event.stage != RenderLevelStage::AfterLevel {
        return;
    }
    let usage = event.draw.usage();
    let avail = usage.line_capacity.saturating_sub(usage.lines) as usize;
    if avail == 0 {
        return;
    }
    let batch = {
        let mut cli = state.client();
        if !(cli.ultimine_key_pressed && cli.can_ultimine) || cli.shape_blocks.is_empty() {
            return;
        }
        if cli.edges_dirty {
            cli.cached_edges = extract_merged_edges(&cli.shape_blocks, INFLATE);
            cli.edges_dirty = false;
        }
        let edges = &cli.cached_edges;
        let mut batch: Vec<WorldLine> = edges
            .iter()
            .take(avail)
            .map(|e| WorldLine::new(e.from, e.to, 0xFFFF_FFFF, 2.0, WorldDepth::Tested))
            .collect();
        let alpha = cli.config.preview_line_alpha;
        if alpha > 0 {
            let color = (u32::from(alpha) << 24) | 0x00FF_FFFF;
            let room = avail - batch.len();
            batch.extend(
                edges
                    .iter()
                    .take(room)
                    .map(|e| WorldLine::new(e.from, e.to, color, 1.0, WorldDepth::ThroughWalls)),
            );
        }
        batch
    };
    if !batch.is_empty() {
        // A refused batch (capacity raced by another mod) just skips this frame's outline.
        let _ = event.draw.lines(&batch);
    }
}
