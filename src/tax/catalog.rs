//! Schema-driven catalog for the independent native workpaper engine.
use serde_json::{json, Value};
use super::calculations::{sources, RULESET, T1_SCOPE, T2_SCOPE, T3_SCOPE, T5013_SCOPE};

fn field(key:&str,label:&str,kind:&str,default:Value,help:&str)->Value {
    json!({"key":key,"label":label,"kind":kind,"default":default,"help":help})
}
fn money(key:&str,label:&str,help:&str)->Value {field(key,label,"money",json!(0),help)}
fn yes(key:&str,label:&str,help:&str)->Value {field(key,label,"boolean",json!(false),help)}
pub const T1_FIELD_CATALOG_VERSION:&str="nxt-tax-fields/T1/2025/4";
fn optional(key:&str,label:&str,kind:&str,verify:&str,form:&str,help:&str)->Value {
    let mut f=json!({"key":key,"label":label,"kind":kind,"optional":true,"default_policy":"omit","blank_policy":"delete_property","tax_years":[2025],"jurisdictions":super::coverage::entry_jurisdictions("T1",2025),"verification_key":verify,"source_form":form,"help":help});
    if kind=="money"{f["min"]=json!(0);f["max"]=json!(100_000_000_000_000_i64);}f
}
fn section(id:&str,label:&str,fields:Vec<Value>)->Value {json!({"id":id,"label":label,"fields":fields})}
fn scope_fields(scope:&str)->Value {section("scope","Calculation scope",vec![
    yes("scope_confirmed","I verified that this case fits the stated calculation scope",scope),
    yes("other_tax_situations","There are additional income categories, credits or special tax situations","Selecting this prevents supported review; missing rules must be implemented and independently verified."),
])}
fn program(id:&str,label:&str,description:&str,jurisdiction:&str,scope:&str,sections:Vec<Value>)->Value {
    let mut defaults=serde_json::Map::new();let mut properties=serde_json::Map::new();
    for section in &sections {for field in section["fields"].as_array().expect("static catalog fields") {
        let key=field["key"].as_str().expect("static catalog key");if field["optional"]!=true{defaults.insert(key.into(),field["default"].clone());}
        let kind=field["kind"].as_str().expect("static catalog kind");
        let schema=match kind {"money"=>json!({"type":"integer","minimum":0,"maximum":100_000_000_000_000_i64,"unit":"CAD cents"}),"number"=>json!({"type":"integer"}),"boolean"=>json!({"type":"boolean"}),"json"=>json!({"type":"array","items":{"type":"object","additionalProperties":false,"required":["id","name","share_bps"],"properties":{"id":{"type":"string"},"name":{"type":"string"},"share_bps":{"type":"integer","minimum":0,"maximum":10000}}}}),_=>json!({"type":"string"})};
        properties.insert(key.into(),schema);
    }}
    json!({"id":id,"label":label,"description":description,"years":super::coverage::entry_years(id),"jurisdictions":super::coverage::entry_jurisdictions(id,2025),"default_jurisdiction":jurisdiction,"entry_schema_id":super::coverage::entry_schema_id(id),"entry_schema_year":2025,"scope":scope,"ruleset":RULESET,"filing_supported":false,"sections":sections,"defaults":defaults,"input_schema":{"type":"object","additionalProperties":false,"properties":properties},"sources":sources(id)})
}

fn t2125_expense_fields() -> Vec<Value> {
    let mut fields = vec![optional(
        super::t2125::CONFIRMATION, "Verify sole-proprietor business and expense breakdown", "boolean",
        super::t2125::CONFIRMATION, "T2125",
        "Confirm a single ordinary sole proprietorship with verified adjusted gross revenue and deductible expenses. Inventory, partnership, professional or commission reporting, rebates, reserves and business-use-of-home adjustments are outside this itemized path.",
    )];
    for expense in super::t2125::EXPENSES {
        fields.push(optional(
            expense.key, &format!("{} (line {})", expense.label, expense.line), "money",
            super::t2125::CONFIRMATION, "T2125",
            "Enter the verified deductible business portion. Apply applicable limits or adjustments before entering the amount.",
        ));
    }
    fields
}

/// Reference-only DT Max 2025 keyword names for the gross and net T1 lines a field feeds.
fn dtmax(mut field: Value, gross: &str, net: &str) -> Value {
    field["dtmax_keywords"] = json!({"gross": gross, "net": net, "mapping": "reference_only"});
    field
}

