import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {createHash,webcrypto} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8');
const sandbox={};vm.runInNewContext(source,sandbox);const api=sandbox.NXTDTMaxT1;
const page=()=>({source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:[2013,2025],clients:[{client_number:'100',name:'Synthetic <img>',historical_years:[2013,2025],pending_years:[2013]}]});
const officeProfile=JSON.parse(fs.readFileSync(new URL('../src/source_import/retained_office_T1_profile.json',import.meta.url),'utf8')).profile_sha256;
const stableOfficeProfile=JSON.parse(fs.readFileSync(new URL('../src/source_import/retained_office_T1_stable_identifier_profile.json',import.meta.url),'utf8')).profile_sha256;
const scannedOfficeProfile=JSON.parse(fs.readFileSync(new URL('../src/source_import/retained_office_T1_scanned_profile.json',import.meta.url),'utf8')).profile_sha256;
function scannedOfficePdf(record){const p=officePdf(record);p.retained_office_provenance.kind='retained_office_T1_scanned_document';p.retained_office_provenance.profile_sha256=scannedOfficeProfile;return p;}
function stableOfficePdf(record){const p=officePdf(record);p.retained_office_provenance.kind='retained_office_T1_stable_identifier_document';p.retained_office_provenance.profile_sha256=stableOfficeProfile;return p;}
function officePdf(record){
  const p=structuredClone(record);delete p.export_provenance;p.original_print_output=false;
  p.retained_office_provenance={kind:'retained_office_T1_document',profile_sha256:officeProfile,source_manifest_sha256:'a'.repeat(64),source_occurrence_manifest_sha256:'b'.repeat(64),owner_year_evidence_sha256:'c'.repeat(64),mixed_owner_year_review_sha256:'d'.repeat(64),static_pdf_review_sha256:'e'.repeat(64),source_metadata_sha256:'f'.repeat(64),native_generation_verified:false,as_filed_verified:false,financial_values_approved:false,source_bytes_modified:false};return p;
}
const nativeView=()=>({client_number:'100',family_file_id:'999',source_return_year:2025,vendor_module:'T1',vendor_slip_kind:'T4',vendor_slip_ordinal:0,source_view_number:1,from_data_segment:false,source_archive_sha256:'a'.repeat(64),source_view_file_sha256:'b'.repeat(64),source_view_line_sha256:'c'.repeat(64),financial_profile_sha256:'4bb64f7185d758cc4a1ccacd3a62b6a00b9656d320ca7412b9fe2598d68d69fc',original_field_count:5,field_labels:{13908:'Box 16 · CPP contributions'},financial_fields:[{source_field_index:1,field_id:13908,source_type_tag:4,value:{kind:'text',text:'123.45'}}],economic_slip_uniqueness_verified:false,normalized_tax_slip_created:false,taxpayer_allocations_applied:false,financial_values_approved:false,selection_review:'Source order and cached origin retained.'});

