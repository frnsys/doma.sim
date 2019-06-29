use super::city::City;
use super::design::Design;
use md5::Digest;
use redis::Commands;
use serde_json::{json, Value};
use std::collections::HashMap;

pub fn jsonify(month: usize, city: &City, design: &Design, stats: Value) -> Value {
    let mut parcels: HashMap<isize, HashMap<isize, Value>> = HashMap::new();
    let mut buildings: HashMap<String, Value> = HashMap::new();
    let mut units: HashMap<usize, Value> = HashMap::new();

    for (pos, parcel) in city.parcels.iter() {
        let g = parcels.entry(pos.0).or_insert(HashMap::new());
        g.insert(
            pos.1,
            json!({
                "neighb": match parcel.neighborhood {
                    Some(neighb_id) => neighb_id as i32,
                    None => -1
                },
                "type": parcel.typ.to_string(),
                "desirability": parcel.desirability
            }),
        );
        match &city.buildings.get(&pos) {
            None => continue,
            Some(building) => {
                let id = format!("{}_{}", pos.0, pos.1);
                buildings.insert(
                    id,
                    json!({
                        "units": building.units,
                        "nCommercial": building.n_commercial
                    }),
                );
                for &u in &building.units {
                    let unit = &city.units[u];
                    units.insert(
                        u,
                        json!({
                            "id": u,
                            "rent": unit.rent,
                            "tenants": unit.tenants.len(),
                            "occupancy": unit.occupancy,
                            "owner": json!({
                                "id": unit.owner.1,
                                "type": unit.owner.0.to_string()
                            }),
                            "monthsVacant": unit.months_vacant
                        }),
                    );
                }
            }
        }
    }

    json!({
        "time": month,
        "map": {
            "rows": city.grid.rows,
            "cols": city.grid.cols,
            "parcels": parcels
        },
        "buildings": buildings,
        "neighborhoods": design.neighborhoods,
        "units": units,
        "stats": stats
    })
}

pub fn sync(month: usize, city: &City, design: &Design, stats: Value) -> redis::RedisResult<()> {
    // TODO stats
    let client = redis::Client::open("redis://127.0.0.1/1")?;
    let con = client.get_connection()?;

    let state_serialized = jsonify(month, city, design, stats).to_string();
    let hash = md5::Md5::digest(state_serialized.as_bytes());

    con.set("state", state_serialized)?;
    con.set("state_key", format!("{:X}", hash))?;

    Ok(())
}