fn self_employment_activity_fields() -> Vec<Value> {
    let mut fields = vec![];
    for a in super::t2125::ACTIVITIES {
        let scope = match a.id {
            "professional" => "Self-employed professional fees reported on a separate T2125 (Part 3B), including work-in-progress where applicable.",
            "commission" => "Self-employed commission income reported on T2125. Commissions on a T4 slip are employment income, not this line.",
            "farming" => "Verified T2042 totals for a sole-proprietor farm. Confirm no optional or mandatory inventory adjustment (lines 9941/9942), partnership share or AgriStability/AgriInvest item applies; a farming loss blocks review.",
            _ => "Verified T2121 totals for a sole-proprietor fishing operation. Partnership shares and other adjustments require separate preparation.",
        };
        fields.extend([
            dtmax(optional(a.gross_key, &format!("Verified {} gross {} — line {}", a.form, a.label, a.gross_line), "money", a.verify_key, a.form, scope), a.dtmax_gross, a.dtmax_net),
            optional(a.expenses_key, &format!("Verified {} deductible current expenses ({})", a.form, a.label), "money", a.verify_key, a.form, "Deductible current expenses from the records, excluding separately entered CCA. Apply limits such as the meals and vehicle rules before entering."),
            optional(a.cca_key, &format!("Verified {} CCA claim ({})", a.form, a.label), "money", a.verify_key, a.form, "Externally computed CCA; NXT does not compute CCA classes or dispositions. CCA cannot create or increase a loss."),
            optional(a.verify_key, &format!("{} {} totals and claim eligibility verified", a.form, a.label), "boolean", a.verify_key, a.form, &format!("Confirm only after checking the source totals and deductions. Net income goes to line {}. This is not automatic verification or a complete {}.", a.net_line, a.form)),
        ]);
    }
    fields
}

