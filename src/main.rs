use std::fmt::Debug;
use std::fs::File;
use std::path::PathBuf;

use clap::*;
use factorio_blueprint::BlueprintCodec;
use factorio_blueprint::Container::Blueprint;
use petgraph::algo;
use petgraph::data::FromElements;
use plotters::style::HSLColor;

use crate::bp_model::BpModel;
use crate::pole_graph::PoleGraph;

mod better_bp;
mod bp_model;
mod draw;
mod pole_graph;
mod position;
mod prototype_data;
mod rcid;
mod pole_solver;

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
    let entity_data = prototype_data::load_prototype_data()?;
    let model = BpModel::from_bp_entities(&bp_model, &entity_data);


    println!("visualizing");
    let drawing = draw::Drawing::on_area("output.png", model.get_bounding_box(), 1025)?;

    drawing.draw_all_entities(model.all_world_entities())?;
    let graph = model.get_current_pole_graph().0;
    let max_graph = model.get_maximally_connected_pole_graph().0;
    let mst = PoleGraph::from_elements(algo::min_spanning_tree(&graph));
    drawing.draw_pole_graph(
        &max_graph,
        HSLColor(0.5, 0.9, 0.7),
        0.1
    )?;
    drawing.draw_pole_graph(
        &graph,
        HSLColor(0.15, 0.8, 0.5),
        0.2
    )?;

    drawing.area.present()?;

    Ok(())
}
