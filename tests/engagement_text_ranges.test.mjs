import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import vm from 'node:vm';
import {webcrypto,createHash} from 'node:crypto';
const source=fs.readFileSync(new URL('../public/apps/engagements/engagements.js',import.meta.url),'utf8');
const sha=value=>createHash('sha256').update(value,'utf8').digest('hex');
const clone=value=>JSON.parse(JSON.stringify(value));
const deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return {promise,resolve};};
function harness(body='A😀e\u0301\r\nB\rC\nD'){
 const editor={innerHTML:'',querySelector:()=>null,querySelectorAll:()=>[]},main={...editor},dialogs=[],requests=[],toasts=[],controls=[];
 const esc=value=>String(value??'').replace(/[&<>"']/g,c=>'&#'+c.charCodeAt(0)+';');
 const sandbox={NXT:{api:(...args)=>{requests.push(args);throw Error('Selected text must not make any API/financial calls');},esc,modal(){throw Error('Unexpected modal');},toast:(...args)=>toasts.push(args),confirm:async()=>true},document:{querySelector:s=>s==='#main'?main:s==='#editor'?editor:s==='.skip'?{}:null,querySelectorAll:()=>[],addEventListener(){}},window:{addEventListener(){}},location:{hash:'#engagement/eng_synthetic'},crypto:{randomUUID:()=>webcrypto.randomUUID(),subtle:webcrypto.subtle},TextEncoder,Uint8Array,structuredClone,console};sandbox.parent=sandbox.window;
 const bootstrap="window.addEventListener('hashchange',()=>route());boot();";assert.equal(source.split(bootstrap).length,2);
 vm.runInNewContext(source.replace(bootstrap,"window.addEventListener('hashchange',()=>route());").replace(/\}\)\(\);\s*$/,`globalThis.review={S,invoke,addTextRangeAnnotation,addAnnotation,removeAnnotation,renderAnnotations,normalizedSavedText,captureTextRange,setDialog:fn=>dialog=fn};})();`),sandbox);
 const api=sandbox.review,draft={name:'Synthetic',period_start:'2025-01-01',period_end:'2025-12-31',currency:'CAD',accounts:[{id:'a1',code:'100',name:'Synthetic current label'}],groups:[],adjustments:[],issues:[],workpapers:[{id:'p1',reference:'A',title:'Saved text',body,required:true,account_ids:[],adjustment_ids:[],depends_on:[]},{id:'p2',reference:'B',title:'Empty text',body:'',required:false,account_ids:[],depends_on:[]},{id:'p3',reference:'C',title:'Pending placeholder',body:'',document_type:'placeholder',account_ids:[],depends_on:[]}],tickmark_catalogue:[{id:'custom1',code:'XY',label:'Recorded meaning',description:'Original synthetic procedure'}]};
 api.S.record={id:'eng_synthetic',revision:3,input_revision:2,status:'draft',document:clone(draft),signoffs:[]};api.S.draft=draft;api.S.section='annotations';
 const fields=new Map([['workpaper_id','p1'],['kind','note'],['operator_label','Synthetic label'],['note','Synthetic selected-text evidence']]);let selection={start:1,end:3};
 function setDialog(handler){api.setDialog(async(title,html,save,label,setup)=>{
  dialogs.push({title,html,label,setup});
  let control,form;
  if(setup){const normalized=api.S.draft.workpapers.find(p=>p.id===fields.get('workpaper_id'))?.body.replace(/\r\n?/g,'\n')??'';control={value:normalized.startsWith('\n')?normalized.slice(1):normalized,readOnly:true,selectionStart:0,selectionEnd:0,setSelectionRange(start,end){this.selectionStart=start;this.selectionEnd=end;}};form={isConnected:true,querySelector:selector=>selector==='[data-range-body]'?control:{get value(){const name=selector.match(/name="([^"]+)"/)?.[1];return fields.get(name);}}};controls.push(control);setup(form);control.selectionStart=selection.start;control.selectionEnd=selection.end;}
  return handler(title,html,save,label,form,control);
 });}
 setDialog(async(_t,_h,save,_label,form)=>save(fields,form));
 return {api,sandbox,fields,editor,dialogs,controls,requests,toasts,setDialog,select(start,end){selection={start,end};},saved(){api.S.record.document=clone(api.S.draft);api.S.dirty=false;},get control(){return controls.at(-1);}};
}

test('native selection records exact original scalar bytes, owner and both hashes without an API call',async()=>{
 const h=harness();await h.api.addTextRangeAnnotation();const mark=h.api.S.draft.workpapers[0].annotations[0];assert.match(mark.id,/^annotation-/);assert.equal(mark.body_sha256,sha('A😀e\u0301\r\nB\rC\nD'));assert.deepEqual(clone(mark.text_range_anchor),{engagement_id:'eng_synthetic',start_utf8_byte:1,end_utf8_byte:5,selected_text:'😀',selection_sha256:sha('😀')});assert.equal('account_anchor'in mark,false);assert.equal('custom_mark'in mark,false);assert.equal(h.api.S.dirty,true);assert.equal(h.api.S.record.revision,3);assert.equal(h.api.S.record.document.workpapers[0].annotations,undefined);assert.equal(h.requests.length,0);
});

test('UTF16 scalar mapping preserves combining marks and UTF8 prefixes across astral text',()=>{
 const h=harness(),body='é😀e\u0301Z',native=body;
 for(const [start,end,from,to,text]of [[0,1,0,2,'é'],[1,3,2,6,'😀'],[3,5,6,9,'e\u0301'],[4,5,7,9,'\u0301'],[5,6,9,10,'Z']]){const range=h.api.captureTextRange(body,native,start,end);assert.deepEqual(clone(range),{start_utf8_byte:from,end_utf8_byte:to,selected_text:text});}
 for(const [start,end]of [[1,2],[2,3],[2,4]])assert.throws(()=>h.api.captureTextRange(body,native,start,end),/scalar boundaries/);
});

test('LF/CRLF/bareCR normalization maps native ranges back to original saved text exactly',()=>{
 const h=harness(),body='é\r\n😀\rZ\nQ',native='é\n😀\nZ\nQ';assert.equal(h.api.normalizedSavedText(body).value,native);
 for(const [start,end,from,to,text]of [[1,2,2,4,'\r\n'],[2,4,4,8,'😀'],[4,5,8,9,'\r'],[5,7,9,11,'Z\n'],[1,5,2,9,'\r\n😀\r']])assert.deepEqual(clone(h.api.captureTextRange(body,native,start,end)),{start_utf8_byte:from,end_utf8_byte:to,selected_text:text});
});

test('setup restores a leading LF/CRLF after HTML parsing before any native selection is captured',async()=>{
 for(const body of ['\nLeading','\r\nLeading','\rLeading','\n\nSecond']){const h=harness(body);h.select(0,1);await h.api.addTextRangeAnnotation();assert.equal(h.control.value,body.replace(/\r\n?/g,'\n'));assert.equal(h.control.readOnly,true);assert.equal(h.api.S.draft.workpapers[0].annotations[0].text_range_anchor.selected_text,body.startsWith('\r\n')?'\r\n':body[0]);}
});

test('whitespace is a valid exact copied selection, but empty, malformed and oversized ranges are rejected',async()=>{
 const h=harness(' \r\n\tX');h.select(0,3);await h.api.addTextRangeAnnotation();assert.equal(h.api.S.draft.workpapers[0].annotations[0].text_range_anchor.selected_text,' \r\n\t');
 for(const [start,end]of [[0,0],[-1,1],[0,999],[0.5,2],[0,NaN],[0,Number.MAX_SAFE_INTEGER+1]])assert.throws(()=>h.api.captureTextRange('abc','abc',start,end),/nonempty/);
 assert.throws(()=>h.api.captureTextRange('x'.repeat(4097),'x'.repeat(4097),0,4097),/4,096/);
 assert.throws(()=>h.api.captureTextRange('x'.repeat(16385),'x'.repeat(16385),0,1),/UTF-8 bytes/);
 assert.throws(()=>h.api.captureTextRange('\ud800','\ud800',0,1),/UTF-8|scalar/);
});

test('copied selection permits4096UTF8bytes and an end offset of16384 without truncation',async()=>{
 const h=harness('a'.repeat(12288)+'😀'.repeat(1024));h.select(12288,14336);await h.api.addTextRangeAnnotation();const range=h.api.S.draft.workpapers[0].annotations[0].text_range_anchor;assert.equal(range.start_utf8_byte,12288);assert.equal(range.end_utf8_byte,16384);assert.equal(Buffer.byteLength(range.selected_text),4096);assert.equal(range.selection_sha256,sha('😀'.repeat(1024)));
});

test('a valid range copy cannot exceed the existing whole-engagement input limit',async()=>{
 const h=harness();h.api.S.draft.workpapers=Array.from({length:32},(_,i)=>({id:'p'+(i+1),reference:String(i+1),title:'Synthetic paper',body:'x'.repeat(16000),account_ids:[],adjustment_ids:[],depends_on:[]}));h.api.S.draft.notes='';const before=Buffer.byteLength(JSON.stringify(h.api.S.draft)),padding=524000-before;assert.ok(padding>0&&padding<16384);h.api.S.draft.notes='n'.repeat(padding);h.saved();h.select(0,4096);assert.equal(Buffer.byteLength(JSON.stringify(h.api.S.draft)),524000);await assert.rejects(h.api.addTextRangeAnnotation(),/512 KiB input limit/);assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.dirty,false);
});

test('saved/manual/nonempty/revision eligibility rejects unsupported states before opening dialogs',async()=>{
 for(const mode of ['dirty','phantom','locked','archived','empty','placeholder','revision']){const h=harness();if(mode==='dirty')h.api.S.dirty=true;if(mode==='phantom')h.api.S.draft.currency='USD';if(mode==='locked'||mode==='archived')h.api.S.record.status=mode;if(mode==='empty'){h.api.S.draft.workpapers[0].body='';h.saved();}if(mode==='placeholder'){h.api.S.draft.workpapers[0].body='';h.api.S.draft.workpapers[0].document_type='placeholder';h.saved();}if(mode==='revision')h.api.S.record.input_revision=undefined;await assert.rejects(h.api.addTextRangeAnnotation());assert.equal(h.dialogs.length,0);assert.equal(h.requests.length,0);}
});

test('first picker is bounded to100 papers, excludes empty/placeholders and rejects injected targets',async()=>{
 const h=harness();await h.api.addTextRangeAnnotation();assert.ok(h.dialogs[0].html.includes('value="p1"'));assert.ok(!h.dialogs[0].html.includes('value="p2"'));assert.ok(!h.dialogs[0].html.includes('value="p3"'));
 const forced=harness();forced.fields.set('workpaper_id','p3');await assert.rejects(forced.api.addTextRangeAnnotation(),/current saved nonempty manual/);
 const bound=harness();bound.api.S.draft.workpapers=Array.from({length:100},(_,i)=>({id:'p'+i,reference:String(i),title:'Synthetic',body:'Text'}));bound.saved();bound.fields.set('workpaper_id','p99');bound.select(0,1);await bound.api.addTextRangeAnnotation();assert.equal((bound.dialogs[0].html.match(/<option /g)||[]).length,100);assert.equal(bound.api.S.draft.workpapers[99].annotations[0].text_range_anchor.selected_text,'T');
});

test('changed readonly text/property, surrogate splits, empty and oversized native selections never append',async()=>{
 for(const mode of ['value','readonly','split','empty','oversize']){const h=harness(mode==='oversize'?'x'.repeat(4097):'A😀B');if(mode==='split')h.select(1,2);if(mode==='empty')h.select(0,0);if(mode==='oversize')h.select(0,4097);h.setDialog(async(_title,_html,save,_label,form,control)=>{if(control){if(mode==='value')control.value='Changed';if(mode==='readonly')control.readOnly=false;}return save(h.fields,form);});await assert.rejects(h.api.addTextRangeAnnotation());assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.dirty,false);}
});

test('selection, readonly value, field and form changes during either hash reject the old intent',async()=>{
 for(const hashNumber of [1,2])for(const change of ['selection','value','node','readonly','kind','note','operator','disconnected']){const h=harness(),gate=deferred(),entered=deferred();let count=0,formRef;h.sandbox.crypto.subtle={digest:async(algorithm,data)=>{if(++count===hashNumber){entered.resolve();return gate.promise;}return webcrypto.subtle.digest(algorithm,data);}};h.setDialog(async(_t,_html,save,_l,form)=>{if(form)formRef=form;return save(h.fields,form);});const pending=h.api.addTextRangeAnnotation();await entered.promise;if(change==='selection')h.control.selectionEnd=4;if(change==='value')h.control.value+='x';if(change==='readonly')h.control.readOnly=false;if(change==='kind')h.fields.set('kind','agreed');if(change==='note')h.fields.set('note','Changed evidence');if(change==='operator')h.fields.set('operator_label','Changed label');if(change==='disconnected')formRef.isConnected=false;if(change==='node'){const query=formRef.querySelector;formRef.querySelector=selector=>selector==='[data-range-body]'?{...h.control}:query(selector);}gate.resolve(new Uint8Array(32).buffer);await assert.rejects(pending,/changed while anchoring/);assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);}
});

