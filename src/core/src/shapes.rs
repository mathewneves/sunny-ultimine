//! Ultimine mining shapes: which block positions a shape selects around the
//! block the player broke. The Sunny Ultimine shape set.

use std::collections::{HashSet, VecDeque};

/// A block position `[x, y, z]`.
pub type BlockPos = [i32; 3];

/// A coordinate axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Axis {
    /// East/west axis.
    X,
    /// Up/down axis.
    Y,
    /// North/south axis.
    Z,
}

/// A block face / direction, in Minecraft `Direction` order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Face {
    /// -Y.
    Down,
    /// +Y.
    Up,
    /// -Z.
    North,
    /// +Z.
    South,
    /// -X.
    West,
    /// +X.
    East,
}

impl Face {
    const ALL: [Face; 6] = [
        Face::Down,
        Face::Up,
        Face::North,
        Face::South,
        Face::West,
        Face::East,
    ];

    /// Unit step this face points along.
    pub const fn step(self) -> [i32; 3] {
        match self {
            Face::Down => [0, -1, 0],
            Face::Up => [0, 1, 0],
            Face::North => [0, 0, -1],
            Face::South => [0, 0, 1],
            Face::West => [-1, 0, 0],
            Face::East => [1, 0, 0],
        }
    }

    /// The face pointing the other way.
    pub const fn opposite(self) -> Self {
        match self {
            Face::Down => Face::Up,
            Face::Up => Face::Down,
            Face::North => Face::South,
            Face::South => Face::North,
            Face::West => Face::East,
            Face::East => Face::West,
        }
    }

    /// `true` for `Down` and `Up`.
    pub const fn is_vertical(self) -> bool {
        matches!(self, Face::Down | Face::Up)
    }

    /// The axis this face lies on.
    pub const fn axis(self) -> Axis {
        match self {
            Face::Down | Face::Up => Axis::Y,
            Face::North | Face::South => Axis::Z,
            Face::West | Face::East => Axis::X,
        }
    }

    /// Parses a lowercase Minecraft direction name (case-insensitive).
    pub fn from_str_name(s: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|f| f.as_str().eq_ignore_ascii_case(s))
    }

    /// The lowercase Minecraft direction name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Face::Down => "down",
            Face::Up => "up",
            Face::North => "north",
            Face::South => "south",
            Face::West => "west",
            Face::East => "east",
        }
    }
}

/// `pos + delta`.
pub fn offset(pos: BlockPos, delta: [i32; 3]) -> BlockPos {
    [pos[0] + delta[0], pos[1] + delta[1], pos[2] + delta[2]]
}

/// `pos` moved `dist` blocks towards `face`.
pub fn relative(pos: BlockPos, face: Face, dist: i32) -> BlockPos {
    let s = face.step();
    offset(pos, [s[0] * dist, s[1] * dist, s[2] * dist])
}

/// The registered Ultimine shapes, in registration order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShapeId {
    /// Flood fill of connected matching blocks.
    Shapeless = 0,
    /// 1x1 tunnel into the clicked face.
    SmallTunnel = 1,
    /// 3x3 square on the clicked face.
    SmallSquare = 2,
    /// 3x3 tunnel into the clicked face.
    LargeTunnel = 3,
    /// Diagonal staircase going down.
    MiningTunnel = 4,
    /// Diagonal staircase going up.
    EscapeTunnel = 5,
}

/// `(id, short name, translation key, English name)` per shape, by index.
const SHAPE_NAMES: [(&str, &str, &str, &str); 6] = [
    (
        "sunny_ultimine:shapeless",
        "shapeless",
        "sunny_ultimine.shape.sunny_ultimine.shapeless",
        "Connected Blocks",
    ),
    (
        "sunny_ultimine:small_tunnel",
        "small_tunnel",
        "sunny_ultimine.shape.sunny_ultimine.small_tunnel",
        "Straight Line",
    ),
    (
        "sunny_ultimine:small_square",
        "small_square",
        "sunny_ultimine.shape.sunny_ultimine.small_square",
        "Flat Square (3x3)",
    ),
    (
        "sunny_ultimine:large_tunnel",
        "large_tunnel",
        "sunny_ultimine.shape.sunny_ultimine.large_tunnel",
        "Wide Tunnel (3x3)",
    ),
    (
        "sunny_ultimine:mining_tunnel",
        "mining_tunnel",
        "sunny_ultimine.shape.sunny_ultimine.mining_tunnel",
        "Stairs Down",
    ),
    (
        "sunny_ultimine:escape_tunnel",
        "escape_tunnel",
        "sunny_ultimine.shape.sunny_ultimine.escape_tunnel",
        "Stairs Up",
    ),
];

