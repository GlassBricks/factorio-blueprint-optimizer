use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::hash::Hash;

use clap::builder::TypedValueParser;
use itertools::Itertools;
use noisy_float::types::R64;

use factorio_blueprint::objects as fbp;
use factorio_blueprint::objects::{
    Blueprint, Color, Connection, ControlBehavior, EntityFilterMode, EntityNumber, EntityPriority,
    EntityType, GraphicsVariation, InfinitySettings, Inventory, ItemFilter, ItemRequest,
    ItemStackIndex, LogisticFilter, Prototype, SpeakerAlertParameter, SpeakerParameter,
};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct Position {
    pub x: R64,
    pub y: R64,
}

impl From<&fbp::Position> for Position {
    fn from(pos: &fbp::Position) -> Self {
        Self { x: pos.x, y: pos.y }
    }
}

impl From<&Position> for fbp::Position {
    fn from(pos: &Position) -> Self {
        Self { x: pos.x, y: pos.y }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, Ord, PartialOrd)]
pub struct EntityId(u32);

#[derive(Clone)]
pub struct BlueprintEntityData {
    pub name: Prototype,
    pub position: Position,
    pub direction: Option<u8>,

    pub orientation: Option<R64>,
    pub control_behavior: Option<ControlBehavior>,
    pub items: Option<ItemRequest>,
    pub recipe: Option<Prototype>,
    pub bar: Option<ItemStackIndex>,
    pub inventory: Option<Inventory>,
    pub infinity_settings: Option<InfinitySettings>,
    pub type_: Option<EntityType>,
    pub input_priority: Option<EntityPriority>,
    pub output_priority: Option<EntityPriority>,
    pub filter: Option<Prototype>,
    pub filters: Option<Vec<ItemFilter>>,
    pub filter_mode: Option<EntityFilterMode>,
    pub override_stack_size: Option<u8>,
    pub drop_position: Option<Position>,
    pub pickup_position: Option<Position>,
    pub request_filters: Option<Vec<LogisticFilter>>,
    pub request_from_buffers: bool,
    pub parameters: Option<SpeakerParameter>,
    pub alert_parameters: Option<SpeakerAlertParameter>,
    pub auto_launch: bool,
    pub variation: Option<GraphicsVariation>,
    pub color: Option<Color>,
    pub station: Option<String>,
    pub switch_state: bool,
    pub manual_trains_limit: Option<u32>,
}

trait SkipNone {
    fn skip_none(&mut self, name: &str, value: &Option<impl Debug>) -> &mut Self;
    fn skip_false(&mut self, name: &str, value: bool) -> &mut Self;
}

impl SkipNone for std::fmt::DebugStruct<'_, '_> {
    fn skip_none(&mut self, name: &str, value: &Option<impl Debug>) -> &mut Self {
        if let Some(value) = value {
            self.field(name, value);
        }
        self
    }
    fn skip_false(&mut self, name: &str, value: bool) -> &mut Self {
        if value {
            self.field(name, &value);
        }
        self
    }
}

impl Debug for BlueprintEntityData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BlueprintEntityData")
            .field("name", &self.name)
            .field("position", &self.position)
            .skip_none("direction", &self.direction)
            .skip_none("orientation", &self.orientation)
            .skip_none("control_behavior", &self.control_behavior)
            .skip_none("items", &self.items)
            .skip_none("recipe", &self.recipe)
            .skip_none("bar", &self.bar)
            .skip_none("inventory", &self.inventory)
            .skip_none("infinity_settings", &self.infinity_settings)
            .skip_none("type_", &self.type_)
            .skip_none("input_priority", &self.input_priority)
            .skip_none("output_priority", &self.output_priority)
            .skip_none("filter", &self.filter)
            .skip_none("filters", &self.filters)
            .skip_none("filter_mode", &self.filter_mode)
            .skip_none("override_stack_size", &self.override_stack_size)
            .skip_none("drop_position", &self.drop_position)
            .skip_none("pickup_position", &self.pickup_position)
            .skip_none("request_filters", &self.request_filters)
            .skip_false("request_from_buffers", self.request_from_buffers)
            .skip_none("parameters", &self.parameters)
            .skip_none("alert_parameters", &self.alert_parameters)
            .skip_false("auto_launch", self.auto_launch)
            .skip_none("variation", &self.variation)
            .skip_none("color", &self.color)
            .skip_none("station", &self.station)
            .skip_false("switch_state", self.switch_state)
            .skip_none("manual_trains_limit", &self.manual_trains_limit)
            .finish()
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub enum WireColor {
    Red,
    Green,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct ConnectionPointId {
    pub entity_id: EntityId,
    /// 1 for almost everything, 2 for combinator output
    pub circuit_id: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
pub struct OutgoingConnection {
    pub dest: ConnectionPointId,
    pub color: WireColor,
}

pub struct ConnectionPoint(Option<HashSet<OutgoingConnection>>);

impl ConnectionPoint {
    pub fn iter(&self) -> impl Iterator<Item = &OutgoingConnection> {
        self.0.as_ref().into_iter().flat_map(|set| set.iter())
    }
    pub fn has_any(&self) -> bool {
        self.0.is_some()
    }

    fn add_connection(&mut self, connection: OutgoingConnection) {
        self.0.get_or_insert_with(HashSet::new).insert(connection);
    }
    fn clear_if_empty(&mut self) {
        if let Some(set) = &mut self.0 {
            if set.is_empty() {
                self.0 = None;
            }
        }
    }
    fn remove_connection(&mut self, connection: &OutgoingConnection) {
        if let Some(set) = &mut self.0 {
            set.remove(connection);
        }
        self.clear_if_empty();
    }
    fn clear(&mut self) {
        self.0 = None;
    }
}

impl Debug for ConnectionPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(connections) = &self.0 {
            f.debug_set().entries(connections.iter()).finish()
        } else {
            f.write_str("None")
        }
    }
}

