#[cfg(test)]
use std::collections::HashSet;
use std::hash::Hash;
use std::marker::PhantomData;

use euclid::{point2, vec2, Vector2D};
use hashbrown::HashMap;
use num_traits::abs;

use crate::better_bp::EntityId;
use crate::bp_model::{BpModel, WorldEntity};
#[cfg(test)]
use crate::position::IterTiles;
use crate::position::{MapPosition, MapPositionExt, TilePosition, TileSpace};
use crate::prototype_data::{EntityPrototypeRef, PoleData};

pub trait GetAtPos {
    type Id: Eq + Hash;
    fn get_at_tile(&self, pos: TilePosition) -> impl Iterator<Item = Self::Id>;
}

impl GetAtPos for &BpModel {
    type Id = EntityId;
    fn get_at_tile(&self, pos: TilePosition) -> impl Iterator<Item = EntityId> {
        BpModel::get_at_tile(self, pos).map(|entity| entity.id())
    }
}

impl<Id: Eq + Hash, F, R: Iterator<Item = Id>> GetAtPos for F
where
    F: Fn(TilePosition) -> R,
{
    type Id = Id;
    fn get_at_tile(&self, pos: TilePosition) -> R {
        self(pos)
    }
}

#[derive(Debug)]
pub struct Moving2DWindow<T: GetAtPos> {
    source: T,
    top_left: TilePosition,
    size: i32,
    current_counts: HashMap<T::Id, u32>,

    #[cfg(test)]
    cur_pts: HashSet<TilePosition>,
}

impl<T: GetAtPos> Moving2DWindow<T> {
    pub fn new(source: T, top_left: TilePosition, size: i32) -> Self {
        let mut res = Moving2DWindow {
            source,
            top_left,
            size,
            current_counts: HashMap::new(),
            #[cfg(test)]
            cur_pts: HashSet::new(),
        };
        res.jump_to(top_left);
        res
    }

    pub fn cur_items(&self) -> impl Iterator<Item = &T::Id> {
        self.current_counts.keys()
    }
    pub fn top_left(&self) -> TilePosition {
        self.top_left
    }
    pub fn size(&self) -> i32 {
        self.size
    }

    fn dec_at(&mut self, pos: TilePosition) {
        #[cfg(test)]
        assert!(self.cur_pts.remove(&pos));

        for id in self.source.get_at_tile(pos) {
            let count = self.current_counts.get_mut(&id);
            if count.is_none() {
                println!("not found at {:?}", pos);
                continue;
            }
            let count = count.unwrap();
            if *count == 1 {
                self.current_counts.remove(&id);
            } else {
                *count -= 1;
            }
        }
    }
    fn inc_at(&mut self, pos: TilePosition) {
        #[cfg(test)]
        assert!(self.cur_pts.insert(pos));

        for id in self.source.get_at_tile(pos) {
            *self.current_counts.entry(id).or_insert(0) += 1;
        }
    }

    fn move_inc_x(&mut self) {
        for y in 0..self.size {
            self.inc_at(self.top_left + vec2(self.size, y));
            self.dec_at(self.top_left + vec2(0, y));
        }
        self.top_left.x += 1;
        self.check_invariants()
    }

    fn move_inc_y(&mut self) {
        for x in 0..self.size {
            self.inc_at(self.top_left + vec2(x, self.size));
            self.dec_at(self.top_left + vec2(x, 0));
        }
        self.top_left.y += 1;
        self.check_invariants()
    }
    fn move_dec_x(&mut self) {
        self.top_left.x -= 1;
        for y in 0..self.size {
            self.inc_at(self.top_left + vec2(0, y));
            self.dec_at(self.top_left + vec2(self.size, y));
        }
        self.check_invariants()
    }
    fn move_dec_y(&mut self) {
        self.top_left.y -= 1;
        for x in 0..self.size {
            self.inc_at(self.top_left + vec2(x, 0));
            self.dec_at(self.top_left + vec2(x, self.size));
        }
        self.check_invariants()
    }
    fn move_rel(&mut self, diff: Vector2D<i32, TileSpace>) {
        if diff.x > 0 {
            for _ in 0..diff.x {
                self.move_inc_x();
            }
        } else {
            for _ in 0..-diff.x {
                self.move_dec_x();
            }
        }
        if diff.y > 0 {
            for _ in 0..diff.y {
                self.move_inc_y();
            }
        } else {
            for _ in 0..-diff.y {
                self.move_dec_y();
            }
        }
    }
    pub fn move_to(&mut self, new_top_left: TilePosition) {
        if self.top_left == new_top_left {
            return;
        }
        let dx = abs(new_top_left.x - self.top_left.x);
        let dy = abs(new_top_left.y - self.top_left.y);
        let work_jump = self.size * self.size;
        let work_move = 2 * (dx * self.size + dy * self.size);
        if work_jump < work_move {
            self.jump_to(new_top_left);
        } else {
            self.move_rel(new_top_left - self.top_left);
        }
    }

