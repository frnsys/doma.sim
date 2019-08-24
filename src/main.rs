extern crate chrono;
extern crate md5;
extern crate noise;
extern crate pbr;
extern crate rand;
extern crate redis;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate petgraph;
extern crate rand_distr;

mod agent;
mod social;
mod city;
mod config;
mod design;
mod grid;
mod play;
mod sim;
mod stats;
mod sync;
mod policy;
use self::config::Config;
use self::sim::Simulation;
use self::play::{PlayManager, Control};
use pbr::ProgressBar;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use chrono::{DateTime, Utc, Local};

fn save_run_data(sim: &Simulation, history: &Vec<Value>, conf: &Config) {
    let now: DateTime<Utc> = Utc::now();
    let now_str = now.format("%Y.%m.%d.%H.%M.%S").to_string();
    let results = json!({
        "history": history,
        "meta": {
            "seed": conf.seed,
            "design": conf.design_id,
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
    println!("Wrote output to {:?}", path);
}

fn main() {
    let conf = config::load_config();
    let debug = conf.debug;
    let steps = conf.steps;
    let mut rng: StdRng = SeedableRng::seed_from_u64(conf.seed);

    let mut play = PlayManager::new();
    loop {
        play.set_loading().unwrap();

        // Load and setup world
        let design = design::load_design(&conf.design_id);
        let mut sim = Simulation::new(design, conf.clone(), &mut rng);
        println!("{:?} tenants", sim.tenants.len());
        play.reset().unwrap();

        if debug {
            let mut history = Vec::with_capacity(steps);
            let mut pb = ProgressBar::new(steps as u64);
            for _ in 0..steps {
                sim.step(&mut rng);
                history.push(stats::stats(&sim));
                pb.inc();
            }
            save_run_data(&sim, &history, &sim.conf);

            // Run only once
            break;

        } else {

            // Setup tenants for players to choose
            play.gen_player_tenant_pool(&sim.tenants, &sim.city);
            play.set_ready().unwrap();
            sync::sync(sim.time, &sim.city, &sim.design, stats::stats(&sim)).unwrap();
            println!("Ready: Session {}", Local::now().to_rfc3339());

            loop {
                // Blocks until a run command is received;
                // will process other commands while waiting
                let control = play.wait_for_control(&mut sim, &mut rng);
                match control {
                    Control::Run(steps) => {
                        println!("Running for {:?} steps...", steps);
                        play.set_running().unwrap();
                        for step in 0..steps {
                            sim.step(&mut rng);
                            play.sync_step(step, steps).unwrap();
                        }
                        sync::sync(sim.time, &sim.city, &sim.design, stats::stats(&sim)).unwrap();
                        play.sync_players(&sim.tenants, &sim.city).unwrap();
                        play.set_ready().unwrap();
                        println!("Finished running.");
                    },
                    Control::Reset => {
                        println!("Resetting...");
                        break;
                    }
                }
            }
        }
    }
}
