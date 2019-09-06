use rand::Rng;
use std::cmp::{max, min};
use std::str::FromStr;
use super::design::{Design, Neighborhood};
use super::grid::{HexGrid, Position};
use super::agent::{AgentType};
use strum_macros::{EnumString, Display};
use fnv::{FnvHashMap, FnvHashSet};
use noise::{OpenSimplex, Seedable};
use rand::rngs::StdRng;
use rand_distr::{Beta, Distribution};

pub struct PositionVector<T: Clone> {
    dims: (isize, isize),
    data: Vec<Option<T>>
}

impl<T: Clone> PositionVector<T> {
    pub fn new(dims: (usize, usize)) -> PositionVector<T> {
        PositionVector {
            dims: (dims.0 as isize, dims.1 as isize),
            data: vec![None; dims.0 * dims.1]
        }
    }

    pub fn insert(&mut self, pos: &Position, val: T) {
        let i = self.pos_to_index(pos);
        self.data[i] = Some(val);
    }

    pub fn values<'a>(&'a self) -> impl Iterator<Item=&T> + 'a {
        self.data.iter().filter_map(|v| v.as_ref())
    }

    pub fn values_mut<'a>(&'a mut self) -> impl Iterator<Item=&mut T> + 'a {
        self.data.iter_mut().filter_map(|v| v.as_mut())
    }

    pub fn iter<'a>(&'a self) -> impl Iterator<Item=(Position, &T)> + 'a {
        self.data.iter().enumerate().filter_map(move |(i, v)| {
            match v {
                Some(val) => {
                    Some((self.index_to_pos(i), val))
                },
                None => None
            }
        })
    }

    pub fn get(&self, pos: &Position) -> Option<&T> {
        let i = self.pos_to_index(pos);
        self.data[i].as_ref()
    }

    pub fn get_mut(&mut self, pos: &Position) -> Option<&mut T> {
        let i = self.pos_to_index(pos);
        self.data[i].as_mut()
    }

    fn pos_to_index(&self, pos: &Position) -> usize {
        (self.dims.1 * pos.0 + pos.1) as usize
    }

    fn index_to_pos(&self, idx: usize) -> Position {
        let i = idx as isize;
        (i/self.dims.1, i%self.dims.1)
    }
}

#[derive(Display, PartialEq, Debug, EnumString, Clone)]
pub enum ParcelType {
    Residential,
    Industrial,
    Park,
    River
}

#[derive(Debug, Clone)]
pub struct Parcel {
    pub typ: ParcelType,
    pub desirability: f32,
    pub neighborhood: Option<usize>,
    pub pos: Position
}

pub struct City {
    pub grid: HexGrid,
    pub buildings: PositionVector<Building>,
    pub parcels: PositionVector<Parcel>,
    pub units: Vec<Unit>,
    pub units_by_neighborhood: Vec<Vec<usize>>,
    pub residential_parcels_by_neighborhood: Vec<Vec<Position>>,
    pub commercial: PositionVector<usize>,
    pub neighborhoods: Vec<Neighborhood>,
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
        let mut parcels = PositionVector::new((rows, cols));
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
                                    // return None in that case
                                    let k = id as usize;
                                    if !neighb_ids.contains_key(&k) {
                                        None
                                    } else {
                                        Some(neighb_ids[&k])
                                    }
                                }
                            }
                        };
                        let pos = (r as isize, c as isize);
                        parcels.insert(&pos, parcel);
                    }
                    None => continue
                }
            }
        }

        let mut units = Vec::new();
        let mut buildings = PositionVector::new((rows, cols));
        let mut commercial = PositionVector::new((rows, cols));
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
                        let value = design.city.price_per_sqm*area*neighb.desirability;
                        let rent = value/design.city.price_to_rent_ratio/12.;
                        // println!("value: {:?}, rent: {:?}", value, rent);
                        let area_div = area/neighb.sqm_per_occupant as f32;
                        let occupancy_dist = Beta::new(area_div, 3.).unwrap();
                        let sampled_occupancy = occupancy_dist.sample(rng) * design.city.max_bedrooms as f32;
                        let occupancy = max(1,
                                            min(area_div.round() as usize, sampled_occupancy.round() as usize));
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

                    buildings.insert(&p.pos, Building {
                        units: building_units,
                        n_commercial: n_commercial as usize
                    });

                    if n_commercial > 0 {
                        commercial.insert(&p.pos, n_commercial as usize);
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
                    match buildings.get(&pos) {
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
                u.value = design.city.price_to_rent_ratio * u.rent * 12. * parcels.get(&pos).unwrap().desirability;
            }
        }

        City {
            grid: grid,
            units: units,
            parcels: parcels,
            buildings: buildings,
            commercial: commercial,
            neighborhoods: neighborhoods,
            units_by_neighborhood: units_by_neighborhood,
            residential_parcels_by_neighborhood: residential_parcels_by_neighborhood,
            neighborhood_trends: neighborhood_trends,
        }
    }

    pub fn neighborhood_for_pos(&self, pos: &Position) -> Option<&Neighborhood> {
        let parcel = self.parcels.get(&pos).unwrap();
        match parcel.neighborhood {
            Some(neighb_id) => {
                Some(&self.neighborhoods[neighb_id])
            },
            None => None
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

    pub fn is_doma(&self) -> bool {
        self.owner.0 == AgentType::DOMA
    }
}


#[derive(Debug, Clone)]
pub struct Building {
    pub units: Vec<usize>,
    pub n_commercial: usize
}
