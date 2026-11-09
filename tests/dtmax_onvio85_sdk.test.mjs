import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import {createHash} from 'node:crypto';
import vm from 'node:vm';
const sdkBytes=fs.readFileSync(new URL('../public/apps/tax/dtmax-onvio85.js',import.meta.url));
vm.runInThisContext(sdkBytes.toString());
const {canonicalJSON,deriveTag20SourceInterpretations,parseCalculatedBody,recordSHA256,interpretationAuthority,NativeBodyError}=globalThis.NXTOnvio85;
const PROFILE=JSON.parse(sdkBytes.toString().match(/const PROFILE = Object.freeze\((\{[^\n]+\})\);/)[1]);
const hash = b => createHash('sha256').update(b).digest('hex');
const raw=process.env.NXT_ONVIO85_FIXTURE?fs.readFileSync(process.env.NXT_ONVIO85_FIXTURE):null;
if(raw)assert.equal(hash(raw),'93e164c756094998dd5238a8fdb16a4147f0f663a8d2c65b05c8844621437c3b');
const fixture=raw?JSON.parse(raw):null,sha256=hash;
const expectedMap=new Map((fixture?.expected_records||[]).map(r=>[hash(canonicalJSON(r)),canonicalJSON(r)]));
const privateResults = [];
let allResults;
const body = sequence => {const b=new Uint8Array(164+sequence.length);new DataView(b.buffer).setUint16(72,164,true);b.set(sequence,164);return b;};
const form = [228,69,49,49,0,0];
const field = (tag,id,bytes=[]) => [tag,id&255,id>>>8,...bytes];
const i32 = x => {const b=new Uint8Array(4);new DataView(b.buffer).setInt32(0,x,true);return [...b];};
const makeCRC = bytes => {let c=0xffffffff;for(const b of bytes){c^=b;for(let j=0;j<8;j++)c=(c>>>1)^((c&1)?0xedb88320:0);}return((c^0xffffffff)>>>0).toString(16).padStart(8,'0');};
function mutateVerifiedRecord(full, recordOffset, mutation) {
  const copy=structuredClone(full),r=copy.records.find(r=>r.frame_offset===recordOffset),data=Uint8Array.from(Buffer.from(r.expanded_record_base64,'base64'));
  mutation(data.subarray(18));
  r.expanded_record_base64=Buffer.from(data).toString('base64');r.native_serialized_header_base64=Buffer.from(data.subarray(0,18)).toString('base64');r.expanded_record_sha256=hash(data);r.expanded_body_sha256=hash(data.subarray(18));r.CRC32=makeCRC(data);return copy;
}

test('all frozen448 payloads derive exactly the85 closed canonical records without changing base holds or preferred figures',{skip:!fixture}, async()=>{
  const before=hash(canonicalJSON(fixture.contexts));
  allResults=[];let held=0,oldHeldVariants=0;
  for(const full of fixture.contexts){oldHeldVariants+=full.calculation_source_variants.filter(v=>v.parse_holds.length).length;const out=await deriveTag20SourceInterpretations(full,{sha256});allResults.push(...out.observations);held+=out.held.length;}
  assert.equal(allResults.length,85);assert.equal(new Set(allResults.map(o=>o.record_sha256)).size,85);
  assert.deepEqual(allResults.map(o=>o.record_sha256).sort(),[...PROFILE.record_sha256].sort());
  for(const o of allResults){assert.ok(expectedMap.get(o.record_sha256)===canonicalJSON(o.record),'Canonical source interpretation differs; no values emitted');assert.equal(o.record.financial_approval,false);assert.equal(o.record.preferred_version_selection,false);assert.equal(o.record.old_frozen_grammar_holds_modified,false);}
  assert.equal(hash(canonicalJSON(fixture.contexts)),before);
  const counts={E11_forms:0,E11_target_field_occurrences:0,E11_explicit_zero:0,E11_absent_slots:0,E11_marker1_present:0};
  for(const o of allResults)for(const f of o.record.E11_forms){counts.E11_forms++;counts.E11_marker1_present+=f.marker_field1_present;for(const values of Object.values(f.lines)){if(values===null)counts.E11_absent_slots++;else for(const q of values){counts.E11_target_field_occurrences++;counts.E11_explicit_zero+=q.wire_cents===0;assert.equal(q.type_tag,2);assert.equal(Number.isSafeInteger(q.wire_cents),true);}}}
  assert.deepEqual(counts,{E11_forms:85,E11_target_field_occurrences:410,E11_explicit_zero:61,E11_absent_slots:100,E11_marker1_present:85});
  privateResults.push({check:'all448_85canonical',contexts:448,closed_records:85,old_held_variants:oldHeldVariants,unpromoted_variants:held,counts,input_unchanged:true});
});