impl ShapeId {
    /// All shapes in registration order.
    pub const ALL: [ShapeId; 6] = [
        ShapeId::Shapeless,
        ShapeId::SmallTunnel,
        ShapeId::SmallSquare,
        ShapeId::LargeTunnel,
        ShapeId::MiningTunnel,
        ShapeId::EscapeTunnel,
    ];

    /// Registration index.
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Namespaced id, e.g. `sunny_ultimine:small_tunnel`.
    pub const fn id(self) -> &'static str {
        SHAPE_NAMES[self.index()].0
    }

    /// Id path, e.g. `small_tunnel`.
    pub const fn short_name(self) -> &'static str {
        SHAPE_NAMES[self.index()].1
    }

    /// Lang key, e.g. `sunny_ultimine.shape.sunny_ultimine.small_tunnel`.
    pub const fn translation_key(self) -> &'static str {
        SHAPE_NAMES[self.index()].2
    }

/// English display name, matching `en_us.json`.
    pub const fn english_name(self) -> &'static str {
        SHAPE_NAMES[self.index()].3
    }

    /// `true` when the block set is not a fixed geometric pattern (only `Shapeless`).
    pub const fn is_indeterminate(self) -> bool {
        matches!(self, ShapeId::Shapeless)
    }

    /// `true` for the default shape (`Shapeless`).
    pub const fn is_default(self) -> bool {
        matches!(self, ShapeId::Shapeless)
    }

    /// Shape at `idx`, wrapping around in both directions.
    pub fn from_index(idx: isize) -> Self {
        Self::ALL[idx.rem_euclid(Self::ALL.len() as isize) as usize]
    }

    /// Parses a namespaced id (`sunny_ultimine:small_tunnel`) or short name (`small_tunnel`).
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|s| s.id() == name || s.short_name() == name)
    }

    /// Next (`next == true`) or previous shape, wrapping.
    pub fn cycle(self, next: bool) -> Self {
        Self::from_index(self.index() as isize + if next { 1 } else { -1 })
    }
}

/// Answers whether a position may be included in a shape (it matches the original block).
pub trait ShapeQuery {
    /// `true` if `pos` holds a block the shape may select.
    fn check(&self, pos: BlockPos) -> bool;
}

/// Inputs for [`compute_shape_blocks`].
pub struct ShapeInput<'a> {
    /// The block the player broke.
    pub orig_pos: BlockPos,
    /// The face of `orig_pos` the player targeted.
    pub face: Face,
    /// The player's horizontal facing; a vertical value is treated as `North`.
    pub player_facing: Face,
    /// Maximum number of positions to return.
    pub max_blocks: usize,
    /// Use the 5x5 crop neighbourhood for `Shapeless`.
    pub crop_like: bool,
    /// Block matcher.
    pub query: &'a dyn ShapeQuery,
}

/// Positions `shape` selects, in selection order, at most `ctx.max_blocks` long.
pub fn compute_shape_blocks(shape: ShapeId, ctx: &ShapeInput<'_>) -> Vec<BlockPos> {
    if ctx.max_blocks == 0 {
        return Vec::new();
    }
    match shape {
        ShapeId::Shapeless => shapeless(ctx),
        ShapeId::SmallTunnel => line(ctx, |i| relative(ctx.orig_pos, ctx.face.opposite(), i)),
        ShapeId::SmallSquare => square_layers(ctx, 1),
        ShapeId::LargeTunnel => square_layers(ctx, usize::MAX),
        ShapeId::MiningTunnel => diagonal(ctx, -1),
        ShapeId::EscapeTunnel => diagonal(ctx, 1),
    }
}

/// Positions `pos_at(0), pos_at(1), ...` up to the first non-matching one.
fn line(ctx: &ShapeInput<'_>, pos_at: impl Fn(i32) -> BlockPos) -> Vec<BlockPos> {
    (0..)
        .take(ctx.max_blocks)
        .map(pos_at)
        .take_while(|&p| ctx.query.check(p))
        .collect()
}

