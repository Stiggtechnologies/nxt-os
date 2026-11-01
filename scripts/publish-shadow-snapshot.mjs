#!/usr/bin/env node
// Explicitly selected parallel-test package only. Never posts accounting books.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import { parseStrictJson } from './cutover-rehearsal.mjs';
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const canonical=v=>Array.isArray(v)?`[${v.map(canonical).join(',')}]`:v&&typeof v==='object'?`{${Object.keys(v).sort().map(k=>`${JSON.stringify(k)}:${canonical(v[k])}`).join(',')}}`:JSON.stringify(v);
const options={};
for(let i=2;i<process.argv.length;i++){
 const key=process.argv[i];
 if(key==='--verify-only'){assert(!options[key]);options[key]=true;continue;}
 assert(['--pack','--url','--receipt'].includes(key)&&!options[key]&&process.argv[i+1]);options[key]=process.argv[++i];
}
assert(options['--pack']&&options['--url']&&options['--receipt'],'Supply --pack --url --receipt; credential only through NXT_SHADOW_KEY');
const origin=new URL(options['--url']);
assert(!origin.username&&!origin.password&&!origin.search&&!origin.hash&&origin.pathname==='/');
const local=['127.0.0.1','localhost','[::1]'].includes(origin.hostname);
assert((local&&origin.protocol==='http:')||origin.origin==='https://os.nxtcorporation.ca','Unapproved destination origin');
const key=process.env.NXT_SHADOW_KEY;assert(key&&key.trim()===key,'NXT_SHADOW_KEY required');
const root=path.resolve(options['--pack']);assert(fs.lstatSync(root).isDirectory()&&!fs.lstatSync(root).isSymbolicLink());
const receiptPath=path.resolve(options['--receipt']);assert(!fs.existsSync(receiptPath),'Choose new receipt path');
function read(file,max){
 const before=fs.lstatSync(file);assert(before.isFile()&&!before.isSymbolicLink()&&before.size>0&&before.size<=max);
 const bytes=fs.readFileSync(file),after=fs.lstatSync(file);
 assert(before.dev===after.dev&&before.ino===after.ino&&before.size===after.size&&before.mtimeMs===after.mtimeMs&&bytes.length===after.size&&!after.isSymbolicLink(),'Package file changed');
 return bytes;
}
const manifestBytes=read(path.join(root,'manifest.json'),1024*1024),manifest=parseStrictJson(manifestBytes,'snapshot manifest');
assert(manifest.format==='nxt-shadow-snapshot/v1'&&manifest.mode==='parallel_test'&&manifest.source_of_truth==='original_system');
assert(/^[A-Za-z0-9][A-Za-z0-9_-]{0,95}$/.test(manifest.id));
const references=[...new Set([...manifest.source_archive.chunks,...manifest.reports.flatMap(r=>r.parts.map(p=>p.sha256))])];
assert(references.length>0&&references.length<=512&&references.every(h=>/^[a-f0-9]{64}$/.test(h)));
let referencedBytes=0;
for(const h of references){const bytes=read(path.join(root,'chunks',h+'.txt'),512*1024);assert(sha(bytes)===h,'Local chunk identity mismatch');referencedBytes+=bytes.length;}
assert(referencedBytes<=24*1024*1024);
const headers={authorization:'Bearer '+key};
async function request(route,{method='GET',body,binary=false}={}){
 for(let attempt=0;;attempt++){
  try{
   const init={method,headers:{...headers},redirect:'error',signal:AbortSignal.timeout(60000)};
   if(body!==undefined){init.headers['content-type']='application/json';init.body=JSON.stringify(body);assert(Buffer.byteLength(init.body)<=4*1024*1024);}
   const response=await fetch(origin.origin+route,init);
   if([429,502,503,504].includes(response.status)&&attempt<2){await response.body?.cancel();await new Promise(r=>setTimeout(r,500*(attempt+1)));continue;}
   assert(response.ok,`${method} ${route} HTTP ${response.status}`);
   if(binary){assert(!response.headers.get('content-length')||Number(response.headers.get('content-length'))<=4*1024*1024);const bytes=Buffer.from(await response.arrayBuffer());assert(bytes.length<=4*1024*1024);return bytes;}
   return await response.json();
  }catch(e){if(attempt<2&&e instanceof TypeError){await new Promise(r=>setTimeout(r,500*(attempt+1)));continue;}throw e;}
 }
}
function shadowControls(value){assert(value.shadow_only===true&&value.live_cutover_allowed===false&&value.source_authoritative===true);}
const status=await request('/api/os/status');assert(status.auth_required===true&&status.apps.some(a=>a.id==='migrations'));if(!local)assert.equal(status.storage,'supabase');
const before=await request('/api/migrations');shadowControls(before);if(!local)assert.equal(before.storage,'supabase');
if(!options['--verify-only']){
 let done=0;
 for(let index=0;index<references.length;index+=3){
  await Promise.all(references.slice(index,index+3).map(async h=>{
   const bytes=read(path.join(root,'chunks',h+'.txt'),512*1024);
   const content=new TextDecoder('utf-8',{fatal:true}).decode(bytes);
   const result=await request('/api/migrations/chunks',{method:'POST',body:{sha256:h,content}});assert.equal(result.sha256,h);done++;
  }));
 }
 assert.equal(done,references.length);
 const publication=await request('/api/migrations/snapshots',{method:'POST',body:manifest});shadowControls(publication);
}
const encodedId=encodeURIComponent(manifest.id),detail=await request('/api/migrations/'+encodedId);shadowControls(detail);
assert.equal(canonical(detail.manifest),canonical(manifest),'Published manifest differs');
const archive=await request('/api/migrations/'+encodedId+'/archive',{binary:true});
assert.equal(archive.length,manifest.source_archive.bytes);assert.equal(sha(archive),manifest.source_archive.sha256);
let checked=0,emptyReportsChecked=0;
for(const report of manifest.reports){
 if(!report.parts.length){
  assert.equal(report.row_count,0,'Empty part inventory must declare zero source rows');
  const page=await request(`/api/migrations/${encodedId}/reports/${encodeURIComponent(report.id)}/rows?from_row=1&limit=1`);shadowControls(page);
  assert.deepEqual(page.rows,[]);assert.equal(page.has_more,false);emptyReportsChecked++;continue;
 }
 const rowsToCheck=new Set([report.parts[0].row_start,report.parts.at(-1).row_end,...manifest.exceptions.filter(e=>e.report_id===report.id).map(e=>e.row)]);
 for(const row of rowsToCheck){
  const part=report.parts.find(p=>p.row_start<=row&&p.row_end>=row);assert(part,'Expected row has no part');
  const chunk=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(read(path.join(root,'chunks',part.sha256+'.txt'),512*1024)));
  const expected=chunk.rows.find(r=>r[0]===row);assert(expected,'Expected source row missing');
  const page=await request(`/api/migrations/${encodedId}/reports/${encodeURIComponent(report.id)}/rows?from_row=${row}&limit=1`);shadowControls(page);
  assert.equal(canonical(page.rows[0]),canonical(expected),'Source row changed during storage/readback');checked++;
 }
}
const receipt={format:'nxt-shadow-publication-verification/v1',checked_at:new Date().toISOString(),environment:local?'local_qa':'production_shadow_review',url:origin.origin,snapshot_id:manifest.id,cohort_ref:manifest.cohort_ref,manifest_sha256:detail.manifest_sha256,local_manifest_bytes_sha256:sha(manifestBytes),chunk_count:references.length,referenced_bytes:referencedBytes,source_archive_sha256:manifest.source_archive.sha256,source_archive_bytes:archive.length,archive_exact_readback:true,targeted_source_rows_checked:checked,targeted_source_rows_exact:true,empty_reports_checked:emptyReportsChecked,report_count:manifest.reports.length,exception_count:manifest.exceptions.length,shared_key_authentication:true,shadow_only:true,source_authoritative:true,live_cutover_allowed:false,financial_posts_requested:0,source_system_changes_requested:0,verify_only:!!options['--verify-only']};
fs.writeFileSync(receiptPath,JSON.stringify(receipt,null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify({snapshot_id:receipt.snapshot_id,report_count:receipt.report_count,exception_count:receipt.exception_count,archive_exact_readback:true,targeted_rows_exact:checked,shadow_only:true,live_cutover_allowed:false}));
