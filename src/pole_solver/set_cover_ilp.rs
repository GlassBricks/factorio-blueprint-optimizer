use std::collections::BTreeMap;
use std::error::Error;

use good_lp::variable::UnsolvedProblem;
use good_lp::*;
use good_lp::solvers::highs::{HighsProblem};
use hashbrown::HashSet;
use itertools::Itertools;
use log::warn;
use petgraph::prelude::*;

use crate::pole_graph::CandPoleGraph;
use crate::pole_solver::{get_pole_coverage_dict, PoleCoverSolver};
use crate::position::{BoundingBox, BoundingBoxExt};

type M = HighsProblem;

pub struct SetCoverILPSolver<'a> {
    pub solver: &'a dyn Fn(UnsolvedProblem) -> M,
    pub config: &'a dyn Fn(M) -> Result<M, Box<dyn Error>>,
    pub cost: &'a dyn Fn(&CandPoleGraph, NodeIndex) -> f64,
    pub connectivity: Option<DistanceConnectivity>,
}

/// A constraint to ensures that poles are connected. Might not be optimal.
/// 
/// The idea/heuristic is that every pole must be connected to some pole more "central" to it.
///
/// Some "root" poles are selected based on the root_location; then distance to all other poles is calculated.
/// Adds constraint that if a pole is selected, at least one entity closer to the root pole must be selected.
///
/// This currently uses Euclidean distance as the distance metric.
pub struct DistanceConnectivity {
    pub center_rel_pos: (f64, f64),
}

impl DistanceConnectivity {
    fn maximal_clique(
        graph: &CandPoleGraph,
        nodes: impl IntoIterator<Item = NodeIndex>,
    ) -> Vec<NodeIndex> {
        let mut clique = vec![];
        for node in nodes {
            if clique.iter().all(|&c| graph.contains_edge(c, node)) {
                clique.push(node);
            }
        }
        clique
    }

    pub fn find_root_poles(&self, graph: &CandPoleGraph) -> Vec<NodeIndex> {
        let bbox = BoundingBox::from_points(graph.node_weights().map(|p| p.entity.position));
        let pt = bbox.relative_pt_at(self.center_rel_pos);
        let closest_poles = graph.node_indices().sorted_by_cached_key(|idx| {
            ((graph[*idx].entity.position - pt).square_length() * 64.0 * 64.0).round() as u64
        });
        Self::maximal_clique(graph, closest_poles)
    }

    fn connectivity_constraints(
        &self,
        graph: &CandPoleGraph,
        pole_vars: &BTreeMap<NodeIndex, Variable>,
    ) -> Vec<Constraint> {
        let root_poles = self
            .find_root_poles(graph)
            .into_iter()
            .collect::<HashSet<_>>();
        let pole1 = *root_poles.iter().next().unwrap();
        use petgraph::algo::dijkstra;
        let distances = dijkstra(&graph, pole1, None, |edge| {
            if root_poles.contains(&edge.target()) {
                0.0
            } else {
                *edge.weight() + 3.0
            }
        });
        let mut result = vec![];
        let mut connected = true;
        for pole in pole_vars.keys() {
            if root_poles.contains(pole) {
                continue;
            }
            let this_dist = distances.get(pole).cloned();
            if this_dist.is_none() {
                connected = false;
                continue;
            }
            let neighbors = graph
                .neighbors(*pole)
                .filter(|n| distances[n] < this_dist.unwrap())
                // .sorted_by(|a, b| distances[a].partial_cmp(&distances[b]).unwrap())
                .map(|n| pole_vars[&n]);
            let var_sum: Option<Expression> = neighbors.sum1();
            if let Some(var_sum) = var_sum {
                result.push(constraint!(pole_vars[pole] <= var_sum));
            }
        }
        if !connected {
            warn!("The pole graph is not connected!");
        }
        result
    }
}

impl SetCoverILPSolver<'_> {
    fn add_set_cover_constraints(
        &self,
        graph: &CandPoleGraph,
        pole_vars: &BTreeMap<NodeIndex, Variable>,
    ) -> Vec<Constraint> {
        get_pole_coverage_dict(graph)
            .into_iter()
            .map(|(_, poles)| {
                let var_sum: Expression = poles.iter().map(|idx| pole_vars[idx]).sum();
                constraint!(var_sum >= 1)
            })
            .collect()
    }
}

impl PoleCoverSolver for SetCoverILPSolver<'_> {
    fn solve<'a>(&self, graph: &CandPoleGraph) -> Result<CandPoleGraph, Box<dyn Error + 'a>> {
        let mut vars = ProblemVariables::new();

        let pole_vars = graph
            .node_indices()
            .map(|idx| {
                let var = variable().binary().name(format!("pole_{}", idx.index()));
                (idx, vars.add(var))
            })
            .collect::<BTreeMap<_, _>>();

        let cost_expr: Expression = pole_vars
            .iter()
            .map(|(id, var)| var.into_expression() * (self.cost)(graph, *id))
            .sum();

        // println!("num vars: {}", vars.len());

        let mut problem = (self.solver)(vars.minimise(cost_expr));

        for constraint in self.add_set_cover_constraints(graph, &pole_vars) {
            problem.add_constraint(constraint);
        }
        if let Some(connectivity) = &self.connectivity {
            for constraint in connectivity.connectivity_constraints(graph, &pole_vars) {
                problem.add_constraint(constraint);
            }
        }

        let problem = (self.config)(problem)?;

        let solution = problem.solve()?;

        let subgraph: CandPoleGraph = graph.filter_map(
            |idx, entity| {
                if solution.value(pole_vars[&idx]) > 0.5 {
                    Some(entity.clone())
                } else {
                    None
                }
            },
            |_, w| Some(*w),
        );
        Ok(subgraph)
    }
}

#[cfg(test)]
mod test {
    use euclid::point2;
    use hashbrown::HashSet;

    use crate::bp_model::test_util::small_pole_prototype;
    use crate::bp_model::BpModel;
    use crate::pole_graph::ToCandidatePoleGraph;

    use super::*;

    #[test]
    fn test_simple_instance() {
        let mut model = BpModel::new();
        let e1 = model.add_test_powerable(point2(-2, 1));
        let e2 = model.add_test_powerable(point2(2, 1));
        let e3 = model.add_test_powerable(point2(6, 2));

        let graph = model
            .with_all_candidate_poles(model.get_bounding_box(), &[&small_pole_prototype()])
            .get_maximally_connected_pole_graph()
            .0
            .to_cand_pole_graph(&model);

        let solver = SetCoverILPSolver {
            solver: &highs,
            config: &Ok,
            cost: &|_, _| 1.0,
            connectivity: None,
        };
        let subgraph = solver.solve(&graph).unwrap();

        let powered_entities = subgraph
            .node_indices()
            .flat_map(|idx| subgraph[idx].powered_entities.iter())
            .cloned()
            .collect::<HashSet<_>>();

        assert_eq!(powered_entities, HashSet::from([e1, e2, e3]));
    }
}
