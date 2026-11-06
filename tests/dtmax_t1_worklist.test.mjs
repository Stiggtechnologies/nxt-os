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
  const html=api.recordsHtml('historical-lines',[{tax_year:2013,vendor_module:'T1',mapping_state:'pending_year_verification',values_cents:values}]);assert.ok(html.includes('$0.00'));assert.ok(html.includes('Absent'));assert.ok(html.includes('Mapping needs verification'));
  const slips=api.recordsHtml('afr-slips',[{slip_kind:'T4Slip',tax_year:null,source_folder_year:2025,vendor_field_tree:{tag:'T4Slip',attributes:{},text:'<img src=x>',children:[]}}]);assert.ok(slips.includes('Missing tax year'));assert.ok(!slips.includes('<img'));assert.ok(slips.includes('&#60;img'));
});
test('late source responses cannot replace another tax view',async()=>{
  let resolve,active=true;const pending=new Promise(r=>resolve=r),host={isConnected:true,innerHTML:'',querySelector:()=>null};
  const controller=api.mount(host,{request:()=>pending,isCurrent:()=>active});active=false;host.innerHTML='Different tax view';resolve(page());await controller.ready;assert.equal(host.innerHTML,'Different tax view');
});
function harness(recordClient='100') {
  const nodes=new Map(),buttons=[{dataset:{importedClient:'100'}}],requests=[];
  const node=key=>{if(!nodes.has(key))nodes.set(key,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(key);};
  const host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:key=>key==='[data-imported-client]'?buttons:[]};
  const values=Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map((x,i)=>[x,i===0?'12345':null]));
  const request=async path=>{requests.push(path);return path.includes('/source-clients?')?page():{test_copy:true,client_number:'100',kind:'historical-lines',offset:0,total:1,tax_cases_imported:0,records:[{client_number:recordClient,tax_year:2025,vendor_module:'T1',mapping_state:'vendor_static_verified',values_cents:values}]};};
  return {host,node,buttons,requests,controller:api.mount(host,{request})};
}
test('T1 list opens the selected client source totals and encodes global source search',async()=>{
  const h=harness();await h.controller.ready;
  assert.ok(h.host.innerHTML.includes('1–1 of 1 clients'));assert.ok(h.host.innerHTML.includes('Synthetic &#60;img&#62;'));
  await h.buttons[0].onclick();assert.ok(h.node('[data-dtmax-details]').innerHTML.includes('$123.45'));
  assert.ok(h.requests.at(-1).includes('/clients/100/source-facts?kind=historical-lines'));
  h.node('[name=search]').value='Name & client';h.node('[name=year]').value='2013';await h.node('[data-dtmax-search]').onsubmit({preventDefault(){}});
  assert.ok(h.requests.at(-1).includes('search=Name%20%26%20client&year=2013'));
  assert.ok(h.requests.every(p=>p.startsWith('/api/source-imports/dtmax/companies/dtmax-test-copy/test-copy/')));
});
test('another client record cannot display under the selected imported T1',async()=>{
  const h=harness('200');await h.controller.ready;await h.buttons[0].onclick();const html=h.host.innerHTML+h.node('[data-dtmax-details]').innerHTML;
  assert.ok(html.includes('role="alert"'));assert.ok(!html.includes('$123.45'));
});

