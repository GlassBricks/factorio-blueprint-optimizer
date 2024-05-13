use std::fmt::Debug;
use std::fs::File;
use std::path::PathBuf;

use clap::*;
use factorio_blueprint::BlueprintCodec;
use factorio_blueprint::Container::Blueprint;

mod bp_model;

#[derive(Parser, Debug)]
#[command(version, about, next_line_help = true)]
struct Args {
    #[arg(short, long, name = "FILE")]
    input: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    println!("{:?}", args);

    let file = File::open(args.input)?;

    let bp = BlueprintCodec::decode(file)?;

    let bp = match bp {
        Blueprint(bp) => bp,
        _ => Err("Not a blueprint")?,
    };

    Ok(())
}