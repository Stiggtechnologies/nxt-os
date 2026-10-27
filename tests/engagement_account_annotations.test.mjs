import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto,createHash} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/engagements/engagements.js',import.meta.url),'utf8');
const sha=value=>createHash('sha256').update(value,'utf8').digest('hex');
const context=id=>sha('synthetic-native-context:'+id);
const clone=value=>JSON.parse(JSON.stringify(value));
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
function harness(){
 const editor={innerHTML:'',querySelector:()=>null,querySelectorAll:()=>[]},main={...editor},requests=[],dialogs=[],toasts=[];
 const esc=value=>String(value??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';');let handler;
 const sandbox={NXT:{api:(path,options)=>{requests.push({path,options});return handler(path,options);},esc,toast:(...args)=>toasts.push(args),modal(){throw Error('Unexpected modal');},confirm:async()=>true},document:{querySelector:s=>s==='#main'?main:s==='#editor'?editor:s==='.skip'?{}:null,querySelectorAll:()=>[],addEventListener(){}},window:{addEventListener(){}},location:{hash:'#engagement/eng_synthetic'},crypto:{randomUUID:()=>webcrypto.randomUUID(),subtle:webcrypto.subtle},TextEncoder,Uint8Array,structuredClone,console};sandbox.parent=sandbox.window;
 const bootstrap="window.addEventListener('hashchange',()=>route());boot();";assert.equal(source.split(bootstrap).length,2);
 vm.runInNewContext(source.replace(bootstrap,"window.addEventListener('hashchange',()=>route());").replace(/\}\)\(\);\s*$/,`globalThis.review={S,invoke,addAccountAnnotation,addAnnotation,removeAnnotation,renderAnnotations,validateAccountAnchorOptions,accountAnnotationPapers,setDialog:fn=>dialog=fn};})();`),sandbox);
 const api=sandbox.review,draft={name:'Synthetic engagement',client_ref:'',kind:'internal',period_start:'2025-01-01',period_end:'2025-12-31',prior_period_start:null,prior_period_end:null,currency:'CAD',accounts:[{id:'a1',code:'100',name:'Synthetic cash',kind:'asset',group_id:'g1',debit_cents:100000,credit_cents:0,prior_debit_cents:0,prior_credit_cents:0},{id:'a2',code:'200',name:'Synthetic payable',kind:'liability',group_id:null,debit_cents:0,credit_cents:100000,prior_debit_cents:0,prior_credit_cents:0}],groups:[{id:'g1',code:'G',name:'Synthetic group',kind:'asset',gifi_code:null}],adjustments:[{id:'j1',reference:'JE1',date:'2025-12-31',kind:'adjusting',description:'Synthetic local entry',lines:[{account_id:'a1',debit_cents:100,credit_cents:0},{account_id:'a2',debit_cents:0,credit_cents:100}]}],workpapers:[{id:'p1',reference:'A',title:'First paper',body:'Café\r\n📎',required:true,account_ids:['a1','a2'],adjustment_ids:[],depends_on:[]},{id:'p2',reference:'B',title:'Second paper',body:'Second text',required:false,account_ids:['a2'],adjustment_ids:[],depends_on:[]},{id:'p3',reference:'C',title:'Pending slot',body:'',document_type:'placeholder',required:false,account_ids:['a1'],adjustment_ids:[],depends_on:[]}],issues:[],notes:'',expected_revision:null,tickmark_catalogue:[{id:'custom1',code:'XY',label:'Original meaning',description:'Original synthetic procedure'}]};
 api.S.record={id:'eng_synthetic',status:'draft',revision:3,input_revision:2,document:clone(draft),signoffs:[]};api.S.draft=draft;api.S.section='annotations';
 function options(paperId='p1'){const paper=api.S.draft.workpapers.find(p=>p.id===paperId);return {format:'nxt-account-annotation-options/v1',engagement_id:api.S.record.id,expected_revision:api.S.record.revision,input_revision:api.S.record.input_revision,input_hash:'a'.repeat(64),workpaper_id:paperId,body_sha256:sha(paper.body),accounts:paper.account_ids.map(account_id=>({account_id,context_sha256:context(account_id)})),scope:'Native account/group/local-adjustment context only; no Ledger mutation.'};}
 handler=async path=>{const match=path.match(/^\/api\/engagements\/eng_synthetic\/workpapers\/(p\d+)\/account-anchors$/);assert.ok(match,'Only the selected paper anchor GET is allowed');return options(match[1]);};
 const fields=new Map([['workpaper_id','p1'],['account_id','a1'],['kind','note'],['operator_label','Synthetic operator'],['note','Synthetic account evidence']]);
 function setDialog(fn){api.setDialog(async(title,html,save,...rest)=>{dialogs.push({title,html});return fn(title,html,save,...rest);});}
 setDialog(async(_title,_html,save)=>save(fields));
 return {api,sandbox,fields,editor,requests,dialogs,toasts,options,setDialog,setRequest:fn=>handler=fn,saved(){api.S.record.document=clone(api.S.draft);api.S.dirty=false;}};
}

