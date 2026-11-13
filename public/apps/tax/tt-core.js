/* Guided intake core (pure logic, no DOM, no network). Shared by the T1 interview,
   slip picker, review page and T2 intake, and loaded by node tests.
   Nothing here files anything: NETFILE / EFILE stay disabled. */
(function (root) {
  "use strict";
  const BANNER = "Internal workpaper. Not CRA-certified. NETFILE disabled. A human files.";
  const TAX_YEAR = 2025;
  const PROVINCES = [["AB","Alberta"],["BC","British Columbia"],["MB","Manitoba"],["NB","New Brunswick"],["NL","Newfoundland and Labrador"],["NS","Nova Scotia"],["NT","Northwest Territories"],["NU","Nunavut"],["ON","Ontario"],["PE","Prince Edward Island"],["SK","Saskatchewan"],["YT","Yukon"]];

  // --- SIN: validated and masked in the browser only; never sent to the server or stored.
  function sinDigits(s) { return String(s || "").replace(/[\s-]/g, ""); }
  function validSin(s) {
    const d = sinDigits(s);
    if (!/^\d{9}$/.test(d) || d === "000000000") return false;
    let sum = 0;
    for (let i = 0; i < 9; i++) { let n = +d[i]; if (i % 2 === 1) { n *= 2; if (n > 9) n -= 9; } sum += n; }
    return sum % 10 === 0;
  }
  function maskSin(s) { const d = sinDigits(s); return d.length === 9 ? "*** *** " + d.slice(6) : ""; }

  // --- Live refund meter from calculation totals (balance = tax - paid; negative is a refund).
  function meter(calc) {
    const t = calc && calc.totals;
    if (!t || typeof t.balance !== "number") return { kind: "pending", cents: 0, label: "Calculate to see your estimate" };
    if (t.balance < 0) return { kind: "refund", cents: -t.balance, label: "Estimated refund" };
    if (t.balance > 0) return { kind: "owing", cents: t.balance, label: "Estimated balance owing" };
    return { kind: "even", cents: 0, label: "Nothing owing, no refund" };
  }

  // --- Situations ("what applies to you") mapped to interview screens and engine input keys.
  const SITUATIONS = [
    { id: "employment", label: "Employment income (T4)", screen: "slips", supported: true },
    { id: "investments", label: "Interest from a bank or investments (T5)", screen: "slips", supported: true },
    { id: "pension", label: "Pension or annuity income (T4A)", screen: "slips", supported: true },
    { id: "tuition", label: "Post-secondary tuition (T2202)", screen: "slips", supported: true },
    { id: "securities", label: "Sold stocks, funds or other securities (T5008)", screen: "slips", supported: true },
    { id: "self_employed", label: "Self-employed or freelance income", screen: "deductions", supported: true },
    { id: "rental", label: "Rental property income", screen: "deductions", supported: true },
    { id: "foreign", label: "Foreign property over $100,000 (T1135)", screen: null, supported: false },
    { id: "moved", label: "Moved provinces or left/entered Canada", screen: null, supported: false },
    { id: "deceased", label: "Return for a person who died", screen: null, supported: false },
  ];
  const DEDUCTIONS = [
    { id: "rrsp", label: "RRSP contributions", keys: ["rrsp_deduction_cents", "rrsp_deduction_limit_cents"], verify: "rrsp_verified", supported: true },
    { id: "medical", label: "Medical expenses", keys: ["medical_expenses_cents"], verify: "credits_verified", supported: true },
    { id: "donations", label: "Charitable donations", keys: ["donations_cents"], verify: "credits_verified", supported: true },
    { id: "tuition", label: "Tuition (from T2202)", keys: ["tuition_cents"], verify: "credits_verified", supported: true },
    { id: "student_loan", label: "Student loan interest", keys: ["student_loan_interest_cents"], verify: null, supported: true },
    { id: "home_buyers", label: "First-time home buyers' amount", keys: ["home_buyers_amount_cents"], verify: null, supported: true },
    { id: "self_employment", label: "Business income and expenses (T2125)", keys: ["self_employment_gross_cents", "self_employment_expenses_cents"], verify: "t2125_verified", supported: true },
    { id: "rental", label: "Rental income and expenses (T776)", keys: ["rental_gross_cents", "rental_expenses_cents"], verify: "t776_verified", supported: true },
    { id: "fhsa", label: "FHSA contributions", keys: [], verify: null, supported: false },
    { id: "childcare", label: "Child care expenses", keys: [], verify: null, supported: false },
    { id: "moving", label: "Moving expenses", keys: [], verify: null, supported: false },
  ];
  const FIELD_SCREEN = { full_year_resident: "profile", scope_confirmed: "profile", other_tax_situations: "situations", provincial_adjustments_not_applicable: "profile",
    born_1960_or_earlier: "profile", spouse_net_income_cents: "profile", disability_tax_credit_approved: "profile",
    slip_groups: "slips",
    employment_income_cents: "slips", interest_income_cents: "slips", tax_withheld_cents: "slips", pension_income_cents: "slips", capital_proceeds_cents: "slips", capital_acb_cents: "slips",
    payroll_verified: "slips", cpp_base_credit_cents: "slips", cpp_enhanced_deduction_cents: "slips", ei_credit_cents: "slips", schedule3_verified: "slips" };
  for (const d of DEDUCTIONS) { for (const k of d.keys) FIELD_SCREEN[k] = "deductions"; if (d.verify && !FIELD_SCREEN[d.verify]) FIELD_SCREEN[d.verify] = "deductions"; }
  function screenFor(field) { return FIELD_SCREEN[field] || "deductions"; }

  // Profile answers -> engine inputs. Unsupported situations set other_tax_situations so the
  // engine blocks review with a scope diagnostic instead of silently under-reporting.
  function profileInputs(answers, inputs) {
    const out = Object.assign({}, inputs || {});
    out.full_year_resident = !!answers.full_year_resident;
    out.scope_confirmed = !!answers.scope_confirmed;
    const picked = new Set(answers.situations || []);
    out.other_tax_situations = SITUATIONS.some(s => !s.supported && picked.has(s.id)) || !!answers.other_situations;
    if (answers.born_1960_or_earlier !== undefined) out.born_1960_or_earlier = !!answers.born_1960_or_earlier;
    return out;
  }
  function outOfScope(answers) { const p = new Set(answers.situations || []); return SITUATIONS.filter(s => !s.supported && p.has(s.id)).map(s => s.label); }

  // --- Slip search-to-add over GET /api/tax/slips/catalog.
  function searchSlips(catalog, q) {
    const slips = (catalog && catalog.slips) || [];
    const terms = String(q || "").toLowerCase().split(/\s+/).filter(Boolean);
    if (!terms.length) return slips.slice();
    const scored = slips.map(s => {
      const head = [s.kind, s.title, s.keywords].join(" ").toLowerCase(), boxes = (s.boxes || []).map(b => b.label).join(" ").toLowerCase();
      let score = 0;
      for (const t of terms) { if (s.kind.toLowerCase() === t) score += 100; else if (s.kind.toLowerCase().startsWith(t)) score += 50; if (head.includes(t)) score += 20; else if (boxes.includes(t)) score += 5; else return null; }
      return { s, score };
    }).filter(Boolean);
    return scored.sort((a, b) => b.score - a.score || a.s.kind.localeCompare(b.s.kind)).map(x => x.s);
  }
  // Slip-shaped form: one field per box, in box order, with calculated boxes first.
  function slipForm(entry) {
    const boxes = (entry.boxes || []).slice().sort((a, b) => (a.status === b.status ? String(a.box).localeCompare(String(b.box), undefined, { numeric: true }) : a.status === "calculated" ? -1 : 1));
    return { kind: entry.kind, title: entry.title, fields: boxes.map(b => ({ box: b.box, label: b.label, status: b.status, unit:b.unit||"cents", name: "box_" + b.box })) };
  }
  function toCents(v) {
    let s=String(v??"").trim();if(!s)return 0;s=s.replace(/^\$/,"" );
    if(!/^-?(?:\d+|\d{1,3}(?:,\d{3})+)(?:\.\d{1,2})?$/.test(s))return NaN;
    s=s.replace(/,/g,"");const neg=s[0]==="-";if(neg)s=s.slice(1);const [whole,minor=""]=s.split(".");let n=BigInt(whole)*100n+BigInt(minor.padEnd(2,"0"));if(neg)n=-n;
    return n>=-100000000000000n&&n<=100000000000000n?Number(n):NaN;
  }
  function wholeBox(kind,box){return kind==="T2202"&&["24","25"].includes(box)||kind==="T5008"&&["15","16"].includes(box);}
  function slipFromForm(kind, issuer, values) {
    const boxes={};for(const [box,v]of Object.entries(values||{})){
      if(v==null||String(v).trim()==="")continue;
      const c=wholeBox(kind,box)?(/^\d+$/.test(String(v).trim())?Number(v):NaN):toCents(v);
      if(!Number.isSafeInteger(c)||c<0||c>100000000000000)throw Error("Box "+box+" must be an exact non-negative "+(wholeBox(kind,box)?"whole number":"amount with at most two decimal places"));boxes[box]=c;
    }return {kind,issuer:String(issuer||""),boxes};
  }
  function managedSlipKeys(groups){const mapping={T4:{14:"employment_income_cents",22:"tax_withheld_cents"},T4A:{22:"tax_withheld_cents","016":"pension_income_cents"},T5:{13:"interest_income_cents"},T5008:{20:"capital_acb_cents",21:"capital_proceeds_cents"},T2202:{23:"tuition_cents"}};return [...new Set((groups||[]).flatMap(s=>Object.keys(s.boxes||{}).map(b=>mapping[s.kind]?.[b]).filter(Boolean)))];}
  // Merge aggregated slip inputs into the return inputs (slip-derived keys are replaced, not added twice).
  function mergeSlipInputs(inputs, aggregate, previousSlipKeys) {
    const out = Object.assign({}, inputs || {});
    for (const k of previousSlipKeys || []) delete out[k];
    for (const [k, v] of Object.entries((aggregate && aggregate.inputs) || {})) out[k] = v;
    return out;
  }
  const OPTIONAL_FIELD_CATALOG_VERSION = "nxt-tax-fields/T1/2025/5";
  const OPTIONAL_FIELD_CATALOG_VERSIONS = new Set(["nxt-tax-fields/T1/2025/2", "nxt-tax-fields/T1/2025/3", "nxt-tax-fields/T1/2025/4", OPTIONAL_FIELD_CATALOG_VERSION]);
  function optionalSections(catalog, record) {
    const p=catalog?.programs?.find(p=>p.id==="T1"),sections=p?.sections?.filter(s=>s.fields?.some(f=>f.optional===true))||[];
    if(!sections.length)return [];
    if(record.program!=="T1"||record.tax_year!==2025||!OPTIONAL_FIELD_CATALOG_VERSIONS.has(p.field_catalog_version))throw Error("Optional entry metadata is unavailable for this workpaper.");
    for(const s of sections)for(const f of s.fields)if(f.optional!==true||f.default_policy!=="omit"||!["money","boolean","number"].includes(f.kind)||!f.tax_years?.includes(record.tax_year)||!f.jurisdictions?.includes(record.jurisdiction))throw Error("Optional field scope or omission contract differs.");
    return sections;
  }
  function applyOptionalInput(out, field, raw, {changed=false,clear=false}={}) {
    if(field.optional!==true||field.default_policy!=="omit")throw Error("Optional field contract differs.");
    if(clear){delete out[field.key];return out;}
    if(field.kind==="boolean"){
      if(typeof raw!=="boolean")throw Error("Verification must be an explicit checkbox value.");
      if(changed||Object.hasOwn(out,field.key))out[field.key]=raw;
    }else if(field.kind==="money"){
      if(typeof raw!=="string")throw Error("Enter a dollar amount or leave it blank.");
      if(raw.trim()===""){delete out[field.key];return out;}
      const value=toCents(raw);if(!Number.isSafeInteger(value)||value<0||value>1e14)throw Error("Enter a nonnegative dollar amount with at most two decimals, within the displayed bound.");out[field.key]=value;
    }else if(field.kind==="number"){
      if(typeof raw!=="string")throw Error("Enter a whole number or leave it blank.");
      const text=raw.trim();
      if(text===""){delete out[field.key];return out;}
      const value=Number(text);
      if(!/^\d+$/.test(text)||!Number.isSafeInteger(value)||!Number.isSafeInteger(field.min)||!Number.isSafeInteger(field.max)||value<field.min||value>field.max)throw Error("Enter a whole number within the displayed bounds.");
      out[field.key]=value;
    }else throw Error("Optional field type is unavailable.");
    return out;
  }

  // --- Review page: diagnostics grouped with Fix-it links back into the interview.
  function reviewItems(returnId, calc) {
    const diags = (calc && calc.diagnostics) || [];
    const items = diags.map(d => ({ code: d.code, message: d.message, blocking: !!d.blocks_review || d.severity === "error", field: d.field || null,
      screen: d.field ? screenFor(d.field) : "situations",
      fix: "#tt-t1/" + encodeURIComponent(returnId) + "?screen=" + (d.field ? screenFor(d.field) : "situations") + (d.field ? "&field=" + encodeURIComponent(d.field) : "") }));
    return { blocking: items.filter(i => i.blocking), notes: items.filter(i => !i.blocking), ready: !items.some(i => i.blocking) && !!(calc && calc.supported) };
  }

  // --- T2 intake: which sections are visible for the server's "next" step (one progressive page).
  const T2_STEPS = ["incorporated", "location", "period", "income", "expenses"];
  function t2Visible(next, answers) {
    const idx = next === "done" ? T2_STEPS.length : T2_STEPS.indexOf(next);
    let vis = T2_STEPS.slice(0, idx < 0 ? 1 : idx + 1);
    if (answers && answers.had_income === true) vis = vis.filter(s => s !== "expenses");
    if (answers && answers.incorporated === false) vis = ["incorporated"];
    if (answers && ["QC", "MULTI", "OUTSIDE"].includes(answers.location)) vis = vis.slice(0, 2);
    return vis;
  }
  // Changing an earlier answer clears the later ones (as the interview re-asks them).
  function t2Answer(answers, step, patch) {
    const out = Object.assign({}, answers, patch);
    const i = T2_STEPS.indexOf(step);
    const keys = { location: ["location"], period: ["period_start", "period_end"], income: ["had_income"], expenses: ["other_expenses"] };
    for (const s of T2_STEPS.slice(i + 1)) for (const k of keys[s] || []) delete out[k];
    return out;
  }


  // ---- Filing fee (Stripe Checkout). Amount always comes from the server quote.
  function formatFee(cents, currency) {
    if (cents === null || cents === undefined || cents === "") return "—";
    const n = Number(cents);
    if (!Number.isFinite(n)) return "—";
    const cur = String(currency || "cad").toUpperCase();
    return cur + " $" + (n / 100).toFixed(2);
  }
  function payScreenState(status) {
    const s = status || {};
    if (s.fee_paid) return { kind: "paid", title: "Filing fee paid", cta: null };
    if (!s.stripe_enabled) return { kind: "disabled", title: "Online payment is not available", cta: null };
    return { kind: "ready", title: "Pay the filing fee", cta: "Pay with card" };
  }
  const api = { BANNER, TAX_YEAR, PROVINCES, SITUATIONS, DEDUCTIONS, FIELD_SCREEN, T2_STEPS, validSin, maskSin, meter, screenFor, profileInputs, outOfScope, searchSlips, slipForm, slipFromForm, toCents, managedSlipKeys, mergeSlipInputs, OPTIONAL_FIELD_CATALOG_VERSION, optionalSections, applyOptionalInput, reviewItems, t2Visible, t2Answer, formatFee, payScreenState };
  root.NXTTurboCore = api;
  if (typeof module !== "undefined" && module.exports) module.exports = api;
})(typeof globalThis !== "undefined" ? globalThis : this);
