use std::collections::HashMap;
use std::ops::Add;

use euclid::{point2, size2, vec2};
use good_lp::{
    constraint, highs, variable, Expression, ProblemVariables, Solution, SolverModel, Variable,
};
use hashbrown::HashSet;
use itertools::{iproduct, Itertools};
use plotters::prelude::{Color, BLACK};

use crate::algorithms::{PoleConnector, PrettyPoleConnector};
use crate::bp_model::{BpModel, WorldEntity};
use crate::draw::Drawing;
use crate::position::{TileBoundingBox, TilePosition, TileSpaceExt};
use crate::prototype_data::{load_prototype_data, EntityPrototypeDict, EntityPrototypeRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct LayoutSpot {
    row: i32,
    side: i8,
    x: i32,
}
impl LayoutSpot {
    fn get_center_pt(&self) -> TilePosition {
        let y = self.row * 7 + 3 + (if self.side == 1 { 2 } else { -2 });
        point2(self.x, y)
    }
}

struct MinerLayout {
    rows: i32,
    len: i32,
    miners: Vec<LayoutSpot>,
    poles: Vec<TilePosition>,
}

fn solve_miner_lp(rows: i32, len: i32, max_per_side: i32, max_on_outer: i32) -> MinerLayout {
    // for each side of each row, variable for space used, * len
    let mut vars = ProblemVariables::new();
    let spots = iproduct!(0..rows, 0..2, 0..len)
        .map(|(row, side, x)| LayoutSpot {
            row,
            side: side as i8,
            x,
        })
        .collect_vec();
    let spot_occupied_vars = spots
        .iter()
        .map(|spot| (*spot, vars.add(variable().binary())))
        .collect::<HashMap<LayoutSpot, Variable>>();

    let pole_spots: Vec<(LayoutSpot, TilePosition)> = spots
        .iter()
        .flat_map(|&spot| {
            let center = spot.get_center_pt();
            [
                (spot, center.add(vec2(0, -1))),
                (spot, center.add(vec2(0, 1))),
            ]
        })
        .collect_vec();

    let pole_vars = pole_spots
        .iter()
        .map(|(_, spot)| (*spot, vars.add(variable().binary())))
        .collect::<HashMap<TilePosition, _>>();

    let miner_spots = spots
        .iter()
        .filter(|&spot| spot.x > 0 && spot.x < len - 1)
        .copied()
        .collect_vec();

    let miner_vars = miner_spots
        .iter()
        .map(|&spot| (spot, vars.add(variable().binary())))
        .collect::<HashMap<LayoutSpot, Variable>>();

    let miners_sum = miner_vars.values().sum::<Expression>();
    let poles_sum = pole_vars.values().sum::<Expression>();

    let mut problem = vars.maximise(miners_sum * 100 - poles_sum).using(highs);

    // spot_occupied_vars counts number of miners in a spot (and is only allowed to be 0 or 1)
    for &spot in &spots {
        let LayoutSpot { x, .. } = spot;
        let left_miner = miner_vars.get(&LayoutSpot { x: x - 1, ..spot });
        let middle_miner = miner_vars.get(&spot);
        let right_miner = miner_vars.get(&LayoutSpot { x: x + 1, ..spot });
        let miners = [left_miner, middle_miner, right_miner]
            .iter()
            .filter_map(|&x| x)
            .sum::<Expression>();
        let spot_var = spot_occupied_vars[&spot];
        problem.add_constraint(constraint!(miners == spot_var));
    }

    // the number of miners in each (row,side) must be at most max_per_side
    for row in 0..rows {
        for side in 0..2 {
            let is_outer = (row==0 && side==0) || (row==rows-1 && side==1);
            let max = if is_outer { max_on_outer } else { max_per_side };
            let miners_in_row = miner_spots
                .iter()
                .filter(|&&spot| spot.row == row && spot.side == side)
                .map(|&spot| miner_vars[&spot])
                .sum::<Expression>();
            problem.add_constraint(constraint!(miners_in_row <= max));
        }
    }

    // miners must be powered
    for (&miner, &miner_var) in &miner_vars {
        // if miner.row == 0 && miner.side == 0 || miner.x <= 3 {
        //     continue;
        // }
        let powering_poles = pole_vars
            .iter()
            .filter(|(&pole, _)| {
                let miner_pos = miner.get_center_pt();
                let diff = miner_pos - pole;
                let norm_inf = diff.x.abs().max(diff.y.abs());
                norm_inf == 2 || norm_inf == 3
            })
            .map(|(_, &pole_var)| pole_var)
            .sum::<Expression>();
        problem.add_constraint(constraint!(powering_poles >= miner_var));
    }

    // pole cannot occupy same spot as miner
    for &(layout_pos, pole_pos) in &pole_spots {
        let pole_var = pole_vars[&pole_pos];
        let spot_var = spot_occupied_vars[&layout_pos];
        problem.add_constraint(constraint!(pole_var + spot_var <= 1));
    }

    let wire_reach = (7.5 * 7.5) as i32;
    let pole_to_pole_neighbors: HashMap<TilePosition, Vec<TilePosition>> = pole_spots
        .iter()
        .map(|&(_, pole_pos)| {
            let neighbors = pole_spots
                .iter()
                .filter(|&&(_, other_pos)| {
                    (other_pos.x + other_pos.y < pole_pos.x + pole_pos.y)
                        && (other_pos - pole_pos).square_length() <= wire_reach
                })
                .map(|&(_, other_pos)| other_pos)
                .collect_vec();
            (pole_pos, neighbors)
        })
        .collect();
    // pole must connect with another pole with either smaller x or y;
    // unless it's on the edge of the map
    for (&pole, &pole_var) in &pole_vars {
        if pole.x < 5 {
            continue;
        }
        let neighbors = &pole_to_pole_neighbors[&pole];

        let neigh_sum = neighbors
            .iter()
            .map(|&neigh| pole_vars[&neigh])
            .sum::<Expression>();
        problem.add_constraint(constraint!(pole_var <= neigh_sum));
    }

    problem = problem
        .set_time_limit(300.0)
        .set_mip_abs_gap(40.0)
        .unwrap();
    problem.set_verbose(true);
    let result = problem.solve().unwrap();

    let selected_poles = pole_vars
        .iter()
        .filter(|&(_, &var)| result.value(var) > 0.5)
        .map(|(&spot, _)| spot)
        .collect::<HashSet<_>>();
    //
    // for pole in selected_poles.iter().sorted_by_key(|&pos| (pos.x, pos.y)) {
    //     println!("Selected pole at {:?}", pole);
    //     let neighbors = &pole_to_pole_neighbors[pole]
    //         .iter()
    //         .filter(|&neigh| selected_poles.contains(neigh))
    //         .copied()
    //         .sorted_by_key(|&pos| (pos.x, pos.y))
    //         .collect_vec();
    //     println!("Neighbors: {:?}", neighbors);
    // }

    MinerLayout {
        rows,
        len,
        miners: miner_vars
            .iter()
            .filter(|&(_, &var)| result.value(var) > 0.5)
            .map(|(&spot, _)| spot)
            .collect_vec(),
        poles: selected_poles.into_iter().collect_vec(),
    }
}

fn visualize_miners(
    name: &impl AsRef<std::path::Path>,
    prototypes: &EntityPrototypeDict,
    layout: &MinerLayout,
) -> Result<(), Box<dyn std::error::Error>> {
    let MinerLayout {
        rows,
        len,
        miners,
        poles,
    } = layout;

    let mut model = BpModel::new();

    let miner_prototype: &EntityPrototypeRef = &prototypes["electric-mining-drill"];
    for spot in miners {
        let center = spot.get_center_pt();
        model
            .add_no_overlap(WorldEntity {
                position: TilePosition::from(center).center_map_pos(),
                direction: 0,
                prototype: miner_prototype.clone(),
            })
            .unwrap();
    }

    let pole_prototype: &EntityPrototypeRef = &prototypes["small-electric-pole"];
    for &pole in poles {
        model
            .add_no_overlap(WorldEntity {
                position: pole.center_map_pos(),
                direction: 0,
                prototype: pole_prototype.clone(),
            })
            .unwrap();
    }

    let drawing = Drawing::on_area(
        name,
        TileBoundingBox::from_size(size2(*len, rows * 7)),
        10,
        10,
    )
    .unwrap();

    let pole_graph = model.get_maximally_connected_pole_graph().0;
    let connected_graph = PrettyPoleConnector::default().connect_poles(&pole_graph);

    drawing.draw_model(&model)?;
    drawing.draw_pole_graph(&connected_graph, 0.1)?;

    for row in 0..*rows {
        drawing
            .draw_line(
                TilePosition::from((0, row * 7 + 3)).center_map_pos(),
                TilePosition::from((*len, row * 7 + 3)).center_map_pos(),
                BLACK.stroke_width(2),
            )
            .unwrap();
    }

    drawing.show()?;

    Ok(())
}

#[test]
fn run_miner_ilp() -> Result<(), Box<dyn std::error::Error>> {
    let rows = 6;
    let len = 13 * 3 + 3;
    let proto_dict = load_prototype_data().unwrap();
    let miners = solve_miner_lp(rows, len, 13, 12);
    println!("Number of miners: {}", miners.miners.len());
    visualize_miners(&"miner_layout.png", &proto_dict, &miners)?;
    Ok(())
}
