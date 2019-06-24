extern crate rand;
extern crate serde;
extern crate serde_json;

mod grid;
mod city;
mod agent;
use self::city::{City, Unit, Building, Parcel, ParcelType};
use self::grid::{Position};
use std::io::BufReader;
use std::fs::File;
use serde::{Deserialize};
use std::str::FromStr;
use std::collections::HashMap;
use rand::Rng;
use std::cmp::{max};

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct Neighborhood {
    desirability: f32,
    min_units: u32,
    max_units: u32,
    min_area: u32,
    max_area: u32,
    sqm_per_occupant: u32,
    p_commercial: f32
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct CityConfig {
    price_per_sqm: f32,
    price_to_rent_ratio: f32
}

#[derive(Deserialize, Debug)]
struct Map {
    layout: Vec<Vec<Option<String>>>,
    neighborhoods: HashMap<usize, Neighborhood>,
    city: CityConfig
}

// TODO
// move into city implementation
// grid should only be for managing positions, does not hold any cell data
// keep parcels in a hashmap<pos, parcel>
// that way we can access grid functions while mutating parcels
// iterate over parcels with hashmap.values

fn main() {
    let file = File::open("testmap.json").expect("could not open file");
    let reader = BufReader::new(file);

    // Read the JSON contents of the file as an instance of `User`.
    let map: Map = serde_json::from_reader(reader).expect("error while reading json");
    // println!("{:#?}", u);

    let rows = map.layout.len();
    let cols = map.layout[0].len();
    let mut city = City::new(rows, cols);

    println!("{:?}", map.neighborhoods);

    for (r, row) in map.layout.iter().enumerate() {
        for (c, cell) in row.iter().enumerate() {
            match cell {
                Some(parcel_str) => {
                    let parts: Vec<&str> = parcel_str.split("|").collect();
                    let neighb_id: i32 = parts[0].parse().unwrap();
                    let parcel_type = ParcelType::from_str(parts[1]).unwrap();
                    let parcel = Parcel {
                        pos: (r as isize, c as isize),
                        typ: parcel_type,
                        desirability: 0.,
                        neighborhood: match neighb_id {
                            -1 => None,
                            id => Some(id as usize)
                        }
                    };
                    // println!("{:?}", parcel);
                    city.grid.set_cell((r as isize, c as isize), parcel);
                }
                None => continue
            }
        }
    }

    let mut rng = rand::thread_rng();
    let mut units = Vec::new();
    let mut buildings = HashMap::new();
    for p in city.parcels_of_type(ParcelType::Residential) {
        match p.neighborhood {
            Some(neighb_id) => {
                let neighb = map.neighborhoods.get(&neighb_id).unwrap();
                let mut n_units = rng.gen_range(neighb.min_units, neighb.max_units);
                let mut n_commercial = 0;

                // Houses have no commercial floors
                // Need to keep these divisible by 4 for towers
                if n_units > 3 {
                    if n_units % 4 != 0 {
                        n_units += 4 - (n_units % 4 as u32);
                    }

                    let n_floors = (n_units as f32)/4.;
                    let total_floors = (n_floors/(1.-neighb.p_commercial)).ceil();
                    n_commercial = (total_floors - n_floors) as u32;
                }

                let mut building_units: Vec<usize> = Vec::new();
                for _ in 0..n_units {
                    let area = rng.gen_range(neighb.min_area, neighb.max_area) as usize;
                    let rent = (map.city.price_per_sqm*(area as f32)*neighb.desirability).round();
                    let value = (map.city.price_to_rent_ratio*(rent*12.)*neighb.desirability).round() as usize;
                    let occupancy = max(1, ((area as f32)/(neighb.sqm_per_occupant as f32)).round() as usize);
                    let id = units.len();
                    let unit = Unit {
                        id: id,
                        rent: rent as usize,
                        occupancy: occupancy,
                        area: area,
                        value: value,
                        tenants: Vec::new(),
                        months_vacant: 0,
                        lease_month: 0,
                    };
                    units.push(unit);
                    building_units.push(id);
                }

                buildings.insert(p.pos, Building {
                    units: building_units,
                    n_commercial: n_commercial as usize
                });
            },
            None => continue
        }
    }
    city.units = units;
    city.buildings = buildings;

    // let mut total = 0.;
    // let mut count = 0;
    let parks: Vec<Position> = city.parcels_of_type(ParcelType::Park).into_iter().map(|p| p.pos).collect();

    for p in city.mut_parcels_of_type(ParcelType::Residential) {
        let park_dist = if parks.len() > 0 {
            parks.iter().map(|o| city.grid.distance(p.pos, *o)).fold(1./0., f64::min) as f32
        } else {
            1.
        };

        // Nearby commercial density
        let n_commercial = city.grid.radius(p.pos, 2).iter()
            .map(|pos| {
                match city.buildings.get(pos) {
                    Some(b) => b.n_commercial,
                    _ => 0
                }
            }).fold(0, |acc, item| acc + item);

        let neighb = match p.neighborhood {
            Some(n) => map.neighborhoods.get(&n).unwrap().desirability,
            _ => 0.
        };
        p.desirability = (1./park_dist * 10.) + neighb + (n_commercial as f32)/10.;
        // total += p.desirability;
        // count += 1;
    }
    // println!("{:?}", city.grid.get_cell((0, 0)));
    // println!("Hello, world!");
}
