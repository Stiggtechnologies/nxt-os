import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto} from 'node:crypto';
const code=fs.readFileSync(new URL('../public/apps/migrations/source-contact-lists.js',import.meta.url),'utf8');
const ctrl={source_authoritative:true,read_only:true,live_cutover_allowed:false,operational_objects_created:false,payroll_migrated:false};
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return{promise,resolve};};
function harness(kind='customer'){
 let current=true,html='',nodes=new Map(),handler=async()=>{throw Error('Unexpected request');};const requests=[];
 const host={isConnected:true,get innerHTML(){return html;},set innerHTML(v){for(const n of nodes.values())n.isConnected=false;nodes=new Map();html=v;},querySelector(selector){const marker=selector.slice(1,-1);if(!html.includes(marker))return null;if(!nodes.has(selector))nodes.set(selector,{isConnected:true,onclick:null});return nodes.get(selector);}};
 const box={crypto:webcrypto,TextEncoder,Uint8Array,console};vm.runInNewContext(code,box);const api=box.NXTSourceContactLists;
 return{api,host,requests,setRequest:fn=>handler=fn,leave:()=>{current=false;},mount(){return api.mount(host,{company_id:'synthetic-A',kind,isCurrent:()=>current,request:async path=>{requests.push(path);return handler(path);}});}};
}
const clone=v=>structuredClone(v),cell=(value,present=true,formula=null)=>({present,type:present?'s':null,value,formula});
async function fixture(api,kind='customer',records=2){
 const headers=kind==='customer'?{B:'Customer',C:'Phone Numbers',D:'Email',E:'Full Name',F:'Billing Address',G:'Shipping Address'}:{B:'Supplier',C:'Phone Numbers',D:'Email',E:'Full Name',F:'Address',G:'Account #'};
 const fields=kind==='customer'?['display_name','phone_numbers','email','full_name','billing_address','shipping_address']:['display_name','phone_numbers','email','full_name','address','supplier_account_number'];
 const source={archive_sha256:'a'.repeat(64),archive_bytes:1234,report_sha256:'b'.repeat(64),member:kind==='customer'?'Customers.xlsx':'Suppliers.xlsx',worksheet_name:'Sheet1',worksheet_file:'xl/worksheets/sheet1.xml',company_heading_sha256:'c'.repeat(64),captured_at:'2026-10-27T00:00:00Z',capture_basis:'Synthetic capture'};
 const family={format:'nxt-company-contact-family/v2',company_id:'synthetic-A',kind,source,headers,parts:[{sha256:'e'.repeat(64),bytes:4096,records,other_rows:1}],records_total:records,nonrecord_rows_total:1,source_snapshot_id:'synthetic-snapshot',source_manifest_sha256:'d'.repeat(64)};
 const familyHash=await api.sha(family),proof={format:'nxt-source-contact-operator-equality/v1',company_id:'synthetic-A',source_archive_sha256:source.archive_sha256,source_report_sha256:source.report_sha256,family_sha256:familyHash,comparison_manifest_sha256:'f'.repeat(64),source_field_equality_verified:true};
 const command={request_id:'synthetic-request',expected_index_head:null,family,source_operator_proof:proof};const receipt={format:'nxt-company-contact-publication/v1',company_id:'synthetic-A',command,command_sha256:await api.sha(command)},captureHash=await api.sha(receipt);
 const capture={capture_sha256:captureHash,family_sha256:familyHash,record_type:kind,records_total:records,nonrecord_rows_total:1};
 const index={company_id:'synthetic-A',index_head_sha256:'1'.repeat(64),captures:[capture],integrity_scope:'bounded_index_metadata_only',source_roster_complete:false,controls:ctrl};
 const detail={company_id:'synthetic-A',capture_sha256:captureHash,receipt,integrity_scope:'publication_manifest_and_source_snapshot',controls:ctrl};
 async function record(row){const values=[cell('<img src=x onerror=synthetic>'),cell(''),cell(null,false),cell('Synthetic café 😀'),cell('Original address'),cell(kind==='supplier'?'0000042':'Cached original',true,'HYPERLINK("javascript:synthetic")')],raw=Object.fromEntries(['B','C','D','E','F','G'].map((c,i)=>[c,clone(values[i])])),mapped=Object.fromEntries(fields.map((f,i)=>[f,clone(values[i])]));return{cohort_ref:'synthetic-A',record_type:kind,source_record_alias:kind+'_'+await api.sha(['synthetic-A',source.archive_sha256,source.report_sha256,source.worksheet_file,row]),source_id:null,source_id_coverage:'archive_row',source_identity_verified:false,operational_object_created:false,linked_to_journal_contacts:false,formula_fields_present:true,source_archive_sha256:source.archive_sha256,source_report_sha256:source.report_sha256,source_member:source.member,source_worksheet_name:source.worksheet_name,source_worksheet_file:source.worksheet_file,source_row:row,source_fields:mapped,raw_source_cells:raw};}
 const rows=await Promise.all(Array.from({length:records},(_,i)=>record(6+i)));
 function page(offset=0,length=100,other=false){const all=other?[{source_row:99,classification:'retained_nonrecord_or_unclassified_row',raw_cells:{A:cell('<source footer>'),...Object.fromEntries(['B','C','D','E','F','G'].map(k=>[k,cell(null,false)]))}}]:rows;const selected=clone(all.slice(offset,offset+length)),more=offset+selected.length<all.length;return{company_id:'synthetic-A',capture_sha256:captureHash,record_type:kind,offset,max_bytes:786432,total:all.length,records:selected,next_offset:more?offset+selected.length:null,has_more:more,row_set:other?'other_source_rows':'records',parts_checked:1,integrity_scope:'selected_parts_and_publication_manifest',source_roster_complete:false,controls:ctrl};}
 function respond(path){if(path.includes('/captures/'))return clone(detail);if(path.includes('/records?'))return page(Number(new URL('https://synthetic.invalid'+path).searchParams.get('offset')));if(path.includes('/other-rows?'))return page(0,100,true);return clone(index);}
 return{family,capture,index,detail,rows,page,respond};
}

