use std::cmp::max;
use std::collections::BinaryHeap;

use euclid::{Angle, Point2D};
use itertools::Itertools;
use num_traits::{Num, Signed};
use petgraph::prelude::*;
use petgraph::unionfind::UnionFind;
use petgraph::visit::{IntoNodeReferences, NodeIndexable};

use crate::pole_graph::WithPosition;
use crate::pole_solver::min_scored::MinScored;
use crate::position::MapPosition;

/// Given a pole graph, gets a graph with a subset of edges that looks nice.
pub trait PoleConnector<N: Clone> {
    /// Connects poles in the graph.
    /// Returns the new graph and a mapping from the old node indices to the new ones.
    fn connect_poles(&self, graph: &UnGraph<N, f64>) -> UnGraph<N, f64>;
}

const MAX_DEGREE: usize = 5;

/// Connects poles with a minimum spanning tree; however, prefers to keep the degree of nodes low.
pub struct WeightedMSTConnector;
impl<N: Clone> PoleConnector<N> for WeightedMSTConnector {
    fn connect_poles(&self, graph: &UnGraph<N, f64>) -> UnGraph<N, f64> {
        const DEGREE_MULT: [f64; MAX_DEGREE] = [1.0, 1.0, 1.0, 1.5, 5.0];

        let mut result = UnGraph::<N, f64>::new_undirected();
        // let node_map = graph
        //     .node_references()
        //     .map(|(idx, node)| (idx, result.add_node(node.clone())))
        //     .collect::<HashMap<_, _>>();
        for (idx, wt) in graph.node_references() {
            let idx2 = result.add_node(wt.clone());
            assert_eq!(idx.index(), idx2.index());
        }
        let mut sort_edges = BinaryHeap::with_capacity(
            (graph.edge_references().size_hint().0 as f64 * 1.5) as usize,
        );
        for edge in graph.edge_references() {
            let weight = *edge.weight();
            sort_edges.push(MinScored(weight, (weight, (edge.source(), edge.target()))));
        }

        let mut uf = UnionFind::new(result.node_bound());
        while let Some(MinScored(weight, (orig_weight, (source, target)))) = sort_edges.pop() {
            if uf.equiv(source.index(), target.index()) {
                continue;
            }
            let max_deg = max(
                result.neighbors(source).count(),
                result.neighbors(target).count(),
            );
            if max_deg >= MAX_DEGREE {
                continue;
            }
            let actual_weight = weight * DEGREE_MULT[max_deg];
            if actual_weight > weight {
                sort_edges.push(MinScored(actual_weight, (orig_weight, (source, target))));
            } else if uf.union(source.index(), target.index()) {
                result.add_edge(source, target, orig_weight);
            }
        }
        result
    }
}

/// Currently assumes that the input graph is maximally connected;
/// all poles that can connect have an edge between them.
/// (If not true, may produce crossings.)
pub struct PrettyPoleConnector {
    /// Any 2 edges must have an angle at least this large
    pub min_angle: Angle<f64>,
    /// Any 2 adjacent angles must sum to at least this large
    pub min_adjacent_angle: Angle<f64>,
}

impl PrettyPoleConnector {
    pub fn default() -> Self {
        Self {
            min_angle: Angle::degrees(30.0),
            min_adjacent_angle: Angle::degrees(100.0),
        }
    }
}

// fn is_left(base: MapPosition, a: MapPosition, b: MapPosition) -> bool {
fn is_left<T: Signed + Num + Copy, U>(
    base: Point2D<T, U>,
    a: Point2D<T, U>,
    b: Point2D<T, U>,
) -> bool {
    let cross = (a - base).cross(b - base);
    cross.is_positive()
}

// fn orientation(a: MapPosition, b: MapPosition, c: MapPosition) -> f64 {
fn orientation<T: Num + Copy, U>(a: Point2D<T, U>, b: Point2D<T, U>, c: Point2D<T, U>) -> T {
    (b - a).cross(c - a)
}

fn line_seg_intersects<T: Signed + Num + Copy, U>(
    a: Point2D<T, U>,
    b: Point2D<T, U>,
    c: Point2D<T, U>,
    d: Point2D<T, U>,
) -> bool {
    let o1 = orientation(a, b, c);
    let o2 = orientation(a, b, d);
    let o3 = orientation(c, d, a);
    let o4 = orientation(c, d, b);
    (o1.signum() != o2.signum()) && (o3.signum() != o4.signum())
}

