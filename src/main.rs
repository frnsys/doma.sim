extern crate rand;
extern crate serde;
extern crate serde_json;
extern crate serde_yaml;
extern crate noise;
extern crate redis;
extern crate md5;
extern crate pbr;
extern crate chrono;

mod grid;
mod city;
mod agent;
mod sync;
mod stats;
mod design;
mod config;
use self::city::{City, Unit, Building, Parcel, ParcelType};
use self::grid::{Position};
use self::agent::{Landlord, Tenant, DOMA, AgentType};
use std::str::FromStr;
use std::collections::{HashMap, HashSet};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use std::cmp::{max};
use rand::prelude::*;
use rand::distributions::WeightedIndex;
use rand::seq::SliceRandom;
use noise::{OpenSimplex, Seedable, NoiseFn};
use pbr::ProgressBar;
use std::fs;
use std::path::Path;
use std::os::unix::fs::symlink;
use serde_json::json;
use chrono::prelude::*;


fn main() {
    let design_id = "philadelphia";
    let mut design = design::load_design(design_id);

    let conf = config::load_config();

    let rows = design.map.layout.len();
    let cols = design.map.layout[0].len();
    let mut city = City::new(rows, cols);

    let mut parcels = HashMap::new();
    for (r, row) in design.map.layout.iter().enumerate() {
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

    let mut rng: StdRng = SeedableRng::seed_from_u64(conf.seed);
    let mut units = Vec::new();
    let mut buildings = HashMap::new();
    let mut commercial = Vec::new();
    let mut commercial_weights = Vec::new();

    // Group units by neighborhood for lookup
    // and create neighborhood desirability trends
    let mut neighborhood_trends: HashMap<usize, OpenSimplex> = HashMap::new();
    for &id in design.neighborhoods.keys() {
        let mut noise = OpenSimplex::new();
        noise = noise.set_seed(rng.gen());
        neighborhood_trends.insert(id, noise);
        city.units_by_neighborhood.insert(id, Vec::new());
    }
    let mut nei_des_min = 1./0.;
    let mut nei_des_max = 0.;
    for n in design.neighborhoods.values() {
        if n.desirability > nei_des_max {
            nei_des_max = n.desirability;
        }
        if n.desirability < nei_des_min {
            nei_des_min = n.desirability;
        }
    }
    let nei_des_range = nei_des_max - nei_des_min;

    for n in design.neighborhoods.values_mut() {
        n.desirability = 1. + (n.desirability - nei_des_min)/(nei_des_range) - 0.5;
    }

    for p in parcels.values().filter(|p| p.typ == ParcelType::Residential) {
    // for p in city.parcels_of_type(ParcelType::Residential) {
        match p.neighborhood {
            Some(neighb_id) => {
                let neighb = design.neighborhoods.get(&neighb_id).unwrap();
                let mut n_units = rng.gen_range(neighb.min_units, neighb.max_units);
                let mut n_commercial = 0;

                city.residential_parcels_by_neighborhood.entry(neighb_id).or_insert(Vec::new()).push(p.pos);

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
                    let rent = (design.city.price_per_sqm*(area as f32)*neighb.desirability).round();
                    let value = (design.city.price_to_rent_ratio*(rent*12.)*neighb.desirability).round() as usize;
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
                        offers: Vec::new(),
                        months_vacant: 0,
                        lease_month: 0,
                        recently_sold: false,
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
            parks.iter().map(|&o| city.grid.distance(p.pos, o)).fold(1./0., f64::min) as f32
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
            Some(n) => design.neighborhoods.get(&n).unwrap().desirability,
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
        for &u_id in b.units.iter() {
            let u = &mut city.units[u_id];
            u.value = (design.city.price_to_rent_ratio * ((u.rent*12) as f32) * parcels[pos].desirability).round() as usize;
        }
    }

    let mut landlords: Vec<Landlord> = (0..design.city.landlords)
        .map(|i| Landlord::new(i as usize, design.neighborhoods.keys().cloned().collect())).collect();

    let income_dist = WeightedIndex::new(design.city.incomes.iter().map(|i| i.p)).unwrap();
    let work_dist = WeightedIndex::new(commercial_weights).unwrap();
    let vacancies: Vec<usize> = city.units.iter().map(|u| u.id).collect();
    // let mut tenants: Vec<Tenant> = (0..design.city.population).map(|i| {
    let mut tenants: Vec<Tenant> = (0..75000).map(|i| {
        let tenant_id = i as usize;
        let income_range = &design.city.incomes[income_dist.sample(&mut rng)];
        let income = rng.gen_range(income_range.low, income_range.high) as usize;
        let work_pos = commercial[work_dist.sample(&mut rng)];

        let mut tenant = Tenant {
            id: tenant_id,
            unit: None,
            units: Vec::new(),
            income: income,
            work: work_pos,
            last_dividend: 0
        };

        let lease_month = rng.gen_range(0, 11) as usize;
        let (best_id, best_desirability) = vacancies.iter().fold((0, 0.), |acc, &u_id| {
            let u = &city.units[u_id];
            let p = &parcels[&u.pos];
            if u.vacancies() <= 0 {
                acc
            } else {
                let desirability = tenant.desirability(u, p);
                if desirability > acc.1 {
                    (u_id, desirability)
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
        for &u_id in b.units.iter() {
            let u = &mut city.units[u_id];
            let roll: f32 = rng.gen();
            u.owner = if !u.vacant() {
                if roll < 0.33 {
                    let landlord = landlords.choose_mut(&mut rng).unwrap();
                    landlord.units.push(u.id);
                    (AgentType::Landlord, landlord.id)
                } else if roll < 0.66 {
                    let unit_tenants: Vec<usize> = u.tenants.iter().cloned().collect();
                    let t_id = *unit_tenants.choose(&mut rng).unwrap();
                    tenants[t_id].units.push(u.id);
                    (AgentType::Tenant, t_id)
                } else {
                    let tenant = tenants.choose_mut(&mut rng).unwrap();
                    tenant.units.push(u.id);
                    (AgentType::Tenant, tenant.id)
                }
            } else {
                if roll < 0.5 {
                    let landlord = landlords.choose_mut(&mut rng).unwrap();
                    landlord.units.push(u.id);
                    (AgentType::Landlord, landlord.id)
                } else {
                    let tenant = tenants.choose_mut(&mut rng).unwrap();
                    tenant.units.push(u.id);
                    (AgentType::Tenant, tenant.id)
                }
            };
        }
    }
    city.parcels = parcels;

    let mut doma = DOMA::new(conf.doma_starting_funds, conf.doma_p_rent_share,
                             conf.doma_p_reserves, conf.doma_p_expenses);
    let synchronize = false;

    println!("{:?} tenants", tenants.len());

    let mut history = Vec::new();
    let mut pb = ProgressBar::new(conf.steps as u64);
    for step in 0..conf.steps {
        let mut transfers = Vec::new();
        for tenant in &mut tenants {
            transfers.extend(tenant.check_purchase_offers(&mut city, design.city.price_to_rent_ratio));
        }
        for landlord in &mut landlords {
            transfers.extend(landlord.check_purchase_offers(&mut city, design.city.price_to_rent_ratio));
        }
        for (landlord_typ, landlord_id, unit_id, amount) in transfers {
            match landlord_typ {
                AgentType::Landlord => {
                    let landlord = &mut landlords[landlord_id];
                    landlord.units.push(unit_id);
                },
                AgentType::DOMA => {
                    doma.units.push(unit_id);
                    // println!("{:?}/{:?}", amount, doma.funds);
                    doma.funds -= amount as i32;
                },
                _ => {}
            }
        }

        for landlord in &mut landlords {
            landlord.step(&mut city, step, design.city.price_to_rent_ratio, &mut rng, &conf);
        }

        let mut vacant_units: Vec<usize> = city.units.iter().filter(|u| u.vacancies() > 0).map(|u| u.id).collect();
        for tenant in &mut tenants {
            tenant.step(&mut city, step, &mut vacant_units, &mut rng, &conf);
        }

        if step % 12 == 0 {
            // Appraise
            for (_, unit_ids) in &city.units_by_neighborhood {
                let units: Vec<&Unit> = unit_ids.iter().map(|&u_id| &city.units[u_id]).collect();
                let sold: Vec<&Unit> = units.iter().filter(|u| u.recently_sold).cloned().collect();
                let mean_value_per_area = if sold.len() == 0 {
                    units.iter().fold(0., |acc, u| {
                        acc + (u.value_per_area() * conf.base_appreciation)
                    })/units.len() as f32
                } else {
                    sold.iter().fold(0., |acc, u| {
                        acc + (u.value_per_area() * conf.base_appreciation)
                    })/sold.len() as f32
                };

                for &u_id in unit_ids {
                    let mut unit = &mut city.units[u_id];
                    if !unit.recently_sold {
                        unit.value = (mean_value_per_area * unit.area as f32).round() as usize;
                    }
                    unit.recently_sold = false;
                }
            }
        }

        doma.step(&mut city, &mut tenants, &mut rng);

        // Desirability changes, random walk
        for (neighb_id, parcel_ids) in &city.residential_parcels_by_neighborhood {
            let last_val = if step > 0 {
                neighborhood_trends[neighb_id].get([(step - 1) as f64/conf.desirability_stretch_factor, 0.])
            } else {
                0.
            };
            let val = neighborhood_trends[neighb_id].get([step as f64/conf.desirability_stretch_factor, 0.]);
            let change = (val - last_val) as f32;
            for p in parcel_ids {
                let parcel = city.parcels.get_mut(p).unwrap();
                parcel.desirability = f32::max(0., parcel.desirability - change);
            }
        }

        if synchronize {
            sync::sync(step, &city).unwrap();
        }
        if conf.debug {
            history.push(stats::stats(&city, &tenants, &landlords, &doma));
        }
        pb.inc();
    }

    if conf.debug {
        // Save run data
        let now: DateTime<Utc> = Utc::now();
        let now_str = now.format("%Y.%m.%d.%H.%M").to_string();
        let results = json!({
            "history": history,
            "meta": {
                "seed": conf.seed,
                "design": design_id,
                "tenants": tenants.len(),
                "units": city.units.len(),
                "occupancy": city.units.iter().fold(0, |acc, u| acc + u.occupancy)
            }
        }).to_string();

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