test('canonical hashes use sorted UTF8 JSON and strict scalar strings',async()=>{const h=harness();assert.equal(await h.api.sha({x:1}),'5041bf1f713df204784353e82f6a4a535931cb64f1f4b4a5aeaffcb720918b22');assert.equal(h.api.canonical({z:'café 😀',a:1}),'{"a":1,"z":"café 😀"}');assert.throws(()=>h.api.canonical('\ud800'),/Unicode/);});
test('actual read-export button opens only GET source paths and renders escaped original fields',async()=>{
 const h=harness(),f=await fixture(h.api);h.setRequest(f.respond);const c=h.mount();await c.ready;const button=h.host.querySelector('[data-source-open="'+f.capture.capture_sha256+'"]');assert.ok(button);await button.onclick();
 assert.ok(h.host.innerHTML.includes('&#60;img src=x onerror=synthetic&#62;'));assert.ok(!h.host.innerHTML.includes('<img'));assert.ok(h.host.innerHTML.includes('Blank source cell'));assert.ok(h.host.innerHTML.includes('Not present in export'));assert.ok(h.host.innerHTML.includes('Formula preserved as text'));assert.ok(!h.host.innerHTML.includes('href="javascript:'));assert.ok(h.host.innerHTML.includes('Operational contact migration remains pending'));
 assert.equal(h.requests.length,3);assert.ok(h.requests.every(p=>p.startsWith('/api/company-books/synthetic-A/source-contact-lists')));
});
test('supplier account number remains original source text with no COA or operational identity substitution',async()=>{
 const h=harness('supplier'),f=await fixture(h.api,'supplier');h.setRequest(f.respond);const c=h.mount();await c.ready;await c.open(f.capture.capture_sha256);assert.ok(h.host.innerHTML.includes('0000042'));assert.ok(h.host.innerHTML.includes('Account #'));assert.ok(h.host.innerHTML.includes('do not establish durable contact identities'));assert.equal(h.requests.some(p=>p.includes('/api/ledger')),false);
});
test('wrong company, source member, alias and raw typed cells never reveal a page',async()=>{
 for(const mutate of [p=>p.company_id='foreign',p=>p.records[0].source_member='Suppliers.xlsx',p=>p.records[0].source_record_alias='customer_'+'0'.repeat(64),p=>p.records[0].source_fields.email.value='invented',p=>p.records[0].operational_object_created=true,p=>p.records[0].unknown_field='unsupported']){
  const h=harness(),f=await fixture(h.api);h.setRequest(path=>{if(path.includes('/records?')){const p=f.page();mutate(p);return p;}return f.respond(path);});const c=h.mount();await c.ready;await c.open(f.capture.capture_sha256);assert.ok(h.host.innerHTML.includes('role="alert"'));assert.ok(!h.host.innerHTML.includes('&#60;img src=x onerror=synthetic&#62;'));
 }
});
test('receipt or family hash tampering is rejected before any source record GET',async()=>{
 const h=harness(),f=await fixture(h.api);h.setRequest(path=>{const r=f.respond(path);if(path.includes('/captures/'))r.receipt.command.family.records_total=7;return r;});const c=h.mount();await c.ready;await c.open(f.capture.capture_sha256);assert.ok(h.host.innerHTML.includes('receipt hash differs'));assert.equal(h.requests.some(p=>p.includes('/records?')),false);
});
test('captured zero, no capture and unavailable service have distinct truthful displays',async()=>{
 for(const mode of ['zero','missing','unavailable']){const h=harness(),f=await fixture(h.api,'customer',0);h.setRequest(path=>{if(mode==='unavailable')throw Error('Synthetic service unavailable');if(mode==='missing')return{...f.index,captures:[]};return f.respond(path);});const c=h.mount();await c.ready;if(mode==='zero'){await c.open(f.capture.capture_sha256);assert.ok(h.host.innerHTML.includes('0 contact rows in this frozen export'));assert.ok(h.host.innerHTML.includes('coverage remains unknown'));}if(mode==='missing')assert.ok(h.host.innerHTML.includes('does not prove the source list is empty'));if(mode==='unavailable'){assert.ok(h.host.innerHTML.includes('status is unavailable'));assert.ok(!h.host.innerHTML.includes('0 contact rows'));}}
});
test('real Next and Previous handlers follow exact returned continuation offsets',async()=>{
 const h=harness(),f=await fixture(h.api);h.setRequest(path=>path.includes('/records?')?f.page(Number(new URL('https://synthetic.invalid'+path).searchParams.get('offset')),1):f.respond(path));const c=h.mount();await c.ready;await c.open(f.capture.capture_sha256);
 await h.host.querySelector('[data-source-next]').onclick();assert.ok(h.requests.at(-1).includes('offset=1'));await h.host.querySelector('[data-source-prev]').onclick();assert.ok(h.requests.at(-1).includes('offset=0'));assert.equal(h.requests.some(p=>p.includes('offset=100')),false);
});
test('stalled pagination, duplicate rows and mixed capture pages fail closed',async()=>{
 for(const mutate of [p=>{p.has_more=true;p.next_offset=p.offset;},p=>p.records.push(clone(p.records[0])),p=>p.capture_sha256='0'.repeat(64)]){
  const h=harness(),f=await fixture(h.api);const p=f.page();mutate(p);await assert.rejects(h.api.validatePage(p,'synthetic-A',f.capture,f.family,0,false));
 }
});
test('late index or page work cannot replace another company or detached view',async()=>{
 for(const phase of ['index','page']){const h=harness(),f=await fixture(h.api),gate=deferred();let c,pending;if(phase==='index'){h.setRequest(()=>gate.promise);c=h.mount();pending=c.ready;}else{h.setRequest(f.respond);c=h.mount();await c.ready;h.setRequest(path=>path.includes('/records?')?gate.promise:f.respond(path));pending=c.open(f.capture.capture_sha256);await new Promise(r=>setTimeout(r,10));}
  h.leave();h.host.innerHTML='Different company';gate.resolve(phase==='index'?f.index:f.page());await pending;assert.equal(h.host.innerHTML,'Different company');
 }
});
test('busy and detached old buttons cannot dispatch duplicate requests',async()=>{
 const h=harness(),f=await fixture(h.api),gate=deferred();h.setRequest(f.respond);const c=h.mount();await c.ready;const old=h.host.querySelector('[data-source-open="'+f.capture.capture_sha256+'"]');h.setRequest(path=>path.includes('/captures/')?gate.promise:f.respond(path));const pending=old.onclick();const before=h.requests.length;await old.onclick();assert.equal(h.requests.length,before);gate.resolve(f.detail);await pending;await old.onclick();assert.equal(h.requests.length,before+1);
});
test('retained nonrecord rows render separately with escaped exact source cells',async()=>{
 const h=harness(),f=await fixture(h.api);h.setRequest(f.respond);const c=h.mount();await c.ready;await c.open(f.capture.capture_sha256);await h.host.querySelector('[data-source-other]').onclick();assert.ok(h.host.innerHTML.includes('Retained original source rows'));assert.ok(h.host.innerHTML.includes('&#60;source footer&#62;'));assert.ok(!h.host.innerHTML.includes('<source footer>'));
});
test('retained source rows cannot omit original header cells',async()=>{
 const h=harness(),f=await fixture(h.api);const p=f.page(0,100,true);delete p.records[0].raw_cells.D;
 await assert.rejects(h.api.validatePage(p,'synthetic-A',f.capture,f.family,0,true),/Retained source row differs/);
});
test('index duplicate captures and false scope claims are rejected',async()=>{
 const h=harness(),f=await fixture(h.api);for(const mutate of [d=>d.captures.push(clone(d.captures[0])),d=>d.controls.operational_objects_created=true,d=>d.captures[0].record_type='employee',d=>d.company_id='foreign']){const d=clone(f.index);mutate(d);assert.throws(()=>h.api.validateIndex(d,'synthetic-A'));}
});
