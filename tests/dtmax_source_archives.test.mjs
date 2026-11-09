import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash,webcrypto} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8');
const profile=JSON.parse(fs.readFileSync(new URL('../src/source_import/identification_source_archive_profile.json',import.meta.url))).profile_sha256;
const digest=b=>createHash('sha256').update(b).digest('hex');
// Synthetic browser transport only; these bytes are never backend-admitted.
const bytes=Buffer.alloc(2097153,97),sha=digest(bytes);
function record(year=2022,mixed=false){
 return {client_number:'100',tax_year:year,sha256:sha,bytes:bytes.length,page_count:mixed?8:3,page_labels:Array.from({length:mixed?8:3},(_,i)=>'Synthetic source page '+(i+1)),original_print_output:false,immutable:true,
  retained_office_provenance:{kind:'retained_office_identification_source_archive',profile_sha256:profile,...Object.fromEntries(['source_manifest_sha256','source_occurrence_manifest_sha256','owner_year_evidence_sha256','mixed_owner_year_review_sha256','static_pdf_review_sha256','source_metadata_sha256'].map(k=>[k,'a'.repeat(64)])),native_generation_verified:false,as_filed_verified:false,financial_values_approved:false,source_bytes_modified:false},
  source_archive_scope:{source_role:'T1_identification_archive',verified_source_pages:[{page:mixed&&year===2017?5:1,layout:'regular',marked_amended:mixed,annotations_visible:false}],same_year_native_context_present_at_source_audit:!mixed,remaining_packet_pages_unverified:true,full_return_completeness_verified:false},
  content_transport:{kind:'identification_source_archive_chunks_v1',chunk_bytes:1048576,chunk_count:3}};
}
function harness(rows){
 const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
 const blobs=new Map();let created=0,fault='',hold=null,resolveHeld,started;
 const context={crypto:webcrypto,TextEncoder,Blob,URL:{createObjectURL(b){const u='blob:synthetic-packet-'+(++created);blobs.set(u,b);return u;},revokeObjectURL(u){blobs.delete(u);}}};vm.runInNewContext(source,context);
 const envelope={source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0},years=[...new Set(rows.map(r=>r.tax_year))].sort((a,b)=>a-b);
 const request=async path=>{
  if(path.includes('/source-clients?'))return{source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:years,clients:[{client_number:'100',name:'Synthetic packet',historical_years:years,pending_years:[],native_historical_years:[],preserved_pdf_figure_years:[],source_archive_years:years,pdf_only_years:years}]};
  if(path.includes('source-facts?'))return{...envelope,client_number:'100',kind:'historical-lines',offset:0,total:0,records:[]};
  if(path.endsWith('/return-pdfs'))return{...envelope,client_number:'100',records:rows};
  const m=path.match(/\/return-pdfs\/(\d+)\/([a-f0-9]{64})(?:\/archive-chunks\/(\d+))?$/);assert.ok(m);const r=rows.find(r=>r.tax_year===Number(m[1])&&r.sha256===m[2]);assert.ok(r);
  if(m[3]===undefined)return{...envelope,record:r,content_transport:r.content_transport};
  const index=Number(m[3]),part=Buffer.from(bytes.subarray(index*1048576,(index+1)*1048576));
  if(index===0&&fault==='missing')throw Error('Source section unavailable');
  if(index===0&&fault==='altered')part[0]^=1;
  const response={...envelope,client_number:'100',tax_year:r.tax_year,sha256:r.sha256,chunk_index:index,bytes:part.length,chunk_sha256:digest(part),content_hex:part.toString('hex')};
  if(index===0&&fault==='owner')response.client_number='200';
  if(index===0&&hold===r.tax_year){started?.();return new Promise(resolve=>{resolveHeld=()=>resolve(response);});}
  return response;
 };
 // This synthetic document harness explicitly exercises the preserved reader
 // lane; private exact-inventory projections cover the production default.
 const controller=context.NXTDTMaxT1.mount(host,{request,historicalKind:'historical-lines'});
 return {node,host,controller,blobs,api:context.NXTDTMaxT1,get created(){return created;},fault(v){fault=v;},hold(year,signal){hold=year;started=signal;},release(){resolveHeld();}};
}

test('archive role is separately bounded and rejects forged scope and provenance',()=>{
 const h=harness([record()]);h.api.validatePdf(record(),'100');
 for(const mutate of [r=>r.bytes=67108865,r=>r.retained_office_provenance.profile_sha256='0'.repeat(64),r=>r.retained_office_provenance.financial_values_approved=true,r=>r.source_archive_scope.full_return_completeness_verified=true,r=>r.source_archive_scope.verified_source_pages[0].page=0,r=>r.source_archive_scope.remaining_packet_pages_unverified=false,r=>r.source_archive_scope.source_role='complete_T1',r=>r.content_transport.chunk_count=2,r=>{delete r.retained_office_provenance;r.original_print_output=true;},r=>r.reviewed_packet_scope={}]){const r=record();mutate(r);assert.throws(()=>h.api.validatePdf(r,'100'));}
 h.controller.dispose();
});

