//! Custom play-phase payload channels and binary packet handlers.

use std::sync::Arc;

use mod_api::{
    ClientHost, IPayloadContext, IPayloadHandler, PayloadBytes, PayloadHandleError, PayloadId,
    PlayerRef, Registrar, ServerHost,
};
use sunny_ultimine_core::config::SyncedServerConfig;
use sunny_ultimine_core::protocol::{
    KeyPressedMsg, MAX_SHAPE_POSITIONS, ModeChangedMsg, SendShapeMsg, SyncServerConfigMsg,
    SyncTimeMsg, TimeSyncKind,
};
use sunny_ultimine_core::shapes::ShapeId;

use crate::state::{ModState, epoch_millis};

/// Client -> server: Ultimine key pressed or released.
pub const CH_KEY_PRESSED: &str = "sunny_ultimine:key_pressed";
/// Client -> server: cycle shape forward or backward.
pub const CH_MODE_CHANGED: &str = "sunny_ultimine:mode_changed";
/// Server -> client: sync current shape index and optional selected block preview.
pub const CH_SEND_SHAPE: &str = "sunny_ultimine:send_shape";
/// Server -> client: sync effective cooldown length or remaining cooldown ms.
pub const CH_SYNC_TIME: &str = "sunny_ultimine:sync_time";
/// Server -> client: sync server rules affecting client HUD checks.
pub const CH_SYNC_SERVER_CONFIG: &str = "sunny_ultimine:sync_server_config";

/// Registers all optional play channels for client<->server sync.
pub fn register_payloads(r: &mut dyn Registrar, state: &Arc<ModState>) {
    let mut handlers = r.payload_handlers();
    let view = handlers.registrar("1");
    let mut view = view.optional();

    view.play_to_server(
        PayloadId(CH_KEY_PRESSED.into()),
        Arc::new(KeyPressedHandler(Arc::clone(state))),
    );
    view.play_to_server(
        PayloadId(CH_MODE_CHANGED.into()),
        Arc::new(ModeChangedHandler(Arc::clone(state))),
    );
    view.play_to_client(
        PayloadId(CH_SEND_SHAPE.into()),
        Some(Arc::new(SendShapeHandler(Arc::clone(state)))),
    );
    view.play_to_client(
        PayloadId(CH_SYNC_TIME.into()),
        Some(Arc::new(SyncTimeHandler(Arc::clone(state)))),
    );
    view.play_to_client(
        PayloadId(CH_SYNC_SERVER_CONFIG.into()),
        Some(Arc::new(SyncServerConfigHandler(Arc::clone(state)))),
    );
}

/// Sends `KeyPressedMsg` from the local client to the server.
pub fn send_key_pressed(host: &mut dyn ClientHost, pressed: bool) {
    let bytes = KeyPressedMsg { pressed }.encode();
    let _ = host.send_payload_to_server(&PayloadId(CH_KEY_PRESSED.into()), PayloadBytes(bytes));
}

/// Sends `ModeChangedMsg` from the local client to the server.
pub fn send_mode_changed(host: &mut dyn ClientHost, next: bool) {
    let bytes = ModeChangedMsg { next }.encode();
    let _ = host.send_payload_to_server(&PayloadId(CH_MODE_CHANGED.into()), PayloadBytes(bytes));
}

/// Sends `SendShapeMsg` from the server to `player`, truncating any position list to [`MAX_SHAPE_POSITIONS`].
pub fn send_shape(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    shape: ShapeId,
    positions: Option<Vec<[i32; 3]>>,
) {
    let positions = positions.map(|mut p| {
        if p.len() > MAX_SHAPE_POSITIONS {
            p.truncate(MAX_SHAPE_POSITIONS);
        }
        p
    });
    let bytes = SendShapeMsg {
        shape_idx: shape.index() as u8,
        positions,
    }
    .encode();
    let _ = player.send_payload(host, &PayloadId(CH_SEND_SHAPE.into()), PayloadBytes(bytes));
}

/// Sends `SyncTimeMsg` from the server to `player`.
pub fn send_sync_time(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    kind: TimeSyncKind,
    value: u64,
) {
    let bytes = SyncTimeMsg { kind, value }.encode();
    let _ = player.send_payload(host, &PayloadId(CH_SYNC_TIME.into()), PayloadBytes(bytes));
}

