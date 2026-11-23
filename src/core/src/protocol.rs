//! Binary codecs for the Sunny Ultimine client/server messages.
//!
//! Every `decode` is strict and panic-free: it rejects truncated input, trailing bytes,
//! out-of-range flags and oversized position lists.

use std::fmt;

use crate::config::SyncedServerConfig;

/// Largest position list a [`SendShapeMsg`] may carry.
pub const MAX_SHAPE_POSITIONS: usize = 32768;

/// Why a message failed to decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CodecError {
    /// The input ended before the message did.
    Truncated,
    /// The input has bytes after the end of the message.
    TrailingBytes,
    /// A boolean or flags byte held an unknown value.
    InvalidFlag(u8),
    /// A [`TimeSyncKind`] byte held an unknown value.
    InvalidKind(u8),
    /// A shape claimed more than [`MAX_SHAPE_POSITIONS`] positions.
    TooManyPositions(u32),
    /// `experience_per_block` was negative, NaN or infinite.
    InvalidExperience,
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated => f.write_str("message truncated"),
            Self::TrailingBytes => f.write_str("trailing bytes after message"),
            Self::InvalidFlag(b) => write!(f, "invalid flag byte {b}"),
            Self::InvalidKind(b) => write!(f, "invalid time sync kind {b}"),
            Self::TooManyPositions(n) => {
                write!(f, "{n} positions exceeds limit {MAX_SHAPE_POSITIONS}")
            }
            Self::InvalidExperience => f.write_str("experience_per_block must be finite and >= 0"),
        }
    }
}

impl std::error::Error for CodecError {}

/// Cursor over the input that never indexes out of bounds.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let (head, rest) = self
            .0
            .split_first_chunk::<N>()
            .ok_or(CodecError::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }

    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take::<1>()?[0])
    }

    fn bool(&mut self) -> Result<bool, CodecError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            b => Err(CodecError::InvalidFlag(b)),
        }
    }

    fn u32(&mut self) -> Result<u32, CodecError> {
        self.take().map(u32::from_le_bytes)
    }

    fn i32(&mut self) -> Result<i32, CodecError> {
        self.take().map(i32::from_le_bytes)
    }

    fn u64(&mut self) -> Result<u64, CodecError> {
        self.take().map(u64::from_le_bytes)
    }

    fn finish(self) -> Result<(), CodecError> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(CodecError::TrailingBytes)
        }
    }
}

/// Client → server: the Ultimine key was pressed or released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyPressedMsg {
    /// Whether the key is now held.
    pub pressed: bool,
}

impl KeyPressedMsg {
    /// Encodes as one byte, `0` or `1`.
    pub fn encode(&self) -> Vec<u8> {
        vec![u8::from(self.pressed)]
    }

    /// Decodes [`Self::encode`] output.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut r = Reader(bytes);
        let pressed = r.bool()?;
        r.finish()?;
        Ok(Self { pressed })
    }
}

/// Client → server: cycle to the next or previous shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeChangedMsg {
    /// `true` for the next shape, `false` for the previous one.
    pub next: bool,
}

impl ModeChangedMsg {
    /// Encodes as one byte, `0` or `1`.
    pub fn encode(&self) -> Vec<u8> {
        vec![u8::from(self.next)]
    }

    /// Decodes [`Self::encode`] output.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut r = Reader(bytes);
        let next = r.bool()?;
        r.finish()?;
        Ok(Self { next })
    }
}

/// Server → client: the active shape and, optionally, the blocks it selected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendShapeMsg {
    /// Index of the active shape.
    pub shape_idx: u8,
    /// Selected block positions, or `None` to clear the preview.
    pub positions: Option<Vec<[i32; 3]>>,
}

impl SendShapeMsg {
    /// Encodes as `[shape_idx, has_positions]`, then `count: u32 LE` and `count * 3` `i32 LE`.
    /// Callers must cap positions at [`MAX_SHAPE_POSITIONS`]; `decode` rejects longer lists.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![self.shape_idx, u8::from(self.positions.is_some())];
        if let Some(positions) = &self.positions {
            out.reserve(4 + positions.len() * 12);
            out.extend_from_slice(&(positions.len() as u32).to_le_bytes());
            for pos in positions {
                for c in pos {
                    out.extend_from_slice(&c.to_le_bytes());
                }
            }
        }
        out
    }

    /// Decodes [`Self::encode`] output.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut r = Reader(bytes);
        let shape_idx = r.u8()?;
        let positions = if r.bool()? {
            let count = r.u32()?;
            if count as usize > MAX_SHAPE_POSITIONS {
                return Err(CodecError::TooManyPositions(count));
            }
            let mut positions = Vec::with_capacity(count as usize);
            for _ in 0..count {
                positions.push([r.i32()?, r.i32()?, r.i32()?]);
            }
            Some(positions)
        } else {
            None
        };
        r.finish()?;
        Ok(Self {
            shape_idx,
            positions,
        })
    }
}