test('source workspace has the three observed views, preserves missing identity and labels roster provenance',async()=>{
  const h=harness();await h.controller.ready;
  for(const label of ['Client list','Data entry','Tax return','F10','F2','F9'])assert.ok(h.host.innerHTML.includes(label));
  await h.buttons[0].onclick();assert.ok(h.host.innerHTML.includes('dtmax-entry'));assert.ok(h.host.innerHTML.includes('Not imported'));assert.ok(h.host.innerHTML.includes('client-list snapshot'));
  assert.ok(!h.host.innerHTML.includes('<input name="SIN"'));assert.ok(h.host.innerHTML.includes('$123.45'));
  await h.controller.show('list');assert.ok(h.host.innerHTML.includes('dtmax-grid'));
  h.host.onkeydown({key:'F2',target:{tagName:'INPUT'},preventDefault(){throw Error('Must not intercept typing');}});
});
test('PDF metadata refuses another client, mismatched page count and oversized content',()=>{
  const pdf={client_number:'100',tax_year:2024,sha256:'a'.repeat(64),bytes:100,page_labels:['T1 Federal p1'],page_count:1,original_print_output:true,immutable:true};
  api.validatePdf(pdf,'100');
  for(const mutate of [p=>p.client_number='200',p=>p.page_count=2,p=>p.bytes=2097153,p=>p.page_labels=['<script>\n'],p=>p.immutable=false]){const p={...pdf};mutate(p);assert.throws(()=>api.validatePdf(p,'100'));}
});
test('disposal prevents pending source requests from painting the workspace',async()=>{
  let resolve;const pending=new Promise(r=>resolve=r),host={isConnected:true,innerHTML:'',querySelector:()=>null};
  const c=api.mount(host,{request:()=>pending});c.dispose();host.innerHTML='Different page';resolve(page());await c.ready;assert.equal(host.innerHTML,'Different page');assert.equal(host.onkeydown,null);
});
test('client sorting, status and province stay global request filters; keyword search keeps familiar shortcut',async()=>{
  const h=harness();await h.controller.ready;
  for(const [field,value] of [['sort','name'],['direction','desc'],['status','Ready & reviewed'],['province','AB']])h.node('[name='+field+']').value=value;
  await h.node('[data-dtmax-search]').onsubmit({preventDefault(){}});
  assert.ok(h.requests.at(-1).includes('sort=name&direction=desc&status=Ready%20%26%20reviewed&province=AB'));
  await h.buttons[0].onclick();
  let prevented=false;h.host.onkeydown({altKey:true,key:'F7',target:{tagName:'BUTTON'},preventDefault(){prevented=true;}});assert.equal(prevented,true);
  assert.ok(h.host.innerHTML.includes('data-dtmax-keyword="15000'));
});
test('family navigation exposes only exact numeric client identities',()=>{
  const h=api.recordsHtml('family-files',[{family_file_id:'999',tax_year:2024,member_client_numbers:['100','200']}]);assert.ok(h.includes('data-family-client="200"'));
  assert.throws(()=>api.recordsHtml('family-files',[{family_file_id:'999',tax_year:2024,member_client_numbers:['<img>']}]));
});

test('T3 source candidates cannot appear as T1 totals in entry, comparison or history',async()=>{
  const values=Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map(x=>[x,'987654']));
  const trust={client_number:'100',tax_year:2025,vendor_module:'T3',mapping_state:'pending_module_verification',values_cents:values};
  const recordHtml=api.recordsHtml('historical-lines',[trust]);assert.ok(recordHtml.includes('Converter label T3'));assert.ok(!recordHtml.includes('$9,876.54'));
  const entry=api.entryHtml(page().clients[0],trust,'');assert.ok(entry.includes('not shown as T1 totals'));assert.ok(!entry.includes('$9,876.54'));
  const h=harness();await h.controller.ready;
  const controller=api.mount(h.host,{request:async path=>path.includes('/source-clients?')?page():{test_copy:true,client_number:'100',kind:'historical-lines',offset:0,total:1,tax_cases_imported:0,records:[trust]}});
  await controller.ready;await controller.open('100');
  h.node('[data-dtmax-compare]').onchange({target:{value:'2025'}});
  assert.ok(h.node('[data-dtmax-comparison]').innerHTML.includes('Converter label T3'));
  assert.ok(!h.node('[data-dtmax-comparison]').innerHTML.includes('$9,876.54'));
  for(const changed of [{...trust,vendor_module:'T2'},{...trust,vendor_module:undefined},{...trust,mapping_state:'vendor_static_verified'}])assert.throws(()=>api.recordsHtml('historical-lines',[changed]));
});

