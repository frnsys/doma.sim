use rand::Rng;
use std::cmp::{max};
use std::str::FromStr;
use super::design::{Design, Neighborhood};
use super::grid::{HexGrid, Position};
use super::agent::{AgentType};
use strum_macros::{EnumString, Display};
use fnv::{FnvHashMap, FnvHashSet};
use noise::{OpenSimplex, Seedable};
use rand::rngs::StdRng;

#[derive(Display, PartialEq, Debug, EnumString)]
pub enum ParcelType {
    Residential,
    Industrial,
    Park,
    River
}

#[derive(Debug)]
pub struct Parcel {
    pub typ: ParcelType,
    pub desirability: f32,
    pub neighborhood: Option<usize>,
    pub pos: Position
}

pub struct City {
    pub grid: HexGrid,
    pub buildings: FnvHashMap<Position, Building>,
    pub parcels: FnvHashMap<Position, Parcel>,
    pub units: Vec<Unit>,
    pub units_by_neighborhood: Vec<Vec<usize>>,
    pub residential_parcels_by_neighborhood: Vec<Vec<Position>>,
    pub commercial: FnvHashMap<Position, usize>,
    pub neighborhood_trends: Vec<OpenSimplex>
}


impl City {
    pub fn new(design: &Design, rng: &mut StdRng) -> City {
        let rows = design.map.layout.len();
        let cols = design.map.layout[0].len();
        let grid = HexGrid::new(rows, cols);

        // Re-id neighborhoods so they are incremental values
        let mut neighborhoods: Vec<Neighborhood> = Vec::new();
        let mut neighb_ids: FnvHashMap<usize, usize> = FnvHashMap::default();
        for (i, (&id, neighb)) in design.neighborhoods.iter().enumerate() {
            neighb_ids.insert(id, i);
            neighborhoods.push(neighb.clone());
        }


        // Initialize parcels
        let mut parcels = FnvHashMap::default();
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
                                id => {
                                    // Sometimes parcels have neighborhood ids
                                    // which have no specification in the design,
                                    // so just check and add a new id if necessary
                                    let k = id as usize;
                                    if !neighb_ids.contains_key(&k) {
                                        neighb_ids.insert(k, neighb_ids.keys().len());
                                    }
                                    Some(neighb_ids[&k])
                                }
                            }
                        };
                        parcels.insert((r as isize, c as isize), parcel);
                    }
                    None => continue
                }
            }
        }

        let mut units = Vec::new();
        let mut buildings = FnvHashMap::default();
        let mut commercial = FnvHashMap::default();
        let mut units_by_neighborhood = Vec::new();
        let mut residential_parcels_by_neighborhood = Vec::new();

        // Group units by neighborhood for lookup
        // and create neighborhood desirability trends
        let mut neighborhood_trends = Vec::new();
        for _ in neighb_ids.values() {
            let mut noise = OpenSimplex::new();
            noise = noise.set_seed(rng.gen());
            neighborhood_trends.push(noise);
            units_by_neighborhood.push(Vec::new());
            residential_parcels_by_neighborhood.push(Vec::new());
        }

        // Adjust neighborhood desirabilities
        // to be in a fixed range (0.5-1.5)
        let mut nei_des_min = 1./0.;
        let mut nei_des_max = 0.;
        for n in &neighborhoods {
            if n.desirability > nei_des_max {
                nei_des_max = n.desirability;
            }
            if n.desirability < nei_des_min {
                nei_des_min = n.desirability;
            }
        }
        let nei_des_range = nei_des_max - nei_des_min;
        for n in &mut neighborhoods {
            n.desirability = 1. + (n.desirability - nei_des_min)/(nei_des_range) - 0.5;
        }

        // Prepare buildings and units
        for p in parcels.values().filter(|p| p.typ == ParcelType::Residential) {
            match p.neighborhood {
                Some(neighb_id) => {
                    let neighb = &neighborhoods[neighb_id];
                    let mut n_units = rng.gen_range(neighb.min_units, neighb.max_units);
                    let mut n_commercial = 0;

                    residential_parcels_by_neighborhood[neighb_id].push(p.pos);

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
                        let area = rng.gen_range(neighb.min_area, neighb.max_area) as f32;
                        let rent = design.city.price_per_sqm*area*neighb.desirability;
                        let value = design.city.price_to_rent_ratio*(rent*12.)*neighb.desirability;
                        let occupancy = max(1, (area/neighb.sqm_per_occupant as f32).round() as usize);
                        let id = units.len();
                        let unit = Unit {
                            id: id,
                            pos: p.pos,
                            rent: rent,
                            occupancy: occupancy,
                            area: area,
                            value: value,
                            condition: 1.0,
                            tenants: FnvHashSet::default(),
                            offers: Vec::new(),
                            months_vacant: 0,
                            lease_month: 0,
                            recently_sold: false,
                            owner: (AgentType::Landlord, 0) // Dummy placeholder
                        };
                        units_by_neighborhood[neighb_id].push(id);
                        units.push(unit);
                        building_units.push(id);
                    }

                    buildings.insert(p.pos, Building {
                        units: building_units,
                        n_commercial: n_commercial as usize
                    });

                    if n_commercial > 0 {
                        commercial.insert(p.pos, n_commercial as usize);
                    }
                },
                None => continue
            }
        }

        // Compute parcel desirabilities
        let mut total = 0.;
        let mut count = 0;
        let parks: Vec<Position> = parcels.values().filter(|p| p.typ == ParcelType::Park).into_iter().map(|p| p.pos).collect();
        for p in parcels.values_mut().filter(|p| p.typ == ParcelType::Residential) {
            let park_dist = if parks.len() > 0 {
                parks.iter().map(|&o| grid.distance(p.pos, o)).fold(1./0., f32::min)
            } else {
                1.
            };

            // Nearby commercial density
            let n_commercial = grid.radius(p.pos, 2).iter()
                .map(|pos| {
                    match buildings.get(pos) {
                        Some(b) => b.n_commercial,
                        _ => 0
                    }
                }).fold(0, |acc, item| acc + item);

            let neighb = match p.neighborhood {
                Some(n) => neighborhoods[n].desirability,
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
        for (pos, b) in buildings.iter() {
            for &u_id in b.units.iter() {
                let u = &mut units[u_id];
                u.value = design.city.price_to_rent_ratio * u.rent * 12. * parcels[pos].desirability;
            }
        }

        City {
            grid: grid,
            units: units,
            parcels: parcels,
            buildings: buildings,
            commercial: commercial,
            units_by_neighborhood: units_by_neighborhood,
            residential_parcels_by_neighborhood: residential_parcels_by_neighborhood,
            neighborhood_trends: neighborhood_trends
        }
    }
}

pub struct Unit {
    pub id: usize,
    pub rent: f32,
    pub occupancy: usize,
    pub condition: f32,
    pub area: f32,
    pub value: f32,
    pub tenants: FnvHashSet<usize>,
    pub months_vacant: usize,
    pub lease_month: usize,
    pub owner: (AgentType, usize),
    pub pos: Position,
    pub recently_sold: bool,
    pub offers: Vec<(AgentType, usize, f32)> // landlord type, landlord id, offer amount
}

impl Unit {
    pub fn vacant(&self) -> bool {
        self.tenants.len() == 0
    }

    pub fn vacancies(&self) -> usize {
        self.occupancy - self.tenants.len()
    }

    pub fn rent_per_area(&self) -> f32 {
        self.rent/self.area
    }

    pub fn value_per_area(&self) -> f32 {
        self.value/self.area
    }
}


#[derive(Debug)]
pub struct Building {
    pub units: Vec<usize>,
    pub n_commercial: usize
}
