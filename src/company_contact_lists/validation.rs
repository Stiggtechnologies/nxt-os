use super::{models::*,*};
use std::collections::HashSet;
pub(super) fn source(v:&Source,k:&Kind)->Result<()> {
    if !digest(&v.archive_sha256)||!digest(&v.report_sha256)||!digest(&v.company_heading_sha256)
        ||v.archive_bytes==0||v.archive_bytes>64*1024*1024||!text(&v.member,256)
        ||v.member.starts_with('/')||v.member.contains('\\')||v.member.split('/').any(|p|p==".."||p.is_empty())
        ||v.member.rsplit('/').next()!=Some(k.member())||!text(&v.worksheet_name,256)
        ||!v.worksheet_file.starts_with("xl/worksheets/")||!v.worksheet_file.ends_with(".xml")
        ||v.worksheet_file.contains("..")||v.worksheet_file.contains('\\')||v.worksheet_file.len()>256
        ||chrono::DateTime::parse_from_rfc3339(&v.captured_at).is_err()||!text(&v.capture_basis,500) {return Err(bad("Invalid original source contact metadata"));}
    Ok(())
}
fn cell(v:&Cell)->bool {
    if !v.present{return v.cell_type.is_none()&&v.value.is_none()&&v.formula.is_none()}
    v.cell_type.as_deref().is_some_and(|t|matches!(t,"n"|"s"|"str"|"inlineStr"))
        &&v.value.as_ref().is_some_and(|s|s.len()<=8192)
        &&v.formula.as_ref().is_none_or(|s|s.len()<=8192)
}
pub(super) fn part(company:&str,p:&Part)->Result<()> {
    if !id(company)||p.company_id!=company||p.format!="nxt-company-contact-part/v1"||p.records.len()>20000||p.other_source_rows.len()>2048
        ||p.records.is_empty()&&p.other_source_rows.is_empty()||encoded(p)?.len()>PART_MAX {return Err(bad("Invalid or oversized contact part"));}
    source(&p.source,&p.kind)?;let mut rows=HashSet::new();let mut last=0;
    for r in &p.records {
        let alias=format!("{}_{}",p.kind.label(),hash(encoded(&serde_json::json!([company,p.source.archive_sha256,p.source.report_sha256,p.source.worksheet_file,r.source_row]))?.as_bytes()));
        let columns=p.kind.fields();
        if r.cohort_ref!=company||r.record_type!=p.kind||r.source_record_alias!=alias||r.source_id.is_some()
            ||r.source_id_coverage!="archive_row"||r.source_identity_verified||r.operational_object_created||r.linked_to_journal_contacts
            ||r.source_archive_sha256!=p.source.archive_sha256||r.source_report_sha256!=p.source.report_sha256
            ||r.source_member!=p.source.member||r.source_worksheet_name!=p.source.worksheet_name||r.source_worksheet_file!=p.source.worksheet_file
            ||r.source_row<=5||r.source_row>1048576||r.source_row<=last||!rows.insert(r.source_row)
            ||r.source_fields.len()!=6||r.raw_source_cells.len()<6||r.raw_source_cells.len()>7
            ||r.raw_source_cells.iter().any(|(k,v)|!matches!(k.as_str(),"A"|"B"|"C"|"D"|"E"|"F"|"G")||!cell(v))
            ||columns.iter().any(|(col,key)|r.raw_source_cells.get(*col).is_none()||r.source_fields.get(*key)!=r.raw_source_cells.get(*col))
            ||r.formula_fields_present!=r.raw_source_cells.values().any(|c|c.formula.is_some()) {return Err(bad("Source contact row/identity/field fidelity differs"));}
        last=r.source_row;
    }
    for r in &p.other_source_rows {
        if r.source_row<=5||r.source_row>1048576||!rows.insert(r.source_row)||r.classification!="retained_nonrecord_or_unclassified_row"
            ||r.raw_cells.len()<6||r.raw_cells.len()>7||p.kind.fields().iter().any(|(col,_)|!r.raw_cells.contains_key(*col))||r.raw_cells.iter().any(|(k,v)|!matches!(k.as_str(),"A"|"B"|"C"|"D"|"E"|"F"|"G")||!cell(v)) {return Err(bad("Retained original rows overlap or differ"));}
    }Ok(())
}
pub(super) fn family(company:&str,f:&Family)->Result<()> {
    if !id(company)||f.company_id!=company||f.format!="nxt-company-contact-family/v2"||f.headers!=f.kind.headers()
        ||f.parts.len()>256||f.records_total>20000||f.nonrecord_rows_total>2048||!id(&f.source_snapshot_id)||!digest(&f.source_manifest_sha256)
        ||encoded(f)?.len()>META_MAX {return Err(bad("Invalid complete contact family"));}
    source(&f.source,&f.kind)?;let mut seen=HashSet::new();let mut bytes=0;let mut rows=0;let mut other=0;
    for r in &f.parts {
        if !digest(&r.sha256)||!seen.insert(&r.sha256)||r.bytes==0||r.bytes>PART_MAX||r.records>20000||r.other_rows>2048 {return Err(bad("Invalid or duplicated contact part reference"));}
        bytes+=r.bytes;rows+=r.records;other+=r.other_rows;
    }
    if bytes>BOOK_MAX||rows!=f.records_total||other!=f.nonrecord_rows_total||f.parts.is_empty()&&(rows!=0||other!=0) {return Err(bad("Complete contact family source coverage differs"));}Ok(())
}
pub(super) fn command(company:&str,p:&Publish)->Result<()> {
    family(company,&p.family)?;let proof=&p.source_operator_proof;
    if !id(&p.request_id)||p.expected_index_head.as_ref().is_some_and(|s|!digest(s))
        ||proof.format!="nxt-source-contact-operator-equality/v1"||proof.company_id!=company
        ||proof.source_archive_sha256!=p.family.source.archive_sha256||proof.source_report_sha256!=p.family.source.report_sha256
        ||proof.family_sha256!=hash(encoded(&p.family)?.as_bytes())||!digest(&proof.comparison_manifest_sha256)||!proof.source_field_equality_verified {return Err(bad("Source operator equality proof or request binding differs"));}Ok(())
}
