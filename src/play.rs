use serde::Deserialize;
use redis::{Commands, Connection};
use strum_macros::{Display};
use super::agent::Tenant;
use super::sim::Simulation;
use super::city::City;
use rand::seq::SliceRandom;
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Display, Debug)]
pub enum SimState {
    Loading,
    Ready,
    Running,
}

#[derive(Display, PartialEq, Debug, Deserialize)]
enum Command {
    SelectTenant(String, usize), // player_id, tenant_id
    ReleaseTenant(String),       // player_id
    MoveTenant(String, usize),   // player_id, unit_id
    DOMAAdd(String, f32),        // player_id, amount
    Run(usize),                  // steps
    Reset,                       //
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

    pub fn gen_player_tenant_pool(&self, tenants: &Vec<Tenant>, city: &City) {
        let mut rng = rand::thread_rng();
        let tenants = tenants.choose_multiple(&mut rng, 100);
        let _: () = self.con.del("tenants").unwrap();
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
                None => {}
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

    pub fn sync_players(&self, tenants: &Vec<Tenant>, city: &City) -> redis::RedisResult<()> {
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
        self.con.set("game_step", step)?;
        self.con.set("game_progress", step as f32/steps as f32)
    }

    fn set_state(&self, state: SimState) -> redis::RedisResult<()> {
        self.con.set("game_state", state.to_string().to_lowercase())?;
        Ok(())
    }

    pub fn set_ready(&self) -> redis::RedisResult<()> {
        self.set_state(SimState::Ready)
    }

    pub fn set_running(&self) -> redis::RedisResult<()> {
        self.set_state(SimState::Running)
    }

    pub fn set_loading(&self) -> redis::RedisResult<()> {
        self.set_state(SimState::Loading)
    }

    pub fn reset(&mut self) -> redis::RedisResult<()> {
        self.players.clear();
        self.con.del("game_step")?;
        self.con.del("cmds")
    }

    pub fn wait_for_control(&mut self, sim: &mut Simulation) -> Control {
        loop {
            let control = self.process_commands(sim);
            match control {
                Some(ctrl) => return ctrl,
                None => continue
            }
        }
    }

    pub fn process_commands(&mut self, sim: &mut Simulation) -> Option<Control> {
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
                            println!("Player {:?} adding to {:?} DOMA", p_id, amount);
                            match self.players.get(&p_id) {
                                Some(&t_id) => {
                                    sim.doma.add_funds(t_id, amount);
                                },
                                None => {}
                            }
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
