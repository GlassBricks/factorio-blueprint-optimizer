use std::ops::Deref;

use euclid::*;
use hashbrown::{HashMap, HashSet};
use itertools::Itertools;

use crate::better_bp::{BlueprintEntities, BlueprintEntityData, EntityId};
use crate::position::{
    BoundingBox, BoundingBoxExt, CardinalDirection, IterTiles, MapPosition, Rotate,
    TileBoundingBox, TilePosition,
};
use crate::prototype_data::{EntityPrototypeDict, EntityPrototypeRef, PoleData};

#[derive(Debug, Clone, PartialEq)]
pub struct WorldEntity {
    pub prototype: EntityPrototypeRef,
    pub position: MapPosition,
    pub direction: u8,
}

impl WorldEntity {
    /**
     * Returns bbox, from the entity's perspective: (0,0) is the center of the entity.
     */
    pub fn local_bbox(&self) -> BoundingBox {
        let bbox = self.prototype.collision_box;
        bbox.rotate(CardinalDirection::from_u8_rounding(self.direction))
    }

    pub fn world_bbox(&self) -> BoundingBox {
        self.local_bbox().translate(self.position.to_vector())
    }

    pub fn uses_power(&self) -> bool {
        self.prototype.pole_data.is_none() && self.prototype.uses_power
    }
}