test('full saved owner/record/draft/body/custom context remains guarded across both hash awaits',async()=>{
 const changes=[h=>h.api.S.version++,h=>h.api.S.record={...h.api.S.record},h=>h.api.S.record.id='eng_foreign',h=>h.api.S.record.revision++,h=>h.api.S.record.input_revision++,h=>h.api.S.draft={...h.api.S.draft},h=>h.api.S.draft.workpapers[0].body='Changed body',h=>h.api.S.draft.workpapers[0].document_type='placeholder',h=>h.api.S.draft.tickmark_catalogue[0].description='Changed meaning',h=>h.api.S.draft.accounts[0].name='Changed account',h=>h.api.S.record.document.name='Changed saved base'];
 for(const hashNumber of [1,2])for(const change of changes){const h=harness(),old=h.api.S.draft.workpapers[0],gate=deferred(),entered=deferred();let count=0;h.sandbox.crypto.subtle={digest:async(algorithm,data)=>{if(++count===hashNumber){entered.resolve();return gate.promise;}return webcrypto.subtle.digest(algorithm,data);}};const pending=h.api.addTextRangeAnnotation();await entered.promise;change(h);gate.resolve(new Uint8Array(32).buffer);await assert.rejects(pending,/saved inputs changed/);assert.equal(old.annotations,undefined);}
});

