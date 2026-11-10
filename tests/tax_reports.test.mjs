import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const ctx=vm.createContext({crypto:webcrypto,TextEncoder,TextDecoder,structuredClone,console});
vm.runInContext(fs.readFileSync(new URL('../public/apps/tax/tax-reports.js',import.meta.url),'utf8'),ctx);
const R=ctx.NXTTaxReports,plain=v=>JSON.parse(JSON.stringify(v));
const columns=[...R.REQUIRED.map(key=>({key,label:key,kind:['tax_year','revision'].includes(key)?'integer':'text',lane:'identity'})),{key:'name',label:'Name',kind:'text',lane:'identity'},{key:'input:employment_income_cents',label:'Employment',kind:'money',lane:'inputs'},{key:'line:42000',label:'Tax',kind:'money',lane:'calculation'},{key:'input:scope_confirmed',label:'Scope',kind:'boolean',lane:'inputs'}];
const catalog={schema:R.SCHEMA,columns,required_columns:plain(R.REQUIRED),operators:['equals','not_equals','greater_than','less_than','contains','is_blank','is_present'],max_rows:10000,max_response_bytes:3145728,money_unit:'integer_cents',blank_is_zero:false,native_preparation_workpapers_only:true,immutable_source_records_included:false};
const req=()=>({program:'T1',years:[2025],jurisdiction:null,statuses:[],client_ref:'A',search:'',columns:['name','input:employment_income_cents','line:42000','input:scope_confirmed'],predicates:[],include_historical_results:false});
async function report(request=req()){
 const row={id:'R1',program:'T1',tax_year:2025,jurisdiction:'AB',revision:3,calculation_state:'current_supported',name:'=Élodie, "示例"\r\nsecond line', 'input:employment_income_cents':-1,'line:42000':0,'input:scope_confirmed':false};
 const p={schema:R.SCHEMA,request,columns,rows:[row],row_count:1,complete:true,atomic_snapshot:true,native_preparation_workpapers_only:true,source_records_included:false,money_unit:'integer_cents',blank_is_zero:false,financial_approval:false,filing_supported:false};
 p.snapshot_sha256=await R.digest({request,columns,rows:p.rows});return p;
}
test('money text and filter cents roundtrip exactly without float rounding',()=>{
 for(const [s,n]of[['0',0],['0.10',10],['-0.01',-1],['999999999999.99',99999999999999],['1000000000000.00',100000000000000]]){assert.equal(R.filterValue(s,'money','equals'),n);assert.equal(R.filterValue(R.moneyText(n),'money','equals'),n);}
 for(const s of['1e5','1.001','NaN','Infinity','1,000','+1','1000000000000.01'])assert.throws(()=>R.filterValue(s,'money','equals'));
 for(const n of[NaN,1.5,Infinity,Number.MAX_SAFE_INTEGER])assert.throws(()=>R.moneyText(n));
});
test('typed filters distinguish flags, integers, missing values and zero',()=>{
 assert.equal(R.filterValue('false','boolean','equals'),false);assert.equal(R.filterValue('0','integer','equals'),0);assert.equal(R.filterValue('','money','is_blank'),null);
 assert.throws(()=>R.filterValue('0','boolean','equals'));assert.throws(()=>R.filterValue('2.5','integer','equals'));assert.throws(()=>R.filterValue('','money','equals'));
});
test('valid complete report binds every cell, selected column and exact request to SHA256',async()=>{
 const p=await report();assert.equal(await R.checkReport(p,req(),R.checkCatalog(catalog)),p);
 for(const mutate of[p=>p.rows[0].name='tampered',p=>p.row_count=0,p=>p.complete=false,p=>p.atomic_snapshot=false,p=>p.source_records_included=true,p=>p.request.search='changed',p=>p.columns[0].label='wrong',p=>delete p.rows[0].revision]){const x=structuredClone(p);mutate(x);await assert.rejects(R.checkReport(x,req(),catalog));}
});
test('duplicate rows, unsafe money and unselected historical results fail closed even with new fingerprints',async()=>{
 for(const mutate of[p=>{p.rows.push(structuredClone(p.rows[0]));p.row_count=2;},p=>p.rows[0]['line:42000']=1.5,p=>p.rows[0]['line:42000']=Number.MAX_SAFE_INTEGER,p=>p.rows[0].calculation_state='stale']){const p=await report();mutate(p);p.snapshot_sha256=await R.digest({request:p.request,columns:p.columns,rows:p.rows});await assert.rejects(R.checkReport(p,req(),catalog));}
 const p=await report();p.rows[0].calculation_state='stale';p.rows[0]['line:42000']=null;p.snapshot_sha256=await R.digest({request:p.request,columns:p.columns,rows:p.rows});await R.checkReport(p,req(),catalog);
 const request=req();request.include_historical_results=true;const historical=await report(request);historical.rows[0].calculation_state='stale';historical.snapshot_sha256=await R.digest({request,columns:historical.columns,rows:historical.rows});await R.checkReport(historical,request,catalog);
});
function parseCsv(text){const rows=[];let row=[],cell='',inside=false;for(let i=1;i<text.length;i++){const c=text[i];if(c==='"'){if(inside&&text[i+1]==='"'){cell+='"';i++;}else inside=!inside;}else if(c===','&&!inside){row.push(cell);cell='';}else if(c==='\r'&&text[i+1]==='\n'&&!inside){row.push(cell);rows.push(row);row=[];cell='';i++;}else cell+=c;}assert.equal(inside,false);return rows;}
test('CSV preserves quoted formula-like Unicode/newlines and exact negative CAD; blanks stay blank',async()=>{
 const p=await report();p.rows.push({...p.rows[0],id:'R2','line:42000':null});const s=R.csv(p),rows=parseCsv(s);assert.equal(s.charCodeAt(0),0xfeff);assert.equal(rows.length,3);
 assert.equal(rows[1][rows[0].indexOf('name')],"'"+p.rows[0].name);assert.equal(rows[1][rows[0].indexOf('input:employment_income_cents_CAD')],"'-0.01");assert.equal(rows[1][rows[0].indexOf('line:42000_CAD')],"'0.00");assert.equal(rows[2][rows[0].indexOf('line:42000_CAD')],"'");assert.equal(rows[1].at(-1),"'false");
 assert.ok(rows[1].every(v=>v.startsWith("'")));
});
test('setup belongs to exact company and validates all fields before any UI change',()=>{
 const setup={schema:'nxt-tax-report-setup/v1',company:'A',request:req()};assert.deepEqual(plain(R.checkSetup(setup,'A',catalog)),req());
 for(const change of[s=>s.company='B',s=>s.request.client_ref='B',s=>s.request.statuses=['unknown'],s=>s.request.columns=['private:identifier'],s=>s.request.years=[2025,2025],s=>s.request.extra='injected',s=>s.request.predicates=[{column:'line:42000',operator:'greater_than',value:'1'}],s=>s.request.predicates=[{column:'name',operator:'less_than',value:'a'}],s=>s.request.predicates=[{column:'name',operator:'is_blank',value:'notnull'}]]){const s=structuredClone(setup);change(s);assert.throws(()=>R.checkSetup(s,'A',catalog));}
});
test('catalog cannot remove provenance or silently add source-report semantics',()=>{
 for(const change of[c=>c.required_columns=[],c=>c.native_preparation_workpapers_only=false,c=>c.columns.push(c.columns[0]),c=>c.columns[0].kind='raw_json']){const c=structuredClone(catalog);change(c);assert.throws(()=>R.checkCatalog(c));}
});
