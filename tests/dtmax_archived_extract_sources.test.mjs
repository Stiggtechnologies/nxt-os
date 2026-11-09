import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto,createHash} from 'node:crypto';
import {gunzipSync} from 'node:zlib';
const source=fs.readFileSync(new URL('../public/apps/tax/dtmax-t1.js',import.meta.url),'utf8');
const profileBytes=fs.readFileSync(new URL('../public/apps/tax/dtmax-archived-extract-profile.json',import.meta.url)),profile=JSON.parse(profileBytes);
function environment(bytes=profileBytes){const s={crypto:webcrypto,TextEncoder,TextDecoder,Uint8Array,DataView,atob,Blob,Response,DecompressionStream,fetch:async(url,o)=>{assert.equal(url,'/apps/tax/dtmax-archived-extract-profile.json');assert.equal(o.credentials,'omit');assert.equal(o.redirect,'error');return{ok:true,arrayBuffer:async()=>bytes};}};vm.runInNewContext(source,s);return s.NXTDTMaxT1;}
const api=environment(),canon=api.canonicalRecoveredSource,sha=b=>createHash('sha256').update(b).digest('hex');
const rows=process.env.NXT_ARCHIVED_EXTRACT_PACKETS?fs.readFileSync(process.env.NXT_ARCHIVED_EXTRACT_PACKETS,'utf8').trim().split('\n').map(JSON.parse):null;
const golden=process.env.NXT_ARCHIVED_BODY_GOLDEN?fs.readFileSync(process.env.NXT_ARCHIVED_BODY_GOLDEN,'utf8').trim().split('\n').map(JSON.parse):null;
const options={profile,inflate:gunzipSync,loadKeywords:async()=>null};
function envelope(){return{test_copy:true,source_scope:'dtmax-test-copy',kind:'archived-extract-sources',archived_source_profile_sha256:profile.profile_sha256,archived_source_profile_file_sha256:sha(profileBytes),source_only:true,vendor_activation_required:false,current_profiles_modified:0,original_or_preferred_rows_modified:0,native_contexts_created:0,tax_cases_imported:0,financial_values_approved:false,preferred_version_selected:false};}
test('archive public closure is byte-pinned and fails closed on altered profile',async()=>{
 assert.equal((await api.loadArchivedProfile()).profile_sha256,profile.profile_sha256);assert.equal(profileBytes.length,91537);
 for(const b of [Buffer.from('{}'),Buffer.concat([profileBytes,Buffer.from(' ')])])await assert.rejects(()=>environment(b).loadArchivedProfile());
 const p=structuredClone(profile);p.contract.packet_sha256[0]='0'.repeat(64);await assert.rejects(()=>api.checkArchivedProfile(p));
 assert.equal(api.exactHundredths(0),'0');assert.equal(api.exactHundredths(-1),'-0.01');assert.equal(api.exactHundredths(2147483647),'21474836.47');
});
test('unknown tag holds ENTIRE record before an embedded fake form or amount',()=>{
 const bytes=Buffer.concat([Buffer.alloc(164),Buffer.from([228,65,65,65,0,0,2,150,0,123,0,0,0,20,1,0,228,69,49,49,0,0,2,150,0,255,255,255,127])]);
 const p=api.parseArchivedBody(bytes,30);assert.equal(p.state,'unsupported_native_tag_record_held');assert.equal(p.unknown_tag,20);assert.equal(p.unknown_tag_body_offset,177);assert.equal(p.forms.length,0);assert.equal(p.tokens.length,0);
 for(const type of [31,41]){const held=api.parseArchivedBody(bytes,type);assert.equal(held.state,'unsupported_continuation_cursor_record_held');assert.equal(held.forms.length,0);}
 for(const first of [20,2]){const b=Buffer.concat([Buffer.alloc(164),Buffer.from([first,1,0,228,69,49,49,0,0,2,150,0,255,255,255,127])]),held=api.parseArchivedBody(b,30);assert.equal(held.state,'unsupported_native_tag_record_held');assert.equal(held.unknown_tag_body_offset,164);assert.equal(held.forms.length,0);}
});
test('all actual archived payloads and serialized records retain SHA CRC header and whole known E11 form closure',{skip:!rows},async()=>{
 let records=0,assigned=0,held=0,known=0,empty=0,renderedHeld=0;
 for(const r of rows){const v=await api.verifyArchivedPacket(r,options);assert.equal(Buffer.from(v.packetBytes).toString(),canon(r));assert.ok(v.payload.records.every(r=>![31,41].includes(r.record_type)),'The448closed inventory has no continuation records');records+=v.payload.records.length;assigned+=r.assigned_SOURCE_client_number!==null;
  for(const{record,body}of v.decoded.values()){empty+=record.expanded_record_bytes===0;if([22,23,30,40].includes(record.record_type)){const p=api.parseArchivedBody(body,record.record_type);if(p.state==='parsed')known++;else{held++;assert.equal(p.forms.length,0);}}}
  const rendered=await api.archivedSourcesHtml([r],options);assert.equal(Buffer.from(rendered.downloads[0].packetBytes).toString(),canon(r));assert.ok(rendered.html.includes('Current and preferred figures stay unchanged'));renderedHeld+=(rendered.html.match(/entire record is held for download only/g)||[]).length;
 }
 assert.equal(rows.length,448);assert.equal(records,2437);assert.equal(assigned,283);assert.equal(rows.length-assigned,165);assert.equal(empty,1);assert.equal(known,799);assert.equal(held,195);assert.equal(renderedHeld,195);
 console.log(JSON.stringify({archive_contexts:448,assigned_SOURCE:283,queue:165,records,known_records:known,held_records:held,empty_type19:empty}));
});
test('all closed known BODY golden projections agree with independent standalone reader; unknowns display nothing',{skip:!golden},()=>{
 let known=0,held=0;for(const g of golden){const b=Buffer.from(g.body_base64,'base64');assert.equal(sha(b),g.body_sha256);const p=api.parseArchivedBody(b,g.type);if(g.unknown_record_held){assert.equal(p.state,'unsupported_native_tag_record_held');assert.equal(p.forms.length,0);held++;}else{assert.equal(p.state,'parsed');assert.equal(sha(canon(p)),g.known_expected_sha256);known++;}}
 assert.equal(golden.length,987);assert.ok(known>700);assert.ok(held>180);
});
test('archive assignment year raw source and compressed-data mutations never pass closed packet allowlist',{skip:!rows},async()=>{
 for(const mutate of [r=>r.assigned_SOURCE_client_number='9999999',r=>r.source_locator.source_return_year=2020,r=>r.source_locator.source_header_client_reference='99999',r=>r.source_payload_gzip_base64='AAAA',r=>r.financial_approval=true,r=>r.preferred_version=true]){const r=structuredClone(rows[0]);mutate(r);await assert.rejects(()=>api.verifyArchivedPacket(r,options));}
});
test('archived lane remains explicit and does not replace historical or effective native defaults',()=>{
 assert.ok(source.includes("kinds['archived-extract-sources']='Archived extract fields'"));
 const defaultFn=source.slice(source.indexOf('function defaultSourceKind()'),source.indexOf('function bindClient()'));
 assert.ok(!defaultFn.includes('archived-extract'));assert.ok(defaultFn.includes('recovered-native-effective-sources'));assert.ok(source.includes('data-dtmax-archived-review'));assert.ok(source.includes('data-archived-year'));
});
test('assigned archive year remains selectable when absent from current historical years',{skip:!rows},async()=>{
 const row=rows.find(r=>r.assigned_SOURCE_client_number&&r.source_locator.source_return_year===2021),number=row.assigned_SOURCE_client_number,nodes=new Map(),button={dataset:{sourceKind:'archived-extract-sources'}},all=s=>s==='[data-source-kind]'?[button]:[],node=k=>{if(!nodes.has(k))nodes.set(k,{innerHTML:'',querySelector:node,querySelectorAll:all});return nodes.get(k);},host={isConnected:true,innerHTML:'',querySelector:node,querySelectorAll:all},requests=[];
 const request=async path=>{requests.push(path);if(path.includes('/source-clients?'))return{source_scope:'dtmax-test-copy',program:'T1',imported_source_only:true,tax_cases_created:0,total:1,offset:0,available_years:[2024],clients:[{client_number:number,name:'Synthetic display fixture',historical_years:[2024],pending_years:[]}]};const u=new URL(path,'https://local.invalid'),kind=u.searchParams.get('kind');if(kind==='historical-lines')return{test_copy:true,tax_cases_imported:0,client_number:number,kind,total:0,offset:0,records:[]};assert.equal(kind,'archived-extract-sources');const y=Number(u.searchParams.get('source_return_year'));return{...envelope(),client_number:number,source_return_year:y,total:y===2021?1:0,offset:0,limit:1,records:y===2021?[row]:[]};};
 const c=environment().mount(host,{request,historicalKind:'historical-lines'});await c.ready;await c.open(number);await button.onclick();assert.ok(node('[data-dtmax-details]').innerHTML.includes('data-archived-client-year'));await node('[data-archived-client-year]').onchange({target:{value:'2021'}});assert.ok(node('[data-dtmax-details]').innerHTML.includes('Archived extract source · 2021'));assert.ok(host.innerHTML.includes('Tax year 2024'));assert.ok(requests.some(s=>s.includes('source_return_year=2021')));c.dispose();
});