test('context changes at either dialog boundary cannot apply a candidate to either engagement',async()=>{
 for(const phase of ['first','second']){const h=harness(),old=h.api.S.draft;h.setDialog(async(title,_html,save,_l,form)=>{const result=await save(h.fields,form);if(phase==='first'&&title==='Choose saved text workpaper'||phase==='second'&&title==='Add selected text tickmark or note'){h.api.S.version++;h.api.S.record={id:'other',status:'draft',revision:1,input_revision:1};h.api.S.draft={workpapers:[]};}return result;});await assert.rejects(h.api.addTextRangeAnnotation(),/changed/);assert.equal(old.workpapers[0].annotations,undefined);assert.equal(h.api.S.draft.workpapers.length,0);assert.equal(h.api.S.dirty,false);}
});

test('capacity is enforced before opening the text dialog and if another mark arrives while hashing',async()=>{
 const h=harness();h.api.S.draft.workpapers[0].annotations=Array.from({length:50},(_,i)=>({id:'old'+i}));h.saved();await assert.rejects(h.api.addTextRangeAnnotation(),/Maximum 50/);assert.equal(h.dialogs.length,1);
 const late=harness(),gate=deferred(),entered=deferred();late.sandbox.crypto.subtle={digest:()=>{entered.resolve();return gate.promise;}};const pending=late.api.addTextRangeAnnotation();await entered.promise;late.api.S.draft.workpapers[0].annotations=Array.from({length:50},()=>({}));gate.resolve(new Uint8Array(32).buffer);await assert.rejects(pending,/changed|Maximum 50/);assert.equal(late.api.S.draft.workpapers[0].annotations.length,50);
});

