import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto,createHash} from 'node:crypto';

const source=fs.readFileSync(new URL('../public/apps/engagements/engagements.js',import.meta.url),'utf8');
const sha=bytes=>createHash('sha256').update(bytes).digest('hex');
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
function harness(){
  const editor={innerHTML:'',querySelector:()=>null,querySelectorAll:()=>[]};
  const main={innerHTML:'',querySelector:()=>null,querySelectorAll:()=>[]};
  const calls=[],downloads=[],objects=[];
  let handler=async()=>{throw Error('Request not expected');},fetchHandler=async()=>{throw Error('Fetch not expected');};
  const esc=v=>String(v??'').replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;').replaceAll("'",'&#39;');
  const sandbox={NXT:{api:(...args)=>{calls.push(args);return handler(...args);},esc,modal(){},toast(){},key:{get:()=> 'synthetic-local-key'},confirm:async()=>{throw Error('Confirmation not expected');}},
    crypto:webcrypto,TextEncoder,Uint8Array,structuredClone,console,
    document:{querySelector:s=>s==='#main'?main:s==='#editor'?editor:null,querySelectorAll:()=>[],createElement:()=>({click(){downloads.push(this.download);}})},
    fetch:(...args)=>fetchHandler(...args),URL:{createObjectURL:blob=>{objects.push(blob);return 'blob:synthetic';},revokeObjectURL(){}},setTimeout:fn=>fn()};
  const boundary=source.indexOf('  const actions={');assert.ok(boundary>0);
  vm.runInNewContext(source.slice(0,boundary)+`globalThis.subject={S,uploadFile,downloadFile,download,renderFiles,hasFiles,remove,setDialog:fn=>dialog=fn};})();`,sandbox);
  const api=sandbox.subject;
  api.S.record={id:'eng_synthetic',status:'draft',revision:1};
  api.S.draft={workpapers:[{id:'paper1',reference:'A',title:'Synthetic'}]};
  const bytes=Uint8Array.from([0,255,128,13,10,65]);
  const fields=new Map([['workpaper_id','paper1'],['file',{name:'Synthetic Café.bin',size:bytes.length,arrayBuffer:async()=>bytes.buffer}],['operator_label',' Synthetic preparer '],['note',' Synthetic only ']]);
  api.setDialog(async(_title,_content,save)=>save(fields));
  return {api,fields,bytes,calls,downloads,objects,editor,sandbox,setRequest:fn=>handler=fn,setFetch:fn=>fetchHandler=fn};
}
function withFile(h){
  h.api.S.draft.workpapers[0].attachment_versions=[{id:'file_one',version:1,file_name:'Synthetic Café.bin',bytes:h.bytes.length,sha256:sha(h.bytes),operator_label:'Synthetic preparer',note:'Synthetic only',uploaded_at:'2026-10-27T00:00:00Z'}];
}

test('a lost upload response retries the identical request identity and lowercase bytes',async()=>{
  const h=harness();let tries=0;
  h.setRequest(async()=>{if(++tries===1)throw Error('Lost response');return null;});
  h.api.setDialog(async(_t,_c,save)=>{await assert.rejects(save(h.fields),/Lost response/);return save(h.fields);});
  await h.api.uploadFile();
  assert.equal(h.calls.length,2);assert.equal(JSON.stringify(h.calls[0]),JSON.stringify(h.calls[1]));
  const [path,{body}]=h.calls[0];assert.equal(path,'/api/engagements/eng_synthetic/workpapers/paper1/attachments');
  assert.equal(body.content_hex,Buffer.from(h.bytes).toString('hex'));assert.equal(body.expected_revision,1);
  assert.match(body.upload_id,/^upload-/);assert.equal(body.operator_label,'Synthetic preparer');
});

test('editing an uncertain upload cannot reuse its identity with different content',async()=>{
  const h=harness();h.setRequest(async()=>{throw Error('Lost response');});
  h.api.setDialog(async(_t,_c,save)=>{await assert.rejects(save(h.fields),/Lost response/);h.fields.set('note','Changed request');await assert.rejects(save(h.fields),/Retry the unchanged upload/);});
  await h.api.uploadFile();assert.equal(h.calls.length,1);
});

