//! Schema-driven catalog for the independent native workpaper engine.
use serde_json::{json, Value};
use super::calculations::{sources, RULESET, T1_SCOPE, T2_SCOPE, T3_SCOPE, T5013_SCOPE};

fn field(key:&str,label:&str,kind:&str,default:Value,help:&str)->Value {
    json!({"key":key,"label":label,"kind":kind,"default":default,"help":help})
}
fn money(key:&str,label:&str,help:&str)->Value {field(key,label,"money",json!(0),help)}
fn yes(key:&str,label:&str,help:&str)->Value {field(key,label,"boolean",json!(false),help)}
fn section(id:&str,label:&str,fields:Vec<Value>)->Value {json!({"id":id,"label":label,"fields":fields})}
fn scope_fields(scope:&str)->Value {section("scope","Calculation scope",vec![
    yes("scope_confirmed","I verified that this case fits the stated calculation scope",scope),
    yes("other_tax_situations","There are additional income categories, credits or special tax situations","Selecting this prevents supported review; missing rules must be implemented and independently verified."),
])}
fn program(id:&str,label:&str,description:&str,jurisdiction:&str,scope:&str,sections:Vec<Value>)->Value {
    let mut defaults=serde_json::Map::new();let mut properties=serde_json::Map::new();
    for section in &sections {for field in section["fields"].as_array().expect("static catalog fields") {
        let key=field["key"].as_str().expect("static catalog key");defaults.insert(key.into(),field["default"].clone());
        let kind=field["kind"].as_str().expect("static catalog kind");
        let schema=match kind {"money"=>json!({"type":"integer","minimum":0,"maximum":100_000_000_000_000_i64,"unit":"CAD cents"}),"number"=>json!({"type":"integer"}),"boolean"=>json!({"type":"boolean"}),"json"=>json!({"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","name","share_bps"],"properties":{"id":{"type":"string"},"name":{"type":"string"},"share_bps":{"type":"integer","minimum":0,"maximum":10000}}}}),_=>json!({"type":"string"})};
        properties.insert(key.into(),schema);
    }}
    json!({"id":id,"label":label,"description":description,"years":[2025],"jurisdictions":[jurisdiction],"scope":scope,"ruleset":RULESET,"filing_supported":false,"sections":sections,"defaults":defaults,"input_schema":{"type":"object","additionalProperties":false,"properties":properties},"sources":sources(id)})
}

