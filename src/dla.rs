use bytemuck as bm;
use nalgebra::Vector3;
use ply_rs::{
    ply::{
        Addable, DefaultElement, ElementDef, Encoding, Ply, Property, PropertyDef, PropertyType,
        ScalarType,
    },
    writer::Writer,
};
use rand::{distributions::Distribution, Rng};
use rand_distr::UnitSphere;
use rand_xoshiro::{rand_core::SeedableRng, Xoshiro256Plus};
use rstar::{primitives::PointWithData, RStarInsertionStrategy, RTree, RTreeParams};
use std::{env, fs::File, path::Path};

pub use crate::*;

type Index = usize;

pub type Point3D = Vector3<f32>;

trait Square {
    fn square(&self) -> Self;
}

impl Square for f32 {
    #[inline]
    fn square(&self) -> Self {
        self * self
    }
}

#[inline]
fn lerp_points(a: &Point3D, b: &Point3D, d: f32) -> Point3D {
    a + (b - a).normalize() * d
}

#[inline]
fn lerp(a: f32, b: f32, l: f32) -> f32 {
    a * (1.0 - l) + b * l
}

type IndexValue = PointWithData<Index, [f32; 3]>;

pub struct Params;
impl RTreeParams for Params {
    const MIN_SIZE: usize = 3;
    const MAX_SIZE: usize = 9;
    const REINSERTION_COUNT: usize = 5;
    type DefaultInsertionStrategy = RStarInsertionStrategy;
}

type Tree = RTree<IndexValue /* , Params */>;

pub struct Model {
    config: Config,
    particle_spacing: f32,
    attraction_distance: f32,
    repulsion_distance: f32,
    stickiness: f32,
    bounding_radius: f32,
    stubbornness: u8,
    join_attempts: Vec<u8>,
    particles: Vec<(Point3D, f32)>,
    tree: Tree,
    rng: Xoshiro256Plus,
}

impl Model {
    pub fn new(config: &Config) -> Model {
        Model {
            // Parameters from config.
            config: config.clone(),

            attraction_distance: config.aggregation.attraction_distance.unwrap_or(3.0),
            repulsion_distance: config.aggregation.repulsion_distance.unwrap_or(1.0),
            stubbornness: config.aggregation.stubbornness.unwrap_or(0),
            stickiness: config.aggregation.stickiness.unwrap_or(1.0),
            // Parameters modified during run().
            particle_spacing: 1.0,
            // Output members.
            bounding_radius: 0.0,
            join_attempts: Vec::new(),
            particles: Vec::new(),
            tree: Tree::new_with_params(),
            rng: Xoshiro256Plus::seed_from_u64(config.aggregation.random_seed.unwrap_or(42)),
        }
    }

