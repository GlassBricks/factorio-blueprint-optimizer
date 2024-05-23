use std::ops::{Neg, Sub};

use euclid::*;
use noisy_float::prelude::r64;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::{DeserializeAs, SerializeAs};

use factorio_blueprint::objects as fbp;

/// Marks coordinate system where +x is right and +y is down.
trait PosRightDownCoords {}

pub struct MapSpace;
impl PosRightDownCoords for MapSpace {}
pub type MapPosition = Point2D<f64, MapSpace>;

pub type BoundingBox = Box2D<f64, MapSpace>;

pub struct TileSpace;
impl PosRightDownCoords for TileSpace {}
pub type TilePosition = Point2D<i32, TileSpace>;
pub type TileBoundingBox = Box2D<i32, TileSpace>;

pub trait IterTiles {
    fn iter_tiles(self) -> impl Iterator<Item = TilePosition>;
}
impl IterTiles for TileBoundingBox {
    fn iter_tiles(self) -> impl Iterator<Item = TilePosition> {
        let min = self.min;
        let max = self.max;
        (min.x..max.x).flat_map(move |x| (min.y..max.y).map(move |y| TilePosition::new(x, y)))
    }
}
impl IterTiles for BoundingBox {
    fn iter_tiles(self) -> impl Iterator<Item = TilePosition> {
        self.round_out_to_tiles().iter_tiles()
    }
}

pub trait TileSpaceExt {
    /// Returns the center of the tile in map coordinates.
    #[must_use]
    fn center_map_pos(self) -> MapPosition;

    fn corner_map_pos(self) -> MapPosition;
}

impl TileSpaceExt for TilePosition {
    fn center_map_pos(self) -> MapPosition {
        point2(self.x as f64 + 0.5, self.y as f64 + 0.5)
    }
    fn corner_map_pos(self) -> MapPosition {
        point2(self.x as f64, self.y as f64)
    }
}

pub trait ContractMax<T> {
    fn contract_max(self, amt: T) -> Self;
}

impl<T: Sub<Output = T> + Copy, U> ContractMax<T> for Box2D<T, U> {
    fn contract_max(self, amt: T) -> Self {
        Box2D::new(self.min, self.max - vec2(amt, amt))
    }
}

trait MapPositionExt {
    /// the tile position this map position is in. Rounds down.
    #[must_use]
    fn tile_pos(&self) -> TilePosition;
}
impl MapPositionExt for MapPosition {
    fn tile_pos(&self) -> TilePosition {
        self.floor().to_i32().cast_unit()
    }
}

pub trait BoundingBoxExt {
    #[must_use]
    fn round_out_to_tiles(&self) -> TileBoundingBox;
    #[must_use]
    fn round_to_tiles_covering_center(&self) -> TileBoundingBox;

    #[must_use]
    fn around_point(center: MapPosition, radius: f64) -> Self;
}

impl BoundingBoxExt for BoundingBox {
    fn round_out_to_tiles(&self) -> TileBoundingBox {
        self.round_out().to_i32().cast_unit()
    }

    fn round_to_tiles_covering_center(&self) -> TileBoundingBox {
        let eps = 1e-6;
        let min = self.min - vec2(eps, eps);
        let max = self.max + vec2(eps, eps);
        Box2D::new(min, max).round().to_i32().cast_unit()
    }

    fn around_point(center: MapPosition, radius: f64) -> Self {
        let vec = vec2(radius, radius);
        Box2D::new(center - vec, center + vec)
    }
}

pub trait ToMapPosition {
    #[must_use]
    fn to_map_position(&self) -> MapPosition;
}

pub trait ToPosition<T> {
    #[must_use]
    fn to_position(&self) -> fbp::Position;
}

impl ToMapPosition for fbp::Position {
    fn to_map_position(&self) -> MapPosition {
        MapPosition::new(self.x.raw(), self.y.raw())
    }
}

impl ToPosition<MapPosition> for MapPosition {
    fn to_position(&self) -> fbp::Position {
        fbp::Position {
            x: r64(self.x),
            y: r64(self.y),
        }
    }
}

/// Associated with [PosRightDownCoords], where +x is right and +y is down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardinalDirection {
    North,
    East,
    South,
    West,
}
impl CardinalDirection {
    pub fn from_u8_rounding(dir: u8) -> Self {
        use CardinalDirection::*;
        match dir % 8 {
            0 | 1 => North,
            2 | 3 => East,
            4 | 5 => South,
            6 | 7 => West,
            _ => unreachable!(),
        }
    }
}

pub trait Rotate {
    #[must_use]
    fn rotate(&self, direction: CardinalDirection) -> Self;
}

impl<N: Neg<Output = N> + Copy, U: PosRightDownCoords> Rotate for Point2D<N, U> {
    fn rotate(&self, direction: CardinalDirection) -> Self {
        use CardinalDirection::*;
        match direction {
            North => *self,
            East => point2(-self.y, self.x),
            South => point2(-self.x, -self.y),
            West => point2(self.y, -self.x),
        }
    }
}

