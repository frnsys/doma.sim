use serde::Deserialize;
use redis::{Commands, Connection};
use strum_macros::{Display};
use super::agent::{Tenant, DOMA};
use super::policy::Policy;
use super::sim::Simulation;
use super::city::{City, Unit};
use rand::seq::SliceRandom;
use serde_json::{json, Value};
use std::collections::HashMap;
use rand::rngs::StdRng;
use std::{thread, time};

static COMMAND_INTERVAL_MS: u64 = 500;

#[derive(Display, Debug)]
pub enum Status {
    Loading,
    Ready,
    Running,
}

#[derive(Display, PartialEq, Debug, Deserialize)]
enum Command {
    SelectTenant(String, usize),    // player_id, tenant_id
    ReleaseTenant(String),          // player_id
    ReleaseTenants,                 //
    MoveTenant(String, usize),      // player_id, unit_id
    DOMAAdd(String, f32),           // player_id, amount
    DOMAPreach(String, f32, bool),  // player_id, amount, trigger
    DOMAConfigure(f32, f32, f32),   // p_dividend, p_rent_share, rent_income_limit
    RentFreeze(usize),              // months
    MarketTax(usize),               // months
    Run(usize),                     // steps
    Reset,                          //
}

pub enum Control {
    Run(usize),
    Reset
}

pub struct PlayManager {
    con: Connection,
    players: HashMap<String, usize>
}

impl PlayManager {
    pub fn new() -> PlayManager {
        let client = redis::Client::open("redis://127.0.0.1/1").unwrap();
        let con = client.get_connection().unwrap();

        PlayManager {
            con: con,
            players: HashMap::new()
        }
    }

    pub fn gen_player_tenant_pool(&self, tenants: &Vec<Tenant>, city: &City, size: usize) {
        let mut rng = rand::thread_rng();
        let tenants = tenants.choose_multiple(&mut rng, size);
        let _: () = self.con.del("tenants").unwrap();

        // Move tenants into vacant units if necessary
        let vacant_units: Vec<&Unit> = city
            .units
            .iter()
            .filter(|u| u.vacancies() > 0)
            .collect();

        for t in tenants {
            let mut adjusted_rent = None;
            let mut unit_neighborhood = None;
            match t.unit {
                Some(u_id) => {
                    let u = &city.units[u_id];
                    adjusted_rent = Some(t.adjusted_rent(&u));
                    unit_neighborhood = match city.neighborhood_for_pos(&u.pos) {
                        Some(neighb) => Some(&neighb.name),
                        None => None
                    };
                },
                None => {
                    for u in &vacant_units {
                        if t.adjusted_rent(u) < t.income {
                            adjusted_rent = Some(t.adjusted_rent(u));
                            unit_neighborhood = match city.neighborhood_for_pos(&u.pos) {
                                Some(neighb) => Some(&neighb.name),
                                None => None
                            };
                        }
                    }
                }
            };

            let work_neighborhood = match city.neighborhood_for_pos(&t.work) {
                Some(neighb) => Some(&neighb.name),
                None => None
            };

            let _: () = self.con.lpush("tenants", json!({
                "id": t.id,
                "income": t.income,
                "work": {
                    "pos": t.work,
                    "neighborhood": work_neighborhood
                },
                "rent": adjusted_rent,
                "unit": {
                    "id": t.unit,
                    "neighborhood": unit_neighborhood
                }
            }).to_string()).unwrap();
        }
    }

    pub fn sync_players(&self, tenants: &Vec<Tenant>, city: &City, doma: &DOMA) -> redis::RedisResult<()> {
        for (player_id, &t_id) in &self.players {
            let tenant = &tenants[t_id];
            let mut adjusted_rent = None;
            let mut unit_neighborhood = None;
            let unit = match tenant.unit {
                Some(u_id) => {
                    Some(&city.units[u_id])
                },
                None => None
            };
            let desirability = match unit {
                Some(unit) => {
                    let parcel = &city.parcels.get(&unit.pos).unwrap();
                    adjusted_rent = Some(tenant.adjusted_rent(&unit));
                    unit_neighborhood = match city.neighborhood_for_pos(&unit.pos) {
                        Some(neighb) => Some(&neighb.name),
                        None => None
                    };
                    tenant.desirability(unit, parcel)
                },
                None => -1.
            };

            let work_neighborhood = match city.neighborhood_for_pos(&tenant.work) {
                Some(neighb) => Some(&neighb.name),
                None => None
            };

            let key = format!("player:{}:tenant", player_id);
            self.con.set(key, json!({
                "id": t_id,
                "income": tenant.income,
                "shares": match doma.shares.get(&t_id) {
                    None => 0.,
                    Some(s) => *s
                },
                "dividend": tenant.last_dividend,
                "rent": adjusted_rent,
                "work": {
                    "pos": tenant.work,
                    "neighborhood": work_neighborhood
                },
                "desirability": desirability,
                "unit": match unit {
                    Some(unit) => {
                        json!({
                            "id": unit.id,
                            "rent": unit.rent,
                            "condition": unit.condition,
                            "pos": unit.pos,
                            "neighborhood": unit_neighborhood
                        })
                    },
                    None => Value::Null
                }
            }).to_string())?
        }
        Ok(())
    }

    pub fn sync_step(&self, step: usize, steps: usize) -> redis::RedisResult<()> {
        self.con.set("step", step)?;
        self.con.set("step", step as f32/steps as f32)
    }

