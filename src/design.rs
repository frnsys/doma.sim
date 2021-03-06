use fnv::FnvHashMap;
use redis::Commands;
use serde::{Serialize, Deserialize};

#[derive(Deserialize, Debug)]
pub struct Design {
    pub map: Map,
    pub neighborhoods: FnvHashMap<usize, Neighborhood>,
    pub city: CityConfig,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Neighborhood {
    pub id: isize,
    pub name: String,
    pub desirability: f32,
    pub min_units: u32,
    pub max_units: u32,
    pub min_area: u32,
    pub max_area: u32,
    pub sqm_per_occupant: u32,
    pub p_commercial: f32,
    pub color: String
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct CityConfig {
    pub name: String,
    pub max_bedrooms: usize,
    pub price_per_sqm: f32,
    pub price_to_rent_ratio: f32,
    pub landlords: u32,
    pub population: u32,
    pub income_mu: f32,
    pub income_sigma: f32,
}

#[derive(Deserialize, Debug)]
pub struct Map {
    pub layout: Vec<Vec<Option<String>>>,
    pub offset: MapOffset,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct MapOffset {
    pub row: bool,
    pub col: bool,
}


pub fn load_design(design_id: &String) -> Design {
    let client = redis::Client::open("redis://127.0.0.1/1").unwrap();
    let con = client.get_connection().unwrap();
    let design_key = format!("design:{}", design_id);
    let design_data: String = con.get(design_key).expect("no design for that id");
    let design: Design = serde_json::from_str(&design_data).expect("error while reading json");
    design
}
