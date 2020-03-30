# Diffusion Limited Aggregation in Rust

Just to play with something while testing the [ɴsɪ](https://crates.io/crates/nsi) crate.

![Result of rendering with 3Delight|NSI from within the tool](dla.jpg)

## Building

Because the ɴsɪ crate uses unstable feature you need to set the local toolchain to `nightly`:

```shell
> rustup override set nightly
```

The space partitioning insertion is extremenly slow for debug builds.
If you want to generate more than 10k particles doing a release build is mandatory.
Because the crate has link time optimizations enabled, the final build step is quiet slow.

```shell
> cargo build --release
```

## Running

Builds will be in the `./target` folder.

```shell
> target/release/rdla -r
```

## Usage

```shell

USAGE:
    rdla [FLAGS] [OPTIONS]

FLAGS:
        --cloud      Render using 3Delight|NSI Cloud
    -h, --help       Prints help information
    -r, --render     Render an image of result with 3Delight|NSI
    -V, --version    Prints version information
    -v, --verbose    Babble a lot

OPTIONS:
    -c, --config <FILE>       Sets a custom config file
    -d, --dump <FILE>         Dump the result into an .nsi stream or
                              into a Standford .ply file.
    -p, --particles <ITER>    Number of particles to generate
```

## Config File

The app looks for config file named `rdla.toml` in the current
directory.

This can be overridden with the `--config` flag.

```toml
[aggregation]
    show_progress = true
    random_seed = 42
    particles = 10000
    # Spacing can be changed over the iteration.
    # The 1st value is used for the first particle place
    # and the last for the last particle. In between,
    # spacing is linearly interpolated.
    spacing = [1.0, 1.0]
    attraction_distance = 3.0
    repulsion_distance = 1.0
    stubbornness = 0
    stickiness = 1.0

    [aggregation.start_shape]
        shape = "point"
        diameter = 0
        particles = 1

[particle]
    # Scale can be changed over the iteration.
    # The 1st value is used for the first particle placed
    # and the last for the last particle. In between,
    # scale is linearly interpolated.
    scale = [2.0, 2.0]
    # A wavefront OBJ (converted to triangles for now)
    # to instace instead of a particle.
    instance_geo = "dodeca.obj"
    subidivsion = true

[material]
    color = [0.8, 0.8, 0.8]
    roughness = 0.3
    metallic = 1.0
    specular_level = 0.8

[environment]
    texture = ""

[nsi]
    resolution = 2048
    shading_samples = 32
    oversampling = 64
    bucket_order = "circle"

[output]
    file_name = "foobar.exr"
    i_display = true
```