    fn set_status(&self, state: Status) -> redis::RedisResult<()> {
        self.con.set("status", state.to_string().to_lowercase())?;
        Ok(())
    }

    pub fn set_ready(&self) -> redis::RedisResult<()> {
        self.set_status(Status::Ready)
    }

    pub fn set_running(&self) -> redis::RedisResult<()> {
        self.set_status(Status::Running)
    }

    pub fn set_loading(&self) -> redis::RedisResult<()> {
        self.set_status(Status::Loading)
    }

    pub fn reset(&mut self) -> redis::RedisResult<()> {
        self.players.clear();
        self.con.del("game_step")?;
        self.con.del("cmds")
    }

    pub fn wait_for_control(&mut self, sim: &mut Simulation, rng: &mut StdRng) -> Control {
        let ms = time::Duration::from_millis(COMMAND_INTERVAL_MS);
        loop {
            let control = self.process_commands(sim, rng);
            match control {
                Some(ctrl) => return ctrl,
                None => {
                    thread::sleep(ms);
                    continue
                }
            }
        }
    }

    pub fn process_commands(&mut self, sim: &mut Simulation, rng: &mut StdRng) -> Option<Control> {
        let mut control = None;
        loop {
            let cmd_raw: Option<String> = self.con.lpop("cmds").unwrap();
            match cmd_raw {
                None => break,
                Some(cmd) => {
                    match serde_json::from_str(&cmd).unwrap() {
                        Command::SelectTenant(p_id, t_id) => {
                            println!("Player joined: {:?}", p_id);
                            self.players.insert(p_id, t_id);
                            let tenant = &mut sim.tenants[t_id];
                            tenant.player = true;

                            // Evict from existing unit, if any
                            match tenant.unit {
                                Some(_u_id) => {
                                    let unit = &mut sim.city.units[_u_id];
                                    unit.tenants.remove(&t_id);
                                    tenant.unit = None;
                                },
                                None => {}
                            }

                            // Reset tenant DOMA shares
                            sim.doma.shares.insert(t_id, 0.);
                        },
                        Command::ReleaseTenant(p_id) => {
                            println!("Player left: {:?}", p_id);
                            match self.players.remove(&p_id) {
                                Some(t_id) => {
                                    sim.tenants[t_id].player = false;
                                },
                                None => {}
                            }
                        },
                        Command::ReleaseTenants => {
                            for t in &mut sim.tenants {
                                t.player = false;
                            }
                        },
                        Command::MoveTenant(p_id, u_id) => {
                            println!("Player {:?} moving to: {:?}", p_id, u_id);
                            match self.players.get(&p_id) {
                                Some(&t_id) => {
                                    let tenant = &mut sim.tenants[t_id];
                                    match tenant.unit {
                                        Some(_u_id) => {
                                            let unit = &mut sim.city.units[_u_id];
                                            unit.tenants.remove(&t_id);
                                        },
                                        None => {}
                                    }
                                    let unit = &mut sim.city.units[u_id];
                                    unit.tenants.insert(t_id);
                                    tenant.unit = Some(u_id);
                                },
                                None => {}
                            }
                        },
                        Command::DOMAAdd(p_id, amount) => {
                            println!("Player {:?} adding {:?} to DOMA", p_id, amount);
                            match self.players.get(&p_id) {
                                Some(&t_id) => {
                                    sim.doma.add_funds(t_id, amount);
                                },
                                None => {}
                            }
                        },
                        Command::DOMAPreach(p_id, amount, trigger) => {
                            println!("Player {:?} preaching {:?}", p_id, amount);
                            match self.players.get(&p_id) {
                                Some(&tenant_id) => {
                                    sim.conf.encounter_rate = f32::min(sim.conf.encounter_rate + amount, 0.75);
                                    sim.conf.base_contribute_prob = f32::min(sim.conf.base_contribute_prob + amount, 0.75);
                                    sim.conf.base_contribute_percent = f32::min(sim.conf.base_contribute_percent + amount, 0.20);
                                    if trigger {
                                        let infected = sim.social_graph.contagion(tenant_id, sim.conf.encounter_rate, sim.conf.transmission_rate, sim.conf.max_contagion_depth, rng);
                                        for t_id in infected {
                                            let t = &sim.tenants[t_id];
                                            sim.doma.add_funds(t_id, sim.conf.base_contribute_percent * t.income);
                                        }
                                    }
                                },
                                None => {}
                            }
                        },
                        Command::DOMAConfigure(p_dividend, p_rent_share, rent_income_limit) => {
                            println!("Configuring DOMA {:?}, {:?}, {:?}", p_dividend, p_rent_share, rent_income_limit);
                            sim.doma.p_reserves = 1.0 - p_dividend - sim.doma.p_expenses;
                            sim.doma.p_rent_share = p_rent_share;
                            sim.doma.rent_income_limit = Some(rent_income_limit);
                        },
                        Command::RentFreeze(months) => {
                            println!("Rent Freeze for {:?} months", months);
                            sim.policies.push((Policy::RentFreeze, months));
                        },
                        Command::MarketTax(months) => {
                            println!("Market Tax for {:?} months", months);
                            sim.policies.push((Policy::MarketTax, months));
                        },
                        Command::Run(n) => {
                            control = Some(Control::Run(n));
                        },
                        Command::Reset => {
                            control = Some(Control::Reset);
                        }
                    }
                }
            }
        }
        control
    }
}