/// Which cooldown value a [`SyncTimeMsg`] carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimeSyncKind {
    /// The cooldown length in ticks.
    Cooldown = 0,
    /// The player's remaining cooldown in milliseconds; `0` means ready (not on cooldown).
    RemainingMs = 1,
}

/// Server → client: cooldown length or remaining cooldown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncTimeMsg {
    /// What `value` means.
    pub kind: TimeSyncKind,
    /// Ticks for [`TimeSyncKind::Cooldown`], milliseconds for [`TimeSyncKind::RemainingMs`].
    pub value: u64,
}

impl SyncTimeMsg {
    /// Encodes as 9 bytes: `[kind, value: u64 LE]`.
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![self.kind as u8];
        out.extend_from_slice(&self.value.to_le_bytes());
        out
    }

    /// Decodes [`Self::encode`] output.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut r = Reader(bytes);
        let kind = match r.u8()? {
            0 => TimeSyncKind::Cooldown,
            1 => TimeSyncKind::RemainingMs,
            b => return Err(CodecError::InvalidKind(b)),
        };
        let value = r.u64()?;
        r.finish()?;
        Ok(Self { kind, value })
    }
}

/// Server → client: the server settings the client needs for its overlay checks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SyncServerConfigMsg(pub SyncedServerConfig);

const FLAG_REQUIRE_TOOL: u8 = 1;
const FLAG_REQUIRE_VALID_TOOL: u8 = 2;

impl SyncServerConfigMsg {
    /// Encodes as 17 bytes: `[flags, experience_per_block: f64 LE, ultimine_cooldown: u64 LE]`.
    pub fn encode(&self) -> Vec<u8> {
        let c = &self.0;
        let flags = if c.require_tool { FLAG_REQUIRE_TOOL } else { 0 }
            | if c.require_valid_tool_for_block {
                FLAG_REQUIRE_VALID_TOOL
            } else {
                0
            };
        let mut out = Vec::with_capacity(17);
        out.push(flags);
        out.extend_from_slice(&c.experience_per_block.to_le_bytes());
        out.extend_from_slice(&c.ultimine_cooldown.to_le_bytes());
        out
    }

