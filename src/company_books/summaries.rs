//! Lightweight company inventory for list screens and the company switcher.
//! Reads the index and the head / latest-preview pointers in one batch; it never
//! loads preview, receipt or journal blobs (those can be megabytes each).
use super::*;

pub const MAX_SUMMARIES: usize = 500;

impl CompanyBooks {
    /// One page of `{company_id, stage, head_sha256}` rows: one SMEMBERS plus one MGET.
    pub async fn list_summaries(&self, offset: usize, limit: usize) -> Result<Value> {
        if limit == 0 || limit > MAX_SUMMARIES {
            return Err(bad("Summary page limit must be 1-500"));
        }
        let mut ids = self.store.smembers(INDEX).await.map_err(storage)?;
        if ids.len() > MAX_SUMMARIES {
            return Err(corrupt());
        }
        ids.retain(|id| valid_id(id));
        ids.sort();
        let total = ids.len();
        let selected: Vec<String> = ids.into_iter().skip(offset).take(limit).collect();
        let mut keys = Vec::with_capacity(selected.len() * 2);
        for id in &selected {
            keys.push(Self::key(id, "head", "v1")?);
            keys.push(Self::key(id, "latest-preview", "v1")?);
        }
        let pointers = self.store.mget(&keys).await.map_err(storage)?;
        if pointers.len() != keys.len() {
            return Err(corrupt());
        }
        let mut companies = Vec::with_capacity(selected.len());
        for (i, id) in selected.iter().enumerate() {
            let head = pointers[i * 2].as_ref().filter(|h| digest(h));
            let latest = pointers[i * 2 + 1].as_ref().filter(|p| digest(p));
            companies.push(json!({
                "company_id": id,
                "stage": if head.is_some() { "import_committed" } else { "staged" },
                "head_sha256": head,
                "latest_preview_id": latest,
            }));
        }
        let next = offset.saturating_add(companies.len());
        Ok(json!({
            "companies": companies,
            "total": total,
            "offset": offset,
            "limit": limit,
            "has_more": next < total,
            "integrity_check": "pointers_only; open a company for validated book data",
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed(n: usize) -> CompanyBooks {
        let s = CompanyBooks::new(Store::memory());
        for i in 0..n {
            let id = format!("example-co-{i:03}");
            s.store.sadd(INDEX, &id).await.unwrap();
            s.store.set(&format!("company-books:{id}:latest-preview:v1"), &"a".repeat(64)).await.unwrap();
            // A deliberately huge preview blob that a list must never read.
            s.store.set(&format!("company-books:{id}:preview:{}", "a".repeat(64)), &"x".repeat(64 * 1024)).await.unwrap();
            if i % 2 == 0 {
                s.store.set(&format!("company-books:{id}:head:v1"), &"b".repeat(64)).await.unwrap();
            }
        }
        s
    }

    #[tokio::test]
    async fn list_summaries_never_reads_preview_keys_and_is_two_round_trips() {
        let s = seed(120).await;
        s.store.take_test_reads();
        let v = s.list_summaries(0, 500).await.ok().unwrap();
        let reads = s.store.take_test_reads();
        assert_eq!(reads.len(), 2, "one SMEMBERS + one MGET, independent of company count");
        assert!(reads.iter().flatten().all(|k| !k.contains(":preview:") && !k.contains(":receipt:")));
        assert_eq!(v["total"], 120);
        assert_eq!(v["companies"].as_array().unwrap().len(), 120);
        assert_eq!(v["companies"][0]["company_id"], "example-co-000");
        assert_eq!(v["companies"][0]["stage"], "import_committed");
        assert_eq!(v["companies"][1]["stage"], "staged");
        let page = s.list_summaries(100, 50).await.ok().unwrap();
        assert_eq!(page["companies"].as_array().unwrap().len(), 20);
        assert_eq!(page["has_more"], false);
        assert!(s.list_summaries(0, 0).await.is_err());
        assert!(s.list_summaries(0, 501).await.is_err());
    }
}
