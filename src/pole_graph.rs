use hashbrown::{HashMap, HashSet};
use petgraph::graph::{NodeIndex, UnGraph};

use crate::better_bp::EntityId;
use crate::bp_model::{BpModel, WorldEntity};

#[derive(Debug, Clone)]
pub struct PoleNode {
    pub entity: WorldEntity,
    pub powered_entities: HashSet<EntityId>,
}
pub type PoleGraph = UnGraph<PoleNode, f64>;

impl BpModel {
    /// A graph of all poles, with no connections between them.
    pub fn get_disconnected_pole_graph(&self) -> (PoleGraph, HashMap<EntityId, NodeIndex>) {
        let mut graph = PoleGraph::new_undirected();
        let mut id_map = HashMap::new();
        for (id, entity) in self.all_entities() {
            let pole_data = entity.pole_data();
            if pole_data.is_none() {
                continue;
            }
            let (pole_prototype, _) = pole_data.unwrap();
            let idx = graph.add_node(PoleNode {
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
    pub fn get_current_pole_graph(&self) -> (PoleGraph, HashMap<EntityId, NodeIndex>) {
        let (mut graph, id_map) = self.get_disconnected_pole_graph();
        for (id, entity) in self.all_entities() {
            let pole_data = entity.pole_data();
            if pole_data.is_none() {
                continue;
            }
            let (_, connections) = pole_data.unwrap();
            let idx = id_map[id];
            for other_id in &connections.connections {
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
    
    pub fn get_maximally_connected_pole_graph(&self) -> (PoleGraph, HashMap<EntityId, NodeIndex>) {
        let (mut graph, id_map) = self.get_disconnected_pole_graph();
        for (id, entity) in self.all_entities() {
            let pole_data = entity.pole_data();
            if pole_data.is_none() {
                continue;
            }
            let (pole_prototype, _) = pole_data.unwrap();
            let idx = id_map[id];
            for other_entity in self.connectable_poles(entity.position, pole_prototype) {
                let other_idx = id_map[&other_entity.id()];
                let distance = entity.position.distance_to(other_entity.position);
                graph.add_edge(idx, other_idx, distance);
            }
        }
        (graph, id_map)
    }

    /*pub fn get_candidate_pole_graph(
        &self,
        area: TileBoundingBox,
        poles_to_use: &[EntityPrototypeRef],
    ) -> PoleGraph {
        let mut temp_grid = GridModel::new();
        for pole in poles_to_use {
            assert_eq!(
                pole.tile_width, pole.tile_height,
                "Non-square poles not supported yet"
            );
            let width = pole.tile_width * 2;
            let possible_area = area.contract_max((width - 1) as i32);
            for top_left in possible_area.iter_tiles() {
                let pos = top_left.corner_pos() + vec2((width / 2) as f64, (width / 2) as f64);
                if self.can_place(pole, pos) {
                    temp_grid.add(ModelEntity {
                        position: pos,
                        direction: 0,
                        prototype: pole.clone(),
                    });
                }
            }
        }
        temp_grid.get_existing_pole_graph()
    }*/
}