    fn check_invariants(&self) {
        #[cfg(test)]
        {
            let target_positions = crate::position::TileBoundingBox::from_origin_and_size(
                self.top_left,
                euclid::size2(self.size, self.size),
            )
            .iter_tiles()
            .collect::<HashSet<_>>();
            assert_eq!(self.cur_pts, target_positions);
        }
    }

    fn jump_to(&mut self, new_top_left: TilePosition) {
        let counts = &mut self.current_counts;
        self.top_left = new_top_left;
        counts.clear();
        #[cfg(test)]
        self.cur_pts.clear();

        for x in 0..self.size {
            for y in 0..self.size {
                self.inc_at(new_top_left + vec2(x, y));
            }
        }
        self.check_invariants()
    }
}

pub trait PoleWindowParams {
    fn get_radius(pole_data: PoleData) -> f64;
}

pub struct PoleWindows<'a, P: PoleWindowParams> {
    model: &'a BpModel,
    windows_by_proto: HashMap<EntityPrototypeRef, Moving2DWindow<&'a BpModel>>,
    marker: PhantomData<P>,
}

impl<'a, P: PoleWindowParams> PoleWindows<'a, P> {
    pub fn new(model: &'a BpModel) -> Self {
        Self {
            model,
            windows_by_proto: HashMap::new(),
            marker: PhantomData,
        }
    }

    fn get_window_top_left(pole_data: PoleData, pos: MapPosition) -> TilePosition {
        let radius = P::get_radius(pole_data);
        (pos - vec2(radius, radius)).tile_pos()
    }
    fn get_window_size(prototype: &EntityPrototypeRef, pole_data: PoleData) -> i32 {
        let tile_width = prototype.tile_width;
        let tile_height = prototype.tile_height;
        let rep_center = point2(
            (tile_width % 2) as f64 / 2.0,
            (tile_height % 2) as f64 / 2.0,
        );
        let radius = P::get_radius(pole_data);
        let top_left = Self::get_window_top_left(pole_data, rep_center);
        let bottom_right = (rep_center + vec2(radius, radius)).tile_pos();
        let size = bottom_right - top_left;
        size.x.max(size.y) + 1
    }
    pub fn get_window_for(&mut self, pole: &WorldEntity) -> &mut Moving2DWindow<&'a BpModel> {
        let prototype = &pole.prototype;
        let pole_data = prototype.pole_data.unwrap();
        let top_left = Self::get_window_top_left(pole_data, pole.position);
        let window = self
            .windows_by_proto
            .entry(prototype.clone())
            .or_insert_with(|| {
                let size = Self::get_window_size(prototype, pole_data);
                Moving2DWindow::new(self.model, top_left, size)
            });
        window.move_to(top_left);
        window
    }
}

pub struct WireReach;

impl PoleWindowParams for WireReach {
    fn get_radius(pole_data: PoleData) -> f64 {
        pole_data.wire_distance
    }
}

pub struct PoleCoverage;

impl PoleWindowParams for PoleCoverage {
    fn get_radius(pole_data: PoleData) -> f64 {
        pole_data.supply_radius
    }
}