test('account mark is a new unsaved record bound to exact body, owner and selected stable account/context',async()=>{
 const h=harness();h.fields.set('engagement_id','untrusted-hidden-owner');await h.api.addAccountAnnotation();const mark=h.api.S.draft.workpapers[0].annotations[0];assert.match(mark.id,/^annotation-/);assert.equal(mark.kind,'note');assert.equal(mark.body_sha256,sha('Café\r\n📎'));assert.deepEqual(clone(mark.account_anchor),{engagement_id:'eng_synthetic',account_id:'a1',context_sha256:context('a1')});assert.equal('custom_mark'in mark,false);assert.equal(h.api.S.dirty,true);assert.equal(h.api.S.record.revision,3);assert.equal(h.api.S.record.document.workpapers[0].annotations,undefined);assert.equal(h.requests.length,1);assert.equal(h.requests[0].options,undefined);assert.equal(h.dialogs.length,2);
});

test('real invoke owns the busy lock and serializes account-mark dialogs without duplicate reads or marks',async()=>{
 const h=harness(),gate=deferred(),entered=deferred(),options=h.options();h.setRequest(()=>{entered.resolve();return gate.promise;});const first=h.api.invoke('account-annotation');await entered.promise;assert.equal(h.api.S.busy,true);await h.api.invoke('account-annotation');assert.equal(h.requests.length,1);gate.resolve(options);await first;assert.equal(h.api.S.draft.workpapers[0].annotations.length,1);assert.equal(h.api.S.busy,false);assert.equal(h.toasts.length,0);
});

test('unsaved, phantom-edited, locked, no-reference and placeholder-only states never read anchors',async()=>{
 for(const kind of ['dirty','phantom','locked','no-refs','placeholder','revision']){const h=harness();if(kind==='dirty')h.api.S.dirty=true;if(kind==='phantom')h.api.S.draft.accounts[0].debit_cents++;if(kind==='locked')h.api.S.record.status='locked';if(kind==='no-refs'){h.api.S.draft.workpapers.forEach(p=>p.account_ids=[]);h.saved();}if(kind==='placeholder'){h.api.S.draft.workpapers=h.api.S.draft.workpapers.filter(p=>p.document_type==='placeholder');h.saved();}if(kind==='revision')h.api.S.record.input_revision=undefined;await assert.rejects(h.api.addAccountAnnotation());assert.equal(h.requests.length,0);assert.equal(h.dialogs.length,0);}
});

test('two-stage bounded picker shows at most100 papers then500 accounts without truncating all-pair targets',async()=>{
 const h=harness();h.api.S.draft.accounts=Array.from({length:500},(_,i)=>({id:'a'+i,code:String(i),name:'Synthetic '+i}));h.api.S.draft.workpapers=Array.from({length:100},(_,i)=>({id:'p'+i,reference:String(i),title:'Synthetic paper',body:'Synthetic text',account_ids:h.api.S.draft.accounts.map(a=>a.id)}));h.saved();h.fields.set('workpaper_id','p99');h.fields.set('account_id','a499');await h.api.addAccountAnnotation();const paperSelect=h.dialogs[0].html.match(/<select name="workpaper_id"[\s\S]*?<\/select>/)[0],accountSelect=h.dialogs[1].html.match(/<select name="account_id"[\s\S]*?<\/select>/)[0];assert.equal((paperSelect.match(/<option /g)||[]).length,100);assert.equal((accountSelect.match(/<option /g)||[]).length,500);assert.equal(h.api.S.draft.workpapers[99].annotations[0].account_anchor.account_id,'a499');assert.ok(h.requests[0].path.includes('/p99/account-anchors'));
});

