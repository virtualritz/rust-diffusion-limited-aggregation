#[cfg(target_os = "macos")]
extern crate jemallocator;

#[cfg(target_os = "macos")]
#[global_allocator]
static ALLOC: jemallocator::Jemalloc = jemallocator::Jemalloc;

use clap::{load_yaml, App};
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
    nsi_render: NsiRender,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct NsiRender {
    resolution: Option<u32>,
    shading_samples: Option<u32>,
    oversampling: Option<u32>,
    bucket_order: Option<String>,
    output: Output,
}

#[derive(Clone, Debug, Default, Deserialize)]
struct Material {
    color: Option<[f32; 3]>,
    roughness: Option<f32>,
    specular_level: Option<f32>,
    metallic: Option<f32>,
    anisotropy: Option<f32>,
    sss_weight: Option<f32>,
    sss_color: Option<[f32; 3]>,
    sss_scale: Option<f32>,
    incandescence: Option<[f32; 3]>,
    incandescence_intensity: Option<f32>,
    incandescence_multiplier: Option<[f32; 3]>,
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
    cloud_render: Option<bool>,
    display: Option<bool>,
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
    let yaml = load_yaml!("cli.yml");
    let app = App::from_yaml(yaml).get_matches();

    // Read config file (if it exists).
    let config_file = app.value_of("config").unwrap_or("rdla.toml");

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
    if let Some(particles) = app.value_of("particles") {
        config.aggregation.particles = Some(particles.parse::<u32>()?);
    }

    match app.subcommand() {
        ("render", Some(render_args)) => {
            if render_args.is_present("cloud") {
                config.nsi_render.output.cloud_render = Some(true);
            // We do not allow the cloud option from the config file.
            // It has to be specified from the command line.
            } else {
                config.nsi_render.output.cloud_render = Some(false);
            }
            if render_args.is_present("display") {
                config.nsi_render.output.display = Some(true);
            }

            if let Some(file_name) = render_args.value_of("FILE") {
                config.nsi_render.output.file_name =
                    Some(file_name.to_string());
            }

            let mut model = dla::Model::new(&mut config);
            model.run();
            model.render_nsi();
        }
        ("dump", Some(dump_args)) => {
            let path = Path::new(dump_args.value_of("FILE").unwrap());

            let mut model = dla::Model::new(&mut config);
            model.run();

            if "ply" == path.extension().unwrap() {
                model.write_ply(&path);
            } else {
                model.write_nsi(&path);
            }
        }
        ("", None) => {
            eprintln!("No subcommand given. Please specify at least one of 'help, 'render' or 'dump'.")
        }
        _ => unreachable!(),
    }
    Ok(())
}
