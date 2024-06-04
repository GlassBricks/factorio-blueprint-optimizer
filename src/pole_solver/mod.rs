use std::error::Error;

use hashbrown::{HashMap, HashSet};
use petgraph::prelude::*;

pub use connections::*;
pub use set_cover_ilp::*;

use crate::better_bp::EntityId;
use crate::pole_graph::CandPoleGraph;

mod connections;
mod min_scored;
mod set_cover_ilp;

/// A solver for the pole cover problem: given a pole graph, find a subgraph
/// of poles that still powers all entities and has the minimum cost.
pub trait PoleCoverSolver {
    fn solve<'a>(&self, graph: &CandPoleGraph) -> Result<CandPoleGraph, Box<dyn Error + 'a>>;
}

pub fn get_pole_coverage_dict(graph: &CandPoleGraph) -> HashMap<EntityId, HashSet<NodeIndex>> {
    let mut entity_coverage = HashMap::new();
    for idx in graph.node_indices() {
        let node = &graph[idx];
        for entity_id in &node.powered_entities {
            entity_coverage
                .entry(*entity_id)
                .or_insert_with(HashSet::new)
                .insert(idx);
        }
    }
    entity_coverage
}

#[cfg(test)]
mod tests {
    use euclid::point2;
    use hashbrown::HashSet;

    use crate::bp_model::BpModel;
    use crate::pole_graph::ToCandidatePoleGraph;

    #[test]
    fn test_get_pole_coverage_dict() {
        let mut model = BpModel::new();
        let p1 = model.add_test_pole(point2(0, 0));
        let p2 = model.add_test_pole(point2(4, 1));
        let e1 = model.add_test_powerable(point2(-2, 1));
        let e2 = model.add_test_powerable(point2(2, 1));
        let e3 = model.add_test_powerable(point2(6, 2));

        let (graph, idx_map) = model.get_maximally_connected_pole_graph();
        let graph = graph.to_cand_pole_graph(&model);
        let entity_coverage = super::get_pole_coverage_dict(&graph);
        assert_eq!(entity_coverage.len(), 3);

        assert_eq!(entity_coverage[&e1], HashSet::from([idx_map[&p1]]));
        assert_eq!(
            entity_coverage[&e2],
            HashSet::from([idx_map[&p1], idx_map[&p2]])
        );
        assert_eq!(entity_coverage[&e3], HashSet::from([idx_map[&p2]]));
    }
}