const exception=()=>({source_archive_sha256:'a'.repeat(64),source_archive_bytes:1000,source_context_offset:100,source_context_sha256:'b'.repeat(64),source_context_bytes:70,source_year_frame_offset:56,source_year_frame_sha256:'c'.repeat(64),family_file_id:'99',client_number:'100',tax_year:2024,reason:'missing_identity_information',frame_tags:['EXC','EXP'],immutable:true,identity_metadata_readable:false,normalized_amounts_created:false,header_ids_are_source_references_only:true});
test('source exceptions retain unreadable state and distinguish header references from ownership',()=>{
  const r=exception();api.validateException(r);const html=api.exceptionsHtml([r]);assert.ok(html.includes('Header client #'));assert.ok(html.includes('do not establish a person, ownership or tax figures'));assert.ok(html.includes('Identity information missing'));assert.ok(!html.includes('$0.00'));
  for(const change of [{immutable:false},{identity_metadata_readable:true},{normalized_amounts_created:true},{source_context_bytes:65537},{source_context_offset:999},{frame_tags:['EXC','EXC']},{client_number:'<img>'},{reason:'guessed'}])assert.throws(()=>api.validateException({...r,...change}));
});
test('year-specific binding review labels preserved totals and rejects scope or profile guesses',()=>{
  const review={state:'original_form_bindings_checked',tax_year:2013,source_form:'E11',package_sha256:'9d7254fabd95f679025e3fc7e441304709f259fd7fbfc2425787a231fa3ed07e',profile:'dtmax-T1-E11-2013-2014/v1',field_ids:Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map(c=>[c,Number(c)/100])),amounts_recalculated:false,tax_figures_approved:false};
  const r={tax_year:2013,vendor_module:'T1',mapping_state:'pending_year_verification',values_cents:{15000:'123456',23600:'0',26000:null,42000:null,48400:null,48500:null},year_mapping_review:review};
  const html=api.recordsHtml('historical-lines',[r]);assert.ok(html.includes('Year-specific form bindings checked'));assert.ok(html.includes('$1,234.56'));assert.ok(html.includes('$0.00'));assert.ok(html.includes('Absent'));assert.equal(r.mapping_state,'pending_year_verification');
  for(const changed of [{...r,vendor_module:'T3',mapping_state:'pending_module_verification'},{...r,tax_year:2012},{...r,year_mapping_review:{...review,package_sha256:'0'.repeat(64)}},{...r,year_mapping_review:{...review,tax_figures_approved:true}},{...r,year_mapping_review:{...review,field_ids:{...review.field_ids,15000:20}}}])assert.throws(()=>api.recordsHtml('historical-lines',[changed]));
});
test('2026 binding evidence keeps preliminary source figures labelled and cannot cover 2005',()=>{
  const review={state:'original_form_bindings_checked',tax_year:2026,source_form:'E11',package_sha256:'0f20e028ba2dbf059f6d585c16c56f4243c2cdbe332a7e1b1ef8de14985ccbb5',profile:'dtmax-T1-E11-additional-years/v1',field_ids:Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map(c=>[c,Number(c)/100])),amounts_recalculated:false,tax_figures_approved:false,source_year_may_be_preliminary:true};
  const r={tax_year:2026,vendor_module:'T1',mapping_state:'pending_year_verification',values_cents:{15000:'0',23600:null,26000:null,42000:null,48400:null,48500:null},year_mapping_review:review};
  assert.ok(api.recordsHtml('historical-lines',[r]).includes('may be preliminary or planning data'));
  assert.throws(()=>api.recordsHtml('historical-lines',[{...r,year_mapping_review:{...review,source_year_may_be_preliminary:false}}]));
  assert.throws(()=>api.recordsHtml('historical-lines',[{...r,tax_year:2005,year_mapping_review:{...review,tax_year:2005}}]));
});
test('2005 coordinate binding review keeps absent amounts absent and rejects another profile',()=>{
  const review={state:'original_form_bindings_checked',tax_year:2005,source_form:'E11',package_sha256:'90e36c34c9ad98691c872b6d0bc5482459ec5db84f9f32f86b926f234234f745',profile:'dtmax-T1-E11-2005/v1',field_ids:Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map(c=>[c,Number(c)/100])),amounts_recalculated:false,tax_figures_approved:false,source_year_may_be_preliminary:false};
  const r={tax_year:2005,vendor_module:'T1',mapping_state:'pending_year_verification',values_cents:Object.fromEntries(Object.keys(review.field_ids).map(c=>[c,null])),year_mapping_review:review};
  const original=JSON.stringify(r),html=api.recordsHtml('historical-lines',[r]);
  assert.equal((html.match(/Absent/g)||[]).length,7);assert.ok(!html.includes('$0.00</td>'));assert.ok(html.includes('Year-specific form bindings checked'));assert.equal(JSON.stringify(r),original);
  assert.throws(()=>api.recordsHtml('historical-lines',[{...r,year_mapping_review:{...review,profile:'dtmax-T1-E11-additional-years/v1'}}]));
  assert.throws(()=>api.recordsHtml('historical-lines',[{...r,vendor_module:'T3',mapping_state:'pending_module_verification'}]));
});
test('exception retrieval is lazy, scope checked and stops when another workspace is selected',async()=>{
  const h=harness();let resolve;const pending=new Promise(r=>resolve=r);let count=0;
  const c=api.mount(h.host,{request:async path=>{if(path.includes('/source-exceptions?')){count++;return pending;}return page();}});await c.ready;assert.equal(count,0);
  const work=h.node('[data-dtmax-exceptions]').ontoggle({target:{open:true}});assert.equal(count,1);
  c.dispose();h.node('[data-dtmax-exception-content]').innerHTML='Different view';resolve({test_copy:true,source_scope:'dtmax-test-copy',source_rows_modified:0,tax_cases_imported:0,offset:0,limit:25,total:1,records:[exception()]});await work;assert.equal(h.node('[data-dtmax-exception-content]').innerHTML,'Different view');
  const d=api.mount(h.host,{request:async path=>path.includes('/source-exceptions?')?{source_scope:'another-company'}:page()});await d.ready;await h.node('[data-dtmax-exceptions]').ontoggle({target:{open:true}});assert.ok(h.node('[data-dtmax-exception-content]').innerHTML.includes('role="alert"'));
});
