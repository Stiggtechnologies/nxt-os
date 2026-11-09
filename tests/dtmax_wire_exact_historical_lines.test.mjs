import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash,webcrypto} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8');
const publicBytes=fs.readFileSync(new URL('../public/apps/tax/dtmax-wire-exact-profile.json',import.meta.url));
const profile=JSON.parse(publicBytes),kind='wire-exact-historical-lines',sha=b=>createHash('sha256').update(b).digest('hex');
const canonical=v=>Array.isArray(v)?'['+v.map(canonical).join(',')+']':v!==null&&typeof v==='object'?'{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+canonical(v[k])).join(',')+'}':JSON.stringify(v);
const path=process.env.NXT_WIRE_EXACT_PROJECTION;
const rows=path?fs.readFileSync(path,'utf8').trimEnd().split('\n').map(JSON.parse):null;
function environment(bytes=publicBytes,pause){const calls=[];const sandbox={crypto:webcrypto,TextEncoder,TextDecoder,Uint8Array,DataView,atob,Blob,URL:{createObjectURL:()=> 'blob:local-fixture',revokeObjectURL(){}},fetch:async(url,options)=>{calls.push(url);assert.equal(url,'/apps/tax/dtmax-wire-exact-profile.json');assert.equal(options.credentials,'omit');if(pause){pause.started();await pause.wait;}return{ok:true,arrayBuffer:async()=>bytes.buffer.slice(bytes.byteOffset,bytes.byteOffset+bytes.byteLength)};}};vm.runInNewContext(source,sandbox);return{api:sandbox.NXTDTMaxT1,calls};}
function envelope(records,extra={}){return{test_copy:true,source_scope:'dtmax-test-copy',kind,wire_exact_profile_sha256:profile.profile_sha256,wire_exact_inventory_sha256:profile.contract.original_row_inventory_file_sha256,source_only:true,vendor_activation_required:false,financial_values_approved:false,recalculated:false,source_versions_reselected:false,original_historical_records_modified:0,current_profiles_modified:0,earlier_figures_or_slips_modified:0,native_contexts_created:0,tax_cases_imported:0,records,...extra};}
const lines=['15000','23600','26000','42000','48400','48500'];

test('exact public profile bytes and closed lists are pinned; response safeguards fail closed',async()=>{
 assert.equal(publicBytes.length,135256);assert.equal(sha(publicBytes),'b5f3cef97f3dbdfcd452349b8118702f6e7445b1ac2129d3d40d2e763bf3a09b');
 const {api,calls}=environment();await api.loadWireExactProfile();assert.equal(calls.length,1);await api.loadWireExactProfile();assert.equal(calls.length,1);
 for(const bytes of [Buffer.from('{}'),Buffer.from(publicBytes.toString().replace('993','994')),Buffer.concat([publicBytes,Buffer.from(' ')])])await assert.rejects(()=>environment(bytes).api.loadWireExactProfile());
 api.checkWireEnvelope(envelope([]));for(const mutate of [p=>p.source_scope='other',p=>p.tax_cases_imported=1,p=>p.financial_values_approved=true,p=>p.wire_exact_inventory_sha256='a'.repeat(64),p=>p.source_versions_reselected=true]){const p=envelope([]);mutate(p);assert.throws(()=>api.checkWireEnvelope(p));}
});