    pub fn run(&mut self) {
        let mut number_of_particles = self.config.aggregation.particles.unwrap_or(1000);

        let progress_bar = if self.config.aggregation.show_progress.unwrap_or(true) {
            ProgressBar::new(number_of_particles as u64)
        } else {
            ProgressBar::hidden()
        };

        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta})",
                )
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );

        let scale = self.config.particle.scale.unwrap_or([2.0f32; 2]);

        match self
            .config
            .aggregation
            .start_shape
            .shape
            .as_ref()
            .unwrap_or(&"point".to_string())
            .as_str()
        {
            "ring" => {
                let radius = self
                    .config
                    .aggregation
                    .start_shape
                    .diameter
                    .unwrap_or(100.0)
                    * 0.5;

                let particles = self.config.aggregation.start_shape.particles.unwrap_or(360);

                for i in 0..particles {
                    let angle = (i as f32 / particles as f32) * std::f32::consts::TAU;
                    let x = angle.cos() * radius;
                    let y = angle.sin() * radius;
                    self.add(&Point3D::new(x, y, 0.0), scale[0]);
                    progress_bar.inc(1);
                }
                number_of_particles -= particles;
            }
            _ => {
                // Single seed point.
                self.add(&dla::Point3D::new(0.0, 0.0, 0.0), scale[0]);
                number_of_particles -= 1;
            }
        };

        // Run diffusion-limited aggregation.
        for p in 0..number_of_particles {
            self.diffuse_particle(lerp(
                scale[0],
                scale[1],
                p as f32 / number_of_particles as f32,
            ));

            let spacing = self.config.aggregation.spacing.unwrap_or([1.0f32; 2]);

            self.particle_spacing = lerp(
                spacing[0],
                spacing[1],
                p as f32 / number_of_particles as f32,
            );

            progress_bar.inc(1);
        }

        progress_bar.finish();
    }

    /// Renders the scene via 3Delight|NSI.
    pub fn render_nsi(&mut self) {
        // Create rendering context.
        let c = {
            if self.config.nsi_render.output.cloud_render.unwrap() {
                nsi::Context::new(&[
                    nsi::integer!("cloud", 1),
                    nsi::string!("software", "RENDERDL"),
                ])
            } else {
                nsi::Context::new(&[])
            }
        }
        .expect("Could not create NSI rendering context.");

        self.output_scene_nsi(&c);
    }

    pub fn write_nsi(&mut self, path: &Path) {
        let c =
            nsi::Context::new(&[nsi::string!("streamfilename", path.to_str().unwrap())]).unwrap();

        self.output_scene_nsi(&c);
    }

    #[allow(unused_must_use)]
    pub fn write_ply(&self, path: &Path) {
        // Create a ply object.
        let mut ply = {
            let mut ply = Ply::<DefaultElement>::new();
            ply.header.encoding = Encoding::Ascii;
            ply.header
                .comments
                .push("Reaction limited diffusion".to_string());

            let mut point_element = ElementDef::new("point".to_string());

            point_element.properties.add(PropertyDef::new(
                "x".to_string(),
                PropertyType::Scalar(ScalarType::Float),
            ));
            point_element.properties.add(PropertyDef::new(
                "y".to_string(),
                PropertyType::Scalar(ScalarType::Float),
            ));
            point_element.properties.add(PropertyDef::new(
                "z".to_string(),
                PropertyType::Scalar(ScalarType::Float),
            ));
            ply.header.elements.add(point_element);

            // Add data
            let mut points = Vec::new();

            for particle in &self.particles {
                let mut point = DefaultElement::new();
                point.insert("x".to_string(), Property::Float(particle.0.x));
                point.insert("y".to_string(), Property::Float(particle.0.y));
                point.insert("z".to_string(), Property::Float(particle.0.z));
                points.push(point);
            }

            ply.payload.insert("point".to_string(), points);

            // only `write_ply` calls this by itself, for all other
            // methods the client is responsible to make the
            // data structure consistent. We do it here for
            // demonstration purpose.
            ply.make_consistent().unwrap();

            ply
        };

        let mut buffer = File::create(&path).unwrap();

        let w = Writer::new();
        w.write_ply(&mut buffer, &mut ply);
    }

    /// Add a prticle to the model 'manually'.
    fn add(&mut self, point: &Point3D, scale: f32) {
        let index = self.particles.len();
        self.tree
            .insert(PointWithData::new(index, [point.x, point.y, point.z]));
        self.particles.push((*point, scale));
        self.join_attempts.push(0);
        self.bounding_radius = self
            .bounding_radius
            .max(point.magnitude() + self.attraction_distance);
    }

    /// Diffuses one new particle and adds it to the model.
    fn diffuse_particle(&mut self, scale: f32) {
        // compute particle starting location
        let particle: &mut Point3D = &mut self.random_particle();

        // do the random walk
        loop {
            // get distance to nearest other particle
            let parent = self.nearest_particle(&particle);
            let distance_squared = (*particle - self.particles[parent].0).magnitude_squared();

            // check if close enough to join
            if distance_squared < self.attraction_distance.square() {
                if !self.should_join(parent) {
                    // push particle away a bit
                    *particle = lerp_points(
                        &self.particles[parent].0,
                        &particle,
                        self.attraction_distance + self.repulsion_distance,
                    );
                    continue;
                }

                // adjust particle position in relation to its parent
                *particle = self.place_particle(&particle, parent);

                // add the point
                self.add(&particle, scale); //, parent);
                break;
            }

            // move randomly
            let move_magnitude = self
                .repulsion_distance
                .max(distance_squared.sqrt() - self.attraction_distance);
            *particle += move_magnitude * self.random_point_on_unit_sphere();

            // reset to a new random particle if is too far away
            if self.out_of_bounds(&particle) {
                *particle = self.random_particle();
            }
        }
    }

    /// Returns the index of the nearest neighbour.
    #[inline]
    fn nearest_particle(&self, point: &Point3D) -> Index {
        self.tree
            .nearest_neighbor(&[point.x, point.y, point.z])
            .unwrap()
            .data
    }

    /// Returns a random, uniformly distributed point inside the unit
    /// sphere.
    fn random_point_on_unit_sphere(&mut self) -> Point3D {
        //let u: ;
        let v: [f32; 3] = UnitSphere.sample(&mut self.rng);
        Point3D::new(v[0], v[1], v[2])
    }

    /// Returns a random point to start a new particle.
    #[inline]
    fn random_particle(&mut self) -> Point3D {
        self.random_point_on_unit_sphere() * self.bounding_radius
    }

    /// Returns true if the particle has traveled
    /// too far outside the current bounding sphere.
    #[inline]
    fn out_of_bounds(&self, point: &Point3D) -> bool {
        point.magnitude_squared() > (self.bounding_radius * 2.0).square()
    }

    /// Returns true if the point should attach to the specified
    /// parent particle. This is only called when the point is already
    /// within the required attraction distance.
    #[inline]
    fn should_join(&mut self, parent: Index) -> bool {
        self.join_attempts[parent as usize] += 1;
        if self.join_attempts[parent as usize] < self.stubbornness {
            false
        } else {
            //let mut rng = rand::thread_rng();
            self.rng.gen_range(0.0..1.0) <= self.stickiness
        }
    }

    /// Computes the final placement of the particle.
    #[inline]
    fn place_particle(&self, point: &Point3D, parent: Index) -> Point3D {
        lerp_points(
            &self.particles[parent as usize].0,
            point,
            self.particle_spacing,
        )
    }

    fn instance_obj_nsi(&self, c: &nsi::Context, instance_obj_path: &Path) {
        let object = tobj::load_obj(instance_obj_path, &tobj::LoadOptions::default());
        if let Err(e) = object {
            eprintln!("Error loading '{}': {}", instance_obj_path.display(), e);
            return;
        }
        let (models, _materials) = object.unwrap();

        c.create("instance", nsi::NodeType::Transform, &[]);
        for model in models {
            let mesh = &model.mesh;

            c.create(model.name.as_str(), nsi::NodeType::Mesh, &[]);

            c.set_attribute(
                model.name.as_str(),
                &[
                    nsi::points!("P", &mesh.positions),
                    nsi::integers!("P.indices", bm::cast_slice(mesh.indices.as_slice())),
                    nsi::integers!(
                        "nvertices",
                        bm::cast_slice(mesh.face_arities.as_slice())
                    ),
                ],
            );

            if self.config.particle.subdivision.unwrap_or(false) {
                c.set_attribute(
                    model.name.as_str(),
                    &[nsi::string!("subdivision.scheme", "catmull-clark")],
                );
            }

            c.connect(model.name.as_str(), "", "instance", "objects", &[]);
        }
    }

    fn output_scene_nsi(&mut self, c: &nsi::Context) {
        if_chain! {
            if let Some(instance_geo) = &self.config.particle.instance_geo;
            if let instance_geo_path = Path::new(&instance_geo);
            if instance_geo_path.exists();
            then {
                // Create instances on each particle.
                self.instance_obj_nsi(c, &instance_geo_path);

                c.create(
                    "particles",
                    nsi::NodeType::Instances,
                    &[],
                );
                c.connect(
                    "particles",
                    "",
                    ".root",
                    "objects",
                    &[],
                );
                c.connect(
                    "instance",
                    "",
                    "particles",
                    "sourcemodels",
                    &[],
                );

                let mut matrix =
                    Vec::<f64>::with_capacity(self.particles.len() * 16);

                self.particles.iter().for_each(|p| {
                    matrix.extend_from_slice(&[
                        p.1 as f64,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        p.1 as f64,
                        0.0,
                        0.0,
                        0.0,
                        0.0,
                        p.1 as f64,
                        0.0,
                        p.0[0] as f64,
                        p.0[1] as f64,
                        p.0[2] as f64,
                        1.0,
                    ])
                });

                c.set_attribute(
                    "particles",
                    &[nsi::double_matrices!("transformationmatrices", &matrix)]
                );

            } else {

                // Send particles.
                c.create(
                    "particles",
                    nsi::NodeType::Particles,
                    &[],
                );
                c.connect(
                    "particles",
                    "",
                    ".root",
                    "objects",
                    &[],
                );

                let mut particle_positions =
                    Vec::<f32>::with_capacity(3 * self.particles.len());
                let mut particle_widths =
                    Vec::<f32>::with_capacity(self.particles.len());

                self.particles.iter().for_each(|p| {
                    p.0.iter().for_each(|c| particle_positions.push(*c));
                    particle_widths.push(p.1);
                });

                c.set_attribute(
                    "particles",
                    &[
                        nsi::points!("P", &particle_positions),
                        nsi::floats!("width", &particle_widths),
                    ],
                );
            }
        }

        self.particles.clear();

        // Get 3Delight path to find shaders.
        let delight = {
            match env::var("DELIGHT") {
                Err(_) => {
                    eprintln!(
                        "3Delight|NSI not found. Shaders will likely not be found.\n\
                        Please download & install 3Delight|NSI from https://www.3delight.com/download."
                    );
                    "".to_string()
                }
                Ok(path) => path,
            }
        };

        let shader_searchpath = Path::new(&delight).join("osl");

        // Setup a camera transform.
        c.create("camera_xform", nsi::NodeType::Transform, &[]);
        c.connect("camera_xform", "", ".root", "objects", &[]);

        c.set_attribute(
            "camera_xform",
            &[nsi::double_matrix!(
                "transformationmatrix",
                &[
                    1.0f64,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    0.0,
                    1.0,
                    0.0,
                    0.0,
                    0.0,
                    4.0f64 * self.bounding_radius as f64,
                    1.0,
                ]
            )],
        );

        // Setup a camera.
        c.create("camera", nsi::NodeType::PerspectiveCamera, &[]);

        c.set_attribute("camera", &[nsi::float!("fov", 30.)]);
        c.connect("camera", "", "camera_xform", "objects", &[]);

        // Setup a screen.
        c.create("screen", nsi::NodeType::Screen, &[]);
        c.connect("screen", "", "camera", "screens", &[]);

        let resolution = self.config.nsi_render.resolution.unwrap_or(2048);
        c.set_attribute(
            "screen",
            &[
                nsi::integers!("resolution", &[resolution as _, resolution as _]).array_len(2),
                nsi::integer!(
                    "oversampling",
                    self.config.nsi_render.oversampling.unwrap_or(64) as _
                ),
            ],
        );

        c.set_attribute(
            ".global",
            &[
                nsi::integer!("renderatlowpriority", 1),
                nsi::string!(
                    "bucketorder",
                    self.config
                        .nsi_render
                        .bucket_order
                        .as_ref()
                        .unwrap_or(&"circle".to_string())
                        .as_str()
                ),
                nsi::integer!(
                    "quality.shadingsamples",
                    self.config.nsi_render.shading_samples.unwrap_or(64) as _
                ),
                nsi::integer!("maximumraydepth.reflection", 6),
            ],
        );

        // Setup an output layer.
        c.create("beauty", nsi::NodeType::OutputLayer, &[]);
        c.connect("beauty", "", "screen", "outputlayers", &[]);
        c.set_attribute(
            "beauty",
            &[
                nsi::string!("variablename", "Ci"),
                nsi::integer!("withalpha", 1),
                nsi::string!("scalarformat", "half"),
            ],
        );

        // We add i-display by default.
        if self.config.nsi_render.output.display.unwrap_or(true) {
            // Setup an i-display driver.
            c.create("display_driver", nsi::NodeType::OutputDriver, &[]);
            c.connect("display_driver", "", "beauty", "outputdrivers", &[]);
            c.set_attribute("display_driver", &[nsi::string!("drivername", "idisplay")]);
        }

        if let Some(file_name) = &self.config.nsi_render.output.file_name {
            // Setup an EXR file output driver.
            c.create("file_driver", nsi::NodeType::OutputDriver, &[]);
            c.connect("file_driver", "", "beauty", "outputdrivers", &[]);
            c.set_attribute(
                "file_driver",
                &[
                    nsi::string!("imagefilename", file_name.as_str()),
                    nsi::string!("drivername", "exr"),
                ],
            );
        }

        // Particle attributes.
        c.create("particle_attrib", nsi::NodeType::Attributes, &[]);
        c.connect(
            "particle_attrib",
            "",
            "particles",
            "geometryattributes",
            &[],
        );

        // Particle shader.
        c.create("particle_shader", nsi::NodeType::Shader, &[]);
        c.connect(
            "particle_shader",
            "",
            "particle_attrib",
            "surfaceshader",
            &[],
        );

        let material = &self.config.material;

        c.set_attribute(
            "particle_shader",
            &[
                nsi::string!(
                    "shaderfilename",
                    shader_searchpath.join("dlPrincipled").to_str().unwrap()
                ),
                nsi::color!("i_color", &material.color.unwrap_or([1.0f32, 0.6, 0.3])),
                //nsi::arg!("coating_thickness", &0.1f32),
                nsi::float!("roughness", material.roughness.unwrap_or(0.)),
                nsi::float!("specular_level", material.specular_level.unwrap_or(0.5)),
                nsi::float!("metallic", material.metallic.unwrap_or(0.)),
                nsi::float!("anisotropy", material.anisotropy.unwrap_or(0.0f32)),
                nsi::float!("sss_weight", material.sss_weight.unwrap_or(0.0f32)),
                nsi::color!(
                    "sss_color",
                    &material.sss_color.unwrap_or([0.5f32, 0.5, 0.5])
                ),
                nsi::float!("sss_scale", material.sss_scale.unwrap_or(0.0f32)),
                nsi::color!(
                    "incandescence",
                    &material.incandescence.unwrap_or([0.0f32, 0.0, 0.0])
                ),
                nsi::float!(
                    "incandescence_intensity",
                    material.incandescence_intensity.unwrap_or(0.0f32)
                ),
                nsi::color!(
                    "incandescence_multiplier",
                    &material
                        .incandescence_multiplier
                        .unwrap_or([1.0f32, 1.0, 1.0])
                ),
            ],
        );

        // Set up an environment light.
        c.create("env_xform", nsi::NodeType::Transform, &[]);
        c.connect("env_xform", "", ".root", "objects", &[]);

        c.create("environment", nsi::NodeType::Environment, &[]);
        c.connect("environment", "", "env_xform", "objects", &[]);

        c.create("env_attrib", nsi::NodeType::Attributes, &[]);
        c.connect("env_attrib", "", "environment", "geometryattributes", &[]);

        c.set_attribute("env_attrib", &[nsi::integer!("visibility.camera", 0)]);

        c.create("env_shader", nsi::NodeType::Shader, &[]);
        c.connect("env_shader", "", "env_attrib", "surfaceshader", &[]);

        // Environment light attributes.
        c.set_attribute(
            "env_shader",
            &[
                nsi::string!(
                    "shaderfilename",
                    shader_searchpath.join("environmentLight").to_str().unwrap()
                ),
                nsi::float!("intensity", self.config.environment.intensity.unwrap_or(1.)),
            ],
        );

        if let Some(texture) = &self.config.environment.texture {
            c.set_attribute("env_shader", &[nsi::string!("image", texture.as_str())]);
        }

        // And now, render it!
        c.render_control(&[nsi::string!("action", "start")]);

        // Block until render is done.
        c.render_control(&[nsi::string!("action", "wait")]);
    }
}
