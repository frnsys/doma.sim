use super::agent::AgentType;
use super::sim::Simulation;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

pub fn init_stats(sim: &Simulation) -> Value {
    let incomes: Vec<f32> = sim.tenants.iter().map(|t| t.income).collect();
    let values: Vec<f32> = sim.city.units.iter().map(|u| u.value).collect();
    let rents: Vec<f32> = sim.city.units.iter().map(|u| u.rent).collect();
    let areas: Vec<f32> = sim.city.units.iter().map(|u| u.area).collect();
    let occupancies: Vec<usize> = sim.city.units.iter().map(|u| u.occupancy).collect();
    let rents_per_occupancy: Vec<f32> = sim.city.units.iter().map(|u| u.rent/u.occupancy as f32).collect();
    json!({
        "incomes": incomes,
        "values": values,
        "rents": rents,
        "rents_per_occupancy": rents_per_occupancy,
        "occupancies": occupancies,
        "areas": areas
    })
}

pub fn stats(sim: &Simulation) -> Value {
    let n_units = sim.city.units.len() as f32;
    let mut n_housed = 0.;
    let mut n_vacant = 0.;
    let mut n_parcels = 0.;
    let mut n_affordable = 0.;
    let mut mean_rent = 0.;
    let mut mean_rent_per_area = 0.;
    let mut mean_rent_per_tenant = 0.;
    let mut mean_adjusted_rent_per_area = 0.;
    let mut mean_months_vacant = 0.;
    let mut mean_value_per_area = 0.;
    let mut mean_condition = 0.;
    let mut mean_price_to_rent_ratio = 0.;
    let mut mean_rent_income_ratio = 0.;
    let mut mean_offers = 0.;
    let mut mean_value = 0.;
    let mut min_value = 1. / 0.;
    let mut mean_desirability = 0.;
    let mut unique_landlords = HashSet::new();
    let mut landlord_data = HashMap::new();
    let mut doma_data = (0., 0.);
    let mean_income = sim.tenants.iter().fold(0., |acc, t| acc + t.income)/sim.tenants.len() as f32;

    let mut neighborhood_stats = HashMap::new();
    for (neighb_id, unit_ids) in sim.city.units_by_neighborhood.iter().enumerate() {
        if unit_ids.len() == 0 {
            continue;
        }

        let mut nei_n_doma = 0;
        let mut nei_n_vacant = 0.;
        let mut nei_n_tenants = 0;
        let mut nei_mean_rent = 0.;
        let mut nei_mean_rent_per_area = 0.;
        let mut nei_mean_rent_per_tenant = 0.;
        let mut nei_mean_adjusted_rent_per_area = 0.;
        let mut nei_mean_value_per_area = 0.;
        let mut nei_mean_months_vacant = 0.;
        let mut nei_mean_rent_income_ratio = 0.;

        for &unit_id in unit_ids {
            let unit = &sim.city.units[unit_id];
            let value = unit.value;
            mean_offers += unit.offers.len() as f32;
            nei_mean_rent += unit.rent;
            nei_mean_rent_per_area += unit.rent_per_area();
            nei_mean_months_vacant += unit.months_vacant as f32;
            nei_mean_value_per_area += value / unit.area;
            mean_value += value;
            mean_condition += unit.condition;
            mean_price_to_rent_ratio += if unit.rent == 0. {
                0.
            } else {
                value / (unit.rent * 12.)
            };
            if value < min_value {
                min_value = value;
            }

            if unit.vacant() {
                nei_n_vacant += 1.;
            }

            let mut rent_discount = 0.;
            let rent_per_tenant = unit.rent / unit.tenants.len() as f32;
            for &t_id in &unit.tenants {
                let tenant = &sim.tenants[t_id];
                rent_discount += tenant.last_dividend;
                nei_mean_rent_income_ratio += rent_per_tenant / tenant.income;
                nei_mean_rent_per_tenant += rent_per_tenant;
                if rent_per_tenant / tenant.income <= 0.3 {
                    n_affordable += 1.;
                }
            }
            nei_mean_adjusted_rent_per_area +=
                f32::max(0., unit.rent - f32::min(unit.rent, rent_discount)) / unit.area;
            n_housed += unit.tenants.len() as f32;
            nei_n_tenants += unit.tenants.len();

            unique_landlords.insert(unit.owner);
            match unit.owner.0 {
                AgentType::Landlord => {
                    let data = landlord_data.entry(unit.owner.1).or_insert((0., 0.));
                    data.0 += unit.condition;
                    data.1 += mean_adjusted_rent_per_area;
                },
                AgentType::DOMA => {
                    doma_data.0 += unit.condition;
                    doma_data.1 += mean_adjusted_rent_per_area;
                    nei_n_doma += 1;
                }
                _ => {}
            }
        }

        let nei_n_units = unit_ids.len() as f32;
        let parcels = &sim.city.residential_parcels_by_neighborhood[neighb_id];
        n_parcels += parcels.len() as f32;
        let nei_mean_desirability = parcels
            .iter()
            .fold(0., |acc, pos| acc + sim.city.parcels.get(&pos).unwrap().desirability);

        neighborhood_stats.insert(
            neighb_id,
            json!({
                "percent_vacant": nei_n_vacant/nei_n_units,
                "mean_rent": nei_mean_rent/nei_n_units,
                "mean_rent_per_tenant": nei_mean_rent_per_tenant/(nei_n_tenants as f32),
                "mean_rent_per_area": nei_mean_rent_per_area/nei_n_units,
                "mean_adjusted_rent_per_area": nei_mean_adjusted_rent_per_area/nei_n_units,
                "mean_value_per_area": nei_mean_value_per_area/nei_n_units,
                "mean_months_vacant": nei_mean_months_vacant/nei_n_units,
                "mean_rent_income_ratio": if nei_n_tenants > 0 {
                    nei_mean_rent_income_ratio/nei_n_tenants as f32
                } else { 0. },
                "mean_desirability": nei_mean_desirability/parcels.len() as f32,
                "doma_units": nei_n_doma
            }),
        );

        n_vacant += nei_n_vacant;
        mean_rent += nei_mean_rent;
        mean_rent_per_tenant += nei_mean_rent_per_tenant;
        mean_rent_per_area += nei_mean_rent_per_area;
        mean_adjusted_rent_per_area += nei_mean_adjusted_rent_per_area;
        mean_value_per_area += nei_mean_value_per_area;
        mean_months_vacant += nei_mean_months_vacant;
        mean_rent_income_ratio += nei_mean_rent_income_ratio;
        mean_desirability += nei_mean_desirability;
    }

    let mut landlord_stats = HashMap::new();
    for landlord in &sim.landlords {
        let data = landlord_data.entry(landlord.id).or_insert((0., 0.));
        let l_n_units = landlord.units.len() as f32;
        landlord_stats.insert(
            landlord.id as i32,
            json!({
                "n_units": l_n_units,
                "p_units": l_n_units/n_units,
                "mean_condition": data.0/l_n_units,
                "mean_adjusted_rent_per_area": data.1/l_n_units
            }),
        );
    }

    // DOMA special id of -1
    let n_doma_units = sim.doma.units.len() as f32;
    landlord_stats.insert(
        -1,
        json!({
            "n_units": n_doma_units,
            "p_units": n_doma_units/n_units,
            "mean_condition": doma_data.0/n_doma_units,
            "mean_adjusted_rent_per_area": doma_data.1/n_doma_units
        }),
    );

    json!({
        "population": sim.tenants.len(),
        "percent_homeless": 1. - n_housed/sim.tenants.len() as f32,
        "percent_vacant": n_vacant/n_units,
        "percent_affordable": n_affordable/n_housed,
        "n_units": n_units,
        "p_units": 1.,
        "mean_income": mean_income,
        "mean_rent": mean_rent/n_units,
        "mean_rent_per_tenant": mean_rent_per_tenant/n_housed,
        "mean_rent_per_area": mean_rent_per_area/n_units,
        "mean_adjusted_rent_per_area": mean_adjusted_rent_per_area/n_units,
        "mean_months_vacant": mean_months_vacant/n_units,
        "mean_value_per_area": mean_value_per_area/n_units,
        "mean_value": mean_value/n_units,
        "min_value": min_value,
        "mean_condition": mean_condition/n_units,
        "mean_price_to_rent_ratio": mean_price_to_rent_ratio/n_units,
        "mean_rent_income_ratio": if n_housed > 0. { mean_rent_income_ratio/n_housed } else { 0. },
        "mean_offers": mean_offers/n_units,
        "unique_landlords": unique_landlords.len(),
        "doma_members": sim.doma.shares.len(),
        "doma_members_p": sim.doma.shares.len() as f32/sim.tenants.len() as f32,
        "doma_raised": sim.doma.raised,
        "doma_property_fund": sim.doma.funds,
        "mean_desirability": mean_desirability/n_parcels,
        // 'doma_total_dividend_payout': self.doma.last_payout,
        // 'n_sales': sum(t.sales for t in self.landlords + self.tenants),
        // 'n_moved': sum(1 for t in self.tenants if t.moved),
        // 'mean_doma_rent_vs_market_rent': 0 if not landlord_units or not self.doma.units else np.mean([u.adjusted_rent_per_area for u in self.doma.units])/np.mean([u.adjusted_rent_per_area for u in landlord_units]),
        "landlords": landlord_stats,
        "neighborhoods": neighborhood_stats
    })
}
