//! Per-client export and restore: one client's returns and families, with a
//! checksum over the bundle. Restore only lands in an empty client reference.
use super::models::{FamilyLink, TaxReturn};
use super::{Tax, TResult, bad, conflict};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};

const FORMAT: &str = "nxt-tax-client/v1";

fn checksum(value: &Value) -> String {
    hex::encode(Sha256::digest(serde_json::to_vec(value).unwrap_or_default()))
}

pub async fn export(t: &Tax, record_id: &str) -> TResult<Value> {
    let record = t.get(record_id).await?;
    let client_ref = record.client_ref.trim().to_string();
    if client_ref.is_empty() {
        return Err(bad("give the return a client reference before exporting the client"));
    }
    let state = t.load_state().await?;
    let returns: Vec<TaxReturn> = state.returns.iter().filter(|r| r.client_ref == client_ref).cloned().collect();
    let ids: HashSet<&str> = returns.iter().map(|r| r.id.as_str()).collect();
    let families: Vec<FamilyLink> = state
        .families
        .iter()
        .filter(|f| ids.contains(f.primary_return_id.as_str()) && ids.contains(f.spouse_return_id.as_str()))
        .cloned()
        .collect();
    let mut bundle = json!({
        "format": FORMAT,
        "client_ref": client_ref,
        "exported_at": super::now(),
        "returns": returns,
        "families": families,
    });
    let sha = checksum(&bundle);
    bundle["sha256"] = json!(sha);
    Ok(bundle)
}

pub async fn import(t: &Tax, body: Value) -> TResult<Value> {
    if body["format"] != FORMAT {
        return Err(bad("unsupported client bundle format"));
    }
    let sha = body["sha256"].as_str().ok_or_else(|| bad("bundle has no checksum"))?.to_string();
    let mut unsigned = body.clone();
    let Some(object) = unsigned.as_object_mut() else { return Err(bad("invalid client bundle")) };
    object.remove("sha256");
    if checksum(&unsigned) != sha {
        return Err(conflict("the bundle checksum does not match its contents"));
    }
    let client_ref = body["client_ref"].as_str().unwrap_or("").trim().to_string();
    if client_ref.is_empty() || client_ref.len() > 100 {
        return Err(bad("the bundle needs a client reference of at most 100 characters"));
    }
    let returns: Vec<TaxReturn> = serde_json::from_value(body["returns"].clone()).map_err(|_| bad("bundle returns are not readable"))?;
    let families: Vec<FamilyLink> = serde_json::from_value(body.get("families").cloned().unwrap_or_else(|| json!([])))
        .map_err(|_| bad("bundle families are not readable"))?;
    if returns.is_empty() {
        return Err(bad("the bundle has no returns"));
    }
    if returns.iter().any(|r| r.client_ref != client_ref) {
        return Err(bad("every bundled return must belong to the bundle's client reference"));
    }
    let count = returns.len();
    let imported_families = families.len();
    t.mutate_state(|state| {
        if state.returns.iter().any(|r| r.client_ref == client_ref) {
            return Err(conflict(format!("client '{client_ref}' already has returns; restore into an empty client reference")));
        }
        let mut mapped: HashMap<String, String> = HashMap::new();
        for mut record in returns.clone() {
            let new_id = format!("ret_{}", uuid::Uuid::new_v4().simple());
            mapped.insert(record.id.clone(), new_id.clone());
            record.id = new_id;
            state.returns.push(record);
        }
        for mut family in families.clone() {
            let (Some(primary), Some(spouse)) = (mapped.get(&family.primary_return_id), mapped.get(&family.spouse_return_id)) else { continue };
            family.id = format!("fam_{}", uuid::Uuid::new_v4().simple());
            family.primary_return_id = primary.clone();
            family.spouse_return_id = spouse.clone();
            state.families.push(family);
        }
        super::audit(state, "", "tax_client_restored", format!("{client_ref}: {count} returns, {imported_families} families"));
        Ok(())
    }).await?;
    Ok(json!({ "imported": count, "families": imported_families, "client_ref": client_ref }))
}