test('custom mark meanings are validated copied snapshots, independent of later catalogue changes',async()=>{
 const h=harness();h.fields.set('kind','custom:custom1');await h.api.addTextRangeAnnotation();const mark=h.api.S.draft.workpapers[0].annotations[0];assert.equal(mark.kind,'custom');assert.notEqual(mark.custom_mark,h.api.S.draft.tickmark_catalogue[0]);h.api.S.draft.tickmark_catalogue[0].label='New meaning';assert.equal(mark.custom_mark.label,'Recorded meaning');assert.equal('account_anchor'in mark,false);
 for(const [key,value]of [['kind','custom:missing'],['kind','invented'],['operator_label','é'.repeat(65)],['note','界'.repeat(1366)],['note','Bad\u0085text']]){const bad=harness();bad.fields.set(key,value);await assert.rejects(bad.api.addTextRangeAnnotation());assert.equal(bad.api.S.draft.workpapers[0].annotations,undefined);}
});

test('cancelling either dialog or a failed digest leaves no partial text-range mutation',async()=>{
 for(const mode of ['first','second','digest']){const h=harness();if(mode==='digest')h.sandbox.crypto.subtle={digest:async()=>{throw Error('Synthetic digest failure');}};else h.setDialog(async(title,_html,save,_l,form)=>{if(mode==='first'||title==='Add selected text tickmark or note')return;return save(h.fields,form);});if(mode==='digest')await assert.rejects(h.api.addTextRangeAnnotation(),/digest failure/);else await h.api.addTextRangeAnnotation();assert.equal(h.api.S.draft.workpapers[0].annotations,undefined);assert.equal(h.api.S.dirty,false);assert.equal(h.requests.length,0);}
});

