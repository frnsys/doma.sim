use rand::Rng;
use std::cmp::{max};
use std::str::FromStr;
use super::design::Design;
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
    pub units_by_neighborhood: FnvHashMap<usize, Vec<usize>>,
    pub residential_parcels_by_neighborhood: FnvHashMap<usize, Vec<Position>>,
    pub commercial: FnvHashMap<Position, usize>,
    pub neighborhood_trends: FnvHashMap<usize, OpenSimplex>
}


impl City {
    pub fn new(design: &mut Design, rng: &mut StdRng) -> City {
        let rows = design.map.layout.len();
        let cols = design.map.layout[0].len();
        let grid = HexGrid::new(rows, cols);

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
                                id => Some(id as usize)
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
        let mut units_by_neighborhood = FnvHashMap::default();
        let mut residential_parcels_by_neighborhood = FnvHashMap::default();

        // Group units by neighborhood for lookup
        // and create neighborhood desirability trends
        let mut neighborhood_trends = FnvHashMap::default();
        for &id in design.neighborhoods.keys() {
            let mut noise = OpenSimplex::new();
            noise = noise.set_seed(rng.gen());
            neighborhood_trends.insert(id, noise);
            units_by_neighborhood.insert(id, Vec::new());
        }

        // Adjust neighborhood desirabilities
        // to be in a fixed range (0.5-1.5)
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

        // Prepare buildings and units
        for p in parcels.values().filter(|p| p.typ == ParcelType::Residential) {
            match p.neighborhood {
                Some(neighb_id) => {
                    let neighb = design.neighborhoods.get(&neighb_id).unwrap();
                    let mut n_units = rng.gen_range(neighb.min_units, neighb.max_units);
                    let mut n_commercial = 0;

                    residential_parcels_by_neighborhood.entry(neighb_id).or_insert(Vec::new()).push(p.pos);

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
                        units_by_neighborhood.get_mut(&neighb_id).unwrap().push(id);
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