fn diagonal(ctx: &ShapeInput<'_>, y_dir: i32) -> Vec<BlockPos> {
    let facing = if ctx.player_facing.is_vertical() {
        Face::North
    } else {
        ctx.player_facing
    };
    let dir = if ctx.face.is_vertical() {
        facing.opposite()
    } else {
        ctx.face
    };
    let [sx, _, sz] = dir.step();
    let o = ctx.orig_pos;
    line(ctx, |i| [o[0] - sx * i, o[1] + y_dir * i, o[2] - sz * i])
}

/// 3x3 layers perpendicular to the face, going into the block, until a layer adds nothing.
fn square_layers(ctx: &ShapeInput<'_>, max_depth: usize) -> Vec<BlockPos> {
    if !ctx.query.check(ctx.orig_pos) {
        return Vec::new();
    }
    let mut base = ctx.orig_pos;
    let mut list = vec![base];
    for depth in 0..max_depth {
        let before = list.len();
        for a in -1..=1 {
            for b in -1..=1 {
                if depth == 0 && a == 0 && b == 0 {
                    continue;
                }
                if list.len() >= ctx.max_blocks {
                    return list;
                }
                let delta = match ctx.face.axis() {
                    Axis::X => [0, a, b],
                    Axis::Y => [a, 0, b],
                    Axis::Z => [a, b, 0],
                };
                let pos = offset(base, delta);
                if ctx.query.check(pos) {
                    list.push(pos);
                }
            }
        }
        if list.len() == before {
            break;
        }
        base = relative(base, ctx.face.opposite(), 1);
    }
    list
}

fn dist_sq(a: BlockPos, b: BlockPos) -> i64 {
    (0..3).map(|i| i64::from(a[i] - b[i]).pow(2)).sum()
}

fn neighbors(crop_like: bool) -> Vec<[i32; 3]> {
    if crop_like {
        // NEIGHBOR_POSITIONS_PLANT: 5x5 horizontal ring plus straight up/down, nearest first.
        let mut n: Vec<[i32; 3]> = (-2..=2)
            .flat_map(|x| (-2..=2).map(move |z| [x, 0, z]))
            .filter(|&p| p != [0, 0, 0])
            .chain([[0, 1, 0], [0, -1, 0]])
            .collect();
        n.sort_by_key(|&p| dist_sq(p, [0, 0, 0]));
        n
    } else {
        (-1..=1)
            .flat_map(|x| (-1..=1).flat_map(move |y| (-1..=1).map(move |z| [x, y, z])))
            .filter(|&p| p != [0, 0, 0])
            .collect()
    }
}