impl PrettyPoleConnector {
    fn can_connect<N: WithPosition>(
        &self,
        cand_graph: &UnGraph<N, f64>,
        res_graph: &UnGraph<N, f64>,
        a: NodeIndex,
        b: NodeIndex,
    ) -> bool {
        if res_graph.contains_edge(a, b) {
            return false;
        }
        if res_graph.neighbors(a).count() >= MAX_DEGREE
            || res_graph.neighbors(b).count() >= MAX_DEGREE
        {
            return false;
        }
        // disallow crossing edges
        // assumption: if a edge c,d may cross a,b, then they are both neighbors of a,b
        let pos_a = cand_graph[a].position();
        let pos_b = cand_graph[b].position();
        let (left, right): (Vec<_>, _) = cand_graph
            .neighbors(a)
            .chain(cand_graph.neighbors(b))
            .unique()
            .filter(|&idx| idx != a && idx != b)
            .partition(|idx| is_left(pos_a, pos_b, cand_graph[*idx].position()));
        if left.into_iter().cartesian_product(right).any(|(l, r)| {
            res_graph.contains_edge(l, r)
                && line_seg_intersects(
                    pos_a,
                    pos_b,
                    cand_graph[l].position(),
                    cand_graph[r].position(),
                )
        }) {
            return false;
        }

        for (a, pos_a, ab) in [(a, pos_a, pos_b - pos_a), (b, pos_b, pos_a - pos_b)] {
            let angles = res_graph.neighbors(a).map(|n| {
                let ac = cand_graph[n].position() - pos_a;
                ab.angle_to(ac).radians
            }).collect_vec();
            if angles.iter().any(|&angle| angle.abs() < self.min_angle.radians.abs()) {
                return false;
            }
            let (n,p): (Vec<f64>,_) = angles.into_iter().partition(|&angle| angle < 0.0);
            let n_max = n.iter().max_by(|a,b| a.partial_cmp(b).unwrap());
            let p_min = p.iter().min_by(|a,b| a.partial_cmp(b).unwrap());
            if let (Some(n_max), Some(p_min)) = (n_max, p_min) {
                if (p_min - n_max).abs() < self.min_adjacent_angle.radians.abs() {
                    return false;
                }
            }
        }

        true
    }

    fn edge_weight(orig_weight: f64, src: MapPosition, tgt: MapPosition) -> f64 {
        let vec = (tgt - src).normalize();
        let axis_alightment = (vec.x.abs() - vec.y.abs()).powi(2);
        orig_weight / (1.0 + 2.0 * axis_alightment)
    }
}

impl<N: WithPosition + Clone> PoleConnector<N> for PrettyPoleConnector {
    fn connect_poles(&self, graph: &UnGraph<N, f64>) -> UnGraph<N, f64> {
        let mut result = WeightedMSTConnector.connect_poles(graph);
        let edges = graph
            .edge_references()
            .map(|edge| {
                let source = edge.source();
                let target = edge.target();
                let wt = *edge.weight();
                (
                    Self::edge_weight(wt, graph[source].position(), graph[target].position()),
                    wt,
                    source,
                    target,
                )
            })
            .sorted_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        for (_, orig_wt, source, target) in edges {
            if self.can_connect(graph, &result, source, target) {
                result.update_edge(source, target, orig_wt);
            }
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use euclid::point2;

    use crate::bp_model::BpModel;
    use crate::pole_solver::PrettyPoleConnector;
    use crate::position::TilePosition;

    use super::*;

    #[test]
    fn test_is_left() {
        assert!(is_left(point2::<_, ()>(0, 0), point2(1, 0), point2(0, 1)));

        assert!(!is_left(point2::<_, ()>(0, 0), point2(1, 0), point2(0, -1)))
    }

    static INTERSECTING_SEGS: [(TilePosition, TilePosition, TilePosition, TilePosition); 2] = [
        (point2(0, 0), point2(1, 1), point2(0, 1), point2(1, 0)),
        (point2(2, 2), point2(2, 5), point2(0, -1), point2(3, 6)),
    ];

    #[test]
    fn test_line_seg_intersects() {
        for (a, b, c, d) in INTERSECTING_SEGS {
            assert!(line_seg_intersects(a, b, c, d));
            assert!(!line_seg_intersects(a, c, b, d));
        }
    }

    #[test]
    fn test_does_not_allow_crossing() {
        for (a, b, c, d) in INTERSECTING_SEGS {
            let mut model = BpModel::new();
            let entities = model.add_test_poles(&[a, b, c, d]);
            let [a, b, c, d] = entities[..4] else {
                panic!()
            };
            model.add_cable_connection(a, b);
            let (cur, map) = model.get_current_pole_graph();
            // let cand = model.get_maximally_connected_pole_graph().0;
            let mut cand = cur.clone();
            model.maximally_connect_poles(&mut cand, &map);

            let connector = PrettyPoleConnector::default();

            let res = connector.can_connect(&cand, &cur, map[&c], map[&d]);

            assert!(!res);
        }
    }
}
