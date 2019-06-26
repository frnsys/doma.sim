use serde_json::{json, Value};
use super::agent::{Landlord, Tenant, DOMA, AgentType};
use super::city::City;
use std::cmp::{max};
use std::collections::{HashMap, HashSet};

pub fn stats(city: &City, tenants: &Vec<Tenant>, landlords: &Vec<Landlord>, doma: &DOMA) -> Value {
    let n_units = city.units.len() as f32;
    let mut n_housed = 0;
    let mut n_vacant = 0.;
    let mut mean_rent_per_area = 0.;
    let mut mean_adjusted_rent_per_area = 0.;
    let mut mean_months_vacant = 0.;
    let mut mean_value_per_area = 0.;
    let mut mean_condition = 0.;
    let mut mean_price_to_rent_ratio = 0.;
    let mut mean_rent_income_ratio = 0.;
    let mut mean_offers = 0.;
    let mut mean_value = 0.;
    let mut min_value = 1./0.;
    let mut unique_landlords = HashSet::new();
    let mut landlord_data = HashMap::new();
    let mut doma_data = (0., 0.);

    let mut neighborhood_stats = HashMap::new();
    for (neighb_id, unit_ids) in &city.units_by_neighborhood {
        let mut nei_n_doma = 0;
        let mut nei_n_vacant = 0.;
        let mut nei_n_tenants = 0;
        let mut nei_mean_rent_per_area = 0.;
        let mut nei_mean_adjusted_rent_per_area = 0.;
        let mut nei_mean_value_per_area = 0.;
        let mut nei_mean_months_vacant = 0.;
        let mut nei_mean_rent_income_ratio = 0.;

        for &unit_id in unit_ids {
            let unit = &city.units[unit_id];
            let value = unit.value as f32;
            mean_offers += unit.offers.len() as f32;
            nei_mean_rent_per_area += unit.rent_per_area();
            nei_mean_months_vacant += unit.months_vacant as f32;
            nei_mean_value_per_area += value/unit.area as f32;
            mean_value += value;
            mean_condition += unit.condition;
            mean_price_to_rent_ratio += if unit.rent == 0 {
                0.
            } else {
                value/(unit.rent*12) as f32
            };
            if value < min_value {
                min_value = value;
            }

            if unit.vacant() {
                nei_n_vacant += 1.;
            }

            let mut rent_discount = 0;
            let rent_per_tenant = unit.rent as f32/unit.tenants.len() as f32;
            for &t_id in &unit.tenants {
                let tenant = &tenants[t_id];
                rent_discount += tenant.last_dividend;
                nei_mean_rent_income_ratio += rent_per_tenant/tenant.income as f32;
            }
            nei_mean_adjusted_rent_per_area += (max(0, unit.rent - rent_discount)) as f32/unit.area as f32;
            n_housed += unit.tenants.len();
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
                },
                _ => {}
            }
        }
        let nei_n_units = unit_ids.len() as f32;
        neighborhood_stats.insert(neighb_id, json!({
            "percent_vacant": nei_n_vacant/nei_n_units,
            "mean_rent_per_area": nei_mean_rent_per_area/nei_n_units,
            "mean_adjusted_rent_per_area": nei_mean_adjusted_rent_per_area/nei_n_units,
            "mean_value_per_area": nei_mean_value_per_area/nei_n_units,
            "mean_months_vacant": nei_mean_months_vacant/nei_n_units,
            "mean_rent_income_ratio": if nei_n_tenants > 0 {
                nei_mean_rent_income_ratio/nei_n_tenants as f32
            } else { 0. },
            "doma_units": nei_n_doma
        }));

        n_vacant += nei_n_vacant;
        mean_rent_per_area += nei_mean_rent_per_area;
        mean_adjusted_rent_per_area += nei_mean_adjusted_rent_per_area;
        mean_value_per_area += nei_mean_value_per_area;
        mean_months_vacant += nei_mean_months_vacant;
        mean_rent_income_ratio += nei_mean_rent_income_ratio;
        //     'mean_desirability': sum(p.weighted_desirability for p in parcels_by_neighb[neighb])/len(parcels_by_neighb[neighb]),
    }

    let mut landlord_stats = HashMap::new();
    for landlord in landlords {
        let data = landlord_data.entry(landlord.id).or_insert((0., 0.));
        let n_units = landlord.units.len() as f32;
        landlord_stats.insert(landlord.id as i32, json!({
            "n_units": n_units,
            "mean_condition": data.0/n_units,
            "mean_adjusted_rent_per_area": data.1/n_units
        }));
    }

    // DOMA special id of -1
    landlord_stats.insert(-1, json!({
        "n_units": doma.units.len(),
        "mean_condition": doma_data.0/doma.units.len() as f32,
        "mean_adjusted_rent_per_area": doma_data.1/doma.units.len() as f32
    }));

    json!({
        "percent_homeless": n_housed/tenants.len(),
        "percent_vacant": n_vacant/n_units,
        "n_units": n_units,
        "mean_rent_per_area": mean_rent_per_area/n_units,
        "mean_adjusted_rent_per_area": mean_adjusted_rent_per_area/n_units,
        "mean_months_vacant": mean_months_vacant/n_units,
        "mean_value_per_area": mean_value_per_area/n_units,
        "mean_value": mean_value/n_units,
        "min_value": min_value,
        "mean_condition": mean_condition/n_units,
        "mean_price_to_rent_ratio": mean_price_to_rent_ratio/n_units,
        "mean_rent_income_ratio": if n_housed > 0 { mean_rent_income_ratio/n_housed as f32 } else { 0. },
        "mean_offers": mean_offers/n_units,
        "unique_landlords": unique_landlords.len(),
        "doma_members": doma.shares.len() as f32/tenants.len() as f32,
        "doma_units": doma.units.len(),
        "doma_property_fund": doma.funds,
        // 'mean_desirability': sum(p.weighted_desirability for p in parcels)/len(parcels),
        // 'doma_total_dividend_payout': self.doma.last_payout,
        // 'n_sales': sum(t.sales for t in self.landlords + self.tenants),
        // 'n_moved': sum(1 for t in self.tenants if t.moved),
        // 'mean_doma_rent_vs_market_rent': 0 if not landlord_units or not self.doma.units else np.mean([u.adjusted_rent_per_area for u in self.doma.units])/np.mean([u.adjusted_rent_per_area for u in landlord_units]),
        "landlords": landlord_stats,
        "neighborhoods": neighborhood_stats
    })
}
