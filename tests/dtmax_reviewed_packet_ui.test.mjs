import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash,webcrypto} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8');
const profile=JSON.parse(fs.readFileSync(new URL('../src/source_import/retained_office_T1_reviewed_packet_profile.json',import.meta.url))).profile_sha256;
const digest=b=>createHash('sha256').update(b).digest('hex');
// Synthetic browser transport only; these bytes are never backend-admitted.
const bytes=Buffer.alloc(2097153,97),sha=digest(bytes);
function record(year=2022,mixed=false){
 return {client_number:'100',tax_year:year,sha256:sha,bytes:bytes.length,page_count:mixed?8:3,page_labels:Array.from({length:mixed?8:3},(_,i)=>'Synthetic source page '+(i+1)),original_print_output:false,immutable:true,
  retained_office_provenance:{kind:'retained_office_T1_reviewed_packet',profile_sha256:profile,...Object.fromEntries(['source_manifest_sha256','source_occurrence_manifest_sha256','owner_year_evidence_sha256','mixed_owner_year_review_sha256','static_pdf_review_sha256','source_metadata_sha256'].map(k=>[k,'a'.repeat(64)])),native_generation_verified:false,as_filed_verified:false,financial_values_approved:false,source_bytes_modified:false},
  reviewed_packet_scope:{source_tax_years:mixed?[2017,2018]:[2022],year_first_page:mixed&&year===2017?5:1,year_last_page:mixed&&year===2018?4:mixed?8:3,federal_core_pages_present:mixed?[1,2,3,4]:[1,2,3],federal_core_pages_expected:mixed?4:8,federal_core_sequence_complete:mixed,marked_amended:mixed,hand_annotations_present:!mixed,full_return_completeness_verified:false},
  content_transport:{kind:'reviewed_scanned_packet_chunks_v1',chunk_bytes:1048576,chunk_count:3}};
}
function harness(rows){
 const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
 const blobs=new Map();let created=0,fault='',hold=null,resolveHeld,started;
 const context={crypto:webcrypto,Blob,URL:{createObjectURL(b){const u='blob:synthetic-packet-'+(++created);blobs.set(u,b);return u;},revokeObjectURL(u){blobs.delete(u);}}};vm.runInNewContext(source,context);
 const envelope={source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0},years=[...new Set(rows.map(r=>r.tax_year))].sort((a,b)=>a-b);
 const request=async path=>{
  if(path.includes('/source-clients?'))return{source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:years,clients:[{client_number:'100',name:'Synthetic packet',historical_years:years,pending_years:[]}]};
  if(path.includes('source-facts?'))return{...envelope,client_number:'100',kind:'historical-lines',offset:0,total:0,records:[]};
  if(path.endsWith('/return-pdfs'))return{...envelope,client_number:'100',records:rows};
  const m=path.match(/\/return-pdfs\/(\d+)\/([a-f0-9]{64})(?:\/chunks\/(\d+))?$/);assert.ok(m);const r=rows.find(r=>r.tax_year===Number(m[1])&&r.sha256===m[2]);assert.ok(r);
  if(m[3]===undefined)return{...envelope,record:r,content_transport:r.content_transport};
  const index=Number(m[3]),part=Buffer.from(bytes.subarray(index*1048576,(index+1)*1048576));
  if(index===0&&fault==='missing')throw Error('Source section unavailable');
  if(index===0&&fault==='altered')part[0]^=1;
  const response={...envelope,client_number:'100',tax_year:r.tax_year,sha256:r.sha256,chunk_index:index,bytes:part.length,chunk_sha256:digest(part),content_hex:part.toString('hex')};
  if(index===0&&fault==='owner')response.client_number='200';
  if(index===0&&hold===r.tax_year){started?.();return new Promise(resolve=>{resolveHeld=()=>resolve(response);});}
  return response;
 };
 const controller=context.NXTDTMaxT1.mount(host,{request});
 return {node,host,controller,blobs,api:context.NXTDTMaxT1,get created(){return created;},fault(v){fault=v;},hold(year,signal){hold=year;started=signal;},release(){resolveHeld();}};
}
test('larger scanned packets require their separate contract and keep partial/completeness qualifications',()=>{
 const h=harness([record()]);h.api.validatePdf(record(),'100');
 for(const mutate of [r=>r.bytes=20971521,r=>r.retained_office_provenance.profile_sha256='0'.repeat(64),r=>r.retained_office_provenance.as_filed_verified=true,r=>r.reviewed_packet_scope.full_return_completeness_verified=true,r=>r.reviewed_packet_scope.federal_core_sequence_complete=true,r=>r.reviewed_packet_scope.year_last_page=4,r=>r.content_transport.chunk_count=2,r=>{delete r.retained_office_provenance;r.original_print_output=true;}]){const r=record();mutate(r);assert.throws(()=>h.api.validatePdf(r,'100'));}
 h.controller.dispose();
});
test('browser reassembles exact original bytes and rejects missing, wrong-owner and whole-hash changes',async()=>{
 const r=record(),h=harness([r]);await h.controller.ready;await h.controller.open('100');await h.controller.show('return');
 const html=h.node('[data-dtmax-pdf-canvas]').innerHTML;assert.match(html,/Partial return: only federal pages 1, 2, 3 of 8/);assert.match(html,/Download unchanged source packet/);assert.match(html,/Missing pages have not been reconstructed/);
 assert.deepEqual(Buffer.from(await [...h.blobs.values()][0].arrayBuffer()),bytes);
 for(const fault of ['missing','owner','altered']){h.fault(fault);const created=h.created;await h.controller.show('return');assert.equal(h.created,created);assert.match(h.node('[data-dtmax-pdf-canvas]').innerHTML,/role="alert"/);assert.ok(!h.node('[data-dtmax-pdf-canvas]').innerHTML.includes('<iframe'));}
 h.controller.dispose();assert.equal(h.blobs.size,0);
});
test('multi-year amended packet opens its year section and discards a stale other-year transfer',async()=>{
 const h=harness([record(2018,true),record(2017,true)]);await h.controller.ready;await h.controller.open('100');
 let start;const signal=new Promise(resolve=>{start=resolve;});h.hold(2018,start);const old=h.controller.show('return',false,{sha,page:1});await signal;
 await h.node('[data-dtmax-year]').onchange({target:{value:'2017'}});await h.controller.show('return',false,{sha,page:5});const html=h.node('[data-dtmax-pdf-canvas]').innerHTML,created=h.created;
 assert.match(html,/Amended return packet/);assert.match(html,/Selected tax year 2017/);assert.match(html,/#page=5/);assert.match(html,/2017-2018\.pdf/);h.release();await old;assert.equal(h.node('[data-dtmax-pdf-canvas]').innerHTML,html);assert.equal(h.created,created);h.controller.dispose();
});