test('all actual Rust projections preserve exact values, original rows, module holds and993 proofs',{skip:!rows},async()=>{
 const {api}=environment();let numeric=0,blank=0,changed=0,overlays=0,held=0;const allowed=new Set(),originals=new Set();
 assert.equal(rows.length,32744);
 for(let i=0;i<rows.length;i++){
  const r=rows[i];try{await api.verifyWireExactHistoricalRecord(r,profile);}catch(e){throw Error('Private projection failed at index '+i+': '+e.message);}
  const original=r.original_historical_row;originals.add(sha(canonical(original)));assert.equal(r.precision_state,'verified_exact_wire');assert.ok(canonical(r.reader_values_cents)===canonical(original.values_cents));
  const html=api.recordsHtml(kind,[r]);assert.ok(html.includes('Family source #'));assert.ok(html.includes(r.archive_sha256.slice(0,12)));
  if(r.vendor_module==='T3'&&!r.native_module_review){held++;assert.ok(html.includes('T1 totals are unavailable')||html.includes('candidate values are not shown as T1 totals'));}
  else assert.ok(html.includes('Exact original source cents'));
  if(r.wire_exact_overlay){overlays++;const h=sha(canonical(r.wire_exact_overlay));allowed.add(h);assert.ok(profile.context_sha256.includes(h));assert.ok(canonical(r.values_cents)===canonical(r.wire_exact_overlay.exact_wire_values_cents));}
  else{assert.equal(r.precision_basis,'inventory_exact_unchanged');assert.ok(canonical(r.values_cents)===canonical(r.reader_values_cents));}
  for(const line of lines){if(original.values_cents[line]===null){blank++;assert.equal(r.values_cents[line],null);}else{numeric++;changed+=r.values_cents[line]!==original.values_cents[line];}}
 }
 assert.equal(originals.size,32744);assert.equal(allowed.size,993);assert.equal(overlays,993);assert.equal(numeric,66340);assert.equal(blank,130124);assert.equal(changed,1973);assert.equal(held,88);
 console.log(JSON.stringify({wire_exact_private_rows:32744,overlays,numeric_slots:numeric,absent_slots:blank,precision_differences:changed,held_module_contexts:held}));
});

test('whole original/reader/overlay identity, amounts, source version and flags cannot mutate',{skip:!rows},async()=>{
 const {api}=environment(),original=rows.find(r=>r.wire_exact_overlay);
 const mutations=[r=>r.client_number='999999999',r=>r.family_file_id='999999999',r=>r.tax_year++,r=>r.mapping_state=r.mapping_state==='vendor_static_verified'?'pending_year_verification':'vendor_static_verified',r=>r.vendor_module='T3',r=>r.reader_values_cents['15000']='123',r=>r.original_historical_row.values_cents['15000']='123',r=>r.values_cents['15000']='123',r=>r.wire_exact_overlay.source_selection.reader_q_plan=99,r=>r.wire_exact_overlay.field_observations[0].exact_wire_cents='123',r=>r.wire_exact_overlay_sha256='a'.repeat(64),r=>r.financial_values_approved=true,r=>r.precision_basis='inventory_exact_unchanged',r=>r.wire_exact_overlay=null];
 for(const [index,mutate]of mutations.entries()){const r=structuredClone(original);mutate(r);await assert.rejects(()=>api.verifyWireExactHistoricalRecord(r,profile),'Mutation index '+index);}
 const p=structuredClone(profile);p.context_sha256[0]='a'.repeat(64);await assert.rejects(()=>api.verifyWireExactHistoricalRecord(structuredClone(original),p));
 const r=structuredClone(original);await api.verifyWireExactHistoricalRecord(r,profile);r.values_cents['15000']='123';assert.throws(()=>api.historicalTotalsHtml(r));
});

test('missing required overlays and unknown rows display unavailable, never absent orzero',{skip:!rows},async()=>{
 const {api}=environment();const pending=structuredClone(rows.find(r=>r.wire_exact_overlay));pending.precision_state='pending_required_wire_overlay';pending.precision_basis=null;pending.wire_exact_overlay=null;pending.wire_exact_overlay_sha256=null;pending.values_cents=Object.fromEntries(lines.map(k=>[k,null]));
 await api.verifyWireExactHistoricalRecord(pending,profile);const html=api.historicalTotalsHtml(pending);assert.ok(html.includes('pending verification'));assert.ok(!html.includes('Absent'));assert.ok(!html.includes('$0.00'));assert.ok(!html.includes('data-wire-download="exact"'));
 const unknown=structuredClone(pending);unknown.precision_state='unverified_original_row';unknown.original_historical_row.client_number='999999999';unknown.client_number='999999999';unknown.original_historical_row_sha256=sha(canonical(unknown.original_historical_row));await api.verifyWireExactHistoricalRecord(unknown,profile);assert.ok(api.historicalTotalsHtml(unknown).includes('outside the frozen precision inventory'));
 const invented=structuredClone(pending);invented.values_cents['15000']='0';await assert.rejects(()=>api.verifyWireExactHistoricalRecord(invented,profile));
 const bypass=structuredClone(pending);bypass.precision_state='verified_exact_wire';bypass.precision_basis='inventory_exact_unchanged';bypass.values_cents=bypass.reader_values_cents;await assert.rejects(()=>api.verifyWireExactHistoricalRecord(bypass,profile));
});

