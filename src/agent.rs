use super::city::{City, Parcel, Unit};
use super::config::SimConfig;
use super::grid::Position;
use fnv::FnvHashMap;
use linreg::linear_regression;
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use rand::Rng;
use strum_macros::Display;

fn distance(a: Position, b: Position) -> f32 {
    (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)) as f32).sqrt()
}

#[derive(Display, Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum AgentType {
    Tenant,
    Landlord,
    DOMA,
}

#[derive(Debug)]
pub struct Tenant {
    pub id: usize,
    pub income: f32,
    pub unit: Option<usize>,
    pub work: Position,
    pub units: Vec<usize>,
    pub last_dividend: f32,
    pub player: bool
}

impl Tenant {
    pub fn step(
        &mut self,
        city: &mut City,
        month: usize,
        vacant_units: &mut Vec<usize>,
        rng: &mut StdRng,
        conf: &SimConfig,
    ) {
        let mut reconsider;
        let mut current_desirability = 0.;
        let mut moving_penalty = conf.moving_penalty;

        match self.unit {
            // If currently w/o home,
            // will always look for a place to move into,
            // with no moving penalty
            None => {
                reconsider = true;
                current_desirability = -1.;
                moving_penalty = 0.;
            }

            // Otherwise, only consider moving
            // between leases or if their current
            // place is no longer affordable
            Some(u_id) => {
                let unit = &mut city.units[u_id];
                let elapsed = if month > unit.lease_month {
                    month - unit.lease_month
                } else {
                    0
                };
                reconsider = elapsed > 0 && elapsed % 12 == 0;
                if !reconsider {
                    // No longer can afford
                    let parcel = &city.parcels.get(&unit.pos).unwrap();
                    current_desirability = self.desirability(unit, parcel);
                    if current_desirability == 0. {
                        reconsider = true;
                        unit.tenants.remove(&self.id);
                        vacant_units.push(u_id);
                        self.unit = None;
                    }
                }
            }
        }
        if reconsider && vacant_units.len() > 0 {
            let sample = vacant_units.choose_multiple(rng, conf.tenant_sample_size);
            let (best_id, best_desirability) = sample.fold((0, 0.), |acc, &u_id| {
                let u = &city.units[u_id];
                let p = &city.parcels.get(&u.pos).unwrap();
                if u.vacancies() <= 0 {
                    acc
                } else {
                    let desirability = self.desirability(u, p);
                    if desirability > acc.1 {
                        (u_id, desirability)
                    } else {
                        acc
                    }
                }
            });
            if best_desirability > 0. && best_desirability - moving_penalty > current_desirability {
                match self.unit {
                    Some(u_id) => {
                        let unit = &mut city.units[u_id];
                        unit.tenants.remove(&self.id);
                        vacant_units.push(u_id);
                    }
                    None => {}
                }

                self.unit = Some(best_id);
                let unit = &mut city.units[best_id];

                // If unit was vacant, this is a new lease
                if unit.vacant() {
                    unit.lease_month = month % 12;
                }

                unit.tenants.insert(self.id);

                // Remove unit if it no longer has
                // any vacancies
                if unit.vacancies() == 0 {
                    vacant_units.retain(|&u_id| u_id != best_id);
                }
            }
        }
    }

    pub fn adjusted_rent(&self, unit: &Unit) -> f32 {
        let rent_per_tenant = f32::max(1., unit.rent / unit.tenants.len() as f32);
        rent_per_tenant - f32::min(rent_per_tenant, self.last_dividend)
    }

    pub fn desirability(&self, unit: &Unit, parcel: &Parcel) -> f32 {
        let n_tenants = (unit.tenants.len() + 1) as f32;

        // Adjust rent by last DOMA dividend
        let rent_per_tenant = f32::max(1., unit.rent / n_tenants);
        let adjusted_rent_per_tenant = rent_per_tenant - f32::min(rent_per_tenant, self.last_dividend);

        if self.income < adjusted_rent_per_tenant {
            0.
        } else {
            let ratio = (self.income / adjusted_rent_per_tenant).sqrt();
            let spaciousness = f32::max(unit.area / n_tenants, 0.).powf(1. / 32.);
            let commute_distance = distance(self.work, unit.pos);
            let commute: f32 = if commute_distance == 0. {
                1.
            } else {
                1. / commute_distance
            };
            ratio * (spaciousness + parcel.desirability + unit.condition + commute)
        }
    }

