use super::agent::{AgentType, Landlord, Tenant, DOMA};
use super::city::{City, Unit};
use super::config::SimConfig;
use super::design::Design;
use noise::NoiseFn;
use rand::distributions::WeightedIndex;
use rand::prelude::*;
use rand::rngs::StdRng;

pub struct Simulation {
    pub city: City,
    pub doma: DOMA,
    pub tenants: Vec<Tenant>,
    pub landlords: Vec<Landlord>,
    pub design: Design
}

impl Simulation {
    pub fn new(mut design: Design, config: &SimConfig, mut rng: &mut StdRng) -> Simulation {
        // Generate city from provided design
        let mut city = City::new(&mut design, &mut rng);

        // Create landlords
        let mut landlords: Vec<Landlord> = (0..design.city.landlords)
            .map(|i| Landlord::new(i as usize, design.neighborhoods.keys().cloned().collect()))
            .collect();

        // Create tenants
        let income_dist = WeightedIndex::new(design.city.incomes.iter().map(|i| i.p)).unwrap();
        let mut commercial = Vec::new();
        let mut commercial_weights = Vec::new();
        for (pos, n) in &city.commercial {
            commercial.push(*pos);
            commercial_weights.push(n);
        }
        let work_dist = WeightedIndex::new(commercial_weights).unwrap();
        let vacancies: Vec<usize> = city.units.iter().map(|u| u.id).collect();
        let mut tenants: Vec<Tenant> = (0..design.city.population)
            .map(|i| {
                let tenant_id = i as usize;
                let income_range = &design.city.incomes[income_dist.sample(&mut rng)];
                let income = rng.gen_range(income_range.low, income_range.high) as f32;
                let work_pos = commercial[work_dist.sample(&mut rng)];

                let mut tenant = Tenant {
                    id: tenant_id,
                    unit: None,
                    units: Vec::new(),
                    income: income,
                    work: work_pos,
                    last_dividend: 0.,
                    player: false,
                };

                let lease_month = rng.gen_range(0, 11) as usize;
                let (best_id, best_desirability) = vacancies.iter().fold((0, 0.), |acc, &u_id| {
                    let u = &city.units[u_id];
                    let p = &city.parcels[&u.pos];
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
            })
            .collect();

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

        let doma = DOMA::new(
            config.doma_starting_funds,
            config.doma_p_rent_share,
            config.doma_p_reserves,
            config.doma_p_expenses,
        );

        Simulation {
            city: city,
            landlords: landlords,
            tenants: tenants,
            doma: doma,
            design: design,
        }
    }

    pub fn step(&mut self, time: usize, mut rng: &mut StdRng, conf: &SimConfig) {
        let mut transfers = Vec::new();
        for tenant in &mut self.tenants {
            transfers.extend(
                tenant.check_purchase_offers(&mut self.city, self.design.city.price_to_rent_ratio),
            );
        }
        for landlord in &mut self.landlords {
            transfers.extend(
                landlord
                    .check_purchase_offers(&mut self.city, self.design.city.price_to_rent_ratio),
            );
        }
        for (landlord_typ, landlord_id, unit_id, amount) in transfers {
            match landlord_typ {
                AgentType::Landlord => {
                    let landlord = &mut self.landlords[landlord_id];
                    landlord.units.push(unit_id);
                }
                AgentType::DOMA => {
                    self.doma.units.push(unit_id);
                    self.doma.funds -= amount;
                }
                _ => {}
            }
        }

        for landlord in &mut self.landlords {
            landlord.step(
                &mut self.city,
                time,
                self.design.city.price_to_rent_ratio,
                &mut rng,
                &conf,
            );
        }

        let mut vacant_units: Vec<usize> = self
            .city
            .units
            .iter()
            .filter(|u| u.vacancies() > 0)
            .map(|u| u.id)
            .collect();
        for tenant in &mut self.tenants {
            if !tenant.player {
                tenant.step(
                    &mut self.city,
                    time,
                    &mut vacant_units,
                    &mut rng,
                    &conf,
                );
            }
        }

        if time % 12 == 0 {
            // Appraise
            for (_, unit_ids) in &self.city.units_by_neighborhood {
                let units: Vec<&Unit> = unit_ids
                    .iter()
                    .map(|&u_id| &self.city.units[u_id])
                    .collect();
                let sold: Vec<&Unit> = units.iter().filter(|u| u.recently_sold).cloned().collect();
                let mean_value_per_area = if sold.len() == 0 {
                    units.iter().fold(0., |acc, u| {
                        acc + (u.value_per_area() * conf.base_appreciation)
                    }) / units.len() as f32
                } else {
                    sold.iter().fold(0., |acc, u| {
                        acc + (u.value_per_area() * conf.base_appreciation)
                    }) / sold.len() as f32
                };

                for &u_id in unit_ids {
                    let mut unit = &mut self.city.units[u_id];
                    if !unit.recently_sold {
                        unit.value = mean_value_per_area * unit.area;
                    }
                    unit.recently_sold = false;
                }
            }
        }

        self.doma.step(&mut self.city, &mut self.tenants, &mut rng);

        // Desirability changes, random walk
        for (neighb_id, parcel_ids) in &self.city.residential_parcels_by_neighborhood {
            let last_val = if time > 0 {
                self.city.neighborhood_trends[neighb_id].get([
                    (time - 1) as f64 / conf.desirability_stretch_factor,
                    0.,
                ])
            } else {
                0.
            };
            let val = self.city.neighborhood_trends[neighb_id]
                .get([time as f64 / conf.desirability_stretch_factor, 0.]);
            let change = (val - last_val) as f32;
            for p in parcel_ids {
                let parcel = self.city.parcels.get_mut(p).unwrap();
                parcel.desirability = f32::max(0., parcel.desirability - change);
            }
        }
    }
}