#[derive(Debug)]
pub struct BlueprintEntity {
    id: EntityId,
    pub data: BlueprintEntityData,
    pub connections: (ConnectionPoint, ConnectionPoint),
    pub neighbours: Option<HashSet<EntityId>>,
}

impl BlueprintEntity {
    pub fn new(id: EntityId, data: BlueprintEntityData) -> Self {
        Self {
            id,
            data,
            connections: (ConnectionPoint(None), ConnectionPoint(None)),
            neighbours: None,
        }
    }
    pub fn id(&self) -> EntityId {
        self.id
    }
    pub fn connection_pt(&self, pt_id: bool) -> &ConnectionPoint {
        match pt_id {
            false => &self.connections.0,
            true => &self.connections.1,
        }
    }
    fn connection_pt_mut(&mut self, pt_id: bool) -> &mut ConnectionPoint {
        match pt_id {
            false => &mut self.connections.0,
            true => &mut self.connections.1,
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct BlueprintEntities {
    pub entities: HashMap<EntityId, BlueprintEntity>,
}

impl BlueprintEntities {
    pub fn from_blueprint(bp: &Blueprint) -> Self {
        let entities: HashMap<EntityId, BlueprintEntity> = bp
            .entities
            .iter()
            .map(move |entity| {
                let id = EntityId(entity.entity_number.get() as u32);
                let result = BlueprintEntity::new(
                    id,
                    BlueprintEntityData {
                        name: entity.name.clone(),
                        position: (&entity.position).into(),
                        direction: entity.direction,
                        orientation: entity.orientation,
                        control_behavior: entity.control_behavior.clone(),
                        items: entity.items.clone(),
                        recipe: entity.recipe.clone(),
                        bar: entity.bar,
                        inventory: entity.inventory.clone(),
                        infinity_settings: entity.infinity_settings.clone(),
                        type_: entity.type_,
                        input_priority: entity.input_priority,
                        output_priority: entity.output_priority,
                        filter: entity.filter.clone(),
                        filters: entity.filters.clone(),
                        filter_mode: entity.filter_mode,
                        override_stack_size: entity.override_stack_size,
                        drop_position: entity.drop_position.as_ref().map(|pos| pos.into()),
                        pickup_position: entity.pickup_position.as_ref().map(|pos| pos.into()),
                        request_filters: entity.request_filters.clone(),
                        request_from_buffers: entity.request_from_buffers.unwrap_or(false),
                        parameters: entity.parameters.clone(),
                        alert_parameters: entity.alert_parameters.clone(),
                        auto_launch: entity.auto_launch.unwrap_or(false),
                        variation: entity.variation,
                        color: entity.color.clone(),
                        station: entity.station.clone(),
                        switch_state: entity.switch_state.unwrap_or(false),
                        manual_trains_limit: entity.manual_trains_limit,
                    },
                );
                (id, result)
            })
            .collect();

        let mut res = Self { entities };
        for bp_entity in &bp.entities {
            if bp_entity.neighbours.is_none() {
                continue;
            }
            let id = EntityId(bp_entity.entity_number.get() as u32);
            let neighbors = bp_entity.neighbours.as_ref().unwrap();
            for neighbor in neighbors {
                res.add_cable_connection(id, EntityId(neighbor.get() as u32));
            }
        }

        let import_connections =
            |src: &mut BlueprintEntity, connections: &fbp::EntityConnections| {
                let add_colors =
                    |pt: &mut ConnectionPoint,
                     color: WireColor,
                     data: &Option<Vec<fbp::ConnectionData>>| {
                        if let Some(data) = data {
                            for connection in data {
                                pt.add_connection(OutgoingConnection {
                                    dest: ConnectionPointId {
                                        entity_id: EntityId(connection.entity_id.get() as u32),
                                        circuit_id: connection.circuit_id.unwrap_or(1) == 2,
                                    },
                                    color,
                                });
                            }
                        }
                    };

                let mut add_pt = |pt: &mut ConnectionPoint, data: &fbp::ConnectionPoint| {
                    add_colors(pt, WireColor::Red, &data.red);
                    add_colors(pt, WireColor::Green, &data.green);
                };
                use factorio_blueprint::objects::Connection::{Multiple, Single};
                use factorio_blueprint::objects::EntityConnections::{NumberIdx, StringIdx};
                let mut map_connections =
                    |pt: &mut ConnectionPoint, connection: &fbp::Connection| match connection {
                        Single(data) => add_pt(pt, data),
                        Multiple(_) => panic!("This is just wrong??"),
                    };
                let (p1, p2) = match connections {
                    StringIdx(map) => (map.get("1"), map.get("2")),
                    NumberIdx(map) => (
                        map.get(&EntityNumber::new(1).unwrap()),
                        map.get(&EntityNumber::new(2).unwrap()),
                    ),
                };
                if let Some(p1) = p1 {
                    map_connections(&mut src.connections.0, p1);
                }
                if let Some(p2) = p2 {
                    map_connections(&mut src.connections.1, p2);
                }
            };

        for bp_entity in &bp.entities {
            let id = EntityId(bp_entity.entity_number.get() as u32);
            let mut entity = res.get_mut(id).unwrap();
            if let Some(connections) = &bp_entity.connections {
                import_connections(&mut entity, connections);
            }
        }

        res
    }

    pub fn to_blueprint_entities(&self) -> Vec<fbp::Entity> {
        let mut sorted_entities = self.entities.values().collect::<Vec<_>>();
        sorted_entities.sort_by_key(|entity| entity.id);

        let id_to_new = sorted_entities
            .iter()
            .enumerate()
            .map(|(i, entity)| (entity.id, EntityNumber::new(i + 1).unwrap()))
            .collect::<HashMap<_, _>>();

        let new_entities = sorted_entities
            .iter()
            .map(|(old_entity)| fbp::Entity {
                entity_number: id_to_new[&old_entity.id],
                name: old_entity.data.name.clone(),
                position: (&old_entity.data.position).into(),
                direction: old_entity.data.direction,
                orientation: old_entity.data.orientation,
                control_behavior: old_entity.data.control_behavior.clone(),
                items: old_entity.data.items.clone(),
                recipe: old_entity.data.recipe.clone(),
                bar: old_entity.data.bar,
                inventory: old_entity.data.inventory.clone(),
                infinity_settings: old_entity.data.infinity_settings.clone(),
                type_: old_entity.data.type_,
                input_priority: old_entity.data.input_priority,
                output_priority: old_entity.data.output_priority,
                filter: old_entity.data.filter.clone(),
                filters: old_entity.data.filters.clone(),
                filter_mode: old_entity.data.filter_mode,
                override_stack_size: old_entity.data.override_stack_size,
                drop_position: old_entity.data.drop_position.as_ref().map(|pos| pos.into()),
                pickup_position: old_entity
                    .data
                    .pickup_position
                    .as_ref()
                    .map(|pos| pos.into()),
                request_filters: old_entity.data.request_filters.clone(),
                request_from_buffers: if old_entity.data.request_from_buffers {
                    Some(true)
                } else {
                    None
                },
                parameters: old_entity.data.parameters.clone(),
                alert_parameters: old_entity.data.alert_parameters.clone(),
                auto_launch: if old_entity.data.auto_launch {
                    Some(true)
                } else {
                    None
                },
                variation: old_entity.data.variation,
                color: old_entity.data.color.clone(),
                station: old_entity.data.station.clone(),
                manual_trains_limit: old_entity.data.manual_trains_limit,
                switch_state: if old_entity.data.switch_state {
                    Some(true)
                } else {
                    None
                },
                connections: {
                    if !old_entity.connections.0.has_any() && !old_entity.connections.1.has_any() {
                        None
                    } else {
                        let map_pts = |pts: &Vec<OutgoingConnection>| {
                            let vec: Vec<fbp::ConnectionData> = pts
                                .iter()
                                .map(|conn| fbp::ConnectionData {
                                    entity_id: id_to_new[&conn.dest.entity_id],
                                    circuit_id: if conn.dest.circuit_id { Some(2) } else { None },
                                    wire_id: None,
                                })
                                .sorted_by_key(|conn| (conn.entity_id, conn.circuit_id))
                                .collect();
                            if vec.is_empty() {
                                None
                            } else {
                                Some(vec)
                            }
                        };
                        let map_pt = |pt: &ConnectionPoint| {
                            let (red, green) = pt
                                .iter()
                                .partition::<Vec<_>, _>(|conn| conn.color == WireColor::Red);
                            let (red, green) = (map_pts(&red), map_pts(&green));
                            if red.is_none() && green.is_none() {
                                None
                            } else {
                                Some(Connection::Single(fbp::ConnectionPoint { red, green }))
                            }
                        };
                        let pt1 = map_pt(&old_entity.connections.0);
                        let pt2 = map_pt(&old_entity.connections.1);
                        if pt1.is_none() && pt2.is_none() {
                            None
                        } else {
                            Some(fbp::EntityConnections::StringIdx({
                                let mut map = HashMap::new();
                                if let Some(pt1) = pt1 {
                                    map.insert("1".into(), pt1);
                                }
                                if let Some(pt2) = pt2 {
                                    map.insert("2".into(), pt2);
                                }
                                map
                            }))
                        }
                    }
                },
                neighbours: old_entity
                    .neighbours
                    .as_ref()
                    .map(|neigh| neigh.iter().map(|id| id_to_new[id])
                        .sorted()
                        .collect()),
            })
            .collect();

        new_entities
    }
}

impl BlueprintEntities {
    pub fn has_id(&self, id: EntityId) -> bool {
        self.entities.contains_key(&id)
    }

    pub fn add_wire_connection(
        &mut self,
        p1: ConnectionPointId,
        p2: ConnectionPointId,
        color: WireColor,
    ) -> bool {
        if p1 == p2 || !self.has_id(p1.entity_id) || !self.has_id(p2.entity_id) {
            return false;
        }
        let e1 = self.get_mut(p1.entity_id).unwrap();
        e1.connection_pt_mut(p1.circuit_id)
            .add_connection(OutgoingConnection { dest: p2, color });
        let e2 = self.get_mut(p2.entity_id).unwrap();
        e2.connection_pt_mut(p2.circuit_id)
            .add_connection(OutgoingConnection { dest: p1, color });
        true
    }

    pub fn add_cable_connection(&mut self, entity1: EntityId, entity2: EntityId) -> bool {
        if entity1 == entity2 || !self.has_id(entity1) || !self.has_id(entity2) {
            return false;
        }
        let e1 = self.entities.get_mut(&entity1).unwrap();
        e1.neighbours
            .get_or_insert_with(HashSet::new)
            .insert(entity2);
        let e2 = self.entities.get_mut(&entity2).unwrap();
        e2.neighbours
            .get_or_insert_with(HashSet::new)
            .insert(entity1);
        true
    }

    fn get(&self, id: EntityId) -> Option<&BlueprintEntity> {
        self.entities.get(&id)
    }
    fn get_mut(&mut self, id: EntityId) -> Option<&mut BlueprintEntity> {
        self.entities.get_mut(&id)
    }

    fn entities(&self) -> &HashMap<EntityId, BlueprintEntity> {
        &self.entities
    }
}

//
// struct EntityRef<'a> {
//     id: EntityId,
//     entity: &'a mut BlueprintEntity,
//     pub parent: &'a mut BlueprintEntities,
// }
//
// impl<'a> Debug for EntityRef<'a> {
//     fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("EntityRef")
//             .field("id", &self.id)
//             .finish()
//     }
// }
//
// impl<'a> EntityRef<'a> {
//     fn get(&self) -> Option<&BlueprintEntity> {
//         self.parent.entities.get(&self.id)
//     }
//     fn get_mut(&mut self) -> Option<&mut BlueprintEntity> {
//         self.parent.entities.get_mut(&self.id)
//     }
// }
