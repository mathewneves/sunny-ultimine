//! HUD overlay: the Ultimine status panel and the timed hint above the hotbar.

use mod_api::{ClientHost, GuiLayer, GuiLayerDrawEvent, GuiLayerPlacement, RegisterGuiLayersEvent};
use sunny_ultimine_core::cooldown::remaining_progress_until;

use crate::state::{ModState, OVERLAY_LAYER_ID, epoch_millis};

/// Vanilla font line height in GUI pixels.
const LINE_H: f32 = 9.0;
/// Indent of shape-menu lines (built in `client_input`), which leaves room for the scrollbar.
const MENU_INDENT: f32 = 16.0;
/// Vanilla's overlay (action bar) message baseline: `guiHeight - 68`, text drawn 4px above.
const HINT_FROM_BOTTOM: f32 = 72.0;
/// The hint fades out over its last half second.
const HINT_FADE_MS: u64 = 500;

/// Registers the overlay layer above every other HUD layer.
pub fn register_gui_layers(event: &mut RegisterGuiLayersEvent<'_>) {
    event.layers.push(GuiLayer {
        id: OVERLAY_LAYER_ID.to_owned(),
        placement: GuiLayerPlacement::AboveAll,
    });
}

/// `0xRRGGBB` with alpha `a_mult` (`0.0..=1.0`) as `0xAARRGGBB`.
fn argb(rgb: u32, a_mult: f32) -> u32 {
    let a = (a_mult.clamp(0.0, 1.0) * 255.0).round() as u32;
    (a << 24) | (rgb & 0x00FF_FFFF)
}

/// Draws the overlay layer.
pub fn on_gui_layer_draw(
    host: &dyn ClientHost,
    state: &ModState,
    event: &mut GuiLayerDrawEvent<'_>,
) {
    if event.layer != OVERLAY_LAYER_ID {
        return;
    }
    let now = host.millis();
    let (alpha, lines, max_panel_width, cfg, cooldown_progress, hint) = {
        let cli = state.client();
        (
            cli.panel_alpha_mult,
            cli.info_panel_lines.clone(),
            cli.max_panel_width,
            cli.config.clone(),
            remaining_progress_until(epoch_millis(), cli.cooldown_ready_at_ms, cli.cooldown_ticks),
            cli.hint_text.clone().filter(|&(_, expires)| now < expires),
        )
    };
    let draw = &mut *event.draw;
    let [sw, sh] = draw.size();

    if let Some((text, expires)) = hint {
        let a = ((expires - now) as f32 / HINT_FADE_MS as f32).min(1.0);
        let w = draw.text_width(&text);
        let x = (sw - w) / 2.0;
        let y = sh - HINT_FROM_BOTTOM;
        draw.fill(
            [x - 2.0, y - 2.0, w + 4.0, LINE_H + 3.0],
            argb(0x000000, 0.5 * a),
        );
        draw.text(&text, [x, y], argb(0xFFFFFF, a), true);
    }

    if alpha <= 0.0 || lines.is_empty() {
        return;
    }

    let text_w = lines
        .iter()
        .map(|l| {
            l.indent
                + l.segments
                    .iter()
                    .map(|(s, _)| draw.text_width(s))
                    .sum::<f32>()
        })
        .fold(0.0_f32, f32::max);
    // Sticky width so the panel doesn't jitter as lines change; reset per world.
    let width = max_panel_width.max(text_w + 10.0);
    if width > max_panel_width {
        state.client().max_panel_width = width;
    }

    let scale = cfg.overlay_scale as f32;
    let unscaled_h = LINE_H * lines.len() as f32;
    let [ox, oy] = cfg.overlay_pos.panel_origin(
        sw,
        sh,
        width * scale,
        unscaled_h * scale,
        cfg.overlay_inset_x,
        cfg.overlay_inset_y,
    );

    // Background with a 1px border.
    let (bx, by) = (ox - 2.0 * scale, oy - 2.0 * scale);
    let (bw, bh) = ((width + 4.0) * scale, (unscaled_h + 4.0) * scale);
    draw.fill([bx, by, bw, bh], argb(0x282828, 0.5 * alpha));
    let border = argb(0x808080, 0.5 * alpha);
    draw.fill([bx, by, bw, 1.0], border);
    draw.fill([bx, by + bh - 1.0, bw, 1.0], border);
    draw.fill([bx, by + 1.0, 1.0, bh - 2.0], border);
    draw.fill([bx + bw - 1.0, by + 1.0, 1.0, bh - 2.0], border);

    if cooldown_progress < 1.0 {
        draw.fill(
            [
                ox,
                oy,
                (width * cooldown_progress + 1.0) * scale,
                (LINE_H + 1.0) * scale,
            ],
            argb(0x40A040, 0.68 * alpha),
        );
    }

    // Shape-menu scrollbar beside the menu lines, with end caps and a marker at the selection.
    if let Some(first) = lines.iter().position(|l| l.indent == MENU_INDENT) {
        let count = lines.iter().filter(|l| l.indent == MENU_INDENT).count() as f32;
        let white = argb(0xFFFFFF, 0.75 * alpha);
        let x = ox + 3.0 * scale;
        let top = oy + (first as f32 * LINE_H + 1.0) * scale;
        let h = (LINE_H * count - 2.0) * scale;
        draw.fill([x, top, scale, h], white);
        draw.fill([x - scale, top, 3.0 * scale, scale], white);
        draw.fill([x - scale, top + h - scale, 3.0 * scale, scale], white);
        let mid = oy + ((first as f32 + (count - 1.0) / 2.0) * LINE_H + LINE_H / 2.0 - 1.0) * scale;
        draw.fill([x - scale, mid, 4.0 * scale, 2.0 * scale], white);
    }

    for (idx, l) in lines.iter().enumerate() {
        let y = oy + idx as f32 * LINE_H * scale;
        let mut x = ox + l.indent * scale;
        for (text, rgb) in &l.segments {
            draw.text_scaled(text, [x, y], scale, argb(*rgb, alpha), true);
            x += draw.text_width(text) * scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::argb;

    #[test]
    fn argb_packs_alpha() {
        assert_eq!(argb(0x123456, 1.0), 0xFF12_3456);
        assert_eq!(argb(0xFF123456, 0.0), 0x0012_3456);
        assert_eq!(argb(0x000000, 0.5), 0x8000_0000);
        assert_eq!(argb(0xFFFFFF, 2.0), 0xFFFF_FFFF);
    }
}
