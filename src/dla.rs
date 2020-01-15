use rand::prelude::*;
use nalgebra::{Vector3};
use rstar::primitives::PointWithData;
use rstar::{RStarInsertionStrategy, RTree, RTreeParams};
use ply_rs::ply::{
    Addable, DefaultElement, ElementDef, Encoding, Ply, Property, PropertyDef, PropertyType,
    ScalarType,
};
use ply_rs::writer::Writer;
use std::fs::File;
use std::path::Path;

type Index = usize;

type Point3D = nalgebra::Vector3<f32>;

trait Square<T> {
    fn square(&self) -> T;
}

impl Square<f32> for f32 {
    #[inline]
    fn square(&self) -> f32 {
        self * self
    }
}

type IndexValue = PointWithData<Index, [f32; 3]>;

pub struct Params;
impl RTreeParams for Params {
    const MIN_SIZE: usize = 3;
    const MAX_SIZE: usize = 9;
    const REINSERTION_COUNT: usize = 5;
    type DefaultInsertionStrategy = RStarInsertionStrategy;
}

type Tree = RTree<IndexValue /*, Params*/ >;


#[inline]
fn lerp(a: &Point3D, b: &Point3D, d: f32) -> Point3D {
    a + (b - a).normalize() * d
}

struct Model {
    particle_spacing: f32,
    attraction_distance: f32,
    min_repulsion_distance: f32,
    stickiness: f32,
    bounding_radius: f32,
    stubbornness: u8,
    join_attempts: Vec<u8>,
    particles: Vec<Point3D>,
    tree: Tree,
    rng: ThreadRng,
}

impl Model {
    fn new() -> Model {
        let model = Model {
            particle_spacing: 1.0,
            attraction_distance: 3.0,
            min_repulsion_distance: 1.0,
            stubbornness: 0,
            stickiness: 1.0,
            bounding_radius: 0.0,
            join_attempts: Vec::new(),
            particles: Vec::new(),
            tree: Tree::new_with_params(),
            rng: rand::thread_rng(),
        };
        model
    }

    fn add(&mut self, point: &Point3D) {
        let index = self.particles.len();
        self.tree
            .insert(PointWithData::new(index, [point.x, point.y, point.z]));
        self.particles.push(*point);
        self.join_attempts.push(0);
        self.bounding_radius = self
            .bounding_radius
            .max(point.magnitude() + self.attraction_distance);
    }

    // return the index of the nearest neighbour
    #[inline]
    fn nearest_particle(&self, point: &Point3D) -> Index {
        self.tree
            .nearest_neighbor(&[point.x, point.y, point.z])
            .unwrap()
            .data
    }

    // RandomInUnitSphere returns a random, uniformly distributed point inside the
    // unit sphere
    fn random_point_in_unit_sphere(&mut self) -> Point3D {
        let point = &mut Point3D::new(
            self.rng.gen_range(-1.0, 1.0),
            self.rng.gen_range(-1.0, 1.0),
            self.rng.gen_range(-1.0, 1.0),
        );

        loop {
            if point.magnitude_squared() < 1.0 {
                break *point; // return the point
            }

            point
                .iter_mut()
                .for_each(|e| *e = self.rng.gen_range(-1.0, 1.0));
        }
    }

    // Returns a random point to start a new particle
    fn random_particle(&mut self) -> Point3D {
        self.random_point_in_unit_sphere().normalize() * self.bounding_radius
    }

    // Returns true if the particle has traveled
    // too far outside the current bounding sphere
    #[inline]
    fn out_of_bounds(&self, point: &Point3D) -> bool {
        point.magnitude_squared() > (self.bounding_radius * 2.0).square()
    }

    // Returns true if the point should attach to the specified
    // parent particle. This is only called when the point is already within
    // the required attraction distance.
    #[inline]
    fn should_join(&mut self, parent: Index) -> bool {
        self.join_attempts[parent as usize] += 1;
        if self.join_attempts[parent as usize] < self.stubbornness {
            false
        } else {
            //let mut rng = rand::thread_rng();
            self.rng.gen_range(0.0, 1.0) <= self.stickiness
        }
    }

    // Computes the final placement of the particle.
    #[inline]
    fn place_particle(&self, point: &Point3D, parent: Index) -> Point3D {
        lerp(
            &self.particles[parent as usize],
            point,
            self.particle_spacing,
        )
    }

    // Diffuses one new particle and adds it to the model
    fn add_particle(&mut self) {
        // compute particle starting location
        let particle: &mut Point3D = &mut self.random_particle();

        // do the random walk
        loop {
            // get distance to nearest other particle
            let parent = self.nearest_particle(&particle);
            let distance_squared = (*particle - self.particles[parent]).magnitude_squared();

            // check if close enough to join
            if distance_squared < self.attraction_distance.square() {
                if !self.should_join(parent) {
                    // push particle away a bit
                    *particle = lerp(
                        &self.particles[parent],
                        &particle,
                        self.attraction_distance + self.min_repulsion_distance,
                    );
                    continue;
                }

                // adjust particle position in relation to its parent
                *particle = self.place_particle(&particle, parent);

                // add the point
                self.add(&particle); //, parent);
                break;
            }

            // move randomly
            let move_magnitude = self
                .min_repulsion_distance
                .max(distance_squared.sqrt() - self.attraction_distance);
            *particle += move_magnitude * self.random_point_in_unit_sphere().normalize();

            // check if particle is too far away, reset if so
            if self.out_of_bounds(&particle) {
                *particle = self.random_particle();
            }
        }
    }

    fn write(&self, path: &Path) {
        // crete a ply objet
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
                point.insert("x".to_string(), Property::Float(particle.x));
                point.insert("y".to_string(), Property::Float(particle.y));
                point.insert("z".to_string(), Property::Float(particle.z));
                points.push(point);
            }

            ply.payload.insert("point".to_string(), points);

            // only `write_ply` calls this by itself, for all other methods the client is
            // responsible to make the data structure consistent.
            // We do it here for demonstration purpose.
            ply.make_consistent().unwrap();

            ply
        };

        let mut buffer = File::create(&path).unwrap();

        let w = Writer::new();
        w.write_ply(&mut buffer, &mut ply);
    }
}