    pub fn check_purchase_offers(
        &mut self,
        city: &mut City,
        price_to_rent_ratio: f32,
    ) -> Vec<(AgentType, usize, usize, f32)> {
        // If they own units,
        // check purchase offers
        let mut transfers = Vec::new();
        for &u in &self.units {
            let mut unit = &mut city.units[u];
            if unit.offers.len() == 0 {
                continue;
            } else {
                // This should reflect the following:
                // - since rents decrease as the apartment is vacant,
                //   the longer the vacancy, the more likely they are to sell
                // - maintenance costs become too much
                let parcel = &city.parcels.get(&unit.pos).unwrap();
                let est_value = unit.rent * 12. * price_to_rent_ratio * parcel.desirability;

                // Find best offer, if any
                // and mark offers as rejected or accepted
                let (typ, landlord, best_amount): (AgentType, usize, f32) =
                    unit.offers.iter().fold(
                        (AgentType::Landlord, 0, 0.),
                        |(t, l, best), &(typ, landlord, amount)| {
                            if amount > est_value && amount > best {
                                (typ, landlord, amount)
                            } else {
                                (t, l, best)
                            }
                        },
                    );
                if best_amount > 0. {
                    unit.value = best_amount;
                    unit.owner = (AgentType::Landlord, landlord);
                    transfers.push((typ, landlord, u, best_amount));
                }
            }

            unit.offers.clear();
        }

        // Remove sold units
        for (_, _, unit_id, _) in &transfers {
            self.units.retain(|u_id| u_id != unit_id);
        }
        transfers
    }
}

#[derive(Debug)]
pub struct Landlord {
    pub id: usize,
    pub units: Vec<usize>,
    pub maintenance: f32,
    pub rent_obvs: FnvHashMap<usize, Vec<f32>>,
    pub trend_ests: FnvHashMap<usize, f32>,
    pub invest_ests: FnvHashMap<usize, f32>,
}

impl Landlord {
    pub fn new(id: usize, n_neighborhoods: usize) -> Landlord {
        let mut rent_obvs = FnvHashMap::default();
        let mut trend_ests = FnvHashMap::default();
        let mut invest_ests = FnvHashMap::default();
        for id in 0..n_neighborhoods {
            rent_obvs.insert(id, Vec::new());
            trend_ests.insert(id, 0.);
            invest_ests.insert(id, 0.);
        }

        Landlord {
            id: id,
            units: Vec::new(),
            rent_obvs: rent_obvs,
            trend_ests: trend_ests,
            invest_ests: invest_ests,
            maintenance: 0.001,
        }
    }

    pub fn step(
        &mut self,
        city: &mut City,
        month: usize,
        price_to_rent_ratio: f32,
        rng: &mut StdRng,
        conf: &SimConfig,
    ) {
        // Update market estimates
        self.estimate_rents(city, rng, conf.sample_size);
        self.estimate_trends(conf.trend_months);

        // Maintenance
        for &u in &self.units {
            let mut unit = &mut city.units[u];
            let decay: f32 = rng.gen();
            unit.condition -= decay * 0.1; // TODO deterioration rate based on build year?
            unit.condition += self.maintenance;
            unit.condition = f32::min(f32::max(unit.condition, 0.), 1.);
        }

        // Manage units
        for &u in &self.units {
            let mut unit = &mut city.units[u];
            if unit.vacant() {
                unit.months_vacant += 1;
                if unit.months_vacant % 2 == 0 {
                    unit.rent = unit.rent * 0.98;
                    // TODO u.maintenance += 0.01
                }
            } else {
                // Year-long leases
                let elapsed = month as i32 - unit.lease_month as i32;
                if elapsed > 0 && elapsed % 12 == 0 {
                    // TODO this can be smarter
                    // i.e. depend on gap b/w
                    // current rent and rent estimate/projection
                    unit.rent = unit.rent * conf.rent_increase_rate;
                    // TODO u.maintenance -= 0.01
                }
            }
        }

        // Make purchase offers
        // Choose random neighborhood weighted by investment potential
        let neighbs: Vec<usize> = self.invest_ests.keys().cloned().collect();
        let neighb_weights: Vec<f32> = neighbs
            .iter()
            .map(|neighb_id| f32::max(0., self.invest_ests[neighb_id]))
            .collect();
        let neighb_id = if neighb_weights.iter().all(|&w| w == 0.) {
            *neighbs.choose(rng).unwrap()
        } else {
            let neighb_dist = WeightedIndex::new(&neighb_weights).unwrap();
            neighbs[neighb_dist.sample(rng)]
        };
        let est_future_rent = self.trend_ests[&neighb_id];
        let sample = city.units_by_neighborhood[neighb_id].choose_multiple(rng, conf.sample_size);
        for &u_id in sample {
            let unit = &mut city.units[u_id];
            let parcel = &city.parcels.get(&unit.pos).unwrap();
            let est_value =
                est_future_rent * unit.area * 12. * price_to_rent_ratio * parcel.desirability; // TODO was *100
            if est_value > 0. && est_value > unit.value {
                unit.offers.push((AgentType::Landlord, self.id, est_value));
            }
        }
    }