test('ordinary browser WebCrypto and Uint8Array path reproduce one actual source record',{skip:!fixture},async()=>{
  const e=fixture.expected_records[0],full=fixture.contexts.find(f=>f.source_context_offset===e.source_context_offset),out=await deriveTag20SourceInterpretations(full);
  assert.ok(out.observations.some(o=>o.record_sha256===hash(canonicalJSON(e))),'WebCrypto derived record did not match the closed hash');
  assert.equal(await recordSHA256(e),hash(canonicalJSON(e)));
});

test('all2882 actual tag20 values have exactly four opaque bytes and no assigned scalar semantics',{skip:!fixture},async()=>{
  let opaque=0,forms=0,fields=0;
  for(const e of fixture.expected_records){
    const full=fixture.contexts.find(f=>f.source_context_offset===e.source_context_offset),r=full.records.find(r=>r.frame_offset===e.source_records[0].frame_offset),body=Uint8Array.from(Buffer.from(r.expanded_record_base64,'base64')).subarray(18),parsed=await parseCalculatedBody(body,r.record_type,{sha256});
    forms+=parsed.forms.length;fields+=parsed.forms.reduce((n,f)=>n+f.fields.length,0);
    for(const q of parsed.opaque_tag20_fields){opaque++;assert.equal(q.BODY_field_end-q.BODY_field_start,7);assert.equal(q.opaque_value_bytes,4);assert.equal(q.opaque_value_hex.length,8);assert.equal(q.opaque_value_sha256,hash(body.subarray(q.BODY_field_start+3,q.BODY_field_end)));for(const key of ['wire_cents','source_decimal','value','amount'])assert.equal(Object.hasOwn(q,key),false);}
  }
  assert.equal(opaque,2882);assert.equal(forms,3970);assert.equal(fields,116265);privateResults.push({check:'all_actual_tag20_opaque',opaque_four_byte_fields:opaque,known_extent_forms:forms,known_extent_fields:fields,scalar_semantics_assigned:false});
});

test('known grammar variants receive no addendum and do not change old projections',{skip:!fixture},async()=>{
  let knownVariants=0;
  for(const f of fixture.contexts){const copy=structuredClone(f);copy.calculation_source_variants=copy.calculation_source_variants.filter(v=>!v.parse_holds.length);knownVariants+=copy.calculation_source_variants.length;const before=hash(canonicalJSON(copy));const out=await deriveTag20SourceInterpretations(copy,{sha256});assert.equal(out.observations.length,0);assert.equal(out.held.length,0);assert.equal(hash(canonicalJSON(copy)),before);}
  assert.ok(knownVariants>=65);privateResults.push({check:'known_grammar_not_reinterpreted',known_variants:knownVariants});
});

test('all established extents, groups, controls, integer cents and opaque20 remain separate',async()=>{
  const seq=[...form,14,...i32(3),...field(1,10,i32(100)),...field(2,150,i32(-2147483648)),...field(4,20,[97,98,0]),...field(6,1),...field(7,30,[1,2]),...field(8,31,[3,4]),...field(11,40,[1,2,3,4]),...field(20,50,[255,0,128,1]),15,16,1,2,3,4,5,6,0,226,239,255];
  const b=body(seq),before=hash(b),p=await parseCalculatedBody(b,30,{sha256});assert.equal(p.bytes_consumed,b.length);assert.equal(p.byte_reconstruction_exact,true);assert.equal(hash(b),before);assert.equal(p.forms[0].fields.length,8);assert.equal(p.forms[0].fields[1].source_decimal,'-21474836.48');assert.equal(p.forms[0].fields[1].wire_cents,-2147483648);assert.equal(p.forms[0].fields[1].source_group,3);
  const q=p.opaque_tag20_fields[0];assert.equal(q.opaque_value_bytes,4);assert.equal(q.opaque_value_hex,'ff008001');assert.equal(q.opaque_value_sha256,hash(Uint8Array.from([255,0,128,1])));for(const key of ['wire_cents','source_decimal','value','amount'])assert.equal(Object.hasOwn(q,key),false);
});

test('prefix, record namespace, truncated fields/form/group/control and unterminated strings fail closed',async()=>{
  for(const bad of [new Uint8Array(163),body(form).map((x,i)=>i===72?0:x)])await assert.rejects(parseCalculatedBody(bad,30,{sha256}),e=>e instanceof NativeBodyError&&e.kind==='unproven_prefix_cursor');
  await assert.rejects(parseCalculatedBody(body(form),1,{sha256}),e=>e.kind==='outside_calculated_namespace');
  for(let n=1;n<6;n++)await assert.rejects(parseCalculatedBody(body(form.slice(0,n)),30,{sha256}),e=>e.kind==='read_beyond_BODY');
  const extents=[[1,4],[2,4],[7,2],[8,2],[11,4],[20,4]];
  for(const[tag,size]of extents)for(let n=0;n<3+size-1;n++)await assert.rejects(parseCalculatedBody(body([...form,...field(tag,150,new Array(size).fill(0)).slice(0,n+1)]),30,{sha256}),e=>e.kind==='read_beyond_BODY');
  for(let n=0;n<4;n++)await assert.rejects(parseCalculatedBody(body([...form,14,...new Array(n).fill(0)]),30,{sha256}),e=>e.kind==='read_beyond_BODY');
  for(let n=0;n<6;n++)await assert.rejects(parseCalculatedBody(body([...form,16,...new Array(n).fill(0)]),30,{sha256}),e=>e.kind==='read_beyond_BODY');
  await assert.rejects(parseCalculatedBody(body([...form,...field(4,7,[65,66])]),30,{sha256}),e=>e.kind==='unterminated_byte_string');
});