    /// Decodes [`Self::encode`] output.
    pub fn decode(bytes: &[u8]) -> Result<Self, CodecError> {
        let mut r = Reader(bytes);
        let flags = r.u8()?;
        if flags & !(FLAG_REQUIRE_TOOL | FLAG_REQUIRE_VALID_TOOL) != 0 {
            return Err(CodecError::InvalidFlag(flags));
        }
        let experience_per_block = f64::from_le_bytes(r.take()?);
        if !experience_per_block.is_finite() || experience_per_block < 0.0 {
            return Err(CodecError::InvalidExperience);
        }
        let ultimine_cooldown = r.u64()?;
        r.finish()?;
        Ok(Self(SyncedServerConfig {
            require_tool: flags & FLAG_REQUIRE_TOOL != 0,
            require_valid_tool_for_block: flags & FLAG_REQUIRE_VALID_TOOL != 0,
            experience_per_block,
            ultimine_cooldown,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every strict prefix of a valid encoding must fail to decode.
    fn assert_prefixes_fail<T>(bytes: &[u8], decode: fn(&[u8]) -> Result<T, CodecError>) {
        for len in 0..bytes.len() {
            assert!(
                decode(&bytes[..len]).is_err(),
                "prefix of length {len} decoded"
            );
        }
        let mut long = bytes.to_vec();
        long.push(0);
        assert_eq!(decode(&long).err(), Some(CodecError::TrailingBytes));
    }

    #[test]
    fn bool_messages_roundtrip_and_reject_garbage() {
        for b in [false, true] {
            let m = KeyPressedMsg { pressed: b };
            assert_eq!(m.encode(), vec![u8::from(b)]);
            assert_eq!(KeyPressedMsg::decode(&m.encode()), Ok(m));
            let m = ModeChangedMsg { next: b };
            assert_eq!(ModeChangedMsg::decode(&m.encode()), Ok(m));
        }
        assert_eq!(KeyPressedMsg::decode(&[2]), Err(CodecError::InvalidFlag(2)));
        assert_eq!(
            ModeChangedMsg::decode(&[0xff]),
            Err(CodecError::InvalidFlag(0xff))
        );
        assert_prefixes_fail(&[1], KeyPressedMsg::decode);
        assert_prefixes_fail(&[0], ModeChangedMsg::decode);
    }

    #[test]
    fn send_shape_roundtrip() {
        let none = SendShapeMsg {
            shape_idx: 3,
            positions: None,
        };
        assert_eq!(none.encode(), vec![3, 0]);
        assert_eq!(SendShapeMsg::decode(&none.encode()), Ok(none));

        let some = SendShapeMsg {
            shape_idx: 1,
            positions: Some(vec![[0, -64, 0], [i32::MIN, i32::MAX, 7]]),
        };
        let bytes = some.encode();
        assert_eq!(bytes.len(), 2 + 4 + 2 * 12);
        assert_eq!(SendShapeMsg::decode(&bytes), Ok(some));
        assert_prefixes_fail(&bytes, SendShapeMsg::decode);

        let empty = SendShapeMsg {
            shape_idx: 0,
            positions: Some(Vec::new()),
        };
        assert_eq!(SendShapeMsg::decode(&empty.encode()), Ok(empty));
    }

    #[test]
    fn send_shape_rejects_bad_input() {
        assert_eq!(
            SendShapeMsg::decode(&[0, 2]),
            Err(CodecError::InvalidFlag(2))
        );
        // `None` with trailing data.
        assert_eq!(
            SendShapeMsg::decode(&[0, 0, 0, 0, 0, 0]),
            Err(CodecError::TrailingBytes)
        );
        // Claimed count larger than the payload.
        let mut b = vec![0, 1];
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(&[0; 12]);
        assert_eq!(SendShapeMsg::decode(&b), Err(CodecError::Truncated));
        // Oversized counts are rejected before allocating.
        for count in [MAX_SHAPE_POSITIONS as u32 + 1, u32::MAX] {
            let mut b = vec![0, 1];
            b.extend_from_slice(&count.to_le_bytes());
            assert_eq!(
                SendShapeMsg::decode(&b),
                Err(CodecError::TooManyPositions(count))
            );
        }
        // Exactly the limit is fine.
        let max = SendShapeMsg {
            shape_idx: 0,
            positions: Some(vec![[1, 2, 3]; MAX_SHAPE_POSITIONS]),
        };
        assert_eq!(SendShapeMsg::decode(&max.encode()), Ok(max));
    }

    #[test]
    fn sync_time_roundtrip_and_rejects() {
        for kind in [TimeSyncKind::Cooldown, TimeSyncKind::RemainingMs] {
            let m = SyncTimeMsg {
                kind,
                value: 0x0102_0304_0506_0708,
            };
            let bytes = m.encode();
            assert_eq!(bytes.len(), 9);
            assert_eq!(SyncTimeMsg::decode(&bytes), Ok(m));
            assert_prefixes_fail(&bytes, SyncTimeMsg::decode);
        }
        let mut b = vec![2];
        b.extend_from_slice(&[0; 8]);
        assert_eq!(SyncTimeMsg::decode(&b), Err(CodecError::InvalidKind(2)));
        // Kind byte 1 is RemainingMs; value 0 means ready.
        let ready = SyncTimeMsg::decode(&[1, 0, 0, 0, 0, 0, 0, 0, 0]).unwrap();
        assert_eq!(ready.kind, TimeSyncKind::RemainingMs);
        assert_eq!(ready.value, 0);
    }

    #[test]
    fn sync_server_config_roundtrip_and_rejects() {
        let cfg = SyncedServerConfig {
            require_tool: true,
            require_valid_tool_for_block: false,
            experience_per_block: 2.5,
            ultimine_cooldown: 40,
        };
        let m = SyncServerConfigMsg(cfg);
        let bytes = m.encode();
        assert_eq!(bytes.len(), 17);
        assert_eq!(bytes[0], 1);
        assert_eq!(SyncServerConfigMsg::decode(&bytes), Ok(m));
        assert_prefixes_fail(&bytes, SyncServerConfigMsg::decode);

        let both = SyncServerConfigMsg(SyncedServerConfig {
            require_valid_tool_for_block: true,
            ..cfg
        });
        assert_eq!(both.encode()[0], 3);
        assert_eq!(SyncServerConfigMsg::decode(&both.encode()), Ok(both));

        let mut bad = bytes.clone();
        bad[0] = 4;
        assert_eq!(
            SyncServerConfigMsg::decode(&bad),
            Err(CodecError::InvalidFlag(4))
        );
        for xp in [f64::NAN, f64::INFINITY, -1.0] {
            let mut bad = bytes.clone();
            bad[1..9].copy_from_slice(&xp.to_le_bytes());
            assert_eq!(
                SyncServerConfigMsg::decode(&bad),
                Err(CodecError::InvalidExperience)
            );
        }
    }

    #[test]
    fn decoders_never_panic_on_noise() {
        // Cheap deterministic LCG noise across many lengths.
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        for len in 0..64 {
            for _ in 0..64 {
                let bytes: Vec<u8> = (0..len)
                    .map(|_| {
                        seed = seed
                            .wrapping_mul(6364136223846793005)
                            .wrapping_add(1442695040888963407);
                        (seed >> 56) as u8
                    })
                    .collect();
                let _ = KeyPressedMsg::decode(&bytes);
                let _ = ModeChangedMsg::decode(&bytes);
                let _ = SendShapeMsg::decode(&bytes);
                let _ = SyncTimeMsg::decode(&bytes);
                let _ = SyncServerConfigMsg::decode(&bytes);
            }
        }
    }
}
