use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
// Original source cell nulls must be explicit; absence is a schema error.
fn required_nullable<'de,D:Deserializer<'de>>(d:D)->Result<Option<String>,D::Error>{Option::<String>::deserialize(d)}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind { Customer, Supplier }
impl<'de> Deserialize<'de> for Kind {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self,D::Error> {
        match String::deserialize(d)?.as_str() {
            "customer"=>Ok(Self::Customer), "supplier"=>Ok(Self::Supplier),
            _=>Err(serde::de::Error::custom("Unsupported source contact kind"))
        }
    }
}
impl Kind {
    pub fn label(&self)->&'static str { match self {Self::Customer=>"customer",Self::Supplier=>"supplier"} }
    pub fn member(&self)->&'static str { match self {Self::Customer=>"Customers.xlsx",Self::Supplier=>"Suppliers.xlsx"} }
    pub fn fields(&self)->[(&'static str,&'static str);6] {
        match self {Self::Customer=>[("B","display_name"),("C","phone_numbers"),("D","email"),("E","full_name"),("F","billing_address"),("G","shipping_address")],
            Self::Supplier=>[("B","display_name"),("C","phone_numbers"),("D","email"),("E","full_name"),("F","address"),("G","supplier_account_number")]}
    }
    pub fn headers(&self)->BTreeMap<String,String> {
        let names=match self {Self::Customer=>["Customer","Phone Numbers","Email","Full Name","Billing Address","Shipping Address"],Self::Supplier=>["Supplier","Phone Numbers","Email","Full Name","Address","Account #"]};
        ["B","C","D","E","F","G"].into_iter().zip(names).map(|(a,b)|(a.into(),b.into())).collect()
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cell {pub present:bool,#[serde(rename="type",deserialize_with="required_nullable")]pub cell_type:Option<String>,#[serde(deserialize_with="required_nullable")]pub value:Option<String>,#[serde(deserialize_with="required_nullable")]pub formula:Option<String>}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub archive_sha256:String,pub archive_bytes:usize,pub report_sha256:String,pub member:String,
    pub worksheet_name:String,pub worksheet_file:String,pub company_heading_sha256:String,
    pub captured_at:String,pub capture_basis:String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    pub cohort_ref:String,pub record_type:Kind,pub source_record_alias:String,#[serde(deserialize_with="required_nullable")]pub source_id:Option<String>,
    pub source_id_coverage:String,pub source_identity_verified:bool,pub operational_object_created:bool,
    pub linked_to_journal_contacts:bool,pub formula_fields_present:bool,
    pub source_archive_sha256:String,pub source_report_sha256:String,pub source_member:String,
    pub source_worksheet_name:String,pub source_worksheet_file:String,pub source_row:u32,
    pub source_fields:BTreeMap<String,Cell>,pub raw_source_cells:BTreeMap<String,Cell>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OtherRow {pub source_row:u32,pub classification:String,pub raw_cells:BTreeMap<String,Cell>}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {pub format:String,pub company_id:String,pub kind:Kind,pub source:Source,pub records:Vec<Record>,pub other_source_rows:Vec<OtherRow>}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartRef {pub sha256:String,pub bytes:usize,pub records:usize,pub other_rows:usize}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Family {
    pub format:String,pub company_id:String,pub kind:Kind,pub source:Source,pub headers:BTreeMap<String,String>,
    pub parts:Vec<PartRef>,pub records_total:usize,pub nonrecord_rows_total:usize,
    pub source_snapshot_id:String,pub source_manifest_sha256:String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperatorProof {pub format:String,pub company_id:String,pub source_archive_sha256:String,pub source_report_sha256:String,pub family_sha256:String,pub comparison_manifest_sha256:String,pub source_field_equality_verified:bool}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Publish {pub request_id:String,pub expected_index_head:Option<String>,pub family:Family,pub source_operator_proof:OperatorProof}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {pub format:String,pub company_id:String,pub command:Publish,pub command_sha256:String}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Reservation {pub sha256:String,pub bytes:usize}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Slot {
    pub request_id:String,pub command_sha256:String,pub receipt_sha256:String,pub receipt_bytes:usize,
    pub family_sha256:String,pub kind:Kind,pub records_total:usize,pub nonrecord_rows_total:usize,
    pub published:bool,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct State {pub parts:Vec<Reservation>,pub slots:Vec<Slot>,pub published:Vec<String>,pub charged_bytes:usize}