pub fn catalog()->Value {
    let mut t1=program("T1","T1 — Personal","Selected 2025 workpapers, including optional verified business, professional, commission, farming, fishing, rental and current-year credit totals. Read each section's limits; these are not complete schedules or a filing return.","AB",T1_SCOPE,vec![
        scope_fields(T1_SCOPE),
        section("residence","Residency",vec![yes("full_year_resident","Full-year Canadian resident; resident of the selected province at year-end","Adult age 18–64 in this scope. Newcomer, emigrant, deemed resident, deceased, bankruptcy, spouse/dependant and age-credit cases require additional rules."),yes("provincial_adjustments_not_applicable","Provincial reductions/credits not calculated do not apply (BC, SK, MB, NB, NS, PE, NL, YT, NT, NU only)","Required for the generic provincial engine; the calculation diagnostics list what is not calculated for the chosen province. Ignored for AB and ON.")]),
        section("income","Income",vec![money("employment_income_cents","T4 employment income","Ordinary Canadian employment; dollars in the editor, integer cents in the API. Standard insurable employment only; foreign employment and self-employment are excluded."),money("interest_income_cents","Canadian interest income","Ordinary Canadian-source interest only. Dividends, capital gains and foreign income are excluded.")]),
        section("deductions","Verified deductions",vec![money("rrsp_deduction_cents","RRSP deduction claimed","A deductible amount verified against receipts and the available CRA deduction limit; contribution room and carryforwards are not calculated."),money("rrsp_deduction_limit_cents","Verified available RRSP deduction limit","This is the verified available limit, not an automatic 18% contribution-room calculation."),yes("rrsp_verified","RRSP deduction and available limit verified","Required whenever an RRSP deduction is claimed."),money("cpp_enhanced_deduction_cents","Verified enhanced CPP deduction — line 22215","Schedule 8/RC381 amount, maximum $1,074.00 for 2025. Do not enter the entire T4 box 16 amount.")]),
        section("payroll","Verified payroll credits",vec![money("cpp_base_credit_cents","Verified base CPP credit amount — line 30800","Schedule 8 amount, maximum $3,356.10. Enhanced contributions are deductions, not part of this credit."),money("ei_credit_cents","Verified eligible EI credit — line 31200","Maximum $1,077.48; standard insurable earnings must exceed $2,000. Overpayments and Quebec employment are outside scope."),yes("payroll_verified","CPP/EI amounts verified against Schedule 8 and source documents","Required for any CPP/EI credit or enhanced CPP deduction. This engine does not infer contributions or refunds from wages.")]),
        section("payments","Payments",vec![money("tax_withheld_cents","Income tax withheld","Verified income tax withheld; no automatic payroll calculation."),money("instalments_cents","Income tax instalments paid","Verified payments only. Reported balance excludes refundable benefits and all other return amounts.")]),
        section("business-totals","Optional verified business totals",vec![
            dtmax(optional("self_employment_gross_cents","Verified T2125 gross business income","money","t2125_verified","T2125","Verified gross business income after any applicable sales tax adjustments. Use Schedule 8 Part 4 below for qualifying self-employed-only CPP cases."),super::t2125::DTMAX_BUSINESS_KEYWORDS.0,super::t2125::DTMAX_BUSINESS_KEYWORDS.1),
            optional("self_employment_expenses_cents","Verified T2125 deductible current expenses","money","t2125_verified","T2125","Enter deductible current expenses here when using verified aggregate totals. Leave this blank when entering itemized expenses. Enter CCA separately."),
            optional("self_employment_cca_cents","Verified T2125 CCA claim","money","t2125_verified","T2125","Externally computed CCA claim; NXT does not compute CCA classes or dispositions. This NXT workpaper rejects CCA that creates or increases a business loss; other cases are outside this calculation scope."),
            optional("t2125_verified","T2125 totals and claim eligibility verified","boolean","t2125_verified","T2125","Confirm only after checking the source totals and deductions. This is not automatic verification or a complete T2125."),
            optional("se_cpp_deduction_cents","Verified self-employed CPP deduction — line 22200","money","se_cpp_verified","Schedule 8","Externally calculated Schedule 8 deduction. Contributions, overpayments and elections are not computed here."),
            optional("se_cpp_credit_cents","Verified self-employed CPP credit — line 31000","money","se_cpp_verified","Schedule 8","Externally calculated Schedule 8 eligible credit amount, not the entire CPP payment. Verify separately from employee CPP."),
            optional("se_cpp_verified","Externally prepared CPP amounts verified","boolean","se_cpp_verified","Schedule 8","Applies to manually entered deduction and credit on a partial workpaper."),
            optional(super::schedule8::CONFIRMATION,"Calculate full-year self-employed CPP (Schedule 8 Part 4)","boolean",super::schedule8::CONFIRMATION,"Schedule 8","Confirm you were subject to CPP for all 12 months, had only self-employment earnings for CPP purposes, no employment or Québec earnings, no QPP contributions, no proration, disability pension, election or special adjustments. Leave manually entered CPP amounts blank.")]),
        section("business-expenses","Itemized T2125 business expenses",t2125_expense_fields()),
        section("self-employment-totals","Optional verified professional, commission, farming and fishing totals",self_employment_activity_fields()),
        section("rental-totals","Optional verified rental totals",vec![
            optional("rental_gross_cents","Verified T776 gross rental income","money","t776_verified","T776","Aggregate verified rental totals; no property schedule, co-ownership allocation or personal-use adjustment is computed."),
            optional("rental_expenses_cents","Verified T776 deductible current expenses","money","t776_verified","T776","Current deductible rental expenses from records, excluding separately entered CCA. Capital expenses need separate review."),
            optional("rental_cca_cents","Verified T776 CCA claim","money","t776_verified","T776","Externally calculated CCA; NXT does not compute classes or dispositions. CCA cannot create or increase a rental loss."),
            optional("t776_verified","T776 totals and claim eligibility verified","boolean","t776_verified","T776","Confirm only after verifying the rental totals and deductions. This is not an automatic or complete T776.")]),
        section("current-credits","Optional verified current-year credits",vec![
            optional("medical_expenses_cents","Eligible medical expenses before income threshold","money","credits_verified","Federal worksheet / line 33099","Verified eligible unreimbursed receipts for an allowed 12-month period ending in 2025. Enter gross eligible expenses, not the credit after the net-income threshold. Dependant line 33199 and special cases are not handled."),
            optional("donations_cents","Verified current-year eligible charitable donations","money","credits_verified","Schedule 9","Ordinary eligible receipted donations only, within the existing 75% net-income limit. No carryforwards, transfers, securities gifts or special limits are computed."),
            optional("tuition_cents","Verified current-year eligible tuition","money","credits_verified","Schedule 11 / T2202","Reconcile the entered current-year claim against Schedule 11, including current own-use limits, the Canada training credit and unused balances; a certificate alone does not establish the full claim. No unused balance, carryforward or transfer computation. Provincial tuition/education limitations remain in diagnostics. Saved T2202 groups own this amount when present."),
            optional("credits_verified","Medical, donation and tuition amounts/eligibility verified","boolean","credits_verified","Federal worksheet / Schedules 9 and 11","One explicit verification for these entered current-year claims. Not checked automatically; all calculation diagnostics and scope limits still apply.")]),
    ]);
    t1["field_catalog_version"]=json!(T1_FIELD_CATALOG_VERSION);
    t1["optional_workpaper_statement"]=json!("Optional 2025 verified totals use existing bounded calculation paths. Blank means not supplied; an entered zero remains explicit. No complete schedules, benefits, filing or automatic confirmation are added.");
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
    doc["coverage"]=super::coverage::catalog();
    doc["programs"][0]["provincial_scope"]=json!(super::calculations::T1_PROV_SCOPE);
    doc["programs"][0]["ontario_scope"]=json!(super::calculations::T1_ON_SCOPE);
    doc
}

#[cfg(test)]mod tests {
    use super::*;use super::super::calculations::calculate;
    #[test]fn catalog_defaults_match_engine_inputs_and_never_claim_filing(){
        let c=catalog();assert_eq!(c["filing_supported"],false);assert_eq!(c["certified"],false);
        for p in c["programs"].as_array().unwrap(){let result=calculate(p["id"].as_str().unwrap(),2025,p["jurisdictions"][0].as_str().unwrap(),&p["defaults"]).unwrap();assert!(!result.supported);assert!(!p["sources"].as_array().unwrap().is_empty());let fields:usize=p["sections"].as_array().unwrap().iter().flat_map(|s|s["fields"].as_array().unwrap()).filter(|f|f["optional"]!=true).count();assert_eq!(p["defaults"].as_object().unwrap().len(),fields);}
    }
}