    fn estimate_rents(&mut self, city: &City, rng: &mut StdRng, sample_size: usize) {
        let mut neighborhoods: FnvHashMap<usize, Vec<f32>> = FnvHashMap::default();
        for &u in &self.units {
            let unit = &city.units[u];
            if !unit.vacant() {
                let parcel = &city.parcels.get(&unit.pos).unwrap();
                match parcel.neighborhood {
                    Some(neighb_id) => {
                        let n = neighborhoods.entry(neighb_id).or_insert(Vec::new());
                        n.push(unit.rent_per_area());
                    }
                    None => continue,
                }
            }
        }

        for (&neighb_id, rent_history) in &mut self.rent_obvs {
            let n = neighborhoods.entry(neighb_id).or_insert(Vec::new());
            let sample = city.units_by_neighborhood[neighb_id]
                .choose_multiple(rng, sample_size)
                .map(|&u_id| city.units[u_id].rent_per_area());
            n.extend(sample);
            let max_rent = n.iter().cloned().fold(-1., f32::max);
            rent_history.push(max_rent);
        }
    }

    fn estimate_trends(&mut self, trend_months: usize) {
        for (&neighb_id, rent_history) in &self.rent_obvs {
            if rent_history.len() >= trend_months {
                let ys = &rent_history[rent_history.len() - trend_months..];
                let xs: Vec<f32> = (0..ys.len()).map(|v| v as f32).collect();
                let (slope, intercept): (f32, f32) = linear_regression(&xs, &ys).unwrap();
                let est_market_rent = (trend_months as f32) * slope + intercept;
                self.trend_ests.insert(neighb_id, est_market_rent);
                self.invest_ests
                    .insert(neighb_id, est_market_rent - ys.last().unwrap());
            } else {
                continue;
            }
        }
    }

    pub fn check_purchase_offers(
        &mut self,
        city: &mut City,
        price_to_rent_ratio: f32,
    ) -> Vec<(AgentType, usize, usize, f32)> {
        let mut transfers = Vec::new();
        for &u in &self.units {
            let mut unit = &mut city.units[u];
            if unit.offers.len() == 0 {
                continue;
            } else {
                // This should reflect the following:
                // - since rents decrease as the apartment is vacant,
                //   the longer the vacancy, the more likely they are to sell
                // - maintenance costs become too much
                let parcel = &city.parcels.get(&unit.pos).unwrap();
                let est_future_rent = self.trend_ests[&parcel.neighborhood.unwrap()];
                let est_value =
                    est_future_rent * unit.area * 12. * price_to_rent_ratio * parcel.desirability;

                // Find best offer, if any
                // and mark offers as rejected or accepted
                let (typ, landlord, best_amount): (AgentType, usize, f32) =
                    unit.offers.iter().fold(
                        (AgentType::Landlord, 0, 0.),
                        |(t, l, best), &(typ, landlord, amount)| {
                            if amount > est_value && amount > best {
                                (typ, landlord, amount)
                            } else {
                                (t, l, best)
                            }
                        },
                    );
                if best_amount > 0. {
                    unit.value = best_amount;
                    unit.owner = (AgentType::Landlord, landlord);
                    transfers.push((typ, landlord, u, best_amount));
                }
            }

            // TODO
            // best_offer.landlord.property_fund -= best_offer.amount
            unit.offers.clear();
        }

        for (_, _, unit_id, _) in &transfers {
            self.units.retain(|u_id| u_id != unit_id);
        }
        transfers
    }
}

