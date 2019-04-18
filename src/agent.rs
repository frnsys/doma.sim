use std::rc::Rc;
use std::cell::Ref;
use super::city::{City, Unit, UUnit};

pub struct Offer {
    landlord: Landlord,
    unit: Unit,
    amount: usize,
    accepted: Option<bool>
}

pub trait Owner {
    fn units(&self) -> Vec<Unit>;
    fn add_unit(&self, unit: Unit);
    fn remove_unit(&self, unit: Unit);
}

pub struct Landlord {
    id: usize,
    units: Vec<Unit>,
    out_offers: Vec<Offer>
}

pub struct Tenant {
    id: usize,
    income: usize,
    unit: Option<UUnit>,
    units: Vec<UUnit>
}

impl Tenant {
    // Compute desirability of a housing unit
    // for this tenant
    fn desirability(&self, unit: Ref<Unit>) -> f32 {
        let rent_per_tenant = unit.rent_per_tenant();
        let income = self.income as f32;

        // Is this place affordable?
        if income < rent_per_tenant {
            return 0.
        }

        // Ratio of income to rent they'd pay
        let ratio = income/rent_per_tenant;

        // Space per tenant
        let spaciousness = unit.area_per_tenant();

        ratio * (spaciousness + unit.base_desirability())
    }

    pub fn step(&mut self, time: usize, city: &mut City) {
        let sample_size = 20;

        // Reconsider current unit?
        let reconsider;
        let current_desirability;
        let moving_penalty;

        // If currently w/o home,
        // will always look for a place to move into,
        // with no moving penalty
        if self.unit.is_none() {
            reconsider = true;
            current_desirability = -1.;
            moving_penalty = 0.;

        // Otherwise, only consider moving
        // between leases or if their current
        // place is no longer affordable
        // TODO the latter doesn't happen b/c
        // tenant income doesn't change, and
        // rents only change b/w leases
        } else {
            let unit = self.unit.as_ref().unwrap().borrow();
            let elapsed = time - unit.lease_month;
            reconsider = elapsed > 0 && elapsed % 12 == 0;
            current_desirability = self.desirability(unit);

            // TODO
            // moving_penalty = sim.conf['tenants']['moving_penalty']
            moving_penalty = 50.;
        }

        if reconsider {
            let mut vacants = city.sample_units_with_vacancies(sample_size);
            if vacants.len() > 0 {
                // Hack to roughly sort floats
                vacants.sort_by_key(|u| -(self.desirability(u.borrow())*1e4).round() as i64);

                // Desirability of 0 means that tenant can't afford it
                let des = self.desirability(vacants[0].borrow());
                if des - moving_penalty > current_desirability {
                    if self.unit.is_some() {
                        let mut unit = self.unit.as_ref().unwrap().borrow_mut();
                        unit.move_out();
                    }
                    vacants[0].borrow_mut().move_in(time);
                    self.unit = Some(Rc::clone(vacants[0]));
                }
            }
        }
    }
}
