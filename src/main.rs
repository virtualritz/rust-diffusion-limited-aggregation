use clap::{App, Arg, ArgMatches};
use indicatif::{ProgressBar, ProgressStyle};
use std::io::Write;

#[macro_use]
extern crate error_chain;

#[allow(deprecated)]
error_chain! {
    foreign_links {
        Io(std::io::Error);
        ParseInt(::std::num::ParseIntError);
    }
}

include!("dla.rs");

fn main() {
    if let Err(ref e) = run() {
        let stderr = &mut ::std::io::stderr();
        let errmsg = "Error writing to stderr";

        writeln!(stderr, "error: {}", e).expect(errmsg);

        for e in e.iter().skip(1) {
            writeln!(stderr, "caused by: {}", e).expect(errmsg);
        }

        if let Some(backtrace) = e.backtrace() {
            writeln!(stderr, "backtrace: {:?}", backtrace).expect(errmsg);
        }

        ::std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let args = App::new("rdla")
        .version("0.1.0")
        .author("Moritz Moeller <virtualritz@protonmail.com>")
        .about("Moves images into a folder hierarchy based on EXIF DateTime tags")
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .long("verbose")
                .help("Babble a lot"),
        )
        .arg(
            Arg::with_name("DESTINATION")
                .required(true)
                .help("Output PLY file"),
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

    let destination = args.value_of("DESTINATION").unwrap_or(".");

    let mut number_of_particles = args
        .value_of("iterations")
        .unwrap_or("10000")
        .parse::<u32>()?;

    let mut model = Model::new();

    // add seed point(s)
    //model.add(&Point3D::new(0.0, 0.0, 0.0));

    let progress_bar = ProgressBar::new(number_of_particles as u64);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
            )
            .progress_chars("█▉▊▋▌▍▎▏  "),
    );

    {
        let r = 100.0;
        let n = 360;
        for i in 0..n {
            let t = i as f32 / n as f32;
            let a = t * 2.0 * std::f32::consts::PI;
            let x = a.cos() * r;
            let y = a.sin() * r;
            model.add(&Point3D::new(x, y, 0.0));
            progress_bar.inc(1);
        }

        number_of_particles -= n;
    }

    // run diffusion-limited aggregation
    for _ in 1..number_of_particles {
        model.add_particle();
        progress_bar.inc(1);
    }

    //println!("Writing");
    model.write(Path::new(destination));

    Ok(())
}
