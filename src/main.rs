extern crate rand;
extern crate serde;
extern crate serde_json;
extern crate noise;

mod grid;
mod city;
mod agent;
use self::city::{City, Unit, Building, Parcel, ParcelType};
use self::grid::{Position};
use self::agent::{Landlord, Tenant, AgentType};
use std::io::BufReader;
use std::fs::File;
use serde::{Deserialize};
use std::str::FromStr;
use std::collections::{HashMap, HashSet};
use rand::Rng;
use std::cmp::{max};
use rand::prelude::*;
use rand::distributions::WeightedIndex;
use rand::seq::SliceRandom;
use noise::{OpenSimplex};

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
struct IncomeRange {
    high: usize,
    low: usize,
    p: f32
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct CityConfig {
    price_per_sqm: f32,
    price_to_rent_ratio: f32,
    landlords: u32,
    population: u32,
    incomes: Vec<IncomeRange>
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

    let mut parcels = HashMap::new();
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
                    parcels.insert((r as isize, c as isize), parcel);
                }
                None => continue
            }
        }
    }

    let mut rng = rand::thread_rng();
    let mut units = Vec::new();
    let mut buildings = HashMap::new();
    let mut commercial = Vec::new();
    let mut commercial_weights = Vec::new();

    // Group units by neighborhood for lookup
    // and create neighborhood desirability trends
    let mut neighborhood_trends: HashMap<usize, OpenSimplex> = HashMap::new();
    for id in map.neighborhoods.keys() {
        neighborhood_trends.insert(*id, OpenSimplex::new());
        city.units_by_neighborhood.insert(*id, Vec::new());
    }

    for p in parcels.values().filter(|p| p.typ == ParcelType::Residential) {
    // for p in city.parcels_of_type(ParcelType::Residential) {
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
                        pos: p.pos,
                        rent: rent as usize,
                        occupancy: occupancy,
                        area: area,
                        value: value,
                        condition: 1.0,
                        tenants: HashSet::new(),
                        months_vacant: 0,
                        lease_month: 0,
                        owner: (AgentType::Landlord, 0) // Dummy placeholder
                    };
                    city.units_by_neighborhood.get_mut(&neighb_id).unwrap().push(id);
                    units.push(unit);
                    building_units.push(id);
                }

                buildings.insert(p.pos, Building {
                    units: building_units,
                    n_commercial: n_commercial as usize
                });

                if n_commercial > 0 {
                    commercial.push(p.pos);
                    commercial_weights.push(n_commercial);
                }
            },
            None => continue
        }
    }
    city.units = units;
    city.buildings = buildings;

    let mut total = 0.;
    let mut count = 0;
    // let parks: Vec<Position> = city.parcels_of_type(ParcelType::Park).into_iter().map(|p| p.pos).collect();
    let parks: Vec<Position> = parcels.values().filter(|p| p.typ == ParcelType::Park).into_iter().map(|p| p.pos).collect();

    // for p in city.mut_parcels_of_type(ParcelType::Residential) {
    for p in parcels.values_mut().filter(|p| p.typ == ParcelType::Residential) {
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
        total += p.desirability;
        count += 1;
    }

    // Update weighted parcel desirabilities
    let mean_desirability = total/count as f32;
    for p in parcels.values_mut().filter(|p| p.typ == ParcelType::Residential) {
        p.desirability /= mean_desirability;
    }

    // Update unit values
    for (pos, b) in city.buildings.iter() {
        for u_id in b.units.iter() {
            let u = &mut city.units[*u_id];
            u.value = (map.city.price_to_rent_ratio * ((u.rent*12) as f32) *parcels[pos].desirability).round() as usize;
        }
    }

    let mut landlords: Vec<Landlord> = (0..map.city.landlords)
        .map(|i| Landlord::new(i as usize, map.neighborhoods.keys().cloned().collect())).collect();

    let income_dist = WeightedIndex::new(map.city.incomes.iter().map(|i| i.p)).unwrap();
    let work_dist = WeightedIndex::new(commercial_weights).unwrap();
    let vacancies: Vec<usize> = city.units.iter().map(|u| u.id).collect();
    let mut tenants: Vec<Tenant> = (0..map.city.population).map(|i| {
        let tenant_id = i as usize;
        let income_range = &map.city.incomes[income_dist.sample(&mut rng)];
        let income = rng.gen_range(income_range.low, income_range.high) as usize;
        let work_pos = commercial[work_dist.sample(&mut rng)];

        let mut tenant = Tenant {
            id: tenant_id,
            unit: None,
            units: Vec::new(),
            income: income,
            work: work_pos
        };

        let lease_month = rng.gen_range(0, 11) as usize;
        let (best_id, best_desirability) = vacancies.iter().fold((0, 0.), |acc, u_id| {
            let u = &city.units[*u_id];
            let p = &parcels[&u.pos];
            if u.vacancies() <= 0 {
                acc
            } else {
                let desirability = tenant.desirability(u, p);
                if desirability > acc.1 {
                    (*u_id, desirability)
                } else {
                    acc
                }
            }
        });
        tenant.unit = if best_desirability > 0. {
            let u = &mut city.units[best_id];
            u.tenants.insert(tenant_id);
            u.lease_month = lease_month;
            Some(best_id)
        } else {
            None
        };

        tenant
    }).collect();

    // Distribute ownership of units
    for (_, b) in city.buildings.iter() {
        for u_id in b.units.iter() {
            let u = &mut city.units[*u_id];
            let roll: f32 = rng.gen();
            u.owner = if u.tenants.len() > 0 {
                if roll < 0.33 {
                    (AgentType::Landlord, landlords.choose(&mut rng).unwrap().id)
                } else if roll < 0.66 {
                    let unit_tenants: Vec<usize> = u.tenants.iter().cloned().collect();
                    (AgentType::Tenant, *unit_tenants.choose(&mut rng).unwrap())
                } else {
                    (AgentType::Tenant, tenants.choose(&mut rng).unwrap().id)
                }
            } else {
                if roll < 0.5 {
                    (AgentType::Landlord, landlords.choose(&mut rng).unwrap().id)
                } else {
                    (AgentType::Tenant, tenants.choose(&mut rng).unwrap().id)
                }
            };
        }
    }
    city.parcels = parcels;

    println!("{:?} tenants", tenants.len());

    for step in 0..100 {
        for landlord in &mut landlords {
            landlord.step(&mut city, step);
        }

        let mut vacant_units: Vec<usize> = city.units.iter().filter(|u| u.vacancies() > 0).map(|u| u.id).collect();
        println!("{:?} vacants", vacant_units.len());
        for tenant in &mut tenants {
            tenant.step(&mut city, step, &mut vacant_units);
        }
    }

    println!("Done");
}
