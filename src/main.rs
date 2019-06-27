extern crate chrono;
extern crate md5;
extern crate noise;
extern crate pbr;
extern crate rand;
extern crate redis;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;

mod agent;
mod city;
mod config;
mod design;
mod grid;
mod play;
mod sim;
mod stats;
mod sync;
use self::sim::Simulation;
use chrono::prelude::*;
use pbr::ProgressBar;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde_json::json;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

fn main() {
    let design_id = "philadelphia";
    let design = design::load_design(design_id);

    let conf = config::load_config();
    let steps = conf.steps;
    let debug = conf.debug;
    let sync = conf.sync;
    let seed = conf.seed;

    let mut rng: StdRng = SeedableRng::seed_from_u64(seed);
    let mut sim = Simulation::new(design, conf, &mut rng);

    println!("{:?} tenants", sim.tenants.len());

    let mut history = Vec::new();
    let mut pb = ProgressBar::new(steps as u64);

    for step in 0..steps {
        sim.step(step, &mut rng);

        if sync {
            sync::sync(step, &sim.city).unwrap();
        }
        if debug {
            history.push(stats::stats(&sim));
        }
        pb.inc();
    }

    if debug {
        // Save run data
        let now: DateTime<Utc> = Utc::now();
        let now_str = now.format("%Y.%m.%d.%H.%M").to_string();
        let results = json!({
            "history": history,
            "meta": {
                "seed": seed,
                "design": design_id,
                "tenants": sim.tenants.len(),
                "units": sim.city.units.len(),
                "occupancy": sim.city.units.iter().fold(0, |acc, u| acc + u.occupancy)
            }
        })
        .to_string();

        let dir = format!("runs/{}", now_str);
        let fname = format!("runs/{}/output.json", now_str);

        let path = Path::new(&dir);
        let run_path = Path::new(&now_str);
        let latest_path = Path::new("runs/latest");
        fs::create_dir(path).unwrap();
        fs::write(fname, results).expect("Unable to write file");
        if latest_path.exists() {
            fs::remove_file(latest_path).unwrap();
        }
        symlink(run_path, latest_path).unwrap();

        let conf_path = Path::join(path, Path::new("config.yaml"));
        fs::copy(Path::new("config.yaml"), conf_path).unwrap();
    }
}
