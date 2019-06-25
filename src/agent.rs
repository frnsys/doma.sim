use super::grid::{Position};
use super::city::{Unit, Parcel};
use std::cmp::{max};

static MIN_AREA: f32 = 50.;

fn distance(a: Position, b: Position) -> f64 {
    (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)) as f64).sqrt()
}


#[derive(Debug)]
pub enum AgentType {
    Tenant,
    Landlord
}

#[derive(Debug)]
pub struct Tenant {
    pub id: usize,
    pub income: usize,
    pub unit: Option<usize>,
    pub work: Position,
    pub units: Vec<usize>
}

impl Tenant {
    pub fn desirability(&self, unit: &Unit, parcel: &Parcel) -> f32 {
        // TODO
        // If DOMA is the unit owner,
        // compute rent adjusted for dividends
        // let rent = unit.adjusted_rent(tenants=unit.tenants|set([self]))
        let rent = unit.rent;
        let n_tenants = unit.tenants.len() + 1;
        let rent_per_tenant = max(1, rent/n_tenants);
        if self.income < rent_per_tenant {
            0.
        } else {
            let ratio = (self.income as f32/rent_per_tenant as f32).sqrt();
            let spaciousness = f32::max(unit.area as f32/n_tenants as f32 - MIN_AREA, 0.).powf(1./32.);
            let commute_distance = distance(self.work, unit.pos) as f32;
            let commute: f32 = if commute_distance == 0. {
                1.
            } else {
                1./commute_distance
            };
            ratio * (spaciousness + parcel.desirability + unit.condition + commute)
        }
    }
}

#[derive(Debug)]
pub struct Landlord {
    pub id: usize,
    pub units: Vec<usize>
}
