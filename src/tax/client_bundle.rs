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

fn escape(value: &str) -> String {
    value.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// A single printable HTML packet for filing into an external document cabinet.
/// It is a filing convenience, not a certified form or an electronic filing.
pub async fn cabinet_export(t: &Tax, record_id: &str) -> TResult<Value> {
    let record = t.get(record_id).await?;
    let lines = record
        .calculation
        .as_ref()
        .and_then(|calculation| calculation.result.get("lines"))
        .and_then(|lines| lines.as_array())
        .cloned()
        .unwrap_or_default();
    let rows: String = lines
        .iter()
        .filter_map(|line| {
            let code = line.get("code")?.as_str()?;
            let label = line.get("label").and_then(|v| v.as_str()).unwrap_or("");
            let amount = line.get("amount").and_then(|v| v.as_i64()).unwrap_or(0);
            Some(format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td></tr>",
                escape(code),
                escape(label),
                (amount as f64 / 100.0)
            ))
        })
        .collect();
    let html = format!(
        "<!doctype html><html lang=\"en-CA\"><head><meta charset=\"utf-8\"><title>{} {} — {}</title>\
<style>body{{font-family:system-ui,sans-serif;margin:32px;color:#222}}h1{{font-size:20px}}table{{border-collapse:collapse;width:100%;font-size:13px}}td,th{{border-bottom:1px solid #ddd;padding:5px 4px;text-align:left}}.num{{text-align:right;font-variant-numeric:tabular-nums}}.muted{{color:#666;font-size:12px}}</style></head><body>\
<h1>{} <span class=\"muted\">{} {}</span></h1>\
<p class=\"muted\">Jurisdiction {} · client reference <strong>{}</strong> · status {} · revision {} · updated {}</p>\
<table><thead><tr><th>Line</th><th>Description</th><th class=\"num\">Amount (CAD)</th></tr></thead><tbody>{}</tbody></table>\
<p class=\"muted\">Saved calculation snapshot, if any. This packet is an internal workpaper for an external document cabinet; it is not a certified form, a CRA acknowledgement or an electronic filing.</p>\
</body></html>",
        escape(&record.program),
        escape(&record.tax_year.to_string()),
        escape(&record.name),
        escape(&record.name),
        escape(&record.program),
        escape(&record.tax_year.to_string()),
        escape(&record.jurisdiction),
        escape(&record.client_ref),
        escape(&record.status),
        record.revision,
        escape(&record.updated_at),
        rows
    );
    let filename = format!(
        "{}-{}-{}.html",
        record.client_ref.replace(|c: char| !c.is_ascii_alphanumeric(), "_"),
        record.program,
        record.tax_year
    );
    Ok(json!({
        "format": "nxt-tax-cabinet/v1",
        "return_id": record.id,
        "client_ref": record.client_ref,
        "filename": filename,
        "generated_at": super::now(),
        "html": html,
        "note": "Internal workpaper packet for an external cabinet. Not a certified form, CRA acknowledgement or electronic filing.",
    }))
}
