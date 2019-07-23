use std::thread;
use serde::Deserialize;
use redis::{Commands, Connection};
use strum_macros::{Display};
use super::agent::{Tenant, DOMA};
use super::city::{City};
use rand::seq::SliceRandom;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Display, Debug)]
pub enum SimState {
    Loading,
    Ready,
    FastForward,
    Finished
}

#[derive(Display, PartialEq, Debug, Deserialize)]
enum Command {
    SelectTenant(String, usize), // player_id, tenant_id
    ReleaseTenant(String),       // player_id
    MoveTenant(String, usize),   // player_id, unit_id
    DOMAAdd(String, f32),        // player_id, amount
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

    pub fn wait(&self, seconds: u64) {
        thread::sleep(Duration::from_secs(seconds));
    }

    pub fn sync_step(&self, step: usize, steps: usize) -> redis::RedisResult<()> {
        self.con.set("game_step", step)?;
        self.con.set("game_progress", step as f32/steps as f32)
    }

    pub fn wait_turn(&self, seconds: u64) {
        let start = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
        let end = start + seconds;
        let _: () = self.con.set("turn_timer", format!("{}-{}", start, end)).unwrap();
        self.wait(seconds)
    }

    fn set_state(&self, state: SimState) -> redis::RedisResult<()> {
        self.con.set("game_state", state.to_string().to_lowercase())?;
        Ok(())
    }

    pub fn set_ready(&self) -> redis::RedisResult<()> {
        self.set_state(SimState::Ready)
    }

    pub fn set_loading(&self) -> redis::RedisResult<()> {
        self.set_state(SimState::Loading)
    }

    pub fn set_fast_forward(&self) -> redis::RedisResult<()> {
        self.set_state(SimState::FastForward)
    }

    pub fn set_finished(&self) -> redis::RedisResult<()> {
        self.set_state(SimState::Finished)
    }

    pub fn reset_ready_players(&self) -> redis::RedisResult<()> {
        self.con.del("ready_players")
    }

    pub fn reset(&mut self) -> redis::RedisResult<()> {
        self.con.del("game_step")?;
        self.con.del("cmds")?;
        self.con.del("active_players")?;
        self.con.del("active_tenants")?;
        self.players.clear();
        self.reset_ready_players()
    }

    pub fn all_players_ready(&self) -> bool {
        let mut all_players_ready = false;
        while !all_players_ready {
            let ready_players: Vec<String> = self.con.lrange("ready_players", 0, -1).unwrap();
            let active_players: Vec<String> = self.con.lrange("active_players", 0, -1).unwrap();
            all_players_ready = active_players.iter().all(|id| {
                ready_players.contains(id)
            });
        }
        all_players_ready
    }

    pub fn process_commands(&mut self, tenants: &mut Vec<Tenant>, city: &mut City, doma: &mut DOMA) -> redis::RedisResult<()> {
        let cmds: Vec<String> = self.con.lrange("cmds", 0, -1)?;
        for cmd in cmds {
            match serde_json::from_str(&cmd).unwrap() {
                Command::SelectTenant(p_id, t_id) => {
                    self.players.insert(p_id, t_id);
                    let tenant = &mut tenants[t_id];
                    tenant.player = true;
                    self.sync_players(&tenants, &city).unwrap();
                },
                Command::ReleaseTenant(p_id) => {
                    match self.players.remove(&p_id) {
                        Some(t_id) => {
                            tenants[t_id].player = false;
                        },
                        None => {}
                    }
                },
                Command::MoveTenant(p_id, u_id) => {
                    match self.players.get(&p_id) {
                        Some(&t_id) => {
                            let unit = &mut city.units[u_id];
                            unit.tenants.insert(t_id);
                            let tenant = &mut tenants[t_id];
                            tenant.unit = Some(u_id);
                        },
                        None => {}
                    }
                },
                Command::DOMAAdd(p_id, amount) => {
                    match self.players.get(&p_id) {
                        Some(&t_id) => {
                            doma.add_funds(t_id, amount);
                        },
                        None => {}
                    }
                }
            }
        }

        self.con.del("cmds")
    }
}