// Entirely synthetic printed observations; no private source values or identities.
function printedObservation(){
  const codes=['15000','23600','26000','42000','48400','48500'],comparison=Object.fromEntries(codes.map(c=>[c,'comparison_unavailable_source_absent']));
  const backup='7b760c8406ac29f6942898818c28be03c823128fdb164b5b03885f9f37b0eed5',archive='499bb5f906222614ef4abc60b84ff4c2d5149f529baeb43f0e18eb14c101037c',csv='c16c0bdc3036932f54e09cdd698513344c945cc4e8ba006cca54cd2fe8f93a94';
  const observations=codes.map((line,i)=>{
    const present=i<3,blank=i===3,held=i===4,state=present?'single_distinct_printed_amount':blank?'identified_printed_blank':held?'held_ambiguous_printed_amount':'printed_line_not_located';
    return {line_code:line,state,printed_cents:present?['0','-1','12345'][i]:null,printed_decimal:present?['0.00','-0.01','123.45'][i]:null,
      printed_candidates_json:JSON.stringify(i===5?[]:[{page_index:0,printed_code:line,line_code_bbox:[10,10,20,20],state:present?'printed_amount_present':blank?'identified_printed_amount_blank':'held_ambiguous_money_tokens',raw_money_tokens:present?['0','00']:[],amount:present?'0.00':null}]),
      printed_candidates_sha256:'a'.repeat(64),source_comparison_state:comparison[line],source_presence_state:'absent'};
  });
  return {schema:'dtmax-printed-source-figure-observation/v1',printed_figure_profile_sha256:'502e76e71cc132a5f74154c364ed398e0eae5cbb8c595a2caceff934b2ae0bf6',client_number:'100',family_file_id:'999',source_return_year:2024,
    pdf:{client_number:'100',tax_year:2024,sha256:'b'.repeat(64),bytes:100,page_count:1,page_labels:['Synthetic printed page'],export_provenance:{kind:'cached_native_export',source_backup_sha256:backup,native_family_file_sha256:'c'.repeat(64),export_log_sha256:'d'.repeat(64),exported_at_utc:'2026-11-07T00:00:00Z',recalculated:false,as_filed_verified:false,figures_csv_sha256:csv,figure_comparisons:comparison}},
    anchors:{source_backup_sha256:backup,source_archive_sha256:archive,source_figures_csv_sha256:csv,native_family_file_sha256:'c'.repeat(64),native_family_file_relative_path:'T1/2024/synthetic.P24',pristine_manifest_sha256:'e'.repeat(64),source_context_row_sha256:'f'.repeat(64),source_qa_file_sha256:'a'.repeat(64),source_qa_record_sha256:'b'.repeat(64),source_geometry_sha256:'c'.repeat(64),geometry_verifier_sha256:'d'.repeat(64),official_cra_form_sha256:null,supplemental_identity_proof_sha256:null},geometry_evidence_json:'{}',observations,
    source_fields_only:true,recalculated:false,as_filed_verified:false,preferred_version_selected:false,financial_values_approved:false};
}
function preservedObservation(year=2010,sha='b'.repeat(64),bytes=100,total='0'){
  const codes=['15000','23600','26000','42000','48400','48500'],printed=['150','236','260','420','484','485'];
  const pdf={client_number:'100',tax_year:year,sha256:sha,bytes,page_count:2,page_labels:['Synthetic first','Synthetic second'],original_print_output:true,immutable:true};
  const observations=codes.map((line,i)=>{const present=i<3,state=present?'single_distinct_printed_amount':i===3?'identified_printed_blank':i===4?'printed_line_not_located':'held_ambiguous_printed_amount',value=present?[total,'-1','12345'][i]:null,n=value===null?0n:BigInt(value),a=n<0n?-n:n,decimal=value===null?null:(n<0n?'-':'')+(a/100n)+'.'+(a%100n).toString().padStart(2,'0');
    const candidate={page_index:0,printed_code:printed[i],line_code_bbox:[10,10,20,20],amount:decimal,state:present?'printed_amount_present':i===3?'identified_printed_amount_blank':'held_ambiguous_money_tokens'};const candidates=i===4?[]:[candidate];if(i===0)candidates.push({...candidate,page_index:1});const raw=JSON.stringify(candidates);return{line_code:line,printed_line_code:printed[i],state,printed_cents:value,printed_decimal:decimal,printed_candidates_json:raw,printed_candidates_sha256:createHash('sha256').update(raw).digest('hex')};});
  const anchors=Object.fromEntries(['pdf_record_sha256','source_occurrence_manifest_sha256','owner_year_evidence_sha256','native_context_absence_audit_sha256','printed_line_authority_sha256','source_geometry_sha256','geometry_verifier_sha256','source_money_evidence_sha256','independent_review_sha256'].map(k=>[k,'a'.repeat(64)]));anchors.native_context_absence_audit_sha256='fab25b99c5755b88d6767a4068a636c888cd19c35e070dfb2fb12fac6af4cd2b';anchors.printed_line_authority_sha256='cf22e772a77a90221062da1068ff348857c583137fee4a38de377b40b5be956c';
  return{schema:'dtmax-preserved-PDF-figure-observations/v1',preserved_figure_profile_sha256:'4a22f166c06c7f22be26b4aba232a86260f5b22ad82fc5c1c59463c80e3a1d51',client_number:'100',source_return_year:year,source_kind:'preserved_PDF',native_context_state:'absent_at_pinned_source_audit',pdf,anchors,geometry_evidence_json:'{}',observations,source_fields_only:true,recalculated:false,as_filed_verified:false,preferred_version_selected:false,financial_values_approved:false};
}
test('preserved PDF figures keep both same-year copies and distinguish zero, negative, blank and held values',()=>{
  const one=preservedObservation(),two=preservedObservation(2010,'c'.repeat(64),100,'200'),before=JSON.stringify([one,two]),html=api.recordsHtml('preserved-pdf-figures',[one,two]);
  const empty=api.recordsHtml('preserved-pdf-figures',[]);assert.ok(empty.includes('No records of this kind are imported for this client'));assert.ok(!empty.includes('audit found no matching native'));
  assert.ok(html.includes('$0.00'));assert.ok(html.includes('-$0.01'));assert.ok(html.includes('$2.00'));assert.ok(html.includes('Blank on printed page'));assert.ok(html.includes('Printed line not located'));assert.ok(html.includes('Printed amount needs review'));assert.ok(html.includes(one.pdf.sha256)&&html.includes(two.pdf.sha256));assert.ok(html.includes('Printed line 150'));assert.ok(html.includes('Requested line 15000'));assert.ok(html.includes('import-time source audit'));assert.ok(html.includes('Later native source recovery remains possible'));assert.ok(!html.includes('exported without recalculation'));assert.equal(JSON.stringify([one,two]),before);
});
test('PDF-only figures reject invented native context, cached provenance, mismatched codes and null as zero',()=>{
  for(const mutate of [r=>r.family_file_id='999',r=>r.native_context_state='absent',r=>r.pdf.export_provenance=printedObservation().pdf.export_provenance,r=>r.source_return_year=2024,r=>r.observations[0].printed_line_code='15000',r=>r.observations[0].printed_cents=null,r=>r.observations[3].printed_cents='0',r=>r.financial_values_approved=true,r=>r.preferred_version_selected=true,r=>{const c=JSON.parse(r.observations[0].printed_candidates_json);c[1].amount='1.00';r.observations[0].printed_candidates_json=JSON.stringify(c);}]){const r=preservedObservation();mutate(r);assert.throws(()=>api.validatePreservedPdfFigures(r));}
  const p={...page(),available_years:[2010,2024],clients:[{client_number:'100',name:'Synthetic',historical_years:[2010,2024],native_historical_years:[2024],preserved_pdf_figure_years:[2010],pdf_only_years:[2010],pending_years:[]}]};api.validatePage(p,0);
  for(const mutate of [p=>p.clients[0].native_historical_years=[2010,2024],p=>p.clients[0].pdf_only_years=[],p=>p.clients[0].pending_years=[2010],p=>p.clients[0].historical_years=[2024]]){const bad=structuredClone(p);mutate(bad);assert.throws(()=>api.validatePage(bad,0));}
});
test('PDF-only year defaults to all paper copies, restores native years, targets exact pages and suppresses stale year responses',async()=>{
  const buffers=['%PDF-synthetic-old-one','%PDF-synthetic-old-two'].map(s=>Buffer.from(s)),rows=buffers.map((b,i)=>preservedObservation(2010,createHash('sha256').update(b).digest('hex'),b.length,String(100*(i+1))));
  const roster={...page(),available_years:[2010,2024],clients:[{client_number:'100',name:'Synthetic',historical_years:[2010,2024],native_historical_years:[2024],preserved_pdf_figure_years:[2010],pdf_only_years:[2010],pending_years:[]}]};
  const native={client_number:'100',family_file_id:'999',tax_year:2024,vendor_module:'T1',mapping_state:'vendor_static_verified',values_cents:Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map((c,i)=>[c,i===0?'999':null]))};
  let pageButton={dataset:{printedPdfYear:'2010',printedPdfSha:rows[1].pdf.sha256,printedPdfPage:'2'}};const all=k=>k==='[data-printed-pdf-sha]'?[pageButton]:[],nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:all,scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
  const local={crypto:webcrypto,Blob,URL:{createObjectURL:()=> 'blob:synthetic-old-source',revokeObjectURL(){}}};vm.runInNewContext(source,local);let delay=false,resolvePending,started;const calls=[];
  const paper=()=>({source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,client_number:'100',kind:'preserved-pdf-figures',source_return_year:2010,offset:0,total:2,records:rows,preserved_figure_profile_sha256:rows[0].preserved_figure_profile_sha256,original_history_modified:0,original_PDFs_modified:0,current_profiles_modified:0,cached_printed_observations_modified:0,native_contexts_created:0,financial_values_approved:false,preferred_version_selected:false,as_filed_verified:false});
  const request=async path=>{calls.push(path);if(path.includes('/source-clients?'))return roster;if(path.includes('source-facts?')){if(path.includes('kind=preserved-pdf-figures')){assert.ok(path.includes('source_return_year=2010'));if(delay){started?.();return new Promise(resolve=>{resolvePending=()=>resolve(paper());});}return paper();}return{test_copy:true,tax_cases_imported:0,client_number:'100',kind:'historical-lines',offset:0,total:1,records:[native]};}if(path.endsWith('/return-pdfs'))return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,client_number:'100',records:rows.map(r=>r.pdf)};const i=rows.findIndex(r=>path.endsWith('/2010/'+r.pdf.sha256));assert.ok(i>=0);return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,record:rows[i].pdf,content_hex:buffers[i].toString('hex')};};
  const c=local.NXTDTMaxT1.mount(host,{request});await c.ready;await c.open('100');assert.ok(host.innerHTML.includes('$9.99'));
  await node('[data-dtmax-year]').onchange({target:{value:'2010'}});assert.ok(host.innerHTML.includes('No native extract totals'));assert.ok(host.innerHTML.includes('2010 · Preserved PDF'));const detail=node('[data-dtmax-details]').innerHTML;assert.ok(detail.includes(rows[0].pdf.sha256)&&detail.includes(rows[1].pdf.sha256));assert.ok(detail.includes('$1.00')&&detail.includes('$2.00'));assert.ok(!host.innerHTML.match(/<select data-dtmax-compare>[\s\S]*?<\/select>/)?.[0].includes('2010'));
  await pageButton.onclick();assert.ok(calls.at(-1).endsWith('/2010/'+rows[1].pdf.sha256));assert.match(node('[data-dtmax-pdf-canvas]').innerHTML,/#page=2/);assert.match(host.innerHTML,/id="dtmax-tab-return"[^>]+aria-selected="true"/);
  await c.show('entry');await node('[data-dtmax-year]').onchange({target:{value:'2024'}});assert.ok(host.innerHTML.includes('$9.99'));assert.ok(calls.at(-1).includes('kind=historical-lines'));
  delay=true;const signal=new Promise(resolve=>{started=resolve;});const stale=node('[data-dtmax-year]').onchange({target:{value:'2010'}});await signal;await node('[data-dtmax-year]').onchange({target:{value:'2024'}});const current=host.innerHTML,currentDetail=node('[data-dtmax-details]').innerHTML;resolvePending();await stale;assert.equal(host.innerHTML,current);assert.equal(node('[data-dtmax-details]').innerHTML,currentDetail);c.dispose();
});
test('printed cached figures preserve actual observations independently of absent extract figures',()=>{
  const r=printedObservation(),before=JSON.stringify(r),html=api.recordsHtml('printed-cached-figures',[r]);
  assert.ok(html.includes('$0.00'));assert.ok(html.includes('-$0.01'));assert.ok(html.includes('$123.45'));
  assert.ok(html.includes('Blank on printed page'));assert.ok(html.includes('Printed amount needs review'));assert.ok(html.includes('Printed line not located'));
  assert.ok(html.includes('filed-copy status and financial approval are unverified'));assert.ok(html.includes('View printed page 1'));assert.ok(html.includes('data-printed-pdf-sha="'+'b'.repeat(64)+'"'));
  assert.equal(JSON.stringify(r),before);
});
test('printed observation validator rejects owner, year, approval, geometry and null-as-zero changes',()=>{
  for(const mutate of [r=>r.pdf.client_number='200',r=>r.source_return_year=2022,r=>r.financial_values_approved=true,r=>r.preferred_version_selected=true,r=>r.pdf.export_provenance.as_filed_verified=true,r=>r.pdf.export_provenance.figure_comparisons['15000']='approved',r=>r.observations[0].printed_cents='-0',r=>r.observations[0].printed_decimal='1.00',r=>r.observations[3].printed_cents='0',r=>r.observations.reverse(),r=>r.geometry_evidence_json='[]',r=>r.printed_figure_profile_sha256='a'.repeat(64),r=>r.unreviewed='extra',r=>{const c=JSON.parse(r.observations[0].printed_candidates_json);c[0].page_index=1;r.observations[0].printed_candidates_json=JSON.stringify(c);},r=>{const c=JSON.parse(r.observations[0].printed_candidates_json);c[0].line_code_bbox[0]=20001;r.observations[0].printed_candidates_json=JSON.stringify(c);}]){
    const r=printedObservation();mutate(r);assert.throws(()=>api.validatePrintedFigures(r));
  }
});
test('retained office documents require their distinct provenance and never claim native print or filed status',()=>{
  const record={client_number:'100',tax_year:2024,sha256:'a'.repeat(64),bytes:100,page_count:1,page_labels:['Synthetic office document'],original_print_output:true,immutable:true};
  api.validatePdf(record,'100');const office=officePdf(record);api.validatePdf(office,'100');
  for(const mutate of [r=>r.original_print_output=true,r=>r.retained_office_provenance.native_generation_verified=true,r=>r.retained_office_provenance.as_filed_verified=true,r=>r.retained_office_provenance.financial_values_approved=true,r=>r.retained_office_provenance.source_bytes_modified=true,r=>r.retained_office_provenance.owner_year_evidence_sha256='bad',r=>r.retained_office_provenance.extra=true,r=>r.export_provenance=printedObservation().pdf.export_provenance]){const r=structuredClone(office);mutate(r);assert.throws(()=>api.validatePdf(r,'100'));}
  assert.throws(()=>api.validatePdf({...record,original_print_output:false},'100'));
});
test('stable taxpayer source continuity uses its separate kind and contract without weakening the original office method',()=>{
  const original=officePdf({client_number:'100',tax_year:2024,sha256:'a'.repeat(64),bytes:100,page_count:1,page_labels:['Synthetic preserved page'],original_print_output:true,immutable:true}),r=stableOfficePdf(original);
  api.validatePdf(r,'100');api.validatePdf(original,'100');assert.notEqual(stableOfficeProfile,officeProfile);
  for(const mutate of [p=>p.retained_office_provenance.kind='retained_office_T1_document',p=>p.retained_office_provenance.profile_sha256=officeProfile,p=>p.original_print_output=true,p=>p.retained_office_provenance.financial_values_approved=true,p=>p.retained_office_provenance.raw_identifier='unsupported']){const p=structuredClone(r);mutate(p);assert.throws(()=>api.validatePdf(p,'100'));}
  const cross=structuredClone(original);cross.retained_office_provenance.profile_sha256=stableOfficeProfile;assert.throws(()=>api.validatePdf(cross,'100'));
});
test('stable identifier document visibly qualifies the original name match and source-person continuity',async()=>{
  const bytes=Buffer.from('%PDF-synthetic-stable-source'),r=stableOfficePdf({client_number:'100',tax_year:2024,sha256:createHash('sha256').update(bytes).digest('hex'),bytes:bytes.length,page_count:1,page_labels:['Synthetic preserved page'],original_print_output:true,immutable:true});
  const local={crypto:webcrypto,Blob,URL:{createObjectURL:()=> 'blob:synthetic-stable',revokeObjectURL(){}}};vm.runInNewContext(source,local);
  const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
  const roster={...page(),available_years:[2024],clients:[{client_number:'100',name:'Synthetic',historical_years:[2024],pending_years:[]}]};
  const request=async path=>{if(path.includes('/source-clients?'))return roster;if(path.includes('source-facts?'))return{test_copy:true,client_number:'100',kind:'historical-lines',offset:0,total:0,tax_cases_imported:0,records:[]};if(path.endsWith('/return-pdfs'))return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,client_number:'100',records:[r]};assert.ok(path.endsWith('/2024/'+r.sha256));return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,record:r,content_hex:bytes.toString('hex')};};
  const c=local.NXTDTMaxT1.mount(host,{request});await c.ready;await c.open('100');await c.show('return');
  const html=node('[data-dtmax-pdf-canvas]').innerHTML,index=node('[data-dtmax-pdf-index]').innerHTML;assert.ok(html.includes('Original name match remains unverified; taxpayer identifier continuity matched'));assert.ok(html.includes('continuity with the archived source, not government identity verification or a legal name change'));assert.ok(html.includes('native generation, completeness of the original return and filed-copy status are unverified'));assert.ok(index.includes('Taxpayer identifier continuity; original name match unverified'));assert.ok(index.includes('Native generation, return completeness and filed-copy status unverified'));assert.ok(html.includes('Download retained office T1'));assert.ok(!html.includes('Original DT Max printed return'));c.dispose();
});
test('scan evidence has a third closed kind and cannot claim either earlier office method',()=>{
  const r=scannedOfficePdf({client_number:'100',tax_year:2018,sha256:'a'.repeat(64),bytes:100,page_count:4,page_labels:['One','Two','Three','Four'],original_print_output:true,immutable:true});api.validatePdf(r,'100');assert.notEqual(scannedOfficeProfile,officeProfile);assert.notEqual(scannedOfficeProfile,stableOfficeProfile);
  for(const mutate of [p=>p.retained_office_provenance.kind='retained_office_T1_document',p=>p.retained_office_provenance.kind='retained_office_T1_stable_identifier_document',p=>p.retained_office_provenance.profile_sha256=officeProfile,p=>p.retained_office_provenance.profile_sha256=stableOfficeProfile,p=>p.original_print_output=true,p=>p.retained_office_provenance.native_generation_verified=true]){const bad=structuredClone(r);mutate(bad);assert.throws(()=>api.validatePdf(bad,'100'));}
});
test('scan labels qualify visual source continuity in selector, notice, download and iframe',async()=>{
  const bytes=Buffer.from('%PDF-synthetic-original-scan'),r=scannedOfficePdf({client_number:'100',tax_year:2018,sha256:createHash('sha256').update(bytes).digest('hex'),bytes:bytes.length,page_count:1,page_labels:['Synthetic scan page'],original_print_output:true,immutable:true});
  const local={crypto:webcrypto,Blob,URL:{createObjectURL:()=> 'blob:synthetic-scan',revokeObjectURL(){}}};vm.runInNewContext(source,local);
  const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
  const roster={...page(),available_years:[2018],clients:[{client_number:'100',name:'Synthetic',historical_years:[2018],pending_years:[]}]};
  const request=async path=>{if(path.includes('/source-clients?'))return roster;if(path.includes('source-facts?'))return{test_copy:true,client_number:'100',kind:'historical-lines',offset:0,total:0,tax_cases_imported:0,records:[]};if(path.endsWith('/return-pdfs'))return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,client_number:'100',records:[r]};assert.ok(path.endsWith('/2018/'+r.sha256));return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,record:r,content_hex:bytes.toString('hex')};};
  const c=local.NXTDTMaxT1.mount(host,{request});await c.ready;await c.open('100');await c.show('return');const html=node('[data-dtmax-pdf-canvas]').innerHTML;
  assert.ok(node('[data-dtmax-pdf-index]').innerHTML.includes('Retained office T1 scan'));assert.ok(html.includes('Visual review linked these pages to the archived tax source'));assert.ok(html.includes('This establishes source continuity; government identity, filed-copy status and financial values are unverified'));assert.ok(html.includes('Download retained office T1 scan'));assert.ok(html.includes('retained-office-t1-scan-100-2018.pdf'));assert.ok(html.includes('title="Retained office T1 scan:'));assert.ok(!html.includes('Original DT Max printed return'));assert.ok(!html.includes('No recalculation'));c.dispose();
});
test('default same-year native and office copies show a chooser without a preferred document',async()=>{
  const buffers=['%PDF-native-synthetic','%PDF-office-synthetic'].map(s=>Buffer.from(s)),base=buffers.map(b=>({client_number:'100',tax_year:2024,sha256:createHash('sha256').update(b).digest('hex'),bytes:b.length,page_count:2,page_labels:['Synthetic first','Synthetic second'],original_print_output:true,immutable:true}));
  base[0].export_provenance=printedObservation().pdf.export_provenance;const records=[base[0],officePdf(base[1])];
  const local={crypto:webcrypto,Blob,URL:{createObjectURL:()=> 'blob:synthetic-office',revokeObjectURL(){}}};vm.runInNewContext(source,local);const ui=local.NXTDTMaxT1;
  const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};let downloads=0;
  const roster={...page(),available_years:[2024],clients:[{client_number:'100',name:'Synthetic',historical_years:[2024],pending_years:[]}]};
  const request=async path=>{if(path.includes('/source-clients?'))return roster;if(path.includes('source-facts?'))return{test_copy:true,client_number:'100',kind:'historical-lines',offset:0,total:0,tax_cases_imported:0,records:[]};if(path.endsWith('/return-pdfs'))return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,client_number:'100',records};const i=records.findIndex(r=>path.endsWith('/'+r.sha256));assert.ok(i>=0);downloads++;return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,record:records[i],content_hex:buffers[i].toString('hex')};};
  const c=ui.mount(host,{request});await c.ready;await c.open('100');await c.show('return');
  assert.equal(downloads,0);const choose=node('[data-dtmax-pdf-canvas]').innerHTML;assert.ok(choose.includes('Choose a preserved document'));assert.ok(choose.includes('Cached native export'));assert.ok(choose.includes('Retained office T1'));assert.ok(!choose.includes('not imported'));
  await c.show('return',false,{sha:records[1].sha256,page:2});assert.equal(downloads,1);const html=node('[data-dtmax-pdf-canvas]').innerHTML;
  assert.match(html,/#page=2/);assert.ok(html.includes('Preserved as supplied; native generation, completeness of the original return and filed-copy status are unverified'));assert.ok(html.includes('Download retained office T1'));assert.ok(html.includes('retained-office-t1-100-2024.pdf'));assert.ok(html.includes('title="Retained office T1 document:'));
  assert.ok(!html.includes('Original DT Max printed return'));assert.ok(!html.includes('No recalculation'));assert.match(host.innerHTML,/id="dtmax-tab-return"[^>]+aria-selected="true"/);c.dispose();
});
test('switching from an open PDF to another year with multiple copies clears the old selection and page controls',async()=>{
  const buffers=['%PDF-synthetic-2024','%PDF-synthetic-2023-native','%PDF-synthetic-2023-office'].map(s=>Buffer.from(s));
  const records=buffers.map((b,i)=>({client_number:'100',tax_year:i===0?2024:2023,sha256:createHash('sha256').update(b).digest('hex'),bytes:b.length,page_count:2,page_labels:['Synthetic first','Synthetic second'],original_print_output:true,immutable:true}));records[2]=officePdf(records[2]);
  let revoked=0,downloads=0;const local={crypto:webcrypto,Blob,URL:{createObjectURL:()=> 'blob:synthetic-cross-year',revokeObjectURL(){revoked++;}}};vm.runInNewContext(source,local);
  const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
  const roster={...page(),available_years:[2023,2024],clients:[{client_number:'100',name:'Synthetic',historical_years:[2023,2024],pending_years:[]}]};
  const request=async path=>{if(path.includes('/source-clients?'))return roster;if(path.includes('source-facts?'))return{test_copy:true,client_number:'100',kind:'historical-lines',offset:0,total:0,tax_cases_imported:0,records:[]};if(path.endsWith('/return-pdfs'))return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,client_number:'100',records};const i=records.findIndex(r=>path.endsWith('/'+r.sha256));assert.ok(i>=0);downloads++;return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,record:records[i],content_hex:buffers[i].toString('hex')};};
  const c=local.NXTDTMaxT1.mount(host,{request});await c.ready;await c.open('100');await c.show('return');assert.equal(downloads,1);assert.match(node('[data-dtmax-pdf-index]').innerHTML,/ selected/);assert.match(node('[data-dtmax-pdf-index]').innerHTML,/data-dtmax-page/);
  node('[data-dtmax-year]').onchange({target:{value:'2023'}});await new Promise(setImmediate);
  assert.equal(downloads,1);assert.equal(revoked,1);const index=node('[data-dtmax-pdf-index]').innerHTML;assert.doesNotMatch(index,/ selected/);assert.doesNotMatch(index,/data-dtmax-page/);assert.match(node('[data-dtmax-pdf-canvas]').innerHTML,/Choose a preserved document/);assert.match(node('[data-dtmax-pdf-canvas]').innerHTML,/available for 2023/);c.dispose();
});
test('choosing another year directly in the document selector updates the workspace year as well as the PDF',async()=>{
  const buffers=['%PDF-synthetic-2024','%PDF-synthetic-2023-office'].map(s=>Buffer.from(s)),records=buffers.map((b,i)=>({client_number:'100',tax_year:2024-i,sha256:createHash('sha256').update(b).digest('hex'),bytes:b.length,page_count:2,page_labels:['Synthetic first','Synthetic second'],original_print_output:true,immutable:true}));records[1]=officePdf(records[1]);
  const local={crypto:webcrypto,Blob,URL:{createObjectURL:()=> 'blob:synthetic-selector',revokeObjectURL(){}}};vm.runInNewContext(source,local);
  const nodes=new Map(),node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
  const roster={...page(),available_years:[2023,2024],clients:[{client_number:'100',name:'Synthetic',historical_years:[2023,2024],pending_years:[]}]};const requested=[];
  const request=async path=>{requested.push(path);if(path.includes('/source-clients?'))return roster;if(path.includes('source-facts?'))return{test_copy:true,client_number:'100',kind:'historical-lines',offset:0,total:0,tax_cases_imported:0,records:[]};if(path.endsWith('/return-pdfs'))return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,client_number:'100',records};const i=records.findIndex(r=>path.endsWith('/'+r.sha256));assert.ok(i>=0);return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,record:records[i],content_hex:buffers[i].toString('hex')};};
  const c=local.NXTDTMaxT1.mount(host,{request});await c.ready;await c.open('100');await c.show('return');assert.match(host.innerHTML,/Tax year 2024/);
  await node('[data-dtmax-pdf-version]').onchange({target:{value:'1'}});
  assert.match(host.innerHTML,/Tax year 2023/);assert.match(host.innerHTML,/<option selected>2023<\/option>/);assert.ok(requested.at(-1).endsWith('/2023/'+records[1].sha256));assert.match(node('[data-dtmax-pdf-canvas]').innerHTML,/#page=1/);assert.match(node('[data-dtmax-pdf-canvas]').innerHTML,/Retained office T1 document/);assert.match(node('[data-dtmax-pdf-index]').innerHTML,/<option value="1" selected>/);c.dispose();
});
test('printed page action selects the exact same-year PDF and return tab, suppressing stale PDF responses',async()=>{
  let urls=0;const local={crypto:webcrypto,Blob,URL:{createObjectURL:()=>`blob:synthetic-${++urls}`,revokeObjectURL(){}}};vm.runInNewContext(source,local);const ui=local.NXTDTMaxT1;
  const roster={source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:2,offset:0,available_years:[2024],clients:['100','200'].map(client_number=>({client_number,name:'Synthetic',historical_years:[2024],pending_years:[]}))};
  const buffers=['%PDF-synthetic-one','%PDF-synthetic-two'].map(s=>Buffer.from(s)),records=buffers.map(b=>({client_number:'100',tax_year:2024,sha256:createHash('sha256').update(b).digest('hex'),bytes:b.length,page_count:2,page_labels:['Synthetic first','Synthetic second'],original_print_output:true,immutable:true,export_provenance:printedObservation().pdf.export_provenance}));
  const observation=printedObservation();observation.pdf=Object.fromEntries(['client_number','tax_year','sha256','bytes','page_count','page_labels','export_provenance'].map(k=>[k,records[1][k]]));
  const candidates=JSON.parse(observation.observations[0].printed_candidates_json);candidates[0].page_index=1;observation.observations[0].printed_candidates_json=JSON.stringify(candidates);
  const kindButton={dataset:{sourceKind:'printed-cached-figures'}},pageButton={dataset:{printedPdfYear:'2024',printedPdfSha:records[1].sha256,printedPdfPage:'2'}};
  const nodes=new Map(),all=k=>k==='[data-source-kind]'?[kindButton]:k==='[data-printed-pdf-sha]'?[pageButton]:[],node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',value:'',querySelector:node,querySelectorAll:all,scrollIntoView(){}});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:()=>[]};
  let delaySha='',resolvePending,started;const requests=[];
  const content=r=>({source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,record:r,content_hex:buffers[records.indexOf(r)].toString('hex')});
  const request=async path=>{requests.push(path);if(path.includes('/source-clients?'))return roster;const client=path.match(/\/clients\/(\d+)\//)?.[1];
    if(path.includes('source-facts?')){const printed=path.includes('kind=printed-cached-figures');return{test_copy:true,source_scope:'dtmax-test-copy',client_number:client,kind:printed?'printed-cached-figures':'historical-lines',offset:0,total:printed?1:0,tax_cases_imported:0,records:printed?[observation]:[],source_return_year:2024,printed_figure_profile_sha256:observation.printed_figure_profile_sha256,original_history_modified:0,original_PDFs_modified:0,current_profiles_modified:0,financial_values_approved:false,preferred_version_selected:false,as_filed_verified:false};}
    if(path.endsWith('/return-pdfs'))return{source_scope:'dtmax-test-copy',test_copy:true,tax_cases_imported:0,client_number:client,records:client==='100'?records:[]};
    const r=records.find(r=>path.endsWith('/'+r.sha256));assert.ok(r);if(r.sha256===delaySha){started?.();return new Promise(resolve=>{resolvePending=()=>resolve(content(r));});}return content(r);
  };
  const c=ui.mount(host,{request});await c.ready;await c.open('100');await kindButton.onclick();
  assert.ok(node('[data-dtmax-details]').innerHTML.includes('Printed cached figures'));await pageButton.onclick();
  assert.ok(requests.at(-1).endsWith('/2024/'+records[1].sha256));assert.match(node('[data-dtmax-pdf-canvas]').innerHTML,/#page=2/);assert.match(host.innerHTML,/id="dtmax-tab-return"[^>]+aria-selected="true"/);
  let signal=new Promise(resolve=>{started=resolve;});delaySha=records[0].sha256;const stale=c.show('return',false,{sha:records[0].sha256,page:1});await signal;await c.show('return',false,{sha:records[1].sha256,page:2});const currentCanvas=node('[data-dtmax-pdf-canvas]').innerHTML;resolvePending();await stale;assert.equal(node('[data-dtmax-pdf-canvas]').innerHTML,currentCanvas);
  signal=new Promise(resolve=>{started=resolve;});const other=c.show('return',false,{sha:records[0].sha256,page:1});await signal;await c.open('200');host.innerHTML='Synthetic different client';node('[data-dtmax-pdf-canvas]').innerHTML='Synthetic different client';resolvePending();await other;assert.equal(host.innerHTML,'Synthetic different client');assert.equal(node('[data-dtmax-pdf-canvas]').innerHTML,'Synthetic different client');c.dispose();
});
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
test('stored Production and federal snapshot versions retain different amounts and repeated occurrences',()=>{
  const r={schema:'dtmax-native-E11-source-variants/v1',source_variant_profile_sha256:'641c2b5f56617882829d5049813e06789a3a661580fe6f6d3adf6f8dde5e03ba',source_archive_sha256:'499bb5f906222614ef4abc60b84ff4c2d5149f529baeb43f0e18eb14c101037c',client_number:'100',family_file_id:'999',source_return_year:2024,reader_module:'T1',native_T1_origin_guard_verified:true,source_context_offset:100,source_EXC_sha256:'a'.repeat(64),legacy_form_mapping:{original_reader_module:'T1',original_history_or_CSV_modified:false},source_variants:[]};
  assert.ok(api.recordsHtml('historical-variants',[r]).includes('No calculated source version was retained'));
  const source={record_type:30,outer_plan:0,personal_prefix_bytes:182,CRC_verified:true,frame_offset:200,frame_sha256:'b'.repeat(64),expanded_sha256:'c'.repeat(64)};
  const field=(wire,index=0)=>({field_id:150,field_id_occurrence:index,type_tag:2,wire_cents:wire,wire_integer:wire,state:wire===0?'explicit_zero':'present_wire_cents',source_decimal:wire===0?'0.00':'12.34',source_text:null,source_field_index:index,source_field_sha256:'d'.repeat(64),source_EXR_frame_offset:200,source_EXR_frame_sha256:'b'.repeat(64),source_expanded_record_sha256:'c'.repeat(64)});
  const form={form_name:'E11',E11_form_ordinal:0,reader_marker_field1_selector_eligible:true,original_field_count:3,marker_field1_occurrences:[{field_id:1}],source_form_header_sha256:'e'.repeat(64),lines:{15000:[field(0),field(1234,1)],23600:null,26000:null,42000:null,48400:null,48500:null}};
  r.source_variants=[{variant_name:'Production',reader_q_plan:0,outer_plan:0,variant_occurrence:0,primary_record_present:true,source_records:[source],E11_forms:[form]}];
  const snapshot=JSON.parse(JSON.stringify(r.source_variants[0]));snapshot.variant_name='SnapshotFederal';snapshot.reader_q_plan=7;snapshot.source_records[0].record_type=40;snapshot.E11_forms[0].lines['15000']=[field(1234)];r.source_variants.push(snapshot);
  const before=JSON.stringify(r),html=api.recordsHtml('historical-variants',[r]);assert.ok(html.includes('Production'));assert.ok(html.includes('Federal snapshot'));assert.ok(html.includes('$0.00'));assert.ok(html.includes('$12.34'));assert.ok(html.includes('Source occurrence 2'));assert.ok(html.includes('Absent'));assert.ok(html.includes('No version has been selected as authoritative'));assert.equal(JSON.stringify(r),before);
  for(const mutate of [r=>r.source_variants[1].reader_q_plan=0,r=>r.source_variants[0].source_records[0].CRC_verified=false,r=>r.source_variants[0].E11_forms[0].lines['15000'][0].source_decimal='12.34',r=>r.source_variants[0].E11_forms[0].lines['15000'][0].state='present_wire_cents',r=>r.source_variants[0].E11_forms[0].lines['15000'].reverse(),r=>r.legacy_form_mapping.original_history_or_CSV_modified=true,r=>r.source_variants[0].variant_name='<img>']){const x=JSON.parse(before);mutate(x);assert.throws(()=>api.recordsHtml('historical-variants',[x]));}
});
test('native financial views retain active text and cached origin without using numeric defaults',()=>{
  const r=nativeView();r.financial_fields[0].value.text='0';let html=api.recordsHtml('native-slip-views',[r]);assert.ok(html.includes('Cached source'));assert.ok(html.includes('>0</span>'));assert.ok(!html.includes('$0.00'));assert.ok(html.includes('Allocations and duplicate removal have not been applied'));
  r.financial_fields[0].value.text=null;html=api.recordsHtml('native-slip-views',[r]);assert.ok(html.includes('Present, blank'));
  r.financial_fields[0].value.text='<img src=x>';html=api.recordsHtml('native-slip-views',[r]);assert.ok(!html.includes('<img'));assert.ok(html.includes('&#60;img'));
  r.financial_fields[0].value={kind:'float32',bits_hex:'00000000'};assert.throws(()=>api.recordsHtml('native-slip-views',[r]));
});
test('native view historical bindings remain pending and unreviewed or normalized profiles fail',()=>{
  const r=nativeView();r.source_return_year=2014;let html=api.recordsHtml('native-slip-views',[r]);assert.ok(html.includes('Historical box binding pending'));assert.ok(!html.includes('Box 16'));
  r.financial_fields=[];r.original_field_count=0;html=api.recordsHtml('native-slip-views',[r]);assert.ok(html.includes('No fields from this financial profile'));
  for(const changed of [{...r,financial_profile_sha256:'0'.repeat(64)},{...r,normalized_tax_slip_created:true},{...r,taxpayer_allocations_applied:true}])assert.throws(()=>api.recordsHtml('native-slip-views',[changed]));
});
test('historical extra fields preserve repeated order and show held year-specific converter labels',()=>{
  const r=nativeView();r.source_return_year=2015;r.supplemental_profile_sha256='17b49048f15e6bac3b0feee2aad55b4b9879716eadb34805c4ad00e8ec06667d';
  r.additional_financial_fields=[2,4].map((i,j)=>({source_field_index:i,field_id:12119,source_type_tag:4,value:{kind:'text',text:j?'4.25':'0'}}));
  r.additional_field_review={authority_matrix_sha256:'e20addca37d6105dfa1fb45b12c136a436c40d40d7bd0b27a94101a9e298345d',source_return_year:2015,box_assignment_approved:false,normalization_approved:false,field_bindings:{12119:{possible_labels:['Motor vehicle benefits','Other <label>'],selector_review_required:true,box_assignment_approved:false,normalization_approved:false}}};
  const html=api.recordsHtml('native-slip-views',[r]);assert.equal((html.match(/Additional recovered field/g)||[]).length,2);assert.ok(html.includes('Conversion selection needs review'));assert.ok(html.includes('Other &#60;label&#62;'));assert.ok(!html.includes('Box 16'));assert.ok(html.indexOf('>0<')<html.indexOf('>4.25<'));
  for(const mutate of [b=>b.supplemental_profile_sha256='0'.repeat(64),b=>b.additional_financial_fields[0].source_field_index=1,b=>b.additional_field_review.source_return_year=2016,b=>b.additional_field_review.box_assignment_approved=true,b=>b.additional_financial_fields[0].value.text=null]){const b=JSON.parse(JSON.stringify(r));mutate(b);assert.throws(()=>api.recordsHtml('native-slip-views',[b]));}
});
test('merged T5008 component keeps separate source values and refuses another context or approval',()=>{
  const r=nativeView(),pin='bd98c9f6157e9cd3cb26befa00957c80a639575ba9bb987c45947fce8ae28358';
  Object.assign(r,{source_return_year:2018,vendor_slip_kind:'T5008',vendor_form:'92',from_data_segment:true});
  r.merged_component_field_review={profile_sha256:pin,source_fields_only:true,box_assignment_approved:false,normalization_approved:false,aggregation_approved:false,allocation_approved:false,financial_values_approved:false};
  const anchors=Object.fromEntries(['client_number','family_file_id','source_return_year','vendor_module','vendor_slip_kind','vendor_slip_ordinal','vendor_form','source_view_number','source_view_file_sha256','source_view_line_sha256','source_archive_sha256','original_field_count','from_data_segment'].map(k=>[k,r[k]]));
  r.merged_component_financial_source={...anchors,schema:'dtmax-native-merged-component-financial-sidecar/v1',merged_component_profile_sha256:pin,primary_view:'vendor_GetT1TaxSlip_preferred_view',raw_forms_file_sha256:'d'.repeat(64),raw_forms_line_sha256:'e'.repeat(64),raw_forms_line_number:1,source_components:[{source_component_index:1,form_name:'171',matching_form_occurrence:0,q_plan:0,from_data_segment:false,selector_keyword_id:null,selector_option_number:null,original_field_count:2,source_component_sha256:'f'.repeat(64)}],financial_fields:[{field_id:14485,source_component_index:1,source_form_field_index:0,merged_field_index:5,source_type_tag:4,source_field_sha256:'a'.repeat(64),value:{kind:'text',text:'<img>12.30'},selector_references:[{binding_ordinal:0,selector_binding_sha256:'b'.repeat(64),source_call:{accessor:'M'}}]}]};
  const before=JSON.stringify(r),html=api.recordsHtml('native-slip-views',[r]);assert.ok(html.includes('Additional T5008 source figures'));assert.ok(html.includes('&#60;img&#62;12.30'));assert.ok(!html.includes('<img>'));assert.equal(JSON.stringify(r),before);
  for(const mutate of [x=>x.merged_component_financial_source.client_number='200',x=>x.merged_component_field_review.allocation_approved=true,x=>x.merged_component_financial_source.source_components[0].matching_form_occurrence=1,x=>x.merged_component_financial_source.financial_fields[0].merged_field_index=0,x=>x.merged_component_financial_source.financial_fields[0].value.text=null]){const x=JSON.parse(before);mutate(x);assert.throws(()=>api.recordsHtml('native-slip-views',[x]));}
});
test('historical personal fields stay year-bound, masked and separate from current profiles',()=>{
  const client={client_number:'100',name:'Synthetic Person',historical_years:[2015]};
  const record={client_number:'100',family_file_id:'999',tax_year:2015,archive_sha256:'a'.repeat(64),vendor_module:'T1',mapping_state:'vendor_static_verified',values_cents:Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map(k=>[k,null]))};
  const fields={};for(const name of ['Relation','SIN','BirthDate','Status','Sex','Prov-Residence','Language'])fields[name]={raw_code:null,candidate:null,state:'present_blank',display_eligible:false};
  fields.Relation={raw_code:'1',candidate:{taxpayer_type:'Principal',relationship:'None'},state:'present_mapped_code',display_eligible:true};fields.SIN={raw_code:null,raw_redacted:true,candidate:'***-***-1234',state:'present_nine_digit_candidate_not_identity_verified',display_eligible:true};fields.BirthDate={raw_code:'04-03-1980',candidate:'1980-03-04',state:'present_exact_date_candidate',display_eligible:true};
  record.source_personal_info={schema:'dtmax-personal-source-display/v1',binding_profile_sha256:'9b7fc393b7632f0baa4e74806883a199c12f7cd0f79ed2dc7c6706c322acc10e',client_number:'100',family_file_id:'999',source_year:2015,archive_sha256:'a'.repeat(64),reader_module:'T1',info_record_state:'present_nonempty_crc_verified',display_state:'eligible_t1_source_year',t1_display_eligible:true,current_profile_updated:false,identity_verified:false,fields};
  const html=api.entryHtml(client,record,'');assert.ok(html.includes('Historical personal information · 2015'));assert.ok(html.includes('***-***-1234'));assert.ok(html.includes('1980-03-04'));assert.ok(html.includes('Blank in source'));assert.ok(!html.includes('7 personal fields not imported'));
  for(const mutate of [p=>p.client_number='101',p=>p.source_year=2014,p=>p.current_profile_updated=true,p=>p.fields.SIN.candidate='123456789',p=>p.fields.SIN.raw_code='123456789',p=>p.fields.Language.display_eligible=true]){const r=JSON.parse(JSON.stringify(record));mutate(r.source_personal_info);assert.throws(()=>api.entryHtml(client,r,''));}
  const held=JSON.parse(JSON.stringify(record));held.vendor_module='T3';held.mapping_state='pending_module_verification';Object.assign(held.source_personal_info,{reader_module:'T3',t1_display_eligible:false,display_state:'held_module_origin'});Object.values(held.source_personal_info.fields).forEach(f=>f.display_eligible=false);const heldHtml=api.entryHtml(client,held,'');assert.ok(heldHtml.includes('Source module needs review'));assert.ok(!heldHtml.includes('1980-03-04'));assert.ok(!heldHtml.includes('***-***-1234'));
});
test('linked T5008 source retains the raw owner reference and rejects reassignment or normalization claims',()=>{
  const r={client_number:'100',tax_year:2024,original_source_record:{client_number:'0',tax_year:2024,source_file_sha256:'a'.repeat(64),row:7,boxes:{20:'1,234.50',21:'<img src=x>'}},owner_link:{legacy_record_id:'0:2024:'+'a'.repeat(64)+':7',legacy_record_sha256:'b'.repeat(64),owner_client_number:'100',evidence_mode:'recipient_name_and_all_boxes',matching_cra_records:[{}]},source_owner_id_changed:false,normalized_tax_slip_created:false,economic_slip_uniqueness_verified:false,financial_values_approved:false};
  const html=api.recordsHtml('linked-t5008',[r]);assert.ok(html.includes('1,234.50'));assert.ok(html.includes('Original source client reference: 0'));assert.ok(!html.includes('<img'));assert.ok(html.includes('&#60;img'));assert.ok(html.includes('No additional tax slip has been created'));
  for(const changed of [{...r,client_number:'200'},{...r,source_owner_id_changed:true},{...r,normalized_tax_slip_created:true},{...r,tax_year:2023}])assert.throws(()=>api.recordsHtml('linked-t5008',[changed]));
});
test('late source responses cannot replace another tax view',async()=>{
  let resolve,active=true;const pending=new Promise(r=>resolve=r),host={isConnected:true,innerHTML:'',querySelector:()=>null};
  const controller=api.mount(host,{request:()=>pending,isCurrent:()=>active});active=false;host.innerHTML='Different tax view';resolve(page());await controller.ready;assert.equal(host.innerHTML,'Different tax view');
});
function harness(recordClient='100',extra) {
  const nodes=new Map(),buttons=[{dataset:{importedClient:'100'}}],requests=[];
  const node=key=>{if(!nodes.has(key))nodes.set(key,{innerHTML:'',value:'',querySelector:node,querySelectorAll:()=>[],scrollIntoView(){}});return nodes.get(key);};
  const host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:key=>key==='[data-imported-client]'?buttons:[]};
  const values=Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map((x,i)=>[x,i===0?'12345':null]));
  const request=async path=>{requests.push(path);const supplied=extra?.(path);if(supplied!==undefined)return supplied;return path.includes('/source-clients?')?page():{test_copy:true,client_number:'100',kind:'historical-lines',offset:0,total:1,tax_cases_imported:0,records:[{client_number:recordClient,tax_year:2025,vendor_module:'T1',mapping_state:'vendor_static_verified',values_cents:values}]};};
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
test('owner reconciliation loads lazily and verifies raw, resolved and remaining counts',async()=>{
  const response={source_scope:'dtmax-test-copy',test_copy:true,source_rows_modified:0,normalized_slips_created:0,tax_cases_imported:0,raw_legacy_CSV_owner_ids_unregistered:31,verified_owner_links:31,remaining_owner_exceptions:0};
  const h=harness('100',path=>path.endsWith('legacy-owner-links/summary')?response:undefined);await h.controller.ready;assert.equal(h.requests.length,1);
  await h.node('[data-dtmax-owner-review]').ontoggle({target:{open:true}});let html=h.node('[data-dtmax-owner-content]').innerHTML;assert.ok(html.includes('31 of 31'));assert.ok(html.includes('0 ownership exceptions remain'));
  response.remaining_owner_exceptions=1;await h.node('[data-dtmax-owner-review]').ontoggle({target:{open:true}});html=h.node('[data-dtmax-owner-content]').innerHTML;assert.ok(html.includes('role="alert"'));assert.ok(!html.includes('31 of 31'));
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
test('cached native PDF provenance is distinct and refuses recalculation or filed-copy claims',()=>{
  const pdf={client_number:'100',tax_year:2024,sha256:'a'.repeat(64),bytes:100,page_labels:['T1 Federal p1'],page_count:1,original_print_output:true,immutable:true,export_provenance:{kind:'cached_native_export',source_backup_sha256:'7b760c8406ac29f6942898818c28be03c823128fdb164b5b03885f9f37b0eed5',native_family_file_sha256:'b'.repeat(64),export_log_sha256:'c'.repeat(64),exported_at_utc:'2026-11-07T03:25:46Z',recalculated:false,as_filed_verified:false,figures_csv_sha256:'c16c0bdc3036932f54e09cdd698513344c945cc4e8ba006cca54cd2fe8f93a94',figure_comparisons:Object.fromEntries(['15000','23600','26000','42000','48400','48500'].map(l=>[l,'verified_equal_source']))}};
  api.validatePdf(pdf,'100');
  for(const changes of [{kind:'retained_native_print'},{source_backup_sha256:'0'.repeat(64)},{native_family_file_sha256:'bad'},{export_log_sha256:'bad'},{exported_at_utc:'invalid'},{recalculated:true},{as_filed_verified:true}])assert.throws(()=>api.validatePdf({...pdf,export_provenance:{...pdf.export_provenance,...changes}},'100'));
  assert.ok(api.cachedFigureNotice(pdf.export_provenance).includes('Every available source figure matched'));
  pdf.export_provenance.figure_comparisons['23600']='printed_source_difference';assert.ok(api.cachedFigureNotice(pdf.export_provenance).includes('role="alert"'));assert.ok(api.cachedFigureNotice(pdf.export_provenance).includes('23600'));
  pdf.export_provenance.figure_comparisons['23600']='approved';assert.throws(()=>api.validatePdf(pdf,'100'));
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
test('recovered historical family links retain old identifiers and expose only corroborated members',()=>{
  const r={schema:'dtmax-historical-native-family-lineage/v1',lineage_profile_sha256:'f05734d0cd42f21a347f3dbbdba428cb9b25e0d62ef3b18b5db9c287b855edb0',authority_proof_sha256:'a'.repeat(64),legacy_record_id:'999:2024',legacy_record_sha256:'b'.repeat(64),source_family_file_id:'999',tax_year:2024,source_file_sha256:'c'.repeat(64),source_ids_rewritten:false,tax_payloads_decoded:false,tax_values_approved:false,member_mappings:[{old_native_client_number:'901',current_client_number:'100',native_name_equality_evidence_sha256:'d'.repeat(64),native_DOB_equality_evidence_sha256:'e'.repeat(64)}]};
  const before=JSON.stringify(r),html=api.recordsHtml('historical-family-lineage',[r]);assert.ok(html.includes('Historical client #901'));assert.ok(html.includes('data-family-client="100"'));assert.ok(html.includes('not been decoded into tax figures'));assert.equal(JSON.stringify(r),before);
  for(const mutate of [r=>r.tax_values_approved=true,r=>r.source_ids_rewritten=true,r=>r.member_mappings[0].current_client_number='<img>',r=>r.legacy_record_id='901:2024',r=>r.lineage_profile_sha256='0'.repeat(64)]){const x=JSON.parse(before);mutate(x);assert.throws(()=>api.recordsHtml('historical-family-lineage',[x]));}
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
test('independent native-origin review exposes exact T1 wire facts while retaining original T3 metadata',()=>{
  const r={client_number:'100',family_file_id:'999',tax_year:2024,archive_sha256:'a'.repeat(64),vendor_module:'T3',mapping_state:'pending_module_verification',values_cents:{15000:'12345',23600:null,26000:null,42000:null,48400:null,48500:null}};
  const fields=Object.fromEntries(Object.keys(r.values_cents).map(l=>[l,{field_id:Number(l)/100,state:'absent',source_wire_integer:null,source_decimal:null,scale_divisor:null}]));fields['15000']={field_id:150,state:'present_value',source_wire_integer:12345,source_decimal:'123.45',scale_divisor:100};
  r.native_module_review={schema:'dtmax-native-source-module-review/v1',binding_profile_sha256:'d33fd86f7924d50e96f6ce4916ff14df8d4740a46863fb13cb1900842dd59604',client_number:'100',family_file_id:'999',source_year:2024,archive_sha256:r.archive_sha256,reader_module:'T3',native_source_module:'T1',review_state:'native_source_module_origin_verified',original_history_updated:false,personal_sidecar_flags_updated:false,identity_verified:false,tax_figures_approved:false,native_authority:{module_index:1,path_rule_exact:true,source_archive_magic:'0xaeaeaebe'},native_file:{relative_path:'T1\\2024\\999.P24',physical_raw_stream:true,sha256:'b'.repeat(64)},info1:{crc_verified:true},e11:{source_personal_prefix:182,reader_has_calculated_return:true,form:{name:'E11',record_type:30,outer_plan:0,crc_verified:true,marker_field1_present:true},fields}};
  r.native_module_review.info1={crc_verified:true,frame_offset:200,frame_sha256:'c'.repeat(64),expanded_sha256:'d'.repeat(64),expanded_bytes:350};
  r.native_module_review.source_frames={EXF:{selected_file_id:999},EXY:{frame_offset:56,native_tax_year:2024},EXC:{frame_offset:100,frame_sha256:'e'.repeat(64),client_number:'100',member_file_id:999,descriptor_terminal_byte:0}};
  const personalFields=Object.fromEntries(['Relation','SIN','BirthDate','Status','Sex','Prov-Residence','Language'].map(name=>[name,{raw_code:null,candidate:null,state:'present_blank',display_eligible:false}]));Object.assign(personalFields.SIN,{raw_redacted:true,state:'source_placeholder_zero_sentinel'});
  r.source_personal_info={schema:'dtmax-personal-source-display/v1',binding_profile_sha256:'9b7fc393b7632f0baa4e74806883a199c12f7cd0f79ed2dc7c6706c322acc10e',client_number:'100',family_file_id:'999',source_year:2024,archive_sha256:r.archive_sha256,reader_module:'T3',display_state:'held_module_origin',t1_display_eligible:false,info_record_state:'present_nonempty_crc_verified',current_profile_updated:false,identity_verified:false,info1:{...r.native_module_review.info1},source_EXF_file_id:999,source_year_frame_offset:56,source_context_offset:100,source_EXC_sha256:'e'.repeat(64),fields:personalFields};
  const before=JSON.stringify(r),html=api.recordsHtml('historical-lines',[r]);assert.ok(html.includes('$123.45'));assert.ok(html.includes('original converter label T3 retained'));assert.ok(api.entryHtml(page().clients[0],r,'').includes('$123.45'));assert.equal(JSON.stringify(r),before);
  for(const mutate of [x=>x.native_module_review.client_number='200',x=>x.native_module_review.tax_figures_approved=true,x=>x.native_module_review.e11.fields['15000'].source_wire_integer=12346,x=>x.values_cents['15000']=null,x=>x.native_module_review.e11.fields['23600'].source_wire_integer=0,x=>x.native_module_review.native_file.relative_path='T3\\2024\\999.T24',x=>x.source_personal_info.info1.frame_sha256='f'.repeat(64),x=>x.source_personal_info.source_EXF_file_id=998,x=>x.native_module_review.source_frames.EXC.frame_sha256='f'.repeat(64)]){const x=JSON.parse(before);mutate(x);assert.throws(()=>api.recordsHtml('historical-lines',[x]));}
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
