//! Merged outline edges for a set of blocks: the visual result of the
//! `ShapeMerger` (union of block boxes inflated by a small margin) followed by
//! `VoxelShape.optimize().forAllEdges(..)`.

use std::collections::HashSet;

use crate::shapes::BlockPos;

/// A straight outline segment in world coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    /// Start point.
    pub from: [f64; 3],
    /// End point.
    pub to: [f64; 3],
}

fn step(mut p: [i32; 3], axis: usize, d: i32) -> [i32; 3] {
    p[axis] += d;
    p
}

/// Silhouette edges of the union of `blocks`, each block inflated by `inflate`
/// on every side, with collinear touching segments merged.
///
/// `inflate` must be in `(0, 0.5)`; Ultimine uses `0.005`.
pub fn extract_merged_edges(blocks: &[BlockPos], inflate: f64) -> Vec<Edge> {
    debug_assert!(
        inflate > 0.0 && inflate < 0.5,
        "inflate out of range: {inflate}"
    );
    // Fine grid per axis: cell 2c is the thin slab [c - e, c + e], cell 2c + 1 the body
    // [c + e, c + 1 - e]. A block at c covers fine cells 2c..=2c + 2 once inflated, so the
    // fine voxel set is exactly the union of inflated boxes.
    let mut fine = HashSet::new();
    for b in blocks {
        for d in 0..27 {
            fine.insert([2 * b[0] + d % 3, 2 * b[1] + d / 3 % 3, 2 * b[2] + d / 9]);
        }
    }
    // Fine grid vertex g is the lower bound of fine cell g.
    let coord = |p: [i32; 3]| {
        p.map(|g| {
            f64::from(g.div_euclid(2))
                + if g.rem_euclid(2) == 0 {
                    -inflate
                } else {
                    inflate
                }
        })
    };

    let mut edges = Vec::new();
    for axis in 0..3 {
        let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
        // Unit fine edge from vertex p along `axis`: a silhouette edge when its 4 incident
        // cells are 1 or 3 full, or 2 full diagonally (same rule as DiscreteVoxelShape).
        let is_edge = |p: [i32; 3]| {
            let full = |du: i32, dv: i32| fine.contains(&step(step(p, u, -du), v, -dv));
            let (b00, b10, b01, b11) = (full(1, 1), full(0, 1), full(1, 0), full(0, 0));
            let n = [b00, b10, b01, b11].into_iter().filter(|&b| b).count();
            n == 1 || n == 3 || (n == 2 && b00 == b11)
        };
        let kept: HashSet<[i32; 3]> = fine
            .iter()
            .flat_map(|&c| {
                [[0, 0], [1, 0], [0, 1], [1, 1]].map(|[du, dv]| step(step(c, u, du), v, dv))
            })
            .filter(|&p| is_edge(p))
            .collect();
        let mut starts: Vec<[i32; 3]> = kept
            .iter()
            .copied()
            .filter(|&p| !kept.contains(&step(p, axis, -1)))
            .collect();
        starts.sort_unstable();
        for start in starts {
            let mut end = step(start, axis, 1);
            while kept.contains(&end) {
                end = step(end, axis, 1);
            }
            edges.push(Edge {
                from: coord(start),
                to: coord(end),
            });
        }
    }
    edges
}

#[cfg(test)]
mod tests {
    use super::*;

    const E: f64 = 0.005;

    fn cuboid(min: BlockPos, max: BlockPos) -> Vec<BlockPos> {
        let mut v = Vec::new();
        for x in min[0]..=max[0] {
            for y in min[1]..=max[1] {
                for z in min[2]..=max[2] {
                    v.push([x, y, z]);
                }
            }
        }
        v
    }

    fn edges(blocks: &[BlockPos]) -> Vec<Edge> {
        let out = extract_merged_edges(blocks, E);
        for e in &out {
            assert!(e.from.iter().chain(&e.to).all(|c| c.is_finite()));
            // Axis-aligned, positive length along exactly one axis.
            let diff: Vec<f64> = (0..3).map(|i| e.to[i] - e.from[i]).collect();
            assert_eq!(diff.iter().filter(|&&d| d != 0.0).count(), 1, "{e:?}");
            assert!(diff.iter().all(|&d| d >= 0.0), "{e:?}");
        }
        out
    }

    fn lengths(out: &[Edge]) -> Vec<f64> {
        let mut l: Vec<f64> = out
            .iter()
            .map(|e| (0..3).map(|i| e.to[i] - e.from[i]).sum::<f64>())
            .collect();
        l.sort_by(f64::total_cmp);
        l
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn empty() {
        assert!(edges(&[]).is_empty());
    }

    #[test]
    fn single_cube() {
        let out = edges(&[[3, -2, 7]]);
        assert_eq!(out.len(), 12);
        for e in &out {
            for (p, c) in e
                .from
                .iter()
                .chain(&e.to)
                .zip([3.0, -2.0, 7.0].iter().cycle())
            {
                assert!(close(p - c, -E) || close(p - c, 1.0 + E), "{e:?}");
            }
        }
        assert!(lengths(&out).iter().all(|&l| close(l, 1.0 + 2.0 * E)));
    }

    #[test]
    fn boxes_merge_to_twelve_edges() {
        let bar = edges(&cuboid([0, 0, 0], [1, 0, 0]));
        assert_eq!(bar.len(), 12);
        assert_eq!(
            lengths(&bar)
                .iter()
                .filter(|&&l| close(l, 2.0 + 2.0 * E))
                .count(),
            4
        );
        let slab = edges(&cuboid([0, 0, 0], [2, 0, 2]));
        assert_eq!(slab.len(), 12);
        assert_eq!(
            lengths(&slab)
                .iter()
                .filter(|&&l| close(l, 3.0 + 2.0 * E))
                .count(),
            8
        );
        let cube = edges(&cuboid([-1, -1, -1], [0, 0, 0]));
        assert_eq!(cube.len(), 12);
        assert!(lengths(&cube).iter().all(|&l| close(l, 2.0 + 2.0 * E)));
    }

    #[test]
    fn l_shape() {
        assert_eq!(edges(&[[0, 0, 0], [1, 0, 0], [0, 0, 1]]).len(), 18);
    }

    #[test]
    fn cube_missing_one_corner() {
        let blocks: Vec<BlockPos> = cuboid([0, 0, 0], [1, 1, 1])
            .into_iter()
            .filter(|&p| p != [1, 1, 1])
            .collect();
        // 14 vertices, 9 faces: Euler gives 21 edges.
        let out = edges(&blocks);
        assert_eq!(out.len(), 21);
        // The three concave notch edges sit at 1 + e, inside the notch.
        let concave = out
            .iter()
            .filter(|e| {
                (0..3)
                    .filter(|&i| close(e.from[i], 1.0 + E) && close(e.to[i], 1.0 + E))
                    .count()
                    == 2
            })
            .count();
        assert_eq!(concave, 3);
    }

    #[test]
    fn disconnected_cubes() {
        assert_eq!(edges(&[[0, 0, 0], [2, 0, 0]]).len(), 24);
        assert_eq!(edges(&[[0, 0, 0], [5, 5, 5]]).len(), 24);
    }

    #[test]
    fn duplicates_and_order_do_not_matter() {
        let a = extract_merged_edges(&[[0, 0, 0], [1, 0, 0]], E);
        let b = extract_merged_edges(&[[1, 0, 0], [0, 0, 0], [1, 0, 0]], E);
        assert_eq!(a, b);
    }
}
