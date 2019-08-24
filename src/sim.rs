use super::agent::{AgentType, Landlord, Tenant, DOMA};
use super::city::{City, Unit};
use super::social::{SocialGraph};
use super::config::Config;
use super::policy::Policy;
use super::design::Design;
use noise::NoiseFn;
use rand::distributions::WeightedIndex;
use rand_distr::{LogNormal, Distribution};
use rand::prelude::*;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;

pub struct Simulation {
    pub time: usize,
    pub city: City,
    pub doma: DOMA,
    pub conf: Config,
    pub tenants: Vec<Tenant>,
    pub landlords: Vec<Landlord>,
    pub policies: Vec<(Policy, usize)>,
    pub social_graph: SocialGraph,
    pub design: Design,
    transfers: Vec<(AgentType, usize, usize, f32)>,

    // For random iteration over populations
    landlord_order: Vec<usize>,
    tenant_order: Vec<usize>,
}

impl Simulation {
    pub fn new(design: Design, config: Config, mut rng: &mut StdRng) -> Simulation {
        // Generate city from provided design
        println!("Creating city...");
        let mut city = City::new(&design, &mut rng);

        // Create landlords
        let mut landlords: Vec<Landlord> = (0..design.city.landlords)
            .map(|i| Landlord::new(i as usize, design.neighborhoods.len()))
            .collect();

        // Create tenants
        println!("Creating tenants...");
        let income_dist = LogNormal::new(design.city.income_mu, design.city.income_sigma).unwrap();
        let mut commercial = Vec::new();
        let mut commercial_weights = Vec::new();
        for (pos, n) in city.commercial.iter() {
            commercial.push(pos);
            commercial_weights.push(n);
        }
        let work_dist = WeightedIndex::new(commercial_weights).unwrap();
        let vacancies: Vec<usize> = city.units.iter().map(|u| u.id).collect();
        let population = 1000;
        let mut tenants: Vec<Tenant> = (0..population)
        // let mut tenants: Vec<Tenant> = (0..design.city.population)
            .map(|i| {
                let tenant_id = i as usize;
                let income = income_dist.sample(&mut rng);
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
                    let p = &city.parcels.get(&u.pos).unwrap();
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

        // Create social network
        println!("Creating social network...");
        let social_graph = SocialGraph::new(tenants.len(), config.friend_limit, &mut rng);

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
            config.doma_rent_income_limit,
        );

        let landlord_order = (0..landlords.len()).collect();
        let tenant_order = (0..tenants.len()).collect();

        Simulation {
            time: 0,
            city: city,
            conf: config,
            landlords: landlords,
            tenants: tenants,
            doma: doma,
            design: design,
            policies: Vec::new(),
            social_graph: social_graph,
            landlord_order: landlord_order,
            tenant_order: tenant_order,
            transfers: Vec::new()
        }
    }

    pub fn step(&mut self, mut rng: &mut StdRng) {
        let mut rent_freeze = false;
        let mut market_tax = false;
        for (p, _) in &self.policies {
            match p {
                Policy::RentFreeze => rent_freeze = true,
                Policy::MarketTax => market_tax = true,
            }
        }

        for tenant in &mut self.tenants {
            self.transfers.extend(
                tenant.check_purchase_offers(&mut self.city, self.design.city.price_to_rent_ratio),
            );
        }
        for landlord in &mut self.landlords {
            self.transfers.extend(
                landlord
                    .check_purchase_offers(&mut self.city, self.design.city.price_to_rent_ratio),
            );
        }
        for (landlord_typ, landlord_id, unit_id, amount) in self.transfers.drain(..) {
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

        self.landlord_order.shuffle(&mut rng);
        for &landlord_id in &self.landlord_order {
            self.landlords[landlord_id].step(
                &mut self.city,
                self.time,
                self.design.city.price_to_rent_ratio,
                rent_freeze,
                market_tax,
                &mut rng,
                &self.conf,
            );
        }

        let mut vacant_units: Vec<usize> = self
            .city
            .units
            .iter()
            .filter(|u| u.vacancies() > 0)
            .map(|u| u.id)
            .collect();

        self.tenant_order.shuffle(&mut rng);
        for &tenant_id in &self.tenant_order {
            let tenant = &mut self.tenants[tenant_id];
            if !tenant.player {
                tenant.step(
                    &mut self.city,
                    self.time,
                    &mut vacant_units,
                    &mut rng,
                    &self.conf,
                );

                // Word-of-mouth/contagion
                let roll: f32 = rng.gen();
                if roll < self.conf.base_contribute_prob {
                    self.doma.add_funds(tenant_id, self.conf.base_contribute_percent * tenant.income);
                    let infected = self.social_graph.contagion(tenant_id, self.conf.encounter_rate, self.conf.transmission_rate, &mut rng);
                    for t_id in infected {
                        let t = &self.tenants[t_id];
                        self.doma.add_funds(t_id, self.conf.base_contribute_percent * t.income);
                    }
                }
            }
        }

        if self.time % 12 == 0 {
            // Appraise
            for unit_ids in &self.city.units_by_neighborhood {
                let units: Vec<&Unit> = unit_ids
                    .iter()
                    .map(|&u_id| &self.city.units[u_id])
                    .collect();
                let sold: Vec<&Unit> = units.iter().filter(|u| u.recently_sold).cloned().collect();
                let mean_value_per_area = if sold.len() == 0 {
                    units.iter().fold(0., |acc, u| {
                        acc + (u.value_per_area() * self.conf.base_appreciation)
                    }) / units.len() as f32
                } else {
                    sold.iter().fold(0., |acc, u| {
                        acc + (u.value_per_area() * self.conf.base_appreciation)
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
        for (neighb_id, parcel_ids) in self.city.residential_parcels_by_neighborhood.iter().enumerate() {
            let last_val = if self.time > 0 {
                self.city.neighborhood_trends[neighb_id].get([
                    (self.time - 1) as f64 / self.conf.desirability_stretch_factor,
                    0.,
                ])
            } else {
                0.
            };
            let val = self.city.neighborhood_trends[neighb_id]
                .get([self.time as f64 / self.conf.desirability_stretch_factor, 0.]);
            let change = (val - last_val) as f32;
            for p in parcel_ids {
                let parcel = self.city.parcels.get_mut(p).unwrap();
                parcel.desirability = f32::max(0., parcel.desirability - change);
            }
        }

        // Tick policies
        self.policies = self.policies.drain(..).filter_map(|(p, duration)| {
            let d = duration - 1;
            if d > 0 {
                Some((p, d))
            } else {
                None
            }
        }).collect();

        self.time += 1;
    }
}
