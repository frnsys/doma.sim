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
use self::config::Config;
use self::sim::Simulation;
use self::play::PlayManager;
use chrono::prelude::*;
use pbr::ProgressBar;
use rand::rngs::StdRng;
use rand::SeedableRng;
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;

fn save_run_data(sim: &Simulation, history: &Vec<Value>, conf: &Config) {
    let now: DateTime<Utc> = Utc::now();
    let now_str = now.format("%Y.%m.%d.%H.%M").to_string();
    let results = json!({
        "history": history,
        "meta": {
            "seed": conf.seed,
            "design": conf.sim.design_id,
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
    let steps = if debug {
        conf.steps
    } else {
        conf.play.n_steps
    };
    let mut rng: StdRng = SeedableRng::seed_from_u64(conf.seed);

    let mut play = PlayManager::new();
    play.reset().unwrap();
    play.set_loading().unwrap();

    loop {
        // Load and setup world
        let design = design::load_design(&conf.sim.design_id);
        let mut sim = Simulation::new(design, &conf.sim, &mut rng);
        println!("{:?} tenants", sim.tenants.len());

        let mut speedup = false;
        let mut history = Vec::new();
        let mut pb = ProgressBar::new(steps as u64);

        if !debug {
            // Setup tenants for players to choose
            play.gen_player_tenant_pool(&sim.tenants);
            play.set_ready().unwrap();
            println!("Ready");
        }

        for step in 0..steps {
            if debug || speedup || play.all_players_ready() {
                if !debug {
                    play.sync_step(step).unwrap();
                }

                // TODO commands
                sim.step(step, &mut rng, &conf.sim);

                // Fast forwarding into the future
                if !debug {
                    if step > conf.play.turn_limit && !speedup {
                        println!("Fast forwarding...");
                        play.set_fast_forward().unwrap();
                        play.release_player_tenants(&mut sim.tenants);
                        speedup = true;
                    }

                    sync::sync(step, &sim.city).unwrap();
                    play.sync_players(&sim.tenants, &sim.city).unwrap();
                    play.reset_ready_players().unwrap();

                    if !speedup {
                        play.wait_turn(conf.play.min_step_delay);
                    }
                } else {
                    history.push(stats::stats(&sim));
                }

                pb.inc();
            }
        }
        // End of run

        if debug {
            save_run_data(&sim, &history, &conf);

            // If debug, run only once
            break;
        } else {
            // Wait between runs
            play.set_finished().unwrap();
            play.wait(conf.play.pause_between_runs);
        }
    }
}
