use clap::{App, Arg};
use indicatif::{ProgressBar, ProgressStyle};
use serde_derive::Deserialize;
use std::io::Write;

use std::{fs::File, io::prelude::*, path::Path};

#[macro_use]
extern crate error_chain;

#[macro_use]
extern crate if_chain;

#[allow(deprecated)]
error_chain! {
    foreign_links {
        Io(std::io::Error);
        ParseInt(::std::num::ParseIntError);
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct Config {
    aggregation: Aggregation,
    particle: Particle,
    material: Material,
    environment: Environment,
    nsi: Nsi,
    output: Output,
    cloud_render: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Nsi {
    resolution: Option<u32>,
    shading_samples: Option<u32>,
    oversampling: Option<u32>,
    bucket_order: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Material {
    color: Option<[f32; 3]>,
    roughness: Option<f32>,
    metallic: Option<f32>,
    specular_level: Option<f32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Environment {
    texture: Option<String>,
    intensity: Option<f32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Aggregation {
    show_progress: Option<bool>,
    random_seed: Option<u64>,
    particles: Option<u32>,
    spacing: Option<[f32; 2]>,
    attraction_distance: Option<f32>,
    repulsion_distance: Option<f32>,
    stubbornness: Option<u8>,
    stickiness: Option<f32>,
    start_shape: StartShape,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Particle {
    scale: Option<[f32; 2]>,
    instance_geo: Option<String>,
    subdivision: Option<bool>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct StartShape {
    shape: Option<String>,
    diameter: Option<f32>,
    particles: Option<u32>,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Output {
    file_name: Option<String>,
    i_display: Option<bool>,
}

mod dla;
pub use dla::*;

fn main() {
    if let Err(ref e) = run() {
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "error: {}", e).expect(errmsg);

        for e in e.iter().skip(1) {
            writeln!(stderr, "caused by: {}", e).expect(errmsg);
        }

        if let Some(backtrace) = e.backtrace() {
            writeln!(stderr, "backtrace: {:?}", backtrace)
                .expect(errmsg);
        }

        ::std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let arg = App::new("rdla")
        .version("0.2.0")
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Creates a point cloud based on diffusion limited aggregation.")
        /*.arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Babble a lot"),
        )*/
        // FIXME: add render subcommand
        .arg(
            Arg::with_name("render")
                .short("r")
                .long("render")
                .help("Render an image of result with 3Delight|NSI"),
        )
        // FIXME: add dump subcommand
        .arg(
            Arg::with_name("dump")
                .short("d")
                .long("dump")
                .value_name("FILE")
                .takes_value(true)
                .help("Dump the result into an .nsi stream or into a Standford .ply file"),
        )
        .arg(
            Arg::with_name("config")
               .short("c")
               .long("config")
               .value_name("FILE")
               .help("Sets a custom config file")
               .takes_value(true))
        .arg(
            Arg::with_name("cloud")
                .long("cloud")
                .help("Render using 3Delight|NSI Cloud"),
        )
        .arg(
            Arg::with_name("particles")
                .short("p")
                .long("particles")
                .help("Number of particles to generate (default: 1000)")
                .value_name("N")
                .takes_value(true),
        )
        .get_matches();

    // Read config file (if it exists).
    let config_file = arg.value_of("config").unwrap_or("rdla.toml");

    let mut config: Config = {
        if let Ok(mut file) = File::open(config_file) {
            let mut contents = String::new();
            file.read_to_string(&mut contents)?;

            match toml::from_str::<Config>(&contents.as_str()) {
                Ok(toml) => toml,
                Err(e) => {
                    eprintln!(
                        "Config file error in '{}': {}.",
                        config_file, e
                    );
                    return Ok(());
                }
            }
        } else {
            // Set everything in Config to None.
            Default::default()
        }
    };

    // Override resp. config settings with command line args.
    if let Some(particles) = arg.value_of("particles") {
        config.aggregation.particles = Some(particles.parse::<u32>()?);
    }

    // We do not allow the cloud option from the config file.
    // It has to be specified from the command line.
    config.cloud_render = Some(arg.is_present("cloud"));

    let mut model = dla::Model::new(&mut config);

    model.run();

    if arg.is_present("render") {
        model.render_nsi();
    }

    if let Some(file_name) = arg.value_of("dump") {
        let path = Path::new(file_name);

        if "ply" == path.extension().unwrap() {
            model.write_ply(&path);
        } else {
            model.write_nsi(&path);
        }
    }

    Ok(())
}