/// Sends `SyncServerConfigMsg` from the server to `player`.
pub fn send_server_config(host: &mut dyn ServerHost, player: PlayerRef, cfg: SyncedServerConfig) {
    let bytes = SyncServerConfigMsg(cfg).encode();
    let _ = player.send_payload(
        host,
        &PayloadId(CH_SYNC_SERVER_CONFIG.into()),
        PayloadBytes(bytes),
    );
}

fn ctx_err(e: impl std::fmt::Debug) -> PayloadHandleError {
    PayloadHandleError::Failed(format!("{e:?}"))
}

struct KeyPressedHandler(Arc<ModState>);

impl IPayloadHandler for KeyPressedHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        cx: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        let Ok(msg) = KeyPressedMsg::decode(&payload.0) else {
            return Ok(());
        };
        let player = cx.player().map_err(ctx_err)?;
        let host = cx.server_host().map_err(ctx_err)?;
        let uuid = player.uuid();
        let shape_to_sync = {
            let mut srv = self.0.server();
            let data = srv.players.entry(uuid).or_default();
            data.pressed = msg.pressed;
            data.clear_cache();
            (!data.pressed).then_some(data.shape)
        };
        if let Some(shape) = shape_to_sync {
            send_shape(host, player, shape, None);
        }
        Ok(())
    }
}

struct ModeChangedHandler(Arc<ModState>);

impl IPayloadHandler for ModeChangedHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        cx: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        let Ok(msg) = ModeChangedMsg::decode(&payload.0) else {
            return Ok(());
        };
        let player = cx.player().map_err(ctx_err)?;
        let host = cx.server_host().map_err(ctx_err)?;
        let uuid = player.uuid();
        let shape = {
            let mut srv = self.0.server();
            let data = srv.players.entry(uuid).or_default();
            data.shape = data.shape.cycle(msg.next);
            data.clear_cache();
            data.shape
        };
        send_shape(host, player, shape, None);
        Ok(())
    }
}

struct SendShapeHandler(Arc<ModState>);

impl IPayloadHandler for SendShapeHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        cx: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        let Ok(msg) = SendShapeMsg::decode(&payload.0) else {
            return Ok(());
        };
        let host = cx.client_host().map_err(ctx_err)?;
        let shape = ShapeId::from_index(msg.shape_idx as isize);
        let now_ms = host.millis();
        let translated_shape = host
            .translate(shape.translation_key())
            .unwrap_or_else(|| shape.english_name().to_owned());
        let category = host
            .translate("key.category.sunny_ultimine.default")
            .unwrap_or_else(|| "Sunny Ultimine".to_owned());

        let mut cli = self.0.client();
        let prev_shape = cli.shape;
        cli.shape = shape;
        if let Some(blocks) = msg.positions {
            cli.actual_blocks = blocks.len();
            let max_render = cli.config.render_outline as usize;
            let limit = blocks.len().min(max_render);
            cli.shape_blocks = blocks.into_iter().take(limit).collect();
            cli.edges_dirty = true;
        }
        if shape != prev_shape
            && !cli.ultimine_key_pressed
            && cli.config.shape_feedback_hotbar_message
        {
            cli.hint_text = Some((
                format!("{category} : {translated_shape}"),
                now_ms.saturating_add(2500),
            ));
        }
        Ok(())
    }
}

struct SyncTimeHandler(Arc<ModState>);

impl IPayloadHandler for SyncTimeHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        _cx: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        let Ok(msg) = SyncTimeMsg::decode(&payload.0) else {
            return Ok(());
        };
        let mut cli = self.0.client();
        match msg.kind {
            TimeSyncKind::Cooldown => cli.cooldown_ticks = msg.value,
            TimeSyncKind::RemainingMs => {
                cli.cooldown_ready_at_ms = match msg.value {
                    0 => 0,
                    ms => epoch_millis().saturating_add(ms),
                }
            }
        }
        Ok(())
    }
}

struct SyncServerConfigHandler(Arc<ModState>);

impl IPayloadHandler for SyncServerConfigHandler {
    fn handle(
        &self,
        payload: PayloadBytes,
        _cx: &mut dyn IPayloadContext,
    ) -> Result<(), PayloadHandleError> {
        let Ok(msg) = SyncServerConfigMsg::decode(&payload.0) else {
            return Ok(());
        };
        let mut cli = self.0.client();
        cli.server_config = msg.0;
        Ok(())
    }
}