impl WorldEntity {
    pub fn from_bp_entity(
        prototype_dict: &EntityPrototypeDict,
        bp_entity: &BlueprintEntityData,
    ) -> Self {
        WorldEntity {
            prototype: prototype_dict[&bp_entity.name].clone(),
            position: bp_entity.position,
            direction: bp_entity.direction.unwrap_or(0),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ModelEntity {
    pub data: WorldEntity,
    id: EntityId,
    extra: EntityExtraData,
}

impl Deref for ModelEntity {
    type Target = WorldEntity;
    fn deref(&self) -> &WorldEntity {
        &self.data
    }
}
impl ModelEntity {
    // non-mut access only
    pub fn id(&self) -> EntityId {
        self.id
    }
}

#[derive(Debug, Clone)]
pub enum EntityExtraData {
    Pole(PoleConnections),
    None,
}

#[derive(Debug, Clone)]
pub struct PoleConnections {
    pub connections: HashSet<EntityId>,
}
impl ModelEntity {
    fn new_empty(id: EntityId, data: WorldEntity) -> Self {
        ModelEntity {
            id,
            extra: if data.prototype.pole_data.is_some() {
                EntityExtraData::Pole(PoleConnections {
                    connections: HashSet::new(),
                })
            } else {
                EntityExtraData::None
            },
            data,
        }
    }

    pub fn pole_data(&self) -> Option<(PoleData, &PoleConnections)> {
        match &self.extra {
            EntityExtraData::Pole(pole) => Some((self.prototype.pole_data.unwrap(), pole)),
            _ => None,
        }
    }

    fn pole_connections_mut(&mut self) -> Option<&mut PoleConnections> {
        match &mut self.extra {
            EntityExtraData::Pole(pole) => Some(pole),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct BpModel {
    by_tile: HashMap<TilePosition, Vec<EntityId>>,
    all_entities: HashMap<EntityId, ModelEntity>,
    next_id: EntityId,
}

impl BpModel {
    pub fn new() -> Self {
        BpModel {
            by_tile: HashMap::new(),
            all_entities: HashMap::new(),
            next_id: EntityId(1),
        }
    }
    pub fn from_bp_entities(
        bp: &BlueprintEntities,
        prototype_dict: &EntityPrototypeDict,
    ) -> BpModel {
        let mut res: BpModel = BpModel::new();
        for (id, entity) in bp.entities.iter() {
            res.add_internal(ModelEntity::new_empty(
                *id,
                WorldEntity::from_bp_entity(prototype_dict, &entity.data),
            ));
        }
        for (id, entity) in bp.entities.iter() {
            let neighbors = &entity.neighbours.as_ref();
            if neighbors.is_none() {
                continue;
            }
            for neighbor_id in neighbors.unwrap() {
                res.add_cable_connection(*id, *neighbor_id);
            }
        }
        res.next_id.0 = bp.entities.keys().max().map(|x| x.0).unwrap_or(0) + 1;
        res
    }

    fn add_internal(&mut self, entity: ModelEntity) {
        let id = entity.id;
        for tile in entity.world_bbox().iter_tiles() {
            self.by_tile.entry(tile).or_default().push(id);
        }
        if let Some(x) = self.all_entities.insert(id, entity) {
            panic!("Entity with id {:?} already exists: {:?}", id, x);
        }
    }

    pub fn add_overlap(&mut self, entity: WorldEntity) -> EntityId {
        let id = self.next_id;
        self.next_id.0 += 1;
        self.add_internal(ModelEntity::new_empty(id, entity));
        id
    }

    pub fn can_place(&self, entity: &WorldEntity) -> bool {
        entity
            .world_bbox()
            .iter_tiles()
            .all(|tile| !self.occupied(tile))
    }

    pub fn add_no_overlap(&mut self, entity: WorldEntity) -> Option<EntityId> {
        if entity
            .world_bbox()
            .iter_tiles()
            .all(|tile| !self.occupied(tile))
        {
            Some(self.add_overlap(entity))
        } else {
            None
        }
    }

    pub fn add_cable_connection(&mut self, id: EntityId, other_id: EntityId) -> Option<()> {
        let [this, other] = self.all_entities.get_many_mut([&id, &other_id])?;
        let max_dist = this
            .prototype
            .pole_data?
            .wire_distance
            .min(other.prototype.pole_data?.wire_distance);
        if (this.position - other.position).square_length() > max_dist * max_dist {
            return None;
        }
        let this_connections = this.pole_connections_mut()?;
        let other_connections = other.pole_connections_mut()?;
        this_connections.connections.insert(other_id);
        other_connections.connections.insert(id);
        Some(())
    }

    pub fn remove(&mut self, id: &EntityId) {
        let entity = self.all_entities.remove(id).unwrap();
        for tile in entity.world_bbox().iter_tiles() {
            let entities = self.by_tile.get_mut(&tile).unwrap();
            entities.retain(|x| x != id);
            if entities.is_empty() {
                self.by_tile.remove(&tile);
            }
        }
    }

    pub fn retain(&mut self, mut f: impl FnMut(&ModelEntity) -> bool) {
        let mut to_remove = Vec::new();
        for (id, entity) in &self.all_entities {
            if !f(entity) {
                to_remove.push(*id);
            }
        }
        for id in to_remove {
            self.remove(&id);
        }
    }

    pub fn occupied(&self, tile: TilePosition) -> bool {
        self.by_tile.contains_key(&tile)
    }

    pub fn by_tile(&self) -> &HashMap<TilePosition, Vec<EntityId>> {
        &self.by_tile
    }
    pub fn all_entities(&self) -> &HashMap<EntityId, ModelEntity> {
        &self.all_entities
    }

    pub fn get(&self, id: EntityId) -> Option<&ModelEntity> {
        self.all_entities.get(&id)
    }
    pub fn get_mut(&mut self, id: EntityId) -> Option<&mut ModelEntity> {
        self.all_entities.get_mut(&id)
    }

    pub fn get_at_tile(&self, tile: TilePosition) -> impl Iterator<Item = &ModelEntity> + '_ {
        self.by_tile
            .get(&tile)
            .map(|ids| ids.as_slice())
            .unwrap_or(&[])
            .iter()
            .map(move |id| &self.all_entities[id])
    }

    pub fn all_world_entities(&self) -> impl Iterator<Item = &WorldEntity> {
        self.all_entities.values().map(|entity| &entity.data)
    }

    pub fn get_bounding_box(&self) -> TileBoundingBox {
        let bbox = TileBoundingBox::from_points(self.by_tile.keys());
        TileBoundingBox::new(bbox.min, bbox.max + vec2(1, 1))
    }

    pub fn connectable_poles(
        &self,
        pole_pos: MapPosition,
        pole_data: PoleData,
    ) -> impl Iterator<Item = &ModelEntity> + '_ {
        let this_dist = pole_data.wire_distance;
        const EPS: f64 = 1e-6;
        BoundingBox::around_point(pole_pos, this_dist)
            .round_to_tiles_covering_center()
            .iter_tiles()
            .flat_map(|tile| self.get_at_tile(tile))
            .filter(move |entity| {
                entity.prototype.pole_data.is_some_and(|pd| {
                    let max_dist = this_dist.min(pd.wire_distance);
                    (pole_pos - entity.position).square_length() <= max_dist * max_dist + EPS
                })
            })
            .unique_by(|entity| entity.id)
    }

    pub fn powered_entities(
        &self,
        pole_pos: MapPosition,
        pole_data: PoleData,
    ) -> impl Iterator<Item = &ModelEntity> + '_ {
        let this_area_dist = pole_data.supply_radius;
        // poles in circle around map_pos with radius
        BoundingBox::around_point(pole_pos, this_area_dist)
            .round_out_to_tiles()
            .iter_tiles()
            .flat_map(|tile| self.get_at_tile(tile))
            .filter(|entity| entity.uses_power())
            .unique_by(|entity| entity.id)
    }
}

#[cfg(test)]
pub mod test_util {
    use crate::position::TileSpaceExt;
    use crate::prototype_data::EntityPrototype;
    use crate::rcid::RcId;

    use super::*;

    pub fn small_pole_prototype() -> EntityPrototypeRef {
        RcId::new(EntityPrototype {
            type_: "electric-pole".to_string(),
            name: "test".to_string(),
            tile_width: 1,
            tile_height: 1,
            collision_box: BoundingBox::new(point2(-0.5, -0.5), point2(0.5, 0.5)),
            uses_power: false,
            pole_data: Some(PoleData {
                wire_distance: 7.5,
                supply_radius: 2.5,
            }),
        })
    }
    pub fn powerable_prototype() -> EntityPrototypeRef {
        EntityPrototypeRef::new(EntityPrototype {
            name: "solar-panel".to_string(),
            type_: "generator".to_string(),
            tile_width: 1,
            tile_height: 1,
            uses_power: true,
            collision_box: BoundingBox::new(point2(-0.5, -0.5), point2(0.5, 0.5)),
            pole_data: None,
        })
    }
    impl BpModel {
        pub fn add_test_pole(&mut self, position: TilePosition) -> EntityId {
            self.add_overlap(WorldEntity {
                position: position.center_map_pos(),
                prototype: small_pole_prototype(),
                direction: 0,
            })
        }
        pub fn add_test_powerable(&mut self, position: TilePosition) -> EntityId {
            self.add_overlap(WorldEntity {
                position: position.center_map_pos(),
                prototype: powerable_prototype(),
                direction: 0,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::*;
    use super::*;
    use crate::prototype_data::EntityPrototype;
    use crate::rcid::RcId;

    fn entity_data(uses_power: bool) -> EntityPrototypeRef {
        RcId::new(EntityPrototype {
            type_: "test".to_string(),
            name: "test".to_string(),
            tile_width: 1,
            tile_height: 1,
            collision_box: BoundingBox::new(point2(-0.5, -0.5), point2(0.5, 0.5)),
            uses_power,
            pole_data: None,
        })
    }

    #[test]
    fn add_and_get() {
        let mut grid = BpModel::new();
        let entity = WorldEntity {
            position: point2(0.5, 0.5),
            direction: 0,
            prototype: entity_data(false),
        };
        let entity_id = grid.add_overlap(entity.clone());
        let at0 = grid.get_at_tile(point2(0, 0)).next();
        assert_eq!(at0.unwrap().data, entity);
        assert!(grid.get_at_tile(point2(1, 0)).next().is_none());
        assert!(grid.get_at_tile(point2(0, 1)).next().is_none());

        let a = grid.add_no_overlap(entity.clone());
        assert_eq!(a, None);

        grid.remove(&entity_id);
        assert!(grid.get_at_tile(point2(0, 0)).next().is_none());
    }
    #[test]
    fn powered_entities() {
        let mut grid = BpModel::new();
        let id1 = grid.add_overlap(WorldEntity {
            position: point2(0.5, 0.5),
            direction: 0,
            prototype: entity_data(true),
        });
        grid.add_overlap(WorldEntity {
            position: point2(2.5, 1.5),
            direction: 0,
            prototype: entity_data(false),
        });

        let powered1 = grid
            .powered_entities(point2(2.5, 2.5), small_pole_prototype().pole_data.unwrap())
            .map(|entity| entity.id)
            .collect_vec();
        assert_eq!(powered1, vec![id1]);
        let powered2 = grid
            .powered_entities(point2(3.5, 2.5), small_pole_prototype().pole_data.unwrap())
            .map(|entity| entity.id)
            .collect_vec();
        assert_eq!(powered2, vec![]);
    }

    #[test]
    fn connectable_poles() {
        let mut grid = BpModel::new();
        let pole1 = grid.add_overlap(WorldEntity {
            position: point2(0.5, 0.5),
            direction: 0,
            prototype: small_pole_prototype(),
        });
        let pole2 = grid.add_overlap(WorldEntity {
            position: point2(10.5, 1.5),
            direction: 0,
            prototype: small_pole_prototype(),
        });
        let connectable1 = grid
            .connectable_poles(point2(2.5, 2.5), small_pole_prototype().pole_data.unwrap())
            .map(|entity| entity.id)
            .collect_vec();
        assert_eq!(connectable1, vec![pole1]);
        let connectable2 = grid
            .connectable_poles(point2(8.5, 2.5), small_pole_prototype().pole_data.unwrap())
            .map(|entity| entity.id)
            .collect_vec();
        assert_eq!(connectable2, vec![pole2]);
    }
}
