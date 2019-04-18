use std::{
    sync::atomic::{AtomicUsize, Ordering},
};
use rand::seq::IteratorRandom;
use super::agent::{Owner, Offer};
use std::cell::RefCell;
use std::rc::Rc;

static OBJECT_COUNTER: AtomicUsize = AtomicUsize::new(0);

pub enum ParcelType {
    Residential,
    Park,
    River
}

pub struct Parcel {
    typ: ParcelType,
    desirability: usize,
    building: Option<Building>
}

pub struct Unit {
    id: usize,
    rent: usize,
    occupancy: usize,
    area: usize,
    tenants: usize,
    months_vacant: usize,
    owner: Option<Box<Owner>>,
    offers: Vec<Offer>,
    pub lease_month: usize
}

pub type UUnit = Rc<RefCell<Unit>>;

impl Unit {
    pub fn new(rent: usize, occupancy: usize, area: usize) -> Unit {
        Unit {
            id: OBJECT_COUNTER.fetch_add(1, Ordering::SeqCst),
            rent: rent,
            occupancy: occupancy,
            area: area,
            tenants: 0,
            months_vacant: 0,
            lease_month: 0,
            owner: None,
            offers: Vec::new()
        }
    }

    pub fn move_in(&mut self, time: usize) {
        if self.tenants == 0 {
            self.lease_month = time;
            self.months_vacant = 0;
        }

        self.tenants += 1;
    }

    pub fn move_out(&mut self) {
        self.tenants -= 1;
    }

    pub fn vacant(&self) -> bool {
        self.tenants == 0
    }

    pub fn vacancies(&self) -> usize {
        self.occupancy - self.tenants
    }

    pub fn rent_per_area(&self) -> usize {
        self.rent/self.area
    }

    pub fn rent_per_tenant(&self) -> f32 {
        // Numer of tenants, were someone to move in
        let n_tenants = self.tenants + 1;
        (self.rent as f32)/(n_tenants as f32)
    }

    pub fn area_per_tenant(&self) -> f32 {
        // Numer of tenants, were someone to move in
        let n_tenants = self.tenants + 1;
        (self.area as f32)/(n_tenants as f32)
    }

    pub fn base_desirability(&self) -> f32 {
        // TODO
        // self.building.parcel.desirability
        1.
    }
}

pub struct Building {
    id: usize,
    units: Vec<UUnit>,
    n_commercial: usize
}

impl Building {
    pub fn units_with_vacancies(&self) -> Vec<&UUnit> {
        self.units.iter().filter(|u| u.borrow().vacancies() > 0).collect()
    }
}

pub struct City {
    buildings: Vec<Building>
}

impl City {
    pub fn units_with_vacancies(&self) -> Vec<&UUnit> {
        self.buildings.iter().flat_map(|b| b.units_with_vacancies()).collect()
    }

    pub fn sample_units_with_vacancies(&self, n: usize) -> Vec<&UUnit> {
        let mut rng = &mut rand::thread_rng();
        self.units_with_vacancies().into_iter().choose_multiple(&mut rng, n)
    }
}