fn shapeless(ctx: &ShapeInput<'_>) -> Vec<BlockPos> {
    let neighbors = neighbors(ctx.crop_like);
    let mut known = HashSet::new();
    let mut traversed = HashSet::from([ctx.orig_pos]);
    let mut open = VecDeque::from([ctx.orig_pos]);
    while let Some(ptr) = open.pop_front() {
        if ctx.query.check(ptr) && known.insert(ptr) {
            if known.len() >= ctx.max_blocks {
                break;
            }
            for &n in &neighbors {
                let p = offset(ptr, n);
                if traversed.insert(p) {
                    open.push_back(p);
                }
            }
        }
    }
    let mut list: Vec<BlockPos> = known.into_iter().collect();
    list.sort_by_key(|&p| (dist_sq(p, ctx.orig_pos), p));
    list.truncate(ctx.max_blocks);
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Set(HashSet<BlockPos>);
    impl ShapeQuery for Set {
        fn check(&self, pos: BlockPos) -> bool {
            self.0.contains(&pos)
        }
    }

    struct All;
    impl ShapeQuery for All {
        fn check(&self, _: BlockPos) -> bool {
            true
        }
    }

    fn input(face: Face, max_blocks: usize, query: &dyn ShapeQuery) -> ShapeInput<'_> {
        ShapeInput {
            orig_pos: [0, 0, 0],
            face,
            player_facing: Face::North,
            max_blocks,
            crop_like: false,
            query,
        }
    }

    fn cuboid(min: BlockPos, max: BlockPos) -> Set {
        let mut s = HashSet::new();
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    s.insert([x, y, z]);
                }
            }
        }
        Set(s)
    }

    #[test]
    fn face_basics() {
        for f in Face::ALL {
            assert_eq!(f.opposite().opposite(), f);
            assert_eq!(offset(f.step(), f.opposite().step()), [0, 0, 0]);
            assert_eq!(Face::from_str_name(&f.as_str().to_uppercase()), Some(f));
        }
        assert_eq!(Face::from_str_name("sideways"), None);
        assert_eq!(Face::East.axis(), Axis::X);
        assert!(Face::Down.is_vertical() && !Face::West.is_vertical());
        assert_eq!(relative([1, 2, 3], Face::North, 4), [1, 2, -1]);
    }

    #[test]
    fn shape_id_names_and_wrapping() {
        for (i, s) in ShapeId::ALL.into_iter().enumerate() {
            assert_eq!(s.index(), i);
            assert_eq!(ShapeId::from_name(s.id()), Some(s));
            assert_eq!(ShapeId::from_name(s.short_name()), Some(s));
            assert_eq!(
                s.translation_key(),
                format!("sunny_ultimine.shape.{}", s.id().replace(':', "."))
            );
        }
        assert_eq!(ShapeId::SmallSquare.english_name(), "Flat Square (3x3)");
        assert_eq!(ShapeId::from_index(-1), ShapeId::EscapeTunnel);
        assert_eq!(ShapeId::from_index(6), ShapeId::Shapeless);
        assert_eq!(ShapeId::from_index(-13), ShapeId::EscapeTunnel);
        assert_eq!(ShapeId::EscapeTunnel.cycle(true), ShapeId::Shapeless);
        assert_eq!(ShapeId::Shapeless.cycle(false), ShapeId::EscapeTunnel);
        assert!(ShapeId::Shapeless.is_indeterminate() && ShapeId::Shapeless.is_default());
        assert!(!ShapeId::LargeTunnel.is_indeterminate() && !ShapeId::LargeTunnel.is_default());
        assert_eq!(ShapeId::from_name("nope"), None);
    }

    #[test]
    fn zero_max_blocks_is_empty() {
        for s in ShapeId::ALL {
            assert!(compute_shape_blocks(s, &input(Face::Up, 0, &All)).is_empty());
        }
    }

    #[test]
    fn shapeless_flood_fill_sorted_and_capped() {
        let q = cuboid([-1, 0, 0], [3, 0, 0]);
        let all = compute_shape_blocks(ShapeId::Shapeless, &input(Face::Up, 64, &q));
        assert_eq!(
            all,
            vec![[0, 0, 0], [-1, 0, 0], [1, 0, 0], [2, 0, 0], [3, 0, 0]]
        );
        let capped = compute_shape_blocks(ShapeId::Shapeless, &input(Face::Up, 2, &q));
        assert_eq!(capped.len(), 2);
        assert!(capped.contains(&[0, 0, 0]));
        // Diagonal neighbours connect.
        let diag = Set(HashSet::from([[0, 0, 0], [1, 1, 1], [2, 2, 2]]));
        assert_eq!(
            compute_shape_blocks(ShapeId::Shapeless, &input(Face::Up, 64, &diag)).len(),
            3
        );
        // Origin not matching yields nothing.
        let none = Set(HashSet::from([[1, 0, 0]]));
        assert!(compute_shape_blocks(ShapeId::Shapeless, &input(Face::Up, 64, &none)).is_empty());
    }

    #[test]
    fn shapeless_crop_jumps_two_blocks_but_not_diagonal_vertical() {
        let crops = Set(HashSet::from([
            [0, 0, 0],
            [2, 0, 0],
            [4, 0, 2],
            [4, 1, 2],
            [4, -1, 2],
            [1, 1, 0],
        ]));
        let mut c = input(Face::Up, 64, &crops);
        c.crop_like = true;
        let got = compute_shape_blocks(ShapeId::Shapeless, &c);
        // [1,1,0] is only a diagonal-vertical step away; [4,±1,2] are straight up/down.
        assert_eq!(got.len(), 5);
        assert!(!got.contains(&[1, 1, 0]));
        assert_eq!(neighbors(true).len(), 26);
        assert_eq!(neighbors(false).len(), 26);
        // Without crop mode diagonals connect but the 2-block gap is not crossed.
        c.crop_like = false;
        assert_eq!(
            compute_shape_blocks(ShapeId::Shapeless, &c),
            vec![[0, 0, 0], [1, 1, 0], [2, 0, 0]]
        );
    }

    #[test]
    fn small_tunnel_goes_into_face_and_stops() {
        let q = cuboid([0, 0, -10], [0, 0, 0]);
        let got = compute_shape_blocks(ShapeId::SmallTunnel, &input(Face::South, 4, &q));
        assert_eq!(got, vec![[0, 0, 0], [0, 0, -1], [0, 0, -2], [0, 0, -3]]);
        let short = cuboid([0, 0, -1], [0, 0, 0]);
        assert_eq!(
            compute_shape_blocks(ShapeId::SmallTunnel, &input(Face::South, 9, &short)).len(),
            2
        );
    }

    #[test]
    fn small_square_per_axis() {
        let up = compute_shape_blocks(ShapeId::SmallSquare, &input(Face::Up, 64, &All));
        assert_eq!(up.len(), 9);
        assert_eq!(up[0], [0, 0, 0]);
        assert!(up.iter().all(|p| p[1] == 0));
        let east = compute_shape_blocks(ShapeId::SmallSquare, &input(Face::East, 64, &All));
        assert!(east.iter().all(|p| p[0] == 0) && east.len() == 9);
        let north = compute_shape_blocks(ShapeId::SmallSquare, &input(Face::North, 64, &All));
        assert!(north.iter().all(|p| p[2] == 0) && north.len() == 9);
        assert_eq!(
            compute_shape_blocks(ShapeId::SmallSquare, &input(Face::Up, 1, &All)),
            vec![[0, 0, 0]]
        );
        assert_eq!(
            compute_shape_blocks(ShapeId::SmallSquare, &input(Face::Up, 5, &All)).len(),
            5
        );
    }

    #[test]
    fn large_tunnel_stops_when_layer_empty() {
        // 3x3x2 region going down from the top face.
        let q = cuboid([-1, -1, -1], [1, 0, 1]);
        let got = compute_shape_blocks(ShapeId::LargeTunnel, &input(Face::Up, 64, &q));
        assert_eq!(got.len(), 18);
        assert!(got.iter().all(|p| p[1] == 0 || p[1] == -1));
        assert_eq!(
            compute_shape_blocks(ShapeId::LargeTunnel, &input(Face::Up, 64, &All)).len(),
            64
        );
        assert_eq!(
            compute_shape_blocks(ShapeId::LargeTunnel, &input(Face::Up, 10, &All)).len(),
            10
        );
        // Only the origin matches: first layer adds nothing.
        let lone = Set(HashSet::from([[0, 0, 0]]));
        assert_eq!(
            compute_shape_blocks(ShapeId::LargeTunnel, &input(Face::Up, 64, &lone)),
            vec![[0, 0, 0]]
        );
    }

    #[test]
    fn diagonal_tunnels_horizontal_and_vertical_face() {
        // Horizontal face: go away from the clicked face.
        let down = compute_shape_blocks(ShapeId::MiningTunnel, &input(Face::South, 3, &All));
        assert_eq!(down, vec![[0, 0, 0], [0, -1, -1], [0, -2, -2]]);
        let up = compute_shape_blocks(ShapeId::EscapeTunnel, &input(Face::West, 3, &All));
        assert_eq!(up, vec![[0, 0, 0], [1, 1, 0], [2, 2, 0]]);
        // Vertical face: follow the player's facing.
        let mut c = input(Face::Up, 3, &All);
        c.player_facing = Face::East;
        assert_eq!(
            compute_shape_blocks(ShapeId::MiningTunnel, &c),
            vec![[0, 0, 0], [1, -1, 0], [2, -2, 0]]
        );
        // Vertical player facing falls back to North.
        c.player_facing = Face::Down;
        assert_eq!(
            compute_shape_blocks(ShapeId::EscapeTunnel, &c),
            vec![[0, 0, 0], [0, 1, -1], [0, 2, -2]]
        );
        // Stops at first non-match.
        let q = Set(HashSet::from([[0, 0, 0], [0, -1, -1], [0, -3, -3]]));
        assert_eq!(
            compute_shape_blocks(ShapeId::MiningTunnel, &input(Face::South, 9, &q)).len(),
            2
        );
    }

    #[test]
    fn square_shapes_skip_invalid_origin() {
        let empty = Set(HashSet::new());
        assert!(
            compute_shape_blocks(ShapeId::SmallSquare, &input(Face::Up, 64, &empty)).is_empty()
        );
        assert!(
            compute_shape_blocks(ShapeId::LargeTunnel, &input(Face::Up, 64, &empty)).is_empty()
        );
    }
}