test('additional closed profile accepts an unclassified original layout without widening earlier profile',()=>{
 const r=record(),h=harness([r]);r.retained_office_provenance.profile_sha256='3985c4e4c02638f4abdceb6723fc8b37fea46db25c54306b4da799a5c53a7de4';r.source_archive_scope.verified_source_pages[0].layout='unclassified';h.api.validatePdf(r,'100');
 const old=structuredClone(r);old.retained_office_provenance.profile_sha256=profile;assert.throws(()=>h.api.validatePdf(old,'100'));
 for(const mutate of [r=>r.retained_office_provenance.profile_sha256='0'.repeat(64),r=>r.source_archive_scope.verified_source_pages[0].layout='guessed',r=>r.source_archive_scope.remaining_packet_pages_unverified=false,r=>r.retained_office_provenance.as_filed_verified=true,r=>r.retained_office_provenance.financial_values_approved=true]){const b=structuredClone(r);mutate(b);assert.throws(()=>h.api.validatePdf(b,'100'));}h.controller.dispose();
});

test('additional original uses unchanged source-only viewer and opens its verified physical page',async()=>{
 const r=record(2017,true);r.retained_office_provenance.profile_sha256='3985c4e4c02638f4abdceb6723fc8b37fea46db25c54306b4da799a5c53a7de4';r.source_archive_scope.verified_source_pages[0].layout='unclassified';const h=harness([r]);await h.controller.ready;await h.controller.open('100');
 const html=h.node('[data-dtmax-pdf-canvas]').innerHTML;assert.match(html,/#page=5/);assert.match(html,/Only the listed identification pages/);assert.match(html,/different clients or years/);assert.match(html,/no native return or figures have been created/);assert.match(html,/Financial values and filed-copy status remain unverified/);assert.deepEqual(Buffer.from(await [...h.blobs.values()][0].arrayBuffer()),bytes);h.controller.dispose();
});
test('source archive year overlay rejects null, duplicate and fabricated year membership',()=>{
 const h=harness([record()]),p={source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:[2022],clients:[{client_number:'100',name:'Synthetic',historical_years:[2022],pending_years:[],native_historical_years:[],preserved_pdf_figure_years:[],source_archive_years:[2022],pdf_only_years:[2022]}]};
 h.api.validatePage(p,0);for(const value of [null,[2022,2022],[2005]]){const bad=structuredClone(p);bad.clients[0].source_archive_years=value;assert.throws(()=>h.api.validatePage(bad,0));}h.controller.dispose();
});
test('archive-only year opens verified page; all original bytes and unreviewed-body notice survive transport',async()=>{
 const h=harness([record(2017,true)]);await h.controller.ready;await h.controller.open('100');
 const html=h.node('[data-dtmax-pdf-canvas]').innerHTML;assert.match(html,/Only the listed identification pages/);assert.match(html,/different clients or years/);assert.match(html,/no native return or figures have been created/);assert.match(html,/#page=5/);assert.match(html,/Download unchanged original archive/);assert.deepEqual(Buffer.from(await [...h.blobs.values()][0].arrayBuffer()),bytes);
 for(const fault of ['missing','owner','altered']){h.fault(fault);const created=h.created;await h.controller.show('return');assert.equal(h.created,created);assert.match(h.node('[data-dtmax-pdf-canvas]').innerHTML,/role="alert"/);assert.ok(!h.node('[data-dtmax-pdf-canvas]').innerHTML.includes('<iframe'));}
 h.controller.dispose();
});
test('2005 summary remains summary plus slips and creates no complete-return claim',async()=>{
 const r=record(2005),s=r.source_archive_scope;s.source_role='tax_summary_and_supporting_slips';s.verified_source_pages[0].layout='summary';s.same_year_native_context_present_at_source_audit=false;s.remaining_packet_pages_unverified=false;
 const h=harness([r]);await h.controller.ready;await h.controller.open('100');const html=h.node('[data-dtmax-pdf-canvas]').innerHTML;assert.match(html,/Tax summary and supporting slips/);assert.match(html,/complete federal T1 return has not been recovered/);assert.match(html,/no native return or figures have been created/);assert.deepEqual(Buffer.from(await [...h.blobs.values()][0].arrayBuffer()),bytes);h.controller.dispose();
});
test('archive transfer for a previous source year cannot replace the newly selected year',async()=>{
 const h=harness([record(2018,true),record(2017,true)]);await h.controller.ready;await h.controller.open('100');
 let start;const signal=new Promise(resolve=>{start=resolve;});h.hold(2018,start);const old=h.controller.show('return',false,{sha,page:1});await signal;
 await h.node('[data-dtmax-year]').onchange({target:{value:'2017'}});const html=h.node('[data-dtmax-pdf-canvas]').innerHTML,created=h.created;assert.match(html,/Tax year 2017/);assert.match(html,/#page=5/);h.release();await old;assert.equal(h.node('[data-dtmax-pdf-canvas]').innerHTML,html);assert.equal(h.created,created);h.controller.dispose();
});

const mixedProfile='e14edac6ccc8d3d1067997ce45b858ec943317e03f988bbc47cedae384c21418';
function mixedGeneral(){const r=record(2015,true);r.retained_office_provenance.profile_sha256=mixedProfile;Object.assign(r.source_archive_scope,{known_mixed_client_packet:true,source_name_match:false,source_name_qualification:'first_name_spelling_variation'});return r;}
function printedSlip(){const r=record(2007,true);r.retained_office_provenance.profile_sha256=mixedProfile;Object.assign(r.source_archive_scope,{source_role:'printed_slip_identification_archive',verified_source_pages:[{page:5,layout:'unclassified',marked_amended:false,annotations_visible:true}]});const o={schema:'dtmax-printed-T5007-source-observations/v1',form:'T5007',printed_tax_year:2007,source_original_page:5,source_observations_sha256:'d268925cdadf4cc5438c7388a66ad444bf17f01dd9b500cdb1e4a76cb05909af',physical_copies:['2','3'].map(copy_id=>({copy_id,box10:{state:'amount_printed',raw_printed_amount:'$123.45',amount_cents:12345},box11:{state:'no_amount_printed_not_zero',raw_printed_amount:null,amount_cents:null}})),recipient_identity:'matched_to_original_source_without_identifier_disclosure',recipient_masked_SIN:'***-***-1234',DOB_not_printed:true,current_case_owner_assignment:null,same_year_native_context_present:false,full_packet_owner_or_year_claimed:false,duplicate_copies_not_summed:true,T1_lines_inferred:false,normalized_slip_created:false,financial_values_approved:false,identity_witnesses_sha256:'a'.repeat(64),source_selected_page_render_sha256:'b'.repeat(64),copies_are_two_physical_occurrences_on_one_page:true};r.source_archive_scope.printed_slip_observations_json=JSON.stringify(o);r.source_archive_scope.printed_slip_observations_sha256=digest(r.source_archive_scope.printed_slip_observations_json);return r;}
test('third archive keeps mixed owners and first-name variation qualified without changing earlier methods',async()=>{
 const r=mixedGeneral(),h=harness([r]);h.api.validatePdf(r,'100');await h.controller.ready;await h.controller.open('100');const html=h.node('[data-dtmax-pdf-canvas]').innerHTML;assert.match(html,/contains pages for multiple clients/);assert.match(html,/first-name cell differs/);assert.match(html,/No client profile name was changed/);assert.match(html,/Only the listed identification pages/);for(const mutate of [r=>r.retained_office_provenance.profile_sha256=profile,r=>r.source_archive_scope.known_mixed_client_packet=false,r=>r.source_archive_scope.source_name_match=true,r=>delete r.source_archive_scope.source_name_qualification]){const b=structuredClone(r);mutate(b);assert.throws(()=>h.api.validatePdf(b,'100'));}h.controller.dispose();
});
test('printed T5007 source shows separate copies and explicit blank box without creating a return or normalized slip',async()=>{
 const r=printedSlip(),h=harness([r]);h.api.validatePdf(r,'100');await h.controller.ready;await h.controller.open('100');const html=h.node('[data-dtmax-pdf-canvas]').innerHTML;assert.match(html,/Printed T5007 source archive/);assert.match(html,/Copy 2/);assert.match(html,/Copy 3/);assert.match(html,/Blank · not zero/);assert.match(html,/not summed/);assert.match(html,/#page=5/);assert.match(html,/Birth date is not printed/);assert.deepEqual(Buffer.from(await [...h.blobs.values()][0].arrayBuffer()),bytes);h.controller.dispose();
});
test('printed slip refuses raw identifiers, blank-as-zero, copy aggregation, invented native context and altered evidence hash',async()=>{
 const r=printedSlip(),h=harness([r]);for(const mutate of [o=>o.recipient_masked_SIN='123456789',o=>o.physical_copies[0].box11.amount_cents=0,o=>o.physical_copies[0].box10.amount_cents=24690,o=>o.current_case_owner_assignment='100',o=>o.same_year_native_context_present=true,o=>o.printed_tax_year=2008,o=>o.raw_name='Synthetic']){const b=structuredClone(r),o=JSON.parse(b.source_archive_scope.printed_slip_observations_json);mutate(o);b.source_archive_scope.printed_slip_observations_json=JSON.stringify(o);assert.throws(()=>h.api.validatePdf(b,'100'));}const b=structuredClone(r);b.source_archive_scope.printed_slip_observations_sha256='0'.repeat(64);const bad=harness([b]);await bad.controller.ready;await bad.controller.open('100');assert.match(bad.node('[data-dtmax-pdf-canvas]').innerHTML,/failed hash verification/);assert.equal(bad.created,0);h.controller.dispose();bad.controller.dispose();
});
