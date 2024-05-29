use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::fs::File;
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

use clap::*;
use factorio_blueprint::objects::Blueprint;
use factorio_blueprint::{BlueprintCodec, Container};
use good_lp::highs;
use once_cell::sync::Lazy;
use petgraph::graph::NodeIndex;

use better_bp::BlueprintEntities;
use bp_model::BpModel;
use pole_graph::*;
use pole_solver::*;

use crate::position::{BoundingBoxExt, TileBoundingBox};
use crate::prototype_data::{EntityPrototypeDict, EntityPrototypeRef};

mod better_bp;
mod bp_model;
mod draw;
mod pole_graph;
mod pole_solver;
mod position;
mod prototype_data;
mod rcid;

#[derive(Parser, Debug)]
#[command(version, about, subcommand_required = true, next_line_help = true)]
struct Args {
    #[arg(name = "INPUT_FILE", help = "Input blueprint txt file")]
    input: PathBuf,

    #[arg(
        short,
        long,
        help = "Output file; defaults to input file with '_out' appended"
    )]
    output: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,

    #[arg(short, long="vis", help = "also output a png visualization of the solution", action=ArgAction::SetTrue)]
    visualize: bool,
}

#[derive(Subcommand, Debug)]
enum Command {
    #[command(about = "Optimize poles in a blueprint")]
    Optimize(OptimizePoles),
}

#[derive(Parser, Debug)]
struct OptimizePoles {
    #[arg(
        help = "Candidate poles to use, separated by commas. Can use aliases: s, m, b, t. If none specified, only uses a subset of existing poles",
        name = "POLES"
    )]
    use_poles: Vec<String>,

    #[arg(
        short = 'r',
        long,
        help = "Poles to remove from input blueprint before optimization; allows candidate poles to be placed in their place. Only useful if existing poles are not candidate poles"
    )]
    remove_poles: Vec<String>,

    #[arg(
        short = 'c',
        long,
        help = "Cost for each pole type; format: 'name=cost' separated by commas. Default is 1 for all poles. Can use aliases: s, m, b, t"
    )]
    pole_costs: Option<String>,
    
    #[arg(
        short = 'E',
        long,
        help = "Remove poles that do not power any entities",
        action = ArgAction::SetTrue
    )]
    remove_empty_poles: bool,

    #[arg(
        short = 'e',
        long,
        default_value_t = 1,
        help = "Expand bounding box; allows poles to be placed outside blueprint area"
    )]
    expand: i32,

    #[arg(long, visible_alias = "--no-c", help = "Do not require that poles are connected; may be faster", action = ArgAction::SetFalse)]
    no_connectivity: bool,

    #[arg(
        short = 'P',
        long,
        help = "Relative position of the \"center\" of the blueprint; used for distance cost and connectivity heuristic. Format: 'x,y'",
        default_value = "0.5,0.5"
    )]
    center_pos: String,

    #[arg(
        short = 'D',
        long,
        help = "Cost factor for distance from center, per 10000 tiles. Helps prettify the solution. Set to 0 to disable",
        default_value_t = 1.0
    )]
    distance_cost: f64,

    #[arg(
        short = 't',
        long,
        help = "Time limit for ILP solver",
        default_value_t = 120.0,
        allow_negative_numbers = false
    )]
    time_limit: f64,

    #[arg(
        long,
        help = "MIP gap for ILP solver; also the minimum ratio the solution can be from optimal",
        default_value_t = 0.0004
    )]
    mip_rel_gap: f32,

    #[arg(
        long,
        help = "MIP absolute gap for ILP solver; also the minimum absolute difference the solution can be from optimal",
        default_value_t = 0.0
    )]
    mip_abs_gap: f32,

    #[arg(short, long, help = "Don't output stuff from ILP solver", action = ArgAction::SetTrue)]
    quiet: bool,
}

fn sep_commas(input: &[String]) -> impl Iterator<Item = String> + '_ {
    input
        .iter()
        .flat_map(|s| s.split(',').map(|s| s.to_string()))
}
fn parse_tuple(input: &str) -> Result<(f64, f64), Box<dyn Error>> {
    let mut parts = input.split(',');
    let x = parts.next().ok_or("Missing x")?.parse()?;
    let y = parts.next().ok_or("Missing y")?.parse()?;
    Ok((x, y))
}

static POLE_NAME_ALIASES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
    HashMap::from([
        ("s", "small-electric-pole"),
        ("m", "medium-electric-pole"),
        ("b", "big-electric-pole"),
        ("t", "substation"),
    ])
});

fn get_pole_prototype(name: &str, dict: &EntityPrototypeDict) -> Option<EntityPrototypeRef> {
    let real_name = POLE_NAME_ALIASES.get(name).copied().unwrap_or(name);
    dict.0.get(real_name).cloned()
}

fn get_pole_prototypes(
    names: &[String],
    dict: &EntityPrototypeDict,
) -> Result<Vec<EntityPrototypeRef>, Box<dyn Error>> {
    Ok(sep_commas(names)
        .map(|name| {
            get_pole_prototype(&name, dict).ok_or_else(|| format!("Unknown pole type: {}", name))
        })
        .collect::<Result<Vec<_>, _>>()?)
}

