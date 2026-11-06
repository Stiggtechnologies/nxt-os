import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
const source=fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8');
const sandbox={};vm.runInNewContext(source,sandbox);const api=sandbox.NXTDTMaxT1;
const page=()=>({source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:[2013,2025],clients:[{client_number:'100',name:'Synthetic <img>',historical_years:[2013,2025],pending_years:[2013]}]});
test('source cents retain absent and exact signed i64 cents without floating-point rounding',()=>{
  assert.equal(api.cents(null),'Absent');assert.equal(api.cents('0'),'$0.00');assert.equal(api.cents('-1'),'-$0.01');assert.equal(api.cents('9223372036854775807'),'$92,233,720,368,547,758.07');
  for(const value of [undefined,'1.00','-0','01','9223372036854775808'])assert.throws(()=>api.cents(value));
});
test('worklist rejects wrong scope, case creation and malicious years even on an empty page',()=>{
  api.validatePage(page(),0);
  for(const mutate of [p=>p.source_scope='other',p=>p.tax_cases_created=1,p=>p.clients[0].pending_years=[2026],p=>{p.clients=[];p.available_years=['<img>'];}]){const p=page();mutate(p);assert.throws(()=>api.validatePage(p,0));}
});
test('historical totals preserve absence, zero and pending labels; source slip text is escaped',()=>{
  const values=Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map((x,i)=>[x,i===0?'0':null]));
  const html=api.recordsHtml('historical-lines',[{tax_year:2013,mapping_state:'pending_year_verification',values_cents:values}]);assert.ok(html.includes('$0.00'));assert.ok(html.includes('Absent'));assert.ok(html.includes('Mapping needs verification'));
  const slips=api.recordsHtml('afr-slips',[{slip_kind:'T4Slip',tax_year:null,source_folder_year:2025,vendor_field_tree:{tag:'T4Slip',attributes:{},text:'<img src=x>',children:[]}}]);assert.ok(slips.includes('Missing tax year'));assert.ok(!slips.includes('<img'));assert.ok(slips.includes('&#60;img'));
});
test('late source responses cannot replace another tax view',async()=>{
  let resolve,active=true;const pending=new Promise(r=>resolve=r),host={isConnected:true,innerHTML:'',querySelector:()=>null};
  const controller=api.mount(host,{request:()=>pending,isCurrent:()=>active});active=false;host.innerHTML='Different tax view';resolve(page());await controller.ready;assert.equal(host.innerHTML,'Different tax view');
});
