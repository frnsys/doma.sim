use redis::Commands;
use serde::{Deserialize};
use std::collections::HashMap;

#[derive(Deserialize, Debug)]
pub struct Design {
    pub map: Map,
    pub neighborhoods: HashMap<usize, Neighborhood>,
    pub city: CityConfig
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Neighborhood {
    pub desirability: f32,
    pub min_units: u32,
    pub max_units: u32,
    pub min_area: u32,
    pub max_area: u32,
    pub sqm_per_occupant: u32,
    pub p_commercial: f32
}

#[derive(Deserialize, Debug)]
pub struct IncomeRange {
    pub high: usize,
    pub low: usize,
    pub p: f32
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CityConfig {
    pub price_per_sqm: f32,
    pub price_to_rent_ratio: f32,
    pub landlords: u32,
    pub population: u32,
    pub incomes: Vec<IncomeRange>
}

#[derive(Deserialize, Debug)]
pub struct Map {
    pub layout: Vec<Vec<Option<String>>>
}


pub fn load_design(design_id: &str) -> Design {
    let client = redis::Client::open("redis://127.0.0.1/1").unwrap();
    let con = client.get_connection().unwrap();
    let design_key = format!("design:{}", design_id);
    let design_data: String = con.get(design_key).expect("no design for that id");
    let design: Design = serde_json::from_str(&design_data).expect("error while reading json");
    design
}