fn parse_pole_costs(input: &str) -> Result<HashMap<EntityPrototypeRef, f64>, Box<dyn Error>> {
    input
        .split(',')
        .map(|part| {
            let mut parts = part.split('=');
            let name = parts.next().ok_or("Missing name")?;
            let cost = parts.next().ok_or("Missing cost")?.parse()?;
            let prototype = get_pole_prototype(name, &prototype_data::load_prototype_data()?)
                .ok_or_else(|| format!("Unknown pole type: {}", name))?;
            Ok((prototype, cost))
        })
        .collect::<Result<HashMap<_, _>, _>>()
}

struct BlueprintProcessResult {
    blueprint: Blueprint,
    model: BpModel,
    bounding_box: TileBoundingBox,
}

fn optimize_poles(
    mut bp: Blueprint,
    args: &OptimizePoles,
) -> Result<BlueprintProcessResult, Box<dyn Error>> {
    let prototype_data = prototype_data::load_prototype_data()?;

    // todo: consolidate these 2 representations??
    let mut bp2 = BlueprintEntities::from_blueprint(&bp);
    let mut model = BpModel::from_bp_entities(&bp2, &prototype_data);

    if !args.remove_poles.is_empty() {
        let pole_prototypes = get_pole_prototypes(&args.remove_poles, &prototype_data)?;
        model.retain(|entity| !pole_prototypes.contains(&entity.prototype));
    }

    let poles_to_use = get_pole_prototypes(&args.use_poles, &prototype_data)?;
    let mut pole_costs = prototype_data
        .0
        .iter()
        .filter(|(_, prototype)| prototype.type_ == "electric-pole")
        .map(|(_, prototype)| (prototype.clone(), 1.0))
        .collect::<HashMap<_, _>>();

    if let Some(arg_pole_costs) = &args.pole_costs {
        pole_costs.extend(parse_pole_costs(arg_pole_costs)?);
    }

    let bounding_box = {
        if args.expand == 0 {
            model.get_bounding_box()
        } else {
            model.get_bounding_box().inflate(args.expand, args.expand)
        }
    };

    let cand_graph: CandPoleGraph = model
        .with_all_candidate_poles(bounding_box, &poles_to_use)
        .get_maximally_connected_pole_graph()
        .0
        .to_cand_pole_graph(&model);

    let center_rel_pos = parse_tuple(&args.center_pos)?;

    let center = bounding_box
        .to_f64()
        .cast_unit()
        .relative_pt_at(center_rel_pos);

    let cost_fn = |graph: &CandPoleGraph, idx: NodeIndex| {
        let entity = &graph[idx].entity;
        let score = pole_costs[&entity.prototype];
        score + (entity.position - center).length() / 10000.0 * args.distance_cost
    };

    let solver = SetCoverILPSolver {
        solver: &highs,
        config: &|mut model| {
            model.set_verbose(!args.quiet);
            Ok(model
                .set_mip_rel_gap(args.mip_rel_gap)?
                .set_mip_abs_gap(args.mip_abs_gap)?
                .set_time_limit(args.time_limit))
        },
        cost: &cost_fn,
        connectivity: if args.no_connectivity {
            Some(DistanceConnectivity { center_rel_pos })
        } else {
            None
        },
    };

    let sol_poles = solver.solve(&cand_graph)?;
    let sol_graph = PrettyPoleConnector::default().connect_poles(&sol_poles);

    println!("Result has {} poles", sol_graph.node_count());

    model.remove_all_poles();
    model.add_from_pole_graph(&sol_graph);

    bp2.entities
        .retain(|_, entity| prototype_data[&entity.name].type_ != "electric-pole");
    bp2.add_poles_from(&model);

    bp.entities = bp2.to_blueprint_entities();
    Ok(BlueprintProcessResult {
        blueprint: bp,
        model,
        bounding_box,
    })
}

fn read_blueprint(path: &PathBuf) -> Result<Blueprint, Box<dyn Error>> {
    let file = File::open(path)?;
    match BlueprintCodec::decode(BufReader::new(file))? {
        Container::Blueprint(bp) => Ok(bp),
        _ => Err("Expected input to be a blueprint, got something else".into()),
    }
}

// need to take ownership then return it... for reasons...
// the borrow checker giveth, and the borrow checker taketh away
fn write_blueprint(bp: Blueprint, path: &PathBuf) -> Result<Blueprint, Box<dyn Error>> {
    let file = File::create(path)?;
    let container = Container::Blueprint(bp);
    BlueprintCodec::encode(BufWriter::new(file), &container)?;
    Ok(match container {
        Container::Blueprint(bp) => bp,
        _ => unreachable!(),
    })
}

fn visualize_blueprint(
    result_bp: &BlueprintProcessResult,
    out_file: &Path,
) -> Result<(), Box<dyn Error>> {
    println!("visualizing");
    let png_file = out_file.with_extension("png");
    let bbox = result_bp.bounding_box;
    let drawing = draw::Drawing::on_area(&png_file, bbox, 5, 10)?;
    drawing.draw_model(&result_bp.model)?;

    drawing.show()?;
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();

    let in_file = &args.input;
    let out_file = args.output.unwrap_or_else(|| {
        let file = in_file.with_extension("");
        file.with_file_name(file.file_name().unwrap().to_str().unwrap().to_string() + "_out")
            .with_extension("txt")
    });

    let bp = read_blueprint(in_file)?;

    let mut result = match args.command {
        Command::Optimize(opt) => optimize_poles(bp, &opt)?,
    };

    result.blueprint = write_blueprint(result.blueprint, &out_file)?;

    if args.visualize {
        visualize_blueprint(&result, &out_file)?;
    }

    Ok(())
}
