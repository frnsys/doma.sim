use std::thread;
use std::str::FromStr;
use std::time::{Duration};
use redis::{Commands, Connection};
use strum_macros::{Display, EnumString};
use super::agent::{Tenant};
use super::city::{City};
use rand::seq::SliceRandom;
use serde_json::{json, Value};
use std::collections::HashMap;

/*
 * TODO
 * - Get commands
 * - Speed up/fast-forward
 */

#[derive(Display, Debug)]
pub enum SimState {
    Loading,
    Ready,
    FastForward,
    Finished
}

#[derive(Display, PartialEq, Debug, EnumString)]
enum Command {
    Restart,
    SelectTenant,
    ReleaseTenant,
    MoveTenant,
    DOMAAdd
}


pub struct PlayManager {
    con: Connection,
    players: HashMap<usize, usize>
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
                    let parcel = &city.parcels[&unit.pos];
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

    pub fn sync_step(&self, step: usize) -> redis::RedisResult<()> {
        self.con.set("game_step", step)
    }

    pub fn set_turn_timer(&self, start: u64, end: u64) -> redis::RedisResult<()> {
        self.con.set("turn_timer", format!("{}-{}", start, end))
    }

    fn set_state(&self, state: SimState) -> redis::RedisResult<()> {
        self.con.set("game_state", state.to_string())?;
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

    pub fn process_commands(&self, tenants: &mut Vec<Tenant>) {
        // TODO
        // Need to structure commands in a way that rust can handle...
        // let cmds_raw: Vec<String> = self.con.lrange("cmds", 0, -1).unwrap();
        // let cmds = cmds_raw.iter().map(|c| serde_json::from_str(c).unwrap())
        // let v: Value = serde_json::from_str(data)?;
        // for c in cmds {
        //     match Command::from_str(&c) {
        //         Ok(cmd) => {
        //             match cmd {
        //                 Command::Restart => {
        //                     // TODO
        //                 },
        //                 Command::SelectTenant => {
        //                     pid, tid = data['player_id'], data['tenant_id']
        //                     sim.players[pid] = tid
        //                     tenant = sim.tenants_idx[tid]
        //                     tenant.player = pid
        //                     # Evicted
        //                     if tenant.unit:
        //                         tenant.unit.move_out(tenant)
        //                 }
        //             }
        //         },
        //         Err(_) => {}
        //     }
        // }
        // self.con.del("cmds").unwrap();
    }
}