pub type WireReachWindows<'a> = PoleWindows<'a, WireReach>;
pub type PoleCoverageWindows<'a> = PoleWindows<'a, PoleCoverage>;

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use euclid::size2;
    use itertools::Itertools;
    use rand::prelude::StdRng;
    use rand::{Rng, SeedableRng};

    use crate::bp_model::test_util::{powerable_prototype, small_pole_prototype};
    use crate::bp_model::{BpModel, WorldEntity};
    use crate::position::{IterTiles, TileBoundingBox, TilePosition, TileSpaceExt};

    use super::*;

    fn make_model() -> BpModel {
        let mut model = BpModel::new();
        for x in -1..9 {
            for y in -1..9 {
                let pos = TilePosition::new(x, y);
                if x + y % 4 < 2 {
                    model
                        .add_no_overlap(WorldEntity {
                            position: pos.center_map_pos(),
                            direction: 0,
                            prototype: powerable_prototype(),
                        })
                        .unwrap();
                }
                if x * 2 + 3 * y % 7 % 2 == 0 {
                    model.add_overlap(WorldEntity {
                        position: pos.center_map_pos(),
                        direction: 0,
                        prototype: small_pole_prototype(),
                    });
                }
            }
        }
        model
    }
    #[test]
    fn test_moving_window() {
        let model = make_model();
        let mut window = Moving2DWindow::new(&model, TilePosition::new(0, 0), 3);
        let test_window_correct = |window: &Moving2DWindow<&BpModel>,
                                   expected_pos: TilePosition| {
            assert_eq!(expected_pos, window.top_left());
            let from_model = TileBoundingBox::from_origin_and_size(expected_pos, size2(3, 3))
                .iter_tiles()
                .flat_map(|pos| model.get_at_tile(pos))
                .map(|entity| entity.id())
                .unique()
                .collect::<HashSet<_>>();
            let from_window = window.cur_items().copied().collect::<HashSet<_>>();
            assert_eq!(from_model, from_window);
        };
        test_window_correct(&window, TilePosition::new(0, 0));
        for x in 0..9 {
            window.move_to(TilePosition::new(x, 0));
            test_window_correct(&window, TilePosition::new(x, 0));
        }
        for x in (0..9).rev() {
            window.move_to(TilePosition::new(x, 1));
            test_window_correct(&window, TilePosition::new(x, 1));
        }
        window.move_to(TilePosition::new(3, 6));
        test_window_correct(&window, TilePosition::new(3, 6));
        for y in 0..9 {
            window.move_to(TilePosition::new(3, y + 5));
            test_window_correct(&window, TilePosition::new(3, y + 5));
        }
        for y in (0..9).rev() {
            window.move_to(TilePosition::new(4, y + 5));
            test_window_correct(&window, TilePosition::new(4, y + 5));
        }
        let mut rand = StdRng::seed_from_u64(42);
        for _ in 0..100 {
            let new_pos = {
                if rand.gen_bool(0.5) {
                    let (x, y) = (rand.gen_range(-2..2), rand.gen_range(-2..2));
                    window.top_left() + vec2(x, y)
                } else {
                    let (x, y) = (rand.gen_range(-14..14), rand.gen_range(-14..14));
                    TilePosition::new(x, y)
                }
            };
            window.move_to(new_pos);
            test_window_correct(&window, new_pos);
        }
    }

    #[test]
    fn test_window_params() {
        let pole_data = PoleData {
            supply_radius: 2.0,
            wire_distance: 3.0,
        };
        assert_eq!(WireReach::get_radius(pole_data), 3.0);
        assert_eq!(PoleCoverage::get_radius(pole_data), 2.0);
    }

    #[test]
    fn test_pole_windows() {
        let prototype = small_pole_prototype();
        let model = BpModel::new();
        let mut wire_windows = WireReachWindows::new(&model);
        let mut coverage_windows = PoleCoverageWindows::new(&model);
        let entity = WorldEntity {
            position: point2(1.5, 2.5),
            direction: 0,
            prototype,
        };

        let wire_window = wire_windows.get_window_for(&entity);
        assert_eq!(wire_window.size(), 15);
        assert_eq!(
            wire_window.top_left(),
            (entity.position - vec2(7.5, 7.5)).tile_pos()
        );
        let coverage_window = coverage_windows.get_window_for(&entity);
        assert_eq!(coverage_window.size(), 5);
        assert_eq!(
            coverage_window.top_left(),
            (entity.position - vec2(2.5, 2.5)).tile_pos()
        );
    }
}
