use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::ops::Index;
use std::path::PathBuf;
use std::rc::Rc;

use serde::*;
use serde_with::{serde_as, skip_serializing_none};

use crate::position::*;
use crate::rcid::RcId;

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
#[serde(rename_all = "kebab-case")]
pub enum CollisionMask {
    GroundTile,
    WaterTile,
    ResourceLayer,
    DoodadLayer,
    FloorLayer,
    ItemLayer,
    GhostLayer,
    ObjectLayer,
    PlayerLayer,
    TrainLayer,
    RailLayer,
    TransportBeltLayer,
    NotCollidingWithItself,
    ConsiderTileTransitions,
    CollidingWithTilesOnly,
}

#[derive(Deserialize, Debug)]
pub struct EnergySource {
    #[serde(rename = "type")]
    type_: String,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Deserialize, Debug)]
struct RawPrototypeData {
    #[serde(rename = "type")]
    type_: String,
    name: String,
    tile_width: Option<u32>,
    tile_height: Option<u32>,
    #[serde_as(as = "FactorioPos")]
    #[serde(default)]
    collision_box: BoundingBox,

    energy_source: Option<EnergySource>,

    supply_area_distance: Option<f64>,
    maximum_wire_distance: Option<f64>,
}

#[derive(Serialize, Deserialize, Debug, Copy, Clone)]
pub struct PoleData {
    pub supply_radius: f64,
    pub wire_distance: f64,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct EntityPrototype {
    #[serde(rename = "type")]
    pub type_: String,
    pub name: String,
    pub tile_width: u32,
    pub tile_height: u32,
    #[serde_as(as = "FactorioPos")]
    pub collision_box: BoundingBox,

    pub uses_power: bool,
    pub pole_data: Option<PoleData>,
}

pub type EntityPrototypeRef = RcId<EntityPrototype>;
#[derive(Debug, Clone)]
pub struct EntityPrototypeDict(pub Rc<HashMap<String, EntityPrototypeRef>>);
impl Index<&str> for EntityPrototypeDict {
    type Output = EntityPrototypeRef;

    fn index(&self, index: &str) -> &Self::Output {
        &self.0[index]
    }
}


static ENTITY_TYPES: &[&str] = &[
    "accumulator",
    "artillery-turret",
    "beacon",
    "boiler",
    "burner-generator",
    "arithmetic-combinator",
    "decider-combinator",
    "constant-combinator",
    "container",
    "logistic-container",
    "infinity-container",
    "assembling-machine",
    "rocket-silo",
    "furnace",
    "electric-energy-interface",
    "electric-pole",
    "gate",
    "generator",
    "heat-interface",
    "heat-pipe",
    "inserter",
    "lab",
    "lamp",
    "land-mine",
    "linked-container",
    "mining-drill",
    "offshore-pump",
    "pipe",
    "infinity-pipe",
    "pipe-to-ground",
    "player-port",
    "power-switch",
    "programmable-speaker",
    "pump",
    "radar",
    "curved-rail",
    "straight-rail",
    "rail-chain-signal",
    "rail-signal",
    "reactor",
    "roboport",
    "simple-entity-with-owner",
    "simple-entity-with-force",
    "solar-panel",
    "storage-tank",
    "train-stop",
    "linked-belt",
    "loader-1x1",
    "loader",
    "splitter",
    "transport-belt",
    "underground-belt",
    "turret",
    "ammo-turret",
    "electric-turret",
    "fluid-turret",
    "artillery-wagon",
    "cargo-wagon",
    "fluid-wagon",
    "locomotive",
    "wall",
];

#[allow(dead_code)]
pub fn load_prototype_data_from_raw(
    data_raw_file: &PathBuf,
) -> Result<EntityPrototypeDict, Box<dyn std::error::Error>> {
    // load as json
    let data_raw: serde_json::Value = serde_json::from_reader(File::open(data_raw_file)?)?;
    let mut entity_data = HashMap::new();
    for entity_type in ENTITY_TYPES {
        let source = data_raw.get(entity_type).unwrap();
        let prototypes = <HashMap<String, RawPrototypeData>>::deserialize(source);
        if prototypes.is_err() {
            println!(
                "Error loading entity data for {}, {:?}",
                entity_type,
                prototypes.err()
            );
            continue;
        }
        let is_pole = entity_type == &"electric-pole";
        for (name, raw_data) in prototypes.unwrap() {
            let data = RcId::new(EntityPrototype {
                type_: raw_data.type_,
                name: raw_data.name,
                tile_width: raw_data.tile_width.unwrap_or(1),
                tile_height: raw_data.tile_height.unwrap_or(1),
                collision_box: raw_data.collision_box,
                uses_power: entity_type == &"generator"
                    || raw_data
                        .energy_source
                        .is_some_and(|es| es.type_ == "electric"),

                pole_data: if is_pole {
                    Some(PoleData {
                        supply_radius: raw_data.supply_area_distance.unwrap_or(0.0),
                        wire_distance: raw_data.maximum_wire_distance.unwrap_or(0.0),
                    })
                } else {
                    None
                },
            });
            entity_data.insert(name, data);
        }
    }
    Ok(EntityPrototypeDict(Rc::new(entity_data)))
}

static ENTITY_PROTOTYPE_FILE: &str = "data/entity-data.json";
#[allow(dead_code)]
pub fn save_prototype_data(prototype_data: &EntityPrototypeDict) -> Result<(), Box<dyn std::error::Error>> {
    let file = File::create(ENTITY_PROTOTYPE_FILE)?;
    let writer = BufWriter::new(file);
    let copy = prototype_data
        .0.iter()
        .map(|(k, v)| (k, &**v))
        .collect::<HashMap<_, _>>();
    serde_json::to_writer_pretty(writer, &copy)?;
    Ok(())
}

pub fn load_prototype_data() -> Result<EntityPrototypeDict, Box<dyn std::error::Error>> {
    let file = File::open(ENTITY_PROTOTYPE_FILE)?;
    let entity_data =
        serde_json::from_reader::<_, HashMap<String, EntityPrototype>>(BufReader::new(file))?
            .into_iter()
            .map(|(k, v)| (k, RcId::new(v)))
            .collect();
    Ok(EntityPrototypeDict(Rc::new(entity_data)))
}

#[cfg(test)]
mod tests {
    use super::*;

    static DATA_RAW_DUMP_FILE: &str = "data/data-raw-dump.json";

    #[ignore]
    #[test]
    fn test_load_data_from_raw() {
        let entity_data = load_prototype_data_from_raw(&PathBuf::from(DATA_RAW_DUMP_FILE)).unwrap();
        println!("{:?}", entity_data["small-electric-pole"]);
    }

    #[ignore]
    #[test]
    fn do_save_prototype_data() {
        let entity_data = load_prototype_data_from_raw(&PathBuf::from(DATA_RAW_DUMP_FILE)).unwrap();
        save_prototype_data(&entity_data).unwrap();
    }

    #[test]
    fn do_load_prototype_data() {
        let entity_data = load_prototype_data().unwrap();
        println!("{:?}", entity_data["small-electric-pole"]);
    }
}
