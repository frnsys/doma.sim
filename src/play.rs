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

    pub fn gen_player_tenant_pool(&self, tenants: &Vec<Tenant>) {
        let mut rng = rand::thread_rng();
        let tenants = tenants.choose_multiple(&mut rng, 100);
        let _: () = self.con.del("tenants").unwrap();
        for t in tenants {
            let _: () = self.con.lpush("tenants", json!({
                "id": t.id,
                "income": t.income,
                "work": t.work,
                "unit": t.unit
            }).to_string()).unwrap();
        }
    }

    pub fn sync_players(&self, tenants: &Vec<Tenant>, city: &City) -> redis::RedisResult<()> {
        for (player_id, &t_id) in &self.players {
            let tenant = &tenants[t_id];
            let mut adjusted_rent = 0.;
            let monthly_income = tenant.income as f32/12.;
            let unit = match tenant.unit {
                Some(u_id) => {
                    Some(&city.units[u_id])
                },
                None => None
            };
            let desirability = match unit {
                Some(unit) => {
                    let parcel = &city.parcels.get(&unit.pos).unwrap();
                    adjusted_rent = tenant.adjusted_rent(&unit);
                    tenant.desirability(unit, parcel)
                },
                None => -1.
            };

            let key = format!("player:{}:tenant", player_id);
            self.con.set(key, json!({
                "id": t_id,
                "income": tenant.income,
                "monthlyDisposableIncome": monthly_income - adjusted_rent,
                "rent": adjusted_rent,
                "work": tenant.work,
                "desirability": desirability,
                "unit": match unit {
                    Some(unit) => {
                        json!({
                            "id": unit.id,
                            "rent": unit.rent,
                            "condition": unit.condition,
                            "pos": unit.pos
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
                            match self.players.get(&p_id) {
                                Some(&t_id) => {
                                    let unit = &mut sim.city.units[u_id];
                                    unit.tenants.insert(t_id);
                                    let tenant = &mut sim.tenants[t_id];
                                    tenant.unit = Some(u_id);
                                },
                                None => {}
                            }
                        },
                        Command::DOMAAdd(p_id, amount) => {
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