test('real invoke serializes the action under its busy lock and creates one local-only mark',async()=>{
 const h=harness(),gate=deferred(),entered=deferred();h.sandbox.crypto.subtle={digest:async(algorithm,data)=>{entered.resolve();await gate.promise;return webcrypto.subtle.digest(algorithm,data);}};const first=h.api.invoke('text-range-annotation');await entered.promise;assert.equal(h.api.S.busy,true);await h.api.invoke('text-range-annotation');assert.equal(h.dialogs.length,2);gate.resolve();await first;assert.equal(h.api.S.draft.workpapers[0].annotations.length,1);assert.equal(h.api.S.busy,false);assert.equal(h.toasts.length,0);assert.equal(h.requests.length,0);
});

test('renderer uses escaped recorded copy, preserving leadingLF/CR entities without slicing current body',async()=>{
 const copied='\r\n<script>Recorded</script>\rEnd',h=harness(copied);h.select(0,copied.replace(/\r\n?/g,'\n').length);await h.api.addTextRangeAnnotation();const mark=h.api.S.draft.workpapers[0].annotations[0];h.api.S.draft.workpapers[0].body='x';h.api.renderAnnotations();const html=h.editor.innerHTML,span=html.match(/<pre class="paper-preview"><span>([\s\S]*?)<\/span><\/pre>/)[1],decoded=span.replace(/&#(\d+);/g,(_m,n)=>String.fromCharCode(Number(n)));assert.equal(decoded,copied);assert.ok(html.includes('not reconstructed from current text'));assert.ok(html.includes('Recorded original UTF-8 byte range'));assert.ok(html.includes(mark.text_range_anchor.selection_sha256));assert.ok(!html.includes('<script>'));assert.ok(html.includes('&#13;'));
});

test('body-only and account anchor rendering/field shapes remain unchanged by adding the range renderer',()=>{
 const h=harness(),bodyMark={id:'body1',kind:'note',note:'Body only',operator_label:'Unverified',body_sha256:'a'.repeat(64)},accountMark={id:'account1',kind:'note',note:'Account only',operator_label:'Unverified',body_sha256:'b'.repeat(64),account_anchor:{engagement_id:'eng_synthetic',account_id:'a1',context_sha256:'c'.repeat(64)}};h.api.S.draft.workpapers[0].annotations=[bodyMark,accountMark];const before=JSON.stringify(h.api.S.draft.workpapers[0].annotations);h.api.renderAnnotations();assert.equal(JSON.stringify(h.api.S.draft.workpapers[0].annotations),before);assert.ok(h.editor.innerHTML.includes('Workpaper text only'));assert.ok(h.editor.innerHTML.includes('Recorded account target ID'));assert.ok(h.editor.innerHTML.includes('current label does not attest earlier values'));assert.equal('text_range_anchor'in bodyMark,false);assert.equal('text_range_anchor'in accountMark,false);
});

test('recorded owner/range/copy metadata is escaped independently from current paper labels',async()=>{
 const h=harness('safe');h.select(0,1);await h.api.addTextRangeAnnotation();const anchor=h.api.S.draft.workpapers[0].annotations[0].text_range_anchor;anchor.engagement_id='<svg>';anchor.selected_text='<img src=x>';anchor.selection_sha256='<script>';h.api.S.draft.workpapers[0].title='<iframe>';h.api.renderAnnotations();assert.ok(!h.editor.innerHTML.includes('<img'));assert.ok(!h.editor.innerHTML.includes('<svg>'));assert.ok(!h.editor.innerHTML.includes('<script>'));assert.ok(!h.editor.innerHTML.includes('<iframe>'));assert.ok(h.editor.innerHTML.includes('&#60;img'));
});