test('paper options exclude placeholders/unlinked papers and account selection is restricted to exact refs',async()=>{
 const h=harness();h.api.S.draft.workpapers[1].account_ids=[];h.saved();await h.api.addAccountAnnotation();assert.ok(h.dialogs[0].html.includes('value="p1"'));assert.ok(!h.dialogs[0].html.includes('value="p2"'));assert.ok(!h.dialogs[0].html.includes('value="p3"'));
 const bad=harness();bad.fields.set('workpaper_id','p2');bad.fields.set('account_id','a1');await assert.rejects(bad.api.addAccountAnnotation(),/current referenced account/);assert.equal(bad.api.S.draft.workpapers[1].annotations,undefined);
});

test('malformed, duplicate or missing saved identities/references fail before fetching options',async()=>{
 for(const mutate of [h=>h.api.S.draft.accounts.push(clone(h.api.S.draft.accounts[0])),h=>h.api.S.draft.workpapers[0].account_ids.push('a1'),h=>h.api.S.draft.workpapers[0].account_ids=['foreign'],h=>h.api.S.draft.workpapers[1].id='p1']){const h=harness();mutate(h);h.saved();await assert.rejects(h.api.addAccountAnnotation(),/invalid|duplicated|incomplete/);assert.equal(h.requests.length,0);}
});

test('options owner/paper/revisions/hash/body/scope mismatches cannot produce an annotation',async()=>{
 const mutations=[v=>v.format='other',v=>v.engagement_id='eng_foreign',v=>v.workpaper_id='p2',v=>v.expected_revision++,v=>v.input_revision++,v=>v.expected_revision=3.5,v=>v.input_hash='A'.repeat(64),v=>v.input_hash='short',v=>v.body_sha256='b'.repeat(64),v=>v.body_sha256=v.body_sha256.toUpperCase(),v=>v.scope='',v=>v.scope={claim:'authority'}];
 for(const mutate of mutations){const h=harness();h.setRequest(async()=>{const value=h.options();mutate(value);return value;});await assert.rejects(h.api.addAccountAnnotation(),/do not match/);assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.dirty,false);assert.equal(h.dialogs.length,1);}
});

test('exact ref set rejects absent/extra/duplicate/foreign/null/invalid hash option rows',async()=>{
 for(const mutate of [v=>v.accounts.pop(),v=>v.accounts.push({account_id:'foreign',context_sha256:'a'.repeat(64)}),v=>v.accounts[1]=clone(v.accounts[0]),v=>v.accounts[1].account_id='foreign',v=>v.accounts[0]=null,v=>v.accounts[0].context_sha256='A'.repeat(64),v=>v.accounts[0].context_sha256='short',v=>v.accounts={}]){const h=harness();h.setRequest(async()=>{const value=h.options();mutate(value);return value;});await assert.rejects(h.api.addAccountAnnotation());assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.dirty,false);}
});

test('valid unordered options and an opaque input hash bind by stable account ID, not row position or guessed JSON hash',async()=>{
 const h=harness();h.fields.set('account_id','a2');h.setRequest(async()=>({...h.options(),input_hash:'e'.repeat(64),accounts:h.options().accounts.reverse()}));await h.api.addAccountAnnotation();assert.equal(h.api.S.draft.workpapers[0].annotations[0].account_anchor.account_id,'a2');assert.equal(h.api.S.draft.workpapers[0].annotations[0].account_anchor.context_sha256,context('a2'));
});

