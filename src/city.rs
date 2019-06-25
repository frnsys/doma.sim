use super::grid::{HexGrid, Position};
use super::agent::{AgentType};
use strum_macros::{EnumString};
use std::collections::HashMap;

#[derive(PartialEq, Debug, EnumString)]
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
    pub units: Vec<Unit>
}


impl City {
    pub fn new(rows: usize, cols: usize) -> City {
        City {
            grid: HexGrid::new(rows, cols),
            units: Vec::new(),
            parcels: HashMap::new(),
            buildings: HashMap::new()
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
    pub tenants: Vec<usize>,
    pub months_vacant: usize,
    pub lease_month: usize,
    pub owner: (AgentType, usize),
    pub pos: Position
}

impl Unit {
    pub fn vacancies(&self) -> usize {
        self.occupancy - self.tenants.len()
    }
}


#[derive(Debug)]
pub struct Building {
    pub units: Vec<usize>,
    pub n_commercial: usize
}
