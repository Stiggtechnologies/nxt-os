//! Observed source inventory; inclusion never means an operational book exists.
use super::*;
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderValue, header},
    middleware,
    response::Response,
    routing::get,
};
use serde::Deserialize;
use std::sync::Arc;

const HEAD: &str = "company-books:roster:head:v1";
const FORMAT: &str = "nxt-source-company-roster/v1";

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Row {
    company_id: String,
    company_name: String,
    source_navigation_ref: String,
}
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct Capture {
    format: String,
    source_system: String,
    source_capture_sha256: String,
    observed_at: String,
    companies: Vec<Row>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Publish {
    expected_head: Option<String>,
    capture: Capture,
}

fn validate(c: &Capture) -> Result<()> {
    if c.format != FORMAT
        || !matches!(c.source_system.as_str(), "quickbooks_online" | "synthetic")
        || !digest(&c.source_capture_sha256)
        || chrono::DateTime::parse_from_rfc3339(&c.observed_at).is_err()
        || c.companies.is_empty()
        || c.companies.len() > 1000
        || encoded(c)?.len() > 512 * 1024
    {
        return Err(bad("Invalid or oversized observed company roster"));
    }
    let mut ids = HashSet::new();
    let mut refs = HashSet::new();
    for row in &c.companies {
        if !valid_id(&row.company_id)
            || !ids.insert(&row.company_id)
            || row.company_name.trim().is_empty()
            || row.company_name.len() > 300
            || row.company_name.chars().any(char::is_control)
            || row.source_navigation_ref.is_empty()
            || row.source_navigation_ref.len() > 200
            || row.source_navigation_ref.chars().any(char::is_control)
            || !refs.insert(&row.source_navigation_ref)
        {
            return Err(bad(
                "Invalid, duplicate or ambiguous source roster reference",
            ));
        }
    }
    Ok(())
}

async fn load(s: &CompanyBooks) -> Result<Option<(String, Capture)>> {
    let Some(id) = s.store.get(HEAD).await.map_err(storage)? else {
        return Ok(None);
    };
    if !digest(&id) {
        return Err(corrupt());
    }
    let c: Capture = s
        .object(&format!("company-books:roster:capture:{id}"), &id)
        .await?;
    validate(&c).map_err(|_| corrupt())?;
    Ok(Some((id, c)))
}
fn view(current: Option<(String, Capture)>) -> Value {
    match current {
        None => json!({"roster_present":false,"head_sha256":null,"companies":[],"count":0,
            "inventory_complete":false,"financial_imports_inferred":false}),
        Some((id, c)) => json!({"roster_present":true,"head_sha256":id,"count":c.companies.len(),
            "capture":c,"companies":c.companies,"inventory_complete":false,
            "identity_basis":"Observed source selector references and NXT cohort aliases; not durable source company identities",
            "financial_imports_inferred":false,"verification_basis":"Operator-supplied capture hash; reconcile each company separately"}),
    }
}
async fn list(State(s): State<Arc<CompanyBooks>>) -> Result<Json<Value>> {
    Ok(Json(view(load(&s).await?)))
}
async fn publish(
    State(s): State<Arc<CompanyBooks>>,
    Json(input): Json<Publish>,
) -> Result<Json<Value>> {
    validate(&input.capture)?;
    if input.expected_head.as_deref().is_some_and(|v| !digest(v)) {
        return Err(bad("Invalid expected roster head"));
    }
    let raw = encoded(&input.capture)?;
    let id = hash(raw.as_bytes());
    let current = load(&s).await?;
    if current.as_ref().is_some_and(|(head, _)| *head == id) {
        let mut result = view(current);
        result["idempotent_replay"] = json!(true);
        return Ok(Json(result));
    }
    if current.as_ref().map(|(h, _)| h.as_str()) != input.expected_head.as_deref() {
        return Err(err(
            StatusCode::CONFLICT,
            "Roster changed; refresh before publishing",
        ));
    }
    s.immutable(
        &format!("company-books:roster:capture:{id}"),
        &input.capture,
    )
    .await?;
    if !s
        .store
        .compare_exchange(HEAD, input.expected_head.as_deref(), &id)
        .await
        .map_err(storage)?
    {
        let latest = load(&s).await?;
        if latest.as_ref().is_some_and(|(h, _)| *h == id) {
            let mut result = view(latest);
            result["idempotent_replay"] = json!(true);
            return Ok(Json(result));
        }
        return Err(err(
            StatusCode::CONFLICT,
            "Roster publication conflict; no inventory was replaced",
        ));
    }
    let mut result = view(Some((id, input.capture)));
    result["idempotent_replay"] = json!(false);
    Ok(Json(result))
}

pub(super) fn router(s: Arc<CompanyBooks>) -> Router {
    Router::new()
        .route("/api/company-books/roster", get(list).post(publish))
        .layer(DefaultBodyLimit::max(768 * 1024))
        .layer(middleware::from_fn(crate::referral::routes::require_admin))
        .layer(middleware::map_response(|mut r: Response| async move {
            r.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            r.headers_mut().insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            r
        }))
        .with_state(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Publish {
        serde_json::from_value(json!({"expected_head":null,"capture":{"format":FORMAT,"source_system":"synthetic",
            "source_capture_sha256":"a".repeat(64),"observed_at":"2026-10-27T00:00:00Z",
            "companies":[{"company_id":"synthetic-a","company_name":"Synthetic A","source_navigation_ref":"observed-row-1"}]}})).unwrap()
    }
    #[tokio::test]
    async fn roster_is_inventory_only_immutable_and_compare_exchange_guarded() {
        let s = Arc::new(CompanyBooks::new(Store::memory()));
        assert_eq!(
            list(State(s.clone())).await.ok().unwrap().0["roster_present"],
            false
        );
        let first = publish(State(s.clone()), Json(input()))
            .await
            .ok()
            .unwrap()
            .0;
        assert_eq!(first["count"], 1);
        assert_eq!(first["financial_imports_inferred"], false);
        assert_eq!(
            publish(State(s.clone()), Json(input()))
                .await
                .ok()
                .unwrap()
                .0["idempotent_replay"],
            true
        );
        let mut replacement = input();
        replacement.capture.companies[0].company_name = "Synthetic revised".into();
        assert_eq!(
            publish(State(s.clone()), Json(replacement))
                .await
                .unwrap_err()
                .0,
            409
        );
        assert!(s.store.get("gl:company").await.unwrap().is_none());
        assert!(
            s.store
                .get(&CompanyBooks::key("synthetic-a", "head", "v1").ok().unwrap())
                .await
                .unwrap()
                .is_none()
        );
        assert_eq!(
            list(State(s.clone())).await.ok().unwrap().0["head_sha256"],
            first["head_sha256"]
        );
        s.store
            .set(
                &format!(
                    "company-books:roster:capture:{}",
                    first["head_sha256"].as_str().unwrap()
                ),
                "{}",
            )
            .await
            .unwrap();
        assert_eq!(list(State(s)).await.unwrap_err().0, 500);
    }
    #[test]
    fn duplicate_aliases_and_unknown_fields_are_rejected() {
        let mut p = input();
        p.capture.companies.push(p.capture.companies[0].clone());
        assert!(validate(&p.capture).is_err());
        let mut raw = serde_json::to_value(input().capture).unwrap();
        raw["financially_verified"] = json!(true);
        assert!(serde_json::from_value::<Capture>(raw).is_err());
    }
}