test('full saved preimage blocks all context changes during GET, including local financial/period/group inputs',async()=>{
 const changes=[h=>h.api.S.version++,h=>h.api.S.record={...h.api.S.record},h=>h.api.S.record.revision++,h=>h.api.S.record.input_revision++,h=>h.api.S.draft={...h.api.S.draft},h=>h.api.S.draft.accounts[0].debit_cents++,h=>h.api.S.draft.groups[0].name='Changed',h=>h.api.S.draft.adjustments[0].lines[0].debit_cents++,h=>h.api.S.draft.period_end='2026-12-31',h=>h.api.S.draft.currency='USD',h=>h.api.S.draft.workpapers[0].reference='Changed',h=>h.api.S.draft.workpapers[0].account_ids=['a2'],h=>h.api.S.draft.workpapers[0].document_type='placeholder',h=>h.api.S.record.document.notes='Changed saved base'];
 for(const change of changes){const h=harness(),gate=deferred(),entered=deferred(),value=h.options(),old=h.api.S.draft;h.setRequest(()=>{entered.resolve();return gate.promise;});const pending=h.api.addAccountAnnotation();await entered.promise;change(h);gate.resolve(value);await assert.rejects(pending,/changed/);assert.equal(old.workpapers[0].annotations,undefined);assert.equal(h.api.S.dirty,false);}
});

test('full preimage and body guards still hold while SHA-256 is pending',async()=>{
 for(const change of [h=>h.api.S.draft.workpapers[0].body='Changed text',h=>h.api.S.draft.accounts[1].name='Changed account',h=>h.api.S.draft.tickmark_catalogue[0].description='Changed meaning',h=>h.api.S.draft.workpapers.shift()]){const h=harness(),gate=deferred(),entered=deferred(),old=h.api.S.draft.workpapers[0];h.sandbox.crypto.subtle={digest:()=>{entered.resolve();return gate.promise;}};const pending=h.api.addAccountAnnotation();await entered.promise;change(h);gate.resolve(new Uint8Array(32).buffer);await assert.rejects(pending,/changed/);assert.equal(old.annotations,undefined);}
});

test('changing the live paper selection during GET/hash cannot bind an old response to a new target',async()=>{
 for(const phase of ['get','hash']){const h=harness(),gate=deferred(),entered=deferred(),value=h.options();let selected='p1';const form={isConnected:true,querySelector:()=>({get value(){return selected;}})};h.setDialog(async(_t,_html,save)=>save(h.fields,form));if(phase==='get')h.setRequest(()=>{entered.resolve();return gate.promise;});else h.sandbox.crypto.subtle={digest:()=>{entered.resolve();return gate.promise;}};const pending=h.api.addAccountAnnotation();await entered.promise;selected='p2';gate.resolve(phase==='get'?value:new Uint8Array(32).buffer);await assert.rejects(pending,/selected workpaper changed/);assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.draft.workpapers[1].annotations,undefined);}
});

test('changes between dialogs or after final candidate creation do not mutate either engagement',async()=>{
 for(const phase of ['between','final']){const h=harness(),old=h.api.S.draft;h.setDialog(async(title,_html,save)=>{const value=await save(h.fields);if(phase==='between'&&title==='Choose account-mark workpaper'||phase==='final'&&title==='Add account tickmark or note'){h.api.S.version++;h.api.S.record={id:'other',revision:1,input_revision:1,status:'draft'};h.api.S.draft={workpapers:[]};}return value;});await assert.rejects(h.api.addAccountAnnotation(),/changed/);assert.equal(old.workpapers[0].annotations,undefined);assert.equal(h.api.S.draft.workpapers.length,0);assert.equal(h.api.S.dirty,false);}
});

test('custom meaning and anchor values are immutable copies after application',async()=>{
 const h=harness(),value=h.options();h.fields.set('kind','custom:custom1');h.setRequest(async()=>value);await h.api.addAccountAnnotation();const mark=h.api.S.draft.workpapers[0].annotations[0];assert.notEqual(mark.custom_mark,h.api.S.draft.tickmark_catalogue[0]);h.api.S.draft.tickmark_catalogue[0].description='New meaning for future marks';value.accounts[0].context_sha256='f'.repeat(64);value.engagement_id='foreign';assert.equal(mark.custom_mark.description,'Original synthetic procedure');assert.equal(mark.account_anchor.context_sha256,context('a1'));assert.equal(mark.account_anchor.engagement_id,'eng_synthetic');
});