test('leaving during asynchronous file reading prevents an upload',async()=>{
  const h=harness(),gate=deferred();h.fields.get('file').arrayBuffer=()=>gate.promise;
  const pending=h.api.uploadFile();h.api.S.version++;gate.resolve(h.bytes.buffer);
  await assert.rejects(pending,/Engagement changed/);assert.equal(h.calls.length,0);
});

test('a late upload response cannot replace the new engagement',async()=>{
  const h=harness(),gate=deferred(),entered=deferred();
  h.setRequest(()=>{entered.resolve();return gate.promise;});
  const pending=h.api.uploadFile();await entered.promise;
  const next={id:'eng_other',status:'draft',revision:1};h.api.S.version++;h.api.S.record=next;h.api.S.section='overview';
  gate.resolve({id:'eng_synthetic',revision:2,document:{workpapers:[]}});await pending;
  assert.equal(h.api.S.record,next);assert.equal(h.api.S.section,'overview');
});

test('unsaved inputs, locked engagements and invalid file sizes cannot start uploads',async()=>{
  for(const state of ['dirty','locked','empty','oversized']){
    const h=harness();if(state==='dirty')h.api.S.dirty=true;
    if(state==='locked')h.api.S.record.status='locked';
    if(state==='empty')h.fields.get('file').size=0;
    if(state==='oversized')h.fields.get('file').size=262145;
    await assert.rejects(h.api.uploadFile());assert.equal(h.calls.length,0);
  }
});

test('file downloads verify bytes and SHA before creating a download URL',async()=>{
  const h=harness();withFile(h);
  h.setFetch(async(path,options)=>{assert.equal(path,'/api/engagements/eng_synthetic/workpapers/paper1/attachments/file_one');assert.equal(options.redirect,'error');assert.equal(options.headers.authorization,'Bearer synthetic-local-key');return {ok:true,blob:async()=>new Blob([h.bytes])};});
  await h.api.downloadFile('file_one');assert.deepEqual(h.downloads,['Synthetic Café.bin']);
  assert.equal(h.objects.length,1);assert.deepEqual(Buffer.from(await h.objects[0].arrayBuffer()),Buffer.from(h.bytes));
});

test('mismatched download length or SHA never creates an output file',async()=>{
  for(const bytes of [new Uint8Array([1]),new Uint8Array(6)]){
    const h=harness();withFile(h);h.setFetch(async()=>({ok:true,blob:async()=>new Blob([bytes])}));
    await assert.rejects(h.api.downloadFile('file_one'),/does not match/);assert.equal(h.objects.length,0);
  }
});

test('late file or bundle downloads cannot be attributed to another engagement',async()=>{
  for(const kind of ['file','bundle']){
    const h=harness(),gate=deferred();withFile(h);h.setFetch(()=>gate.promise);
    const pending=kind==='file'?h.api.downloadFile('file_one'):h.api.download('attachments/package');
    h.api.S.version++;h.api.S.record={id:'eng_other',revision:1};
    gate.resolve({ok:true,blob:async()=>new Blob([h.bytes])});await pending;assert.equal(h.objects.length,0);
  }
});

test('file metadata is escaped and locked workspaces have downloads without uploads',()=>{
  const h=harness();withFile(h);const f=h.api.S.draft.workpapers[0].attachment_versions[0];
  f.file_name='<img src=x>';f.note='<svg onload=alert(1)>';f.operator_label='<script>';
  h.api.S.record.status='locked';h.api.renderFiles();
  assert.ok(h.editor.innerHTML.includes('&lt;img src=x&gt;'));assert.ok(!h.editor.innerHTML.includes('<svg'));
  assert.ok(h.editor.innerHTML.includes('download-file'));assert.ok(!h.editor.innerHTML.includes('upload-file'));
});

test('an attached workpaper cannot be removed through the draft editor',async()=>{
  const h=harness();withFile(h);await assert.rejects(h.api.remove('paper','paper1'),/immutable file versions/);
  assert.equal(h.api.S.draft.workpapers.length,1);assert.equal(h.api.S.dirty,false);
});

test('bundle availability includes immutable origin references after rollforward',()=>{
  const h=harness();assert.equal(h.api.hasFiles(),false);
  h.api.S.record.rollforward_origin={source_snapshot:{input_document:{workpapers:[{attachment_versions:[{id:'historical'}]}]}}};
  assert.equal(h.api.hasFiles(),true);
});