test('first unknown and pre-form bytes are held without scanning ahead for a form',async()=>{
  await assert.rejects(parseCalculatedBody(body([...form,99,0,0,...form,...field(2,150,i32(1))]),30,{sha256}),e=>e.kind==='unsupported_native_tag'&&e.offset===170&&e.tag===99);
  for(const tag of [20,2,99])await assert.rejects(parseCalculatedBody(body([tag,0,0,0,0,0,0,...form,...field(2,150,i32(1))]),30,{sha256}),e=>e.kind==='pre_form_byte_not_promoted'&&e.offset===164&&e.tag===tag);
});

test('actual field/form/tag20/context mutations with updated byte digests cannot pass the closed profile',{skip:!fixture},async()=>{
  const e=fixture.expected_records[0],full=fixture.contexts.find(f=>f.source_context_offset===e.source_context_offset),offset=e.source_records[0].frame_offset,r=full.records.find(r=>r.frame_offset===offset),bytes=Uint8Array.from(Buffer.from(r.expanded_record_base64,'base64')).subarray(18),p=await parseCalculatedBody(bytes,r.record_type,{sha256}),money=p.forms.find(f=>f.form_name.toUpperCase()==='E11').fields.find(q=>[150,236,260,420,484,485].includes(q.field_id)),opaque=p.opaque_tag20_fields[0],e11=p.forms.find(f=>f.form_name.toUpperCase()==='E11');
  const mutations=[b=>b[money.BODY_field_start+1]^=1,b=>b[money.BODY_field_start+3]^=1,b=>b[e11.BODY_form_offset+1]=88,b=>b[opaque.BODY_field_start+3]^=1,b=>b[164]=99];
  for(const mutation of mutations){const changed=mutateVerifiedRecord(full,offset,mutation),out=await deriveTag20SourceInterpretations(changed,{sha256});assert.equal(out.observations.length,0);assert.ok(out.held.length>0);}
  const changed=structuredClone(full);changed.source_context_offset++;assert.equal((await deriveTag20SourceInterpretations(changed,{sha256})).observations.length,0);
  const wrong=structuredClone(full);wrong.records.find(r=>r.frame_offset===offset).CRC32='00000000';const out=await deriveTag20SourceInterpretations(wrong,{sha256});assert.equal(out.observations.length,0);assert.ok(out.held.some(h=>h.kind==='expanded_record_digest_or_CRC'));
});

test('invalid source guards and duplicated record locators are held; profiles contain only hashes',{skip:!fixture},async()=>{
  const e=fixture.expected_records[0],full=fixture.contexts.find(f=>f.source_context_offset===e.source_context_offset),copy=structuredClone(full);copy.native_T1_origin_guard_verified=false;assert.equal((await deriveTag20SourceInterpretations(copy,{sha256})).observations.length,0);
  const dup=structuredClone(full);dup.records.push(dup.records.find(r=>r.frame_offset===e.source_records[0].frame_offset));const out=await deriveTag20SourceInterpretations(dup,{sha256});assert.equal(out.observations.length,0);assert.ok(out.held.some(h=>h.kind==='ambiguous_source_record'));
  assert.equal(interpretationAuthority.record_count,85);assert.ok(PROFILE.record_sha256.every(h=>/^[a-f0-9]{64}$/.test(h)));assert.equal(canonicalJSON({z:0,a:'\u00e9'}),'{"a":"é","z":0}');assert.throws(()=>canonicalJSON({x:NaN}));assert.throws(()=>canonicalJSON({x:0.5}));
});

test('classic bundle is byte pinned and contains only reviewed source code plus hash authority',()=>{assert.equal(hash(sdkBytes),'24c09ce73dad88f3b22cf145c0e792415059c0c7585fe00aeca7a156da36f000');assert.equal(sdkBytes.length,19224);assert.ok(Object.isFrozen(globalThis.NXTOnvio85));assert.equal(PROFILE.record_sha256.length,85);assert.ok(PROFILE.record_sha256.every(h=>/^[a-f0-9]{64}$/.test(h)));assert.ok(!/\b(?:import|export)\s+(?:const|function|class|\{)/.test(sdkBytes.toString()));});
