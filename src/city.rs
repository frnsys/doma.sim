use super::grid::{HexGrid, Position};
use super::agent::{AgentType};
use strum_macros::{EnumString, Display};
use std::collections::{HashMap, HashSet};

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
    pub buildings: HashMap<Position, Building>,
    pub parcels: HashMap<Position, Parcel>,
    pub units: Vec<Unit>,
    pub units_by_neighborhood: HashMap<usize, Vec<usize>>,
    pub residential_parcels_by_neighborhood: HashMap<usize, Vec<Position>>,
}


impl City {
    pub fn new(rows: usize, cols: usize) -> City {
        City {
            grid: HexGrid::new(rows, cols),
            units: Vec::new(),
            parcels: HashMap::new(),
            buildings: HashMap::new(),
            units_by_neighborhood: HashMap::new(),
            residential_parcels_by_neighborhood: HashMap::new()
        }
    }

    pub fn parcels_of_type(&self, typ: ParcelType) -> Vec<&Parcel> {
        self.parcels.values().filter(|p| p.typ == typ).collect()
    }

    pub fn mut_parcels_of_type(&mut self, typ: ParcelType) -> Vec<&mut Parcel> {
        self.parcels.values_mut().filter(|p| p.typ == typ).collect()
    }
}

pub struct Unit {
    pub id: usize,
    pub rent: usize,
    pub occupancy: usize,
    pub condition: f32,
    pub area: usize,
    pub value: usize,
    pub tenants: HashSet<usize>,
    pub months_vacant: usize,
    pub lease_month: usize,
    pub owner: (AgentType, usize),
    pub pos: Position,
    pub offers: Vec<(AgentType, usize, usize)> // landlord type, landlord id, offer amount
}

impl Unit {
    pub fn vacant(&self) -> bool {
        self.tenants.len() == 0
    }

    pub fn vacancies(&self) -> usize {
        self.occupancy - self.tenants.len()
    }

    pub fn rent_per_area(&self) -> f32 {
        self.rent as f32/self.area as f32
    }
}


#[derive(Debug)]
pub struct Building {
    pub units: Vec<usize>,
    pub n_commercial: usize
}