impl<N: Neg<Output = N> + Copy, U: PosRightDownCoords> Rotate for Box2D<N, U> {
    fn rotate(&self, direction: CardinalDirection) -> Self {
        use CardinalDirection::*;
        match direction {
            North => *self,
            East => Box2D::new(
                point2(-self.max.y, self.min.x),
                point2(-self.min.y, self.max.x),
            ),
            South => Box2D::new(-self.max, -self.min),
            West => Box2D::new(
                point2(self.min.y, -self.max.x),
                point2(self.max.y, -self.min.x),
            ),
        }
    }
}

/// Deserializers for position and bounding box, following format in Factorio prototypes.
pub struct FactorioPos;
impl<'de> DeserializeAs<'de, MapPosition> for FactorioPos {
    fn deserialize_as<D>(deserializer: D) -> Result<MapPosition, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(<(f64, f64)>::deserialize(deserializer)?.into())
    }
}
impl SerializeAs<MapPosition> for FactorioPos {
    fn serialize_as<S>(source: &MapPosition, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (source.x, source.y).serialize(serializer)
    }
}

impl<'de> DeserializeAs<'de, BoundingBox> for FactorioPos {
    fn deserialize_as<D>(deserializer: D) -> Result<BoundingBox, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (a, b) = <((f64, f64), (f64, f64))>::deserialize(deserializer)?;
        Ok(Box2D::new(a.into(), b.into()))
    }
}

impl SerializeAs<BoundingBox> for FactorioPos {
    fn serialize_as<S>(source: &BoundingBox, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        (source.min.to_tuple(), source.max.to_tuple()).serialize(serializer)
    }
}

#[cfg(test)]
mod tests {
    use CardinalDirection::*;
    use super::*;
    
    #[test]
    fn iter_tiles() {
        let box_ = Box2D::new(point2(1, 2), point2(3, 4));
        let tiles: Vec<_> = box_.iter_tiles().collect();
        assert_eq!(tiles, [point2(1, 2), point2(1, 3), point2(2, 2), point2(2, 3)]);
    }
    
    #[test]
    fn tile_to_map() {
        assert_eq!(point2(1, 2).center_map_pos(), point2(1.5, 2.5));
        assert_eq!(point2(1, 2).corner_map_pos(), point2(1.0, 2.0));
    }
    
    #[test]
    fn contract_max() {
        let box_ = BoundingBox::new(point2(1.0, 2.0), point2(3.0, 4.0));
        assert_eq!(box_.contract_max(1.0), Box2D::new(point2(1.0, 2.0), point2(2.0, 3.0)));
    }
    
    #[test]
    fn tile_pos() {
        assert_eq!(point2(1.0, 2.0).tile_pos(), point2(1, 2));
        assert_eq!(point2(1.5, 2.5).tile_pos(), point2(1, 2));
    }
    
    #[test]
    fn round_out_to_tiles() {
        let box_ = Box2D::new(point2(0.5, 1.5), point2(3.5, 4.5));
        assert_eq!(box_.round_out_to_tiles(), Box2D::new(point2(0, 1), point2(4, 5)));
    }
    
    #[test]
    fn round_to_tiles_covering_center() {
        let box_ = Box2D::new(point2(0.5, 1.6), point2(3.5, 4.4));
        assert_eq!(box_.round_to_tiles_covering_center(), Box2D::new(point2(0, 2), point2(4, 4)));
    }
    
    #[test]
    fn around_point() {
        let box_ = BoundingBox::around_point(point2(1.0, 2.0), 1.0);
        assert_eq!(box_, Box2D::new(point2(0.0, 1.0), point2(2.0, 3.0)));
    }
    

    #[test]
    fn tile_center() {
        assert_eq!(point2(0, 0).center_map_pos(), point2(0.5, 0.5));
        assert_eq!(point2(1, 2).center_map_pos(), point2(1.5, 2.5));
    }

    #[test]
    fn rotate() {
        let pos = MapPosition::new(1.0, 2.0);
        assert_eq!(pos.rotate(North), pos);
        assert_eq!(pos.rotate(East), point2(-2.0, 1.0));
        assert_eq!(pos.rotate(South), point2(-1.0, -2.0));
        assert_eq!(pos.rotate(West), point2(2.0, -1.0));

        let box_: BoundingBox = Box2D::new(point2(1.0, 2.0), point2(3.0, 4.0));
        assert_eq!(box_.rotate(North), box_);
        assert_eq!(
            box_.rotate(East),
            Box2D::new(point2(-4.0, 1.0), point2(-2.0, 3.0))
        );
        assert_eq!(
            box_.rotate(South),
            Box2D::new(point2(-3.0, -4.0), point2(-1.0, -2.0))
        );
        assert_eq!(
            box_.rotate(West),
            Box2D::new(point2(2.0, -3.0), point2(4.0, -1.0))
        );
        for dir in [0, 2, 4, 6] {
            let dir = CardinalDirection::from_u8_rounding(dir);
            assert_eq!(
                box_.rotate(dir),
                Box2D::from_points([box_.min.rotate(dir), box_.max.rotate(dir)])
            )
        }
    }
    
    
}
