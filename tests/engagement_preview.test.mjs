import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';

const source=fs.readFileSync(new URL('../public/apps/engagements/engagements.js',import.meta.url),'utf8');
function harness(){
  const host={innerHTML:''},main={innerHTML:'',querySelector:()=>null,querySelectorAll:()=>[]},skip={},importText={value:'{"format":"synthetic-only"}'};
  const window={addEventListener(){}};
  let requests=0;
  let requestHandler=()=>{throw Error('No request expected');};
  const esc=v=>String(v).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;').replaceAll("'",'&#39;');
  const sandbox={NXT:{esc,api(...args){requests++;return requestHandler(...args);},modal(){},toast(){}},
    document:{querySelector:s=>s==='#main'?main:s==='#import-preview'?host:s==='.skip'?skip:s==='[data-import-json]'?importText:null,querySelectorAll:()=>[],addEventListener(){}},
    window,parent:window,location:{hash:'#worklist'},structuredClone,console};
  const bootstrap="window.addEventListener('hashchange',()=>route());boot();";
  assert.equal(source.split(bootstrap).length,2);
  const runnable=source.replace(bootstrap,"window.addEventListener('hashchange',()=>route());").replace(/\}\)\(\);\s*$/, 'globalThis.subject={S,renderTbPreview,ledgerPreview,previewImport,commitImport};})();');
  vm.runInNewContext(runnable,sandbox);
  sandbox.subject.S.draft={groups:[]};
  sandbox.subject.S.record={id:'eng_synthetic',revision:1,status:'draft'};
  sandbox.subject.S.section='import';
  return {subject:sandbox.subject,host,requests:()=>requests,setRequest:fn=>requestHandler=fn};
}
const preview=()=>({id:'synthetic',engagement_id:'eng_synthetic',expected_revision:1,binding_sha256:'a'.repeat(64),target_input_sha256:'d'.repeat(64),source_hash:'b'.repeat(64),canonical_hash:'c'.repeat(64),summary:{accounts:1},accounts:[{id:'a1',code:'001',name:'Synthetic cash',kind:'asset',debit_cents:123,credit_cents:0,prior_debit_cents:0,prior_credit_cents:0}],warnings:[],checks:{diagnostics:[],totals:{unadjusted_debit_cents:123}},provenance:{source:'native_json'}});
function deferred(){let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};}

test('complete bound preview shows staged amounts and references without making requests',()=>{
  const {subject,host,requests}=harness(),p=preview();
  subject.renderTbPreview(p);
  assert.ok(host.innerHTML.includes('Staged data binding SHA-256 '+'a'.repeat(64)));
  assert.ok(host.innerHTML.includes('$1.23'));
  assert.ok(host.innerHTML.includes('Commit account import'));
  assert.ok(host.innerHTML.includes('not an independent source attestation or balance approval'));
  assert.equal(requests(),0);
});

test('missing or malformed bindings discard the preview before rendering a commit control',()=>{
  for(const binding of [undefined,'','A'.repeat(64),'<img src=x>',12]){
    const {subject,host,requests}=harness(),p=preview();p.binding_sha256=binding;
    subject.S.preview=p;
    assert.throws(()=>subject.renderTbPreview(p),/fresh preview/);
    assert.equal(subject.S.preview,null);
    assert.equal(host.innerHTML,'');
    assert.equal(requests(),0);
  }
});

test('bound empty replacement remains explicit and retains server diagnostics',()=>{
  const {subject,host}=harness(),p=preview();
  p.accounts=[];p.summary.accounts=0;
  p.checks.diagnostics=[{code:'missing_accounts',message:'Synthetic missing accounts',blocking:true}];
  subject.renderTbPreview(p);
  assert.ok(host.innerHTML.includes('Committing an empty account list clears the current draft accounts'));
  assert.ok(host.innerHTML.includes('Synthetic missing accounts'));
});

test('a target input hash is required along with the complete preview binding',()=>{
  const {subject,host}=harness(),p=preview();
  delete p.target_input_sha256;
  subject.S.preview=p;
  assert.throws(()=>subject.renderTbPreview(p),/fresh preview/);
  assert.equal(subject.S.preview,null);
  assert.equal(host.innerHTML,'');
});

test('source metadata and staged account text remain escaped inside a bound preview',()=>{
  const {subject,host}=harness(),p=preview();
  p.accounts[0].name='<img src=x onerror=alert(1)>';
  p.source_hash='<svg onload=alert(1)>';
  subject.renderTbPreview(p);
  assert.ok(!host.innerHTML.includes('<img'));
  assert.ok(!host.innerHTML.includes('<svg'));
  assert.ok(host.innerHTML.includes('&lt;img src=x onerror=alert(1)&gt;'));
  assert.ok(host.innerHTML.includes('&lt;svg onload=alert(1)&gt;'));
});

test('a preview for another target or revision cannot render or begin a commit',async()=>{
  for(const mutation of [p=>p.engagement_id='eng_other',p=>p.expected_revision=2]){
    const {subject,host,requests}=harness(),p=preview();mutation(p);
    subject.S.preview=p;
    await assert.rejects(subject.commitImport(),/fresh preview/);
    assert.equal(requests(),0);
    assert.throws(()=>subject.renderTbPreview(p),/fresh preview/);
    assert.equal(subject.S.preview,null);assert.equal(host.innerHTML,'');
  }
});

test('a late Ledger preview cannot replace another engagement or section',async()=>{
  const {subject,host,setRequest}=harness(),gate=deferred();
  setRequest(path=>{assert.equal(path,'/api/engagements/eng_synthetic/ledger/preview');return gate.promise;});
  const pending=subject.ledgerPreview();
  subject.S.version++;subject.S.record={id:'eng_other',revision:1,status:'draft'};subject.S.section='overview';
  gate.resolve(preview());await pending;
  assert.equal(subject.S.section,'overview');assert.equal(subject.S.record.id,'eng_other');
  assert.equal(subject.S.preview,null);assert.equal(host.innerHTML,'');
});

test('an old native preview cannot appear after leaving and reopening the same engagement',async()=>{
  const {subject,host,setRequest}=harness(),gate=deferred();setRequest(()=>gate.promise);
  const pending=subject.previewImport();
  subject.S.version+=2;subject.S.record={id:'eng_synthetic',revision:1,status:'draft'};
  gate.resolve(preview());await pending;
  assert.equal(subject.S.preview,null);assert.equal(host.innerHTML,'');
});

test('a late commit response cannot replace the next engagement or its preview',async()=>{
  const {subject,setRequest}=harness(),gate=deferred();
  subject.S.preview=preview();
  setRequest(path=>{assert.equal(path,'/api/engagements/eng_synthetic/import/commit');return gate.promise;});
  const pending=subject.commitImport();
  const next={id:'eng_other',revision:1,status:'draft'},nextPreview={id:'other-preview'};
  subject.S.version++;subject.S.record=next;subject.S.preview=nextPreview;
  gate.resolve({id:'eng_synthetic',revision:2,document:{}});await pending;
  assert.equal(subject.S.record,next);assert.equal(subject.S.preview,nextPreview);
});
