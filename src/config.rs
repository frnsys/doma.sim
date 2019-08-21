use rand::Rng;
use serde::Deserialize;
use std::env;
use std::fs::File;
use std::io::BufReader;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub struct Config {
    pub sim: SimConfig,
    pub play: PlayConfig,

    #[serde(default)]
    pub steps: usize,

    #[serde(default)]
    pub debug: bool,

    #[serde(default)]
    pub seed: u64,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub struct SimConfig {
    pub design_id: String,
    pub doma_starting_funds: f32,
    pub doma_p_rent_share: f32,
    pub doma_p_reserves: f32,
    pub doma_p_expenses: f32,
    pub desirability_stretch_factor: f64,
    pub base_appreciation: f32,
    pub sample_size: usize,
    pub tenant_sample_size: usize,
    pub trend_months: usize,
    pub rent_increase_rate: f32,
    pub moving_penalty: f32,
    pub friend_limit: usize,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "UPPERCASE")]
pub struct PlayConfig {
    pub burn_in: usize,
    pub turn_sequence: Vec<usize>,
    pub min_step_delay: u64,
    pub pause_between_runs: u64,
}


pub fn load_config() -> Config {
    let file = File::open("config.yaml").expect("could not open file");
    let reader = BufReader::new(file);
    let mut conf: Config = serde_yaml::from_reader(reader).expect("error while reading yaml");

    conf.steps = match env::var("STEPS") {
        Ok(steps) => steps.parse().unwrap(),
        Err(_) => 100,
    };

    conf.debug = match env::var("DEBUG") {
        Ok(debug) => debug == "1",
        Err(_) => false,
    };

    let mut rng = rand::thread_rng();
    conf.seed = match env::var("SEED") {
        Ok(seed) => seed.parse().unwrap(),
        Err(_) => rng.gen(),
    };

    println!("{:?}", conf);

    conf
}
