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

#[derive(Default, Deserialize)]
pub struct Config {
    aggregation: Aggregation,
    material: Material,
    environment: Environment,
    output: Output,
    cloud_render: Option<bool>,
}

#[derive(Default, Deserialize)]
struct Material {
    color: Option<[f32; 3]>,
    roughness: Option<f32>,
    metallic: Option<f32>,
    specular_level: Option<f32>,
}

#[derive(Default, Deserialize)]
struct Environment {
    texture: Option<String>,
}

#[derive(Default, Deserialize)]
struct Aggregation {
    random_seed: Option<u64>,
    iterations: Option<u32>,
    spacing: Option<f32>,
    attraction_distance: Option<f32>,
    repulsion_distance: Option<f32>,
    stubbornness: Option<u8>,
    stickiness: Option<f32>,
    start_shape: StartShape,
    particle: Particle,
}

#[derive(Default, Deserialize)]
struct Particle {
    scale: Option<f32>,
    instance_geo: Option<String>,
}

#[derive(Default, Deserialize)]
struct StartShape {
    shape: Option<String>,
    diameter: Option<f32>,
    particles: Option<u32>,
}

#[derive(Default, Deserialize)]
struct Output {
    file_name: Option<String>,
    i_display: Option<bool>,
}

/*
struct Global {

}


struct Diffuse {



}

struct Relfection {

}

struct Glossy
{

}*/

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
        .version("0.1.0")
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Creates a point cloud based on diffusion limited aggregation.")
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Babble a lot"),
        )
        .arg(
            Arg::with_name("render")
                .short("r")
                .long("render")
                .help("Render an image of result with 3Delight|NSI"),
        )
        // FIXME: add archive option
        .arg(
            Arg::with_name("dump")
                .short("d")
                .long("dump")
                .value_name("FILE")
                .takes_value(true)
                .help("Dump the result into an .nsi stream or into a Standford .ply file."),
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
            Arg::with_name("iterations")
                .short("i")
                .long("iterations")
                .help("Number of particles to generate")
                .value_name("ITER")
                .takes_value(true),
        )
        .get_matches();

    // Read config file (if it exists).
    let mut config: Config = {
        if let Ok(mut file) =
            File::open(arg.value_of("config").unwrap_or("rdla.toml"))
        {
            let mut contents = String::new();
            file.read_to_string(&mut contents).unwrap();
            toml::from_str::<Config>(&contents.as_str()).unwrap()
        } else {
            Default::default()
        }
    };

    // Override resp. config settings with command line arg.
    config.aggregation.iterations = Some(
        arg.value_of("iterations")
            .unwrap_or("1000")
            .parse::<u32>()?,
    );

    // We do not allow cloud option from the config file.
    // It has to be specified from the command line.
    config.cloud_render = Some(arg.is_present("cloud"));

    let mut model = dla::Model::new(&config);

    let progress_bar =
        ProgressBar::new(config.aggregation.iterations.unwrap() as u64);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );

    let mut number_of_particles =
        config.aggregation.iterations.unwrap();

    match config
        .aggregation
        .start_shape
        .shape
        .as_ref()
        .unwrap_or(&"point".to_string())
        .as_str()
    {
        "ring" => {
            let radius = config
                .aggregation
                .start_shape
                .diameter
                .unwrap_or(100.0)
                * 0.5;
            let n =
                config.aggregation.start_shape.particles.unwrap_or(360);
            for i in 0..n {
                let t = i as f32 / n as f32;
                let a = t * 2.0 * std::f32::consts::PI;
                let x = a.cos() * radius;
                let y = a.sin() * radius;
                model.add(&Point3D::new(x, y, 0.0));
                progress_bar.inc(1);
            }
            number_of_particles -= n;
        }
        _ => {
            // Single seed point.
            model.add(&dla::Point3D::new(0.0, 0.0, 0.0));
            number_of_particles -= 1;
        }
    };

    // run diffusion-limited aggregation
    for _ in 0..number_of_particles {
        model.diffuse_particle();
        progress_bar.inc(1);
    }

    //println!("Writing");
    //model.write(Path::new(destination));

    if arg.is_present("render") {
        model.render_nsi(&config);
    }

    if let Some(file_name) = arg.value_of("dump") {
        let path = Path::new(file_name);

        if "ply" == path.extension().unwrap() {
            model.write_ply(&path);
        } else {
            model.write_nsi(&config, &path);
        }
    }

    Ok(())
}