test('production default loads exact lane forleft figures andcomparison; original reader stays accessible',{skip:!rows},async()=>{
 const {api}=environment();const row=structuredClone(rows.find(r=>r.wire_exact_overlay)),number=row.client_number,year=row.tax_year;
 const nodes=new Map(),buttons=[{dataset:{sourceKind:'historical-lines'}},{dataset:{sourceKind:kind}}],all=k=>k==='[data-source-kind]'?buttons:[],node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:all,scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};const requests=[];
 const request=async path=>{requests.push(path);if(path.includes('/source-clients?'))return{source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:[year],clients:[{client_number:number,name:'Local fixture display',historical_years:[year],pending_years:[]}]};
  const u=new URL(path,'https://local.invalid');const k=u.searchParams.get('kind');assert.equal(k===kind||k==='historical-lines',true);const extra={client_number:number,offset:Number(u.searchParams.get('offset')),limit:Number(u.searchParams.get('limit')),total:1,source_return_year:u.searchParams.has('source_return_year')?year:null};return k===kind?envelope([row],extra):{test_copy:true,tax_cases_imported:0,kind:k,records:[row.original_historical_row],...extra};};
 const c=api.mount(host,{request});await c.ready;await c.open(number);assert.ok(requests.filter(p=>p.includes('source-facts?')).every(p=>p.includes('kind='+kind)));assert.ok(host.innerHTML.includes('Exact original source cents'));assert.ok(node('[data-dtmax-details]').innerHTML.includes('Original reader figures'));
 await node('[data-dtmax-compare]').onchange({target:{value:String(year)}});assert.ok(node('[data-dtmax-comparison]').innerHTML.includes('Exact original source cents'));await buttons[0].onclick();assert.ok(requests.at(-1).includes('kind=historical-lines'));assert.ok(node('[data-dtmax-details]').innerHTML.includes('Original reader figures are retained unchanged'));c.dispose();
});

test('multiple sameyear contexts are explicit and never become a missing-source claim',()=>{
 const {api}=environment(),html=api.entryHtml({client_number:'100',name:'Synthetic',historical_years:[2024]},null,'','2024',2);assert.ok(html.includes('Multiple saved source contexts'));assert.ok(!html.includes('No native extract totals'));
});

test('late exact-profile verification cannot repaint a disposed workspace',{skip:!rows},async()=>{
 let release,started;const wait=new Promise(resolve=>{release=resolve;}),signal=new Promise(resolve=>{started=resolve;});const {api}=environment(publicBytes,{started,wait}),r=structuredClone(rows.find(r=>r.wire_exact_overlay));
 const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[]});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
 const request=async path=>path.includes('/source-clients?')?{source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:[r.tax_year],clients:[{client_number:r.client_number,name:'Local fixture display',historical_years:[r.tax_year],pending_years:[]}]}:envelope([r],{client_number:r.client_number,offset:0,total:1});
 const c=api.mount(host,{request});await c.ready;const pending=c.open(r.client_number);await signal;c.dispose();host.innerHTML='Different source view';release();await pending;assert.equal(host.innerHTML,'Different source view');assert.equal(host.onkeydown,null);
});