test('bounds and unsupported mark choices reject locally without an unsavable annotation',async()=>{
 for(const [key,value]of [['operator_label','é'.repeat(65)],['note','界'.repeat(1366)],['note','bad\u0085note'],['kind','invented'],['kind','custom:missing']]){const h=harness();h.fields.set(key,value);await assert.rejects(h.api.addAccountAnnotation());assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.dirty,false);}
});

test('annotation capacity is checked before the read and after all asynchronous work',async()=>{
 const h=harness();h.api.S.draft.workpapers[0].annotations=Array.from({length:50},(_,i)=>({id:'mark'+i}));h.saved();await assert.rejects(h.api.addAccountAnnotation(),/Maximum 50/);assert.equal(h.requests.length,0);
 const late=harness(),gate=deferred(),entered=deferred(),value=late.options();late.setRequest(()=>{entered.resolve();return gate.promise;});const pending=late.api.addAccountAnnotation();await entered.promise;late.api.S.draft.workpapers[0].annotations=Array.from({length:50},()=>({}));gate.resolve(value);await assert.rejects(pending,/changed|Maximum 50/);assert.equal(late.api.S.draft.workpapers[0].annotations.length,50);
});

test('cancelling either dialog or a failed options GET leaves no partial account mark',async()=>{
 for(const mode of ['first','second','error']){const h=harness();if(mode==='error')h.setRequest(async()=>{throw Object.assign(Error('Synthetic outage'),{status:503});});else h.setDialog(async(title,_html,save)=>{if(mode==='first'||title==='Add account tickmark or note')return;return save(h.fields);});if(mode==='error')await assert.rejects(h.api.addAccountAnnotation(),/outage/);else await h.api.addAccountAnnotation();assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.dirty,false);}
});

test('body-only marks retain the legacy field shape and do not fetch account or financial endpoints',async()=>{
 const h=harness();await h.api.addAnnotation();const mark=h.api.S.draft.workpapers[0].annotations[0];assert.equal('account_anchor'in mark,false);assert.equal(h.requests.length,0);assert.equal(mark.body_sha256,sha(h.api.S.draft.workpapers[0].body));
});

test('rendering shows stable IDs/digests and explicitly distinguishes current labels from old values',async()=>{
 const h=harness();await h.api.addAccountAnnotation();h.api.S.draft.accounts[0].code='<img src=x>';h.api.S.draft.accounts[0].name='<svg onload=bad>';h.api.S.draft.workpapers[0].annotations[0].note='<script>note</script>';h.api.renderAnnotations();const html=h.editor.innerHTML;assert.ok(html.includes('Recorded account target ID'));assert.ok(html.includes(context('a1')));assert.ok(html.includes('eng_synthetic'));assert.ok(html.includes('current label does not attest earlier values'));assert.ok(html.includes('&#60;img'));assert.ok(!html.includes('<img'));assert.ok(!html.includes('<script>'));assert.ok(!html.includes('100000'));
 const mark=h.api.S.draft.workpapers[0].annotations[0];mark.account_anchor.account_id='<img>';mark.account_anchor.context_sha256='<svg>';mark.account_anchor.engagement_id='<script>';h.api.renderAnnotations();assert.ok(!h.editor.innerHTML.includes('<svg>'));assert.ok(!h.editor.innerHTML.includes('<script>'));assert.ok(h.editor.innerHTML.includes('not in the current account list'));
});

test('removal plus reviewed replacement uses a new annotation ID and never rewrites the old snapshot',async()=>{
 const h=harness();await h.api.addAccountAnnotation();const old=h.api.S.draft.workpapers[0].annotations[0],snapshot=clone(old);await h.api.removeAnnotation(old.id);h.api.S.record.revision++;h.api.S.record.input_revision++;h.saved();h.fields.set('account_id','a2');await h.api.addAccountAnnotation();const mark=h.api.S.draft.workpapers[0].annotations[0];assert.notEqual(mark.id,old.id);assert.equal(mark.account_anchor.account_id,'a2');assert.deepEqual(clone(old),snapshot);assert.equal(h.requests.length,2);assert.ok(h.requests.every(call=>call.path.endsWith('/account-anchors')&&call.options===undefined));
});