pub struct DOMA {
    pub funds: f32,
    pub shares: FnvHashMap<usize, f32>,
    pub units: Vec<usize>,
    maintenance: f32,

    // Percent of rent paid to DOMA
    // that converts to shares
    p_rent_share: f32,
    p_reserves: f32,
    p_expenses: f32,
}

impl DOMA {
    pub fn new(funds: f32, p_rent_share: f32, p_reserves: f32, p_expenses: f32) -> DOMA {
        DOMA {
            funds: funds,
            shares: FnvHashMap::default(),
            maintenance: 1.,
            units: Vec::new(),
            p_rent_share: p_rent_share,
            p_reserves: p_reserves,
            p_expenses: p_expenses,
        }
    }

    pub fn step(&mut self, city: &mut City, tenants: &mut Vec<Tenant>, rng: &mut StdRng) {
        // Collect rent
        let mut rent = 0.;
        for &u_id in &self.units {
            let unit = &mut city.units[u_id];

            // Maintenance
            let decay: f32 = rng.gen();
            unit.condition -= decay * 0.1; // TODO deterioration rate based on build year?
            unit.condition += self.maintenance;
            unit.condition = f32::min(f32::max(unit.condition, 0.), 1.);

            if !unit.vacant() {
                rent += unit.rent;
                let rent_per_tenant = rent / unit.tenants.len() as f32;
                for &t in &unit.tenants {
                    let share = self.shares.entry(t).or_insert(0.);
                    *share += rent_per_tenant * self.p_rent_share;
                }
            } else {
                continue;
            }
        }

        // Pay dividends
        let p_dividend = 1.0 - self.p_reserves - self.p_expenses;
        let dividends = rent * p_dividend;
        let total_shares: f32 = self.shares.values().sum();
        for (&tenant_id, share) in &self.shares {
            let tenant = &mut tenants[tenant_id];
            tenant.last_dividend = dividends * share/total_shares;
        }
        self.funds += rent * self.p_reserves;

        // TODO selling of properties

        // Make offers on properties
        // Get non-DOMA properties of DOMA tenants
        let mut candidates: Vec<(usize, f32, f32)> = self
            .shares
            .keys()
            .filter_map(|&t| {
                let tenant = &tenants[t];
                match tenant.unit {
                    Some(u_id) => {
                        let unit = &mut city.units[u_id];
                        if unit.owner.0 != AgentType::DOMA {
                            Some((u_id, unit.value, unit.rent))
                        } else {
                            None
                        }
                    }
                    None => None,
                }
            })
            .collect();

        // Otherwise, consider all unowned properties
        if candidates.len() == 0 {
            candidates = city
                .units
                .iter_mut()
                .filter_map(|unit| {
                    // Ensure unit is affordable
                    if unit.owner.0 != AgentType::DOMA {
                        Some((unit.id, unit.value, unit.rent))
                    } else {
                        None
                    }
                })
                .collect();
        }

        // Filter to affordable
        candidates = candidates
            .into_iter()
            .filter(|&(_, value, _)| value <= self.funds)
            .collect();

        // Prioritize cheap properties with high rent-to-price ratios
        candidates.sort_by_key(|&(_, value, rent)| (value * value / (rent + 1.)).round() as usize);

        // Make offers
        let mut committed = 0.;
        for (id, value, _) in candidates {
            if (committed + value) > self.funds {
                break;
            }
            committed += value;
            let unit = &mut city.units[id];
            unit.offers.push((AgentType::DOMA, 0, value));
        }
    }

    pub fn add_funds(&mut self, tenant_id: usize, amount: f32) {
        self.funds += amount;
        let share = self.shares.entry(tenant_id).or_insert(0.);
        *share += amount;
    }
}