pub fn catalog()->Value {
    let t1=program("T1","T1 — Personal","Federal and Alberta ordinary employment/interest income-tax workpaper.","AB",T1_SCOPE,vec![
        scope_fields(T1_SCOPE),
        section("residence","Residency",vec![yes("full_year_resident","Full-year Canadian resident; Alberta resident at year-end","Adult age 18–64 in this scope. Newcomer, emigrant, deemed resident, deceased, bankruptcy, spouse/dependant and age-credit cases require additional rules.")]),
        section("income","Income",vec![money("employment_income_cents","T4 employment income","Ordinary Canadian employment; dollars in the editor, integer cents in the API. Standard insurable employment only; foreign employment and self-employment are excluded."),money("interest_income_cents","Canadian interest income","Ordinary Canadian-source interest only. Dividends, capital gains and foreign income are excluded.")]),
        section("deductions","Verified deductions",vec![money("rrsp_deduction_cents","RRSP deduction claimed","A deductible amount verified against receipts and the available CRA deduction limit; contribution room and carryforwards are not calculated."),money("rrsp_deduction_limit_cents","Verified available RRSP deduction limit","This is the verified available limit, not an automatic 18% contribution-room calculation."),yes("rrsp_verified","RRSP deduction and available limit verified","Required whenever an RRSP deduction is claimed."),money("cpp_enhanced_deduction_cents","Verified enhanced CPP deduction — line 22215","Schedule 8/RC381 amount, maximum $1,074.00 for 2025. Do not enter the entire T4 box 16 amount.")]),
        section("payroll","Verified payroll credits",vec![money("cpp_base_credit_cents","Verified base CPP credit amount — line 30800","Schedule 8 amount, maximum $3,356.10. Enhanced contributions are deductions, not part of this credit."),money("ei_credit_cents","Verified eligible EI credit — line 31200","Maximum $1,077.48; standard insurable earnings must exceed $2,000. Overpayments and Quebec employment are outside scope."),yes("payroll_verified","CPP/EI amounts verified against Schedule 8 and source documents","Required for any CPP/EI credit or enhanced CPP deduction. This engine does not infer contributions or refunds from wages.")]),
        section("payments","Payments",vec![money("tax_withheld_cents","Income tax withheld","Verified income tax withheld; no automatic payroll calculation."),money("instalments_cents","Income tax instalments paid","Verified payments only. Reported balance excludes refundable benefits and all other return amounts.")]),
    ]);
    let mut limit=money("business_limit_cents","Annual business limit before short-year proration","Maximum $500,000. Only a verified non-associated, unassigned business limit is supported.");limit["default"]=json!(50_000_000);
    let mut days=field("fiscal_days","Days in fiscal period","number",json!(365),"1–366. A year shorter than 51 weeks (357 days) prorates the limit using days/365; 357–366 days uses the full limit.");days["min"]=json!(1);days["max"]=json!(366);
    let t2=program("T2","T2 — Corporation","Federal and Alberta ordinary active-business tax calculation from verified taxable income.","AB",T2_SCOPE,vec![
        scope_fields(T2_SCOPE),
        section("income","Verified tax income",vec![money("taxable_income_cents","Federal and Alberta taxable income","Already reconciled taxable income, equal federally and provincially. No book-to-tax, CCA, investment, foreign or loss schedule is calculated."),money("active_business_income_cents","Active-business income eligible for ordinary SBD rules","For the scoped SBD case, must equal taxable income; personal-services business and specified partnership income are excluded."),days]),
        section("sbd","Small business eligibility",vec![yes("small_business_eligible","Claim ordinary CCPC small business rates","Applies federal 9% and Alberta 2% to eligible income; remaining ordinary active-business income uses 15% and 8%."),yes("sbd_eligibility_verified","CCPC status, eligibility and both business limits verified","Required for SBD. Confirm CCPC throughout year, eligible ABI, no assignments/associations/special industry and no other tax adjustments."),limit,money("taxable_capital_cents","Prior-year taxable capital employed in Canada","Above $10 million requires an unimplemented limit reduction and blocks supported review."),money("passive_income_cents","Prior-year adjusted aggregate investment income","Above $50,000 requires an unimplemented federal limit reduction and blocks supported review. Current-year investment income is outside scope."),yes("associated_corporations","Associated corporations or shared business limits exist","Association/assignment/group schedules are outside the initial native calculation scope.")]),
        section("payments","Payments",vec![money("tax_paid_cents","Federal and Alberta tax payments made","Combined verified payments; provincial AT1 remains a separate filing. No instalment schedule is computed.")]),
    ]);
    let mut trust=field("trust_type","Trust classification","choice",json!("ordinary"),"Ordinary trust: federal 33%. Verified GRE/QDT: graduated federal rates. No basic personal amount applies to trusts.");
    trust["choices"]=json!([{"value":"ordinary","label":"Ordinary trust"},{"value":"gre","label":"Graduated rate estate (GRE)"},{"value":"qdt","label":"Qualified disability trust (QDT)"}]);
    let t3=program("T3","T3 — Trust","Federal-only ordinary retained-income calculation and beneficiary deduction workpaper.","CA-FED",T3_SCOPE,vec![
        scope_fields(T3_SCOPE),section("classification","Classification",vec![trust,yes("classification_verified","GRE/QDT eligibility and elections verified","Required for GRE/QDT. No automatic 36-month, testamentary, beneficiary disability, recovery tax or AMT determination.")]),
        section("income","Ordinary income and deductions",vec![money("income_cents","Ordinary trust income","Canadian ordinary income only. Capital gains, dividends, foreign income and special trusts require additional rules."),money("deductible_expenses_cents","Verified deductible expenses","Deductible current-year expenses only. Losses and carryforwards are excluded."),money("beneficiary_allocations_cents","Deductible ordinary-income allocations to beneficiaries","Verify legal entitlement and deductibility. Does not generate beneficiary slips or classify allocations. Separate routing parties do not change this amount.")]),
        section("payments","Federal payments",vec![money("tax_paid_cents","Federal tax payments made","Federal only. The result excludes provincial tax and is not a complete trust balance owing.")]),
    ]);
    let partners=field("partners","Partner allocations","json",json!([{"id":"partner-1","name":"Partner 1","share_bps":5000},{"id":"partner-2","name":"Partner 2","share_bps":5000}]),"JSON array: unique id, name, share_bps (integer basis points). Shares total 10000. 1–100 general partners; largest remainders receive residual cents, stable ID breaks ties. This array alone drives allocation; routing parties are separate.");
    let t5013=program("T5013","T5013 — Partnership","Ordinary-income reconciliation and exact partner allocation workpaper.","CA",T5013_SCOPE,vec![
        scope_fields(T5013_SCOPE),section("income","Income reconciliation",vec![money("revenue_cents","Ordinary business revenue","Domestic general partnership only; no SIFT, limited/at-risk, resource, foreign or special income categories."),money("expenses_cents","Book expenses","Book net income/loss = revenue less expenses."),money("addbacks_cents","Verified non-deductible expense addbacks","Verified ordinary-income tax reconciliation amounts, such as non-deductible book expenses."),money("deductions_cents","Verified additional tax deductions","Verified ordinary deductions only. CCA, loss restrictions, ACB and special allocations are not calculated.")]),
        section("partners","Partner agreement",vec![partners,yes("allocation_verified","General-partner status, agreement shares and tax adjustments verified","No limited partnership, at-risk, non-resident or special allocation rules. Tax is calculated on partners' own returns, not as ordinary partnership tax here.")]),
    ]);
    let mut doc=json!({"ruleset":RULESET,"engine":"independent_nxt","money_unit":"CAD cents","rounding":"integer arithmetic; nearest cent, half away from zero; partnership largest-remainder allocation","filing_supported":false,"certified":false,"sdk_runtime_required":false,"programs":[t1,t2,t3,t5013]});
    doc["programs"][0]["jurisdictions"]=json!(["AB","ON"]);
    doc["programs"][0]["ontario_scope"]=json!(super::calculations::T1_ON_SCOPE);
    doc
}

#[cfg(test)]mod tests {
    use super::*;use super::super::calculations::calculate;
    #[test]fn catalog_defaults_match_engine_inputs_and_never_claim_filing(){
        let c=catalog();assert_eq!(c["filing_supported"],false);assert_eq!(c["certified"],false);
        for p in c["programs"].as_array().unwrap(){let result=calculate(p["id"].as_str().unwrap(),2025,p["jurisdictions"][0].as_str().unwrap(),&p["defaults"]).unwrap();assert!(!result.supported);assert!(!p["sources"].as_array().unwrap().is_empty());let fields:usize=p["sections"].as_array().unwrap().iter().map(|s|s["fields"].as_array().unwrap().len()).sum();assert_eq!(p["defaults"].as_object().unwrap().len(),fields);}
    }
}
