use std::fmt::Debug;
use std::fs::File;
use std::path::PathBuf;

use clap::*;
use factorio_blueprint::BlueprintCodec;
use factorio_blueprint::Container::Blueprint;
use good_lp::{highs, WithMipGap};
use plotters::style::HSLColor;

use pole_solver::*;

use crate::bp_model::{BpModel, ModelEntity};

mod better_bp;
mod bp_model;
mod draw;
mod pole_graph;
mod pole_solver;
mod position;
mod prototype_data;
mod rcid;

#[derive(Parser, Debug)]
#[command(version, about, next_line_help = true)]
struct Args {
    #[arg(short, long, name = "FILE")]
    input: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    let result = File::open(args.input);
    let result = match result {
        Ok(file) => file,
        Err(e) => Err(format!("Error opening file: {}", e))?,
    };
    let bp = BlueprintCodec::decode(result)?;

    let bp = match bp {
        Blueprint(bp) => bp,
        _ => Err("Not a blueprint")?,
    };

    let bp_model = better_bp::BlueprintEntities::from_blueprint(&bp);

    // println!("{:?}", bp_model);
    let prototype_data = prototype_data::load_prototype_data()?;
    let small_pole = prototype_data["small-electric-pole"].clone();
    let medium_pole = prototype_data["medium-electric-pole"].clone();
    let mut model = BpModel::from_bp_entities(&bp_model, &prototype_data);
    model.retain(|e: &ModelEntity| !e.prototype.is_pole() || e.prototype == medium_pole);

    let model = model;

    let all_poles = model.with_all_candidate_poles(model.get_bounding_box(), &[&small_pole]);
    let mut graph = all_poles.get_maximally_connected_pole_graph().0;
    let mut to_remove = vec![];
    for node in graph.node_indices() {
        let nd = &graph[node];
        if nd.powered_entities.is_empty() {
            to_remove.push(node);
        }
    }
    to_remove.iter().rev().for_each(|node| {
        graph.remove_node(*node);
    });

    let solver = SetCoverILPSolver {
        solver: highs,
        config: |mut model| {
            model = model.with_mip_gap(0.01)?;
            // model.set_parameter("sec", "60");
            model.set_verbose(true);
            model = model.set_time_limit(10.0);
            Ok(model)
        },
        cost: Box::new(move |graph, idx| {
            if graph[idx].entity.prototype == small_pole {
                1.0
            } else {
                6.0
            }
        }),
        connectivity: Some(DistanceConnectivity::default()),
        // connectivity: None,
    };

    let sol_graph = solver
        .solve(&graph)
        .expect("Failed to solve pole set cover");

    println!("Solution has {} poles", sol_graph.node_count());

    println!("visualizing");
    let drawing = draw::Drawing::on_area("output.png", model.get_bounding_box(), 1000)?;

    drawing.draw_all_entities(model.all_world_entities())?;
    let mut model2 = BpModel::new();
    model2.add_from_pole_graph(&sol_graph);

    drawing.draw_all_entities(model2.all_world_entities())?;
    drawing.draw_pole_graph(&sol_graph, HSLColor(0.15, 0.8, 0.5), 0.2)?;

    drawing.area.present()?;

    Ok(())
}
