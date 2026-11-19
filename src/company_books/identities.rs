//! Identity-only directory reads. Financial inventory and book reads keep their
//! existing full validation. Supabase hashes complete immutable preview bytes in
//! the database and returns only the fields the directory actually needs.
use super::*;
impl CompanyBooks {
    pub async fn list_identity_page(&self, offset:usize, limit:usize)->Result<Value>{
        if limit==0||limit>500{return Err(bad("Identity page limit must be 1-500"));}
        let mut ids=self.store.smembers(INDEX).await.map_err(storage)?;
        if ids.len()>500{return Err(corrupt());}ids.sort();let total=ids.len();
        let ids=ids.into_iter().skip(offset).take(limit).collect::<Vec<_>>();
        let mut keys=vec![];for id in &ids{keys.push(Self::key(id,"head","v1")?);keys.push(Self::key(id,"latest-preview","v1")?);}
        let pointers=self.store.mget(&keys).await.map_err(storage)?;
        if pointers.len()!=keys.len(){return Err(corrupt());}
        let mut entries=vec![];
        for(i,id)in ids.iter().enumerate(){
            let head=&pointers[i*2];let latest=&pointers[i*2+1];
            if head.as_ref().is_some_and(|v|!digest(v))||(head.is_none()&&latest.as_ref().is_some_and(|v|!digest(v))){return Err(corrupt());}
            entries.push(json!({"company_id":id,"head_sha256":head,"latest_preview_id":latest}));
        }
        let page=json!({"companies":entries,"total":total,"has_more":offset.saturating_add(ids.len())<total});
        let selected=page["companies"].as_array().ok_or_else(corrupt)?;
        let mut heads=BTreeMap::new();
        let receipts:Vec<_>=selected.iter().filter_map(|r|Some((r["company_id"].as_str()?.to_string(),r["head_sha256"].as_str()?.to_string()))).collect();
        for group in receipts.chunks(8){
            let keys=group.iter().map(|(c,h)|Self::key(c,"receipt",h)).collect::<Result<Vec<_>>>()?;
            let values=self.store.mget(&keys).await.map_err(storage)?;
            if values.len()!=keys.len(){return Err(corrupt());}
            for ((company,hid),raw) in group.iter().zip(values){
                let raw=raw.ok_or_else(corrupt)?;
                if raw.len()>512*1024||hash(raw.as_bytes())!=*hid{return Err(corrupt());}
                let head:Head=serde_json::from_str(&raw).map_err(|_|corrupt())?;
                if encoded(&head)?!=raw||head.company_id!=*company||!matches!(head.format.as_str(),"nxt-company-book-head/v1"|"nxt-company-book-head/v2")||head.revision==0||head.revision>129||!digest(&head.preview_id){return Err(corrupt());}
                heads.insert(company.clone(),head);
            }
        }
        let mut refs=vec![];
        for row in selected {
            let company=row["company_id"].as_str().ok_or_else(corrupt)?.to_string();
            let pid=if let Some(head)=heads.get(&company){head.preview_id.clone()}else if let Some(p)=row["latest_preview_id"].as_str(){p.to_string()}else{
                let mut older=self.store.smembers(&Self::key(&company,"previews","v1")?).await.map_err(storage)?;older.sort();older.pop().ok_or_else(corrupt)?
            };
            if !digest(&pid){return Err(corrupt());}refs.push((company,pid));
        }
        let mut companies=vec![];
        for group in refs.chunks(100){
            let keys=group.iter().map(|(c,p)|Self::key(c,"preview",p)).collect::<Result<Vec<_>>>()?;
            let values=self.store.company_identity_previews(&keys).await.map_err(storage)?;
            if values.len()!=group.len(){return Err(corrupt());}
            for ((company,pid),value) in group.iter().zip(values){
                let value=value.ok_or_else(corrupt)?;
                let (identity,passed)=if let Some(raw)=value["raw"].as_str(){
                    if raw.len()>4*1024*1024||hash(raw.as_bytes())!=*pid{return Err(corrupt());}
                    let p:Preview=serde_json::from_str(raw).map_err(|_|corrupt())?;
                    if encoded(&p)?!=raw||p.company_id!=*company||p.format!="nxt-company-book-preview/v1"{return Err(corrupt());}
                    (p.input.company,p.analysis.controls_passed)
                }else{
                    if value["sha256"]!=*pid||value["company_id"]!=*company||value["format"]!="nxt-company-book-preview/v1"||value["bytes"].as_u64().is_none_or(|n|n>4*1024*1024){return Err(corrupt());}
                    let identity:Company=serde_json::from_value(value["company"].clone()).map_err(|_|corrupt())?;
                    if !text(&identity.name,300)||!text(&identity.source_company_id,200)||identity.currency.len()!=3||!identity.currency.bytes().all(|b|b.is_ascii_uppercase()){return Err(corrupt());}
                    (identity,value["controls_passed"].as_bool().ok_or_else(corrupt)?)
                };
                let head=heads.get(company);
                if head.is_some_and(|h|!self.review_ok(h))&&!passed{return Err(corrupt());}
                companies.push(json!({"company_id":company,"stage":if head.is_some(){"import_committed"}else{"staged"},"head":{"archived":head.is_some_and(|h|matches!(h.operation,Some(Operation::Archive{..})))},"previews":[{"company":identity}]}));
            }
        }
        Ok(json!({"companies":companies,"total":page["total"],"has_more":page["has_more"],"offset":offset,"limit":limit}))
    }
}
