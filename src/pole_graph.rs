use std::borrow::Borrow;
use euclid::vec2;
use hashbrown::{HashMap, HashSet};
use itertools::Itertools;
use petgraph::prelude::*;

use crate::better_bp::EntityId;
use crate::bp_model::{BpModel, WorldEntity};
use crate::position::{ContractMax, IterTiles, MapPosition, TileBoundingBox, TileSpaceExt};
use crate::prototype_data::EntityPrototypeRef;

#[derive(Debug, Clone)]
pub struct CandPoleNode {
    pub entity: WorldEntity,
    pub powered_entities: HashSet<EntityId>,
}

impl Borrow<WorldEntity> for CandPoleNode {
    fn borrow(&self) -> &WorldEntity {
        &self.entity
    }
}

pub type CandPoleGraph = UnGraph<CandPoleNode, f64>;

pub type PoleGraph = UnGraph<WorldEntity, f64>;

impl BpModel {
    /// A graph of all poles, with no connections between them.
    pub fn get_disconnected_pole_graph(&self) -> (CandPoleGraph, HashMap<EntityId, NodeIndex>) {
        let mut graph = CandPoleGraph::new_undirected();
        let mut id_map = HashMap::new();
        for (id, entity) in self.all_entities() {
            let pole_data = entity.pole_data();
            if pole_data.is_none() {
                continue;
            }
            let (pole_prototype, _) = pole_data.unwrap();
            let idx = graph.add_node(CandPoleNode {
                entity: entity.data.clone(),
                powered_entities: self
                    .powered_entities(entity.position, pole_prototype)
                    .map(|e| e.id())
                    .collect(),
            });
            id_map.insert(*id, idx);
        }
        (graph, id_map)
    }

    /// Graph of existing poles and connections.
    #[allow(dead_code)]
    pub fn get_current_pole_graph(&self) -> (CandPoleGraph, HashMap<EntityId, NodeIndex>) {
        let (mut graph, id_map) = self.get_disconnected_pole_graph();
        for (id, entity) in self.all_entities() {
            let pole_data = entity.pole_data();
            if pole_data.is_none() {
                continue;
            }
            let (_, connections) = pole_data.unwrap();
            let idx = id_map[id];
            for other_id in &connections.connections {
                if other_id < id {
                    continue;
                }
                let other_idx = id_map[other_id];
                let distance = graph[idx]
                    .entity
                    .position
                    .distance_to(graph[other_idx].entity.position);
                graph.add_edge(idx, other_idx, distance);
            }
        }
        (graph, id_map)
    }

    pub fn get_maximally_connected_pole_graph(
        &self,
    ) -> (CandPoleGraph, HashMap<EntityId, NodeIndex>) {
        let (mut graph, id_map) = self.get_disconnected_pole_graph();
        self.maximally_connect_poles(&mut graph, &id_map);
        (graph, id_map)
    }

    pub fn maximally_connect_poles<N>(
        &self,
        graph: &mut UnGraph<N, f64>,
        entity_map: &HashMap<EntityId, NodeIndex>,
    ) {
        for (id, entity) in self.all_entities() {
            let pole_data = entity.pole_data();
            if pole_data.is_none() {
                continue;
            }
            let (pole_prototype, _) = pole_data.unwrap();
            let idx = entity_map[id];
            for other_entity in self
                .connectable_poles(entity.position, pole_prototype)
                .collect_vec()
            {
                if other_entity.id() <= entity.id() {
                    continue;
                }
                let other_idx = entity_map[&other_entity.id()];
                let distance = entity.position.distance_to(other_entity.position);
                graph.update_edge(idx, other_idx, distance);
            }
        }
    }

    /// Gets a new model which also contains all poles that may be placed in the given area.
    /// Candidate poles may overlap, if multiple prototypes are given.
    /// See also: `get_maximally_connected_pole_graph`.
    pub fn with_all_candidate_poles(
        &self,
        area: TileBoundingBox,
        pole_prototypes: &[&EntityPrototypeRef],
    ) -> BpModel {
        let mut pole_model = self.clone();
        for pole_prototype in pole_prototypes {
            assert_eq!(
                pole_prototype.tile_width, pole_prototype.tile_height,
                "Non-square poles not supported yet"
            );
            let width = pole_prototype.tile_width;
            let possible_area = area.contract_max((width - 1) as i32);
            for top_left in possible_area.iter_tiles() {
                let pos = top_left.corner_map_pos() + vec2(width as f64 / 2.0, width as f64 / 2.0);
                let entity = WorldEntity {
                    position: pos,
                    direction: 0,
                    prototype: (*pole_prototype).clone(),
                };
                if self.can_place(&entity) {
                    pole_model.add_overlap(entity);
                }
            }
        }
        pole_model
    }
}

impl BpModel {
    pub fn add_from_pole_graph(&mut self, graph: &CandPoleGraph) {
        let added_ids = graph
            .node_indices()
            .map(|idx| {
                let pole = &graph[idx];
                (idx, self.add_no_overlap(pole.entity.clone()))
            })
            .collect::<HashMap<_, _>>();
        for edge in graph.edge_indices() {
            let (a, b) = graph.edge_endpoints(edge).unwrap();
            let a_id = added_ids[&a];
            let b_id = added_ids[&b];
            if let (Some(a), Some(b)) = (a_id, b_id) {
                self.add_cable_connection(a, b);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bp_model::test_util::small_pole_prototype;
    use euclid::point2;
    use itertools::Itertools;

    #[test]
    fn test_pole_graph() {
        let mut model = BpModel::new();
        let p1 = model.add_test_pole(point2(0, 0));
        let p2 = model.add_test_pole(point2(4, 1));
        let p3 = model.add_test_pole(point2(6, 2));
        model.add_cable_connection(p1, p2);
        let e1 = model.add_test_powerable(point2(-2, 1));

        let test_nodes_correct = |graph: &CandPoleGraph, idx_map: &HashMap<EntityId, NodeIndex>| {
            assert_eq!(graph.node_count(), 3);
            assert_eq!(idx_map.len(), 3);
            let i1 = idx_map[&p1];
            let n1 = &graph[i1];
            assert_eq!(n1.entity, model.get(p1).unwrap().data);
            assert_eq!(n1.powered_entities, HashSet::from([e1]));
            let i2 = idx_map[&p2];
            let n2 = &graph[i2];
            assert_eq!(n2.entity, model.get(p2).unwrap().data);
            assert_eq!(n2.powered_entities, HashSet::new());
            let i3 = idx_map[&p3];
            let n3 = &graph[i3];
            assert_eq!(n3.entity, model.get(p3).unwrap().data);
            (i1, i2, i3)
        };

        let (graph, idx_map) = model.get_disconnected_pole_graph();
        assert_eq!(graph.edge_count(), 0);
        test_nodes_correct(&graph, &idx_map);

        let (graph, idx_map) = model.get_current_pole_graph();
        let (i1, i2, i3) = test_nodes_correct(&graph, &idx_map);
        assert_eq!(graph.edge_count(), 1);
        assert_eq!(graph.neighbors(i1).collect_vec(), [i2]);
        assert_eq!(graph.neighbors(i2).collect_vec(), [i1]);
        assert_eq!(graph.neighbors(i3).collect_vec(), []);

        let (graph, idx_map) = model.get_maximally_connected_pole_graph();
        let (i1, i2, i3) = test_nodes_correct(&graph, &idx_map);
        assert_eq!(graph.edge_count(), 3);
        assert_eq!(
            graph.neighbors(i1).collect::<HashSet<_>>(),
            HashSet::from([i2, i3])
        );
        assert_eq!(
            graph.neighbors(i2).collect::<HashSet<_>>(),
            HashSet::from([i1, i3])
        );
        assert_eq!(
            graph.neighbors(i3).collect::<HashSet<_>>(),
            HashSet::from([i1, i2])
        );
    }

    #[test]
    fn test_with_all_candidate_poles() {
        let mut model = BpModel::new();
        let e1 = model.add_test_powerable(point2(0, 0));
        let e2 = model.add_test_powerable(point2(1, 1));
        let area = TileBoundingBox::new(point2(0, 0), point2(2, 2));
        let pole_prototype = small_pole_prototype();
        let model2 = model.with_all_candidate_poles(area, &[&pole_prototype]);
        let at_tile = |x, y| {
            model2
                .get_at_tile(point2(x, y))
                .map(|e| e.id())
                .collect_vec()
        };
        assert_eq!(at_tile(0, 0), [e1]);
        assert_eq!(at_tile(1, 1), [e2]);
        let m_at_tile = |x, y| model2.get_at_tile(point2(x, y)).collect_vec();
        let at1 = m_at_tile(1, 0);
        assert_eq!(at1.len(), 1);
        assert_eq!(at1[0].prototype, pole_prototype);
        assert_eq!(at1[0].position, point2(1, 0).center_map_pos());
        let at2 = m_at_tile(0, 1);
        assert_eq!(at2.len(), 1);
        assert_eq!(at2[0].prototype, pole_prototype);
        assert_eq!(at2[0].position, point2(0, 1).center_map_pos());
    }
}
