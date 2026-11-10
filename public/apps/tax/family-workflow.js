/* Independent family workspace; manual paired entry, never automatic tax elections. */
(function(){
  'use strict';
  const {api,esc,toCents}=window.NXT,BASE='/api/tax/family-workspaces';
  let generation=0,epoch=0,discardActive=()=>{};
  window.NXTCompany?.onChange?.(()=>epoch++);
  window.addEventListener('storage',e=>{if(e.key===window.NXTCompany?.KEY)epoch++;});
  const company=()=>typeof window.NXTCompany?.get==='function'?window.NXTCompany.get():null;
  function belongs(ref,scope){return scope!==null&&(scope===''?ref==='':typeof window.NXTCompany?.inScope==='function'?window.NXTCompany.inScope(ref,scope):ref===scope);}
  function validateWorkspace(w,id){
    const a=w?.primary,b=w?.spouse,f=w?.family;
    if(w?.schema!=='nxt-tax-family-workspace/v1'||f?.id!==id||!a||!b||a.id===b.id||f.primary_return_id!==a.id||f.spouse_return_id!==b.id||a.program!=='T1'||b.program!=='T1'||a.tax_year!==b.tax_year||a.tax_year!==w.tax_year||a.client_ref!==b.client_ref||a.client_ref!==w.client_ref||!Number.isSafeInteger(a.revision)||!Number.isSafeInteger(b.revision)||a.revision<1||b.revision<1||!/^[a-f0-9]{64}$/.test(w.family_sha256)||w.automatic_plan_apply_available!==false||typeof w.write_permitted!=='boolean'||!Array.isArray(w.plan_blockers))throw Error('Family response or membership contract differs. Reload before editing.');
    return w;
  }
  function pairSelection(rows,a,b,scope){
    const x=rows.find(r=>r.id===a),y=rows.find(r=>r.id===b);
    if(!x||!y||x.id===y.id||x.tax_year!==y.tax_year||x.client_ref!==y.client_ref||!belongs(x.client_ref,scope))throw Error('Choose two different same-year T1s in the current workspace.');
    return {primary_return_id:x.id,spouse_return_id:y.id,expected_primary_revision:x.revision,expected_spouse_revision:y.revision,expected_client_ref:x.client_ref};
  }
  function inputValues(form,side,original,fields){
    const out=structuredClone(original);
    for(const f of fields){const el=form.querySelector(`[data-side="${side}"][data-key="${f.key}"]`);if(!el)continue;
      if(f.kind==='boolean'){if(Object.hasOwn(original,f.key)||el.dataset.changed==='true')out[f.key]=el.checked;continue;}
      if(el.value===''){if(Object.hasOwn(original,f.key))throw Error('A saved field cannot be removed through paired entry.');continue;}
      const value=f.kind==='money'?toCents(el.value):f.kind==='number'?Number(el.value):el.value;
      if(['money','number'].includes(f.kind)&&(!Number.isSafeInteger(value)||(f.kind==='money'&&(value<0||value>1e14))))throw Error('Enter a valid value within the displayed scope.');out[f.key]=value;
    }return out;
  }
  async function route(id,main,{isCurrent=()=>true,setDirty=()=>{}}={}){
    const token=++generation,scope=company(),startEpoch=epoch;
    const current=()=>token===generation&&isCurrent()&&main.isConnected!==false&&scope===company()&&epoch===startEpoch;
    discardActive=()=>{if(current()){setDirty(false);void route(id,main,{isCurrent,setDirty});}};
    let list,catalog,record;
    try{[list,catalog]=await Promise.all([api(BASE),api('/api/tax/catalog')]);if(!Array.isArray(list?.families)||!Array.isArray(list?.candidates)||list.families.length>100||list.candidates.length>500)throw Error('Family list response differs.');if(id)record=validateWorkspace(await api(BASE+'/'+encodeURIComponent(id)),id);if(!current())return;}
    catch(e){if(current())main.innerHTML=`<div class="card"><h1>Family workspace unavailable</h1><p role="alert">${esc(e.message)}</p><p>Your account must be assigned to both family members. Unavailable data is not an empty family list.</p><a href="#worklist">Tax worklist</a></div>`;return;}
    if(scope===null||record&&!belongs(record.client_ref,scope)){main.innerHTML='<div class="card"><h1>Choose the correct workspace</h1><p>The family belongs to a different workspace, or the company selector is unavailable.</p><a href="#family-workspaces">Family list</a></div>';return;}
    const p=catalog.programs?.find(p=>p.id==='T1'),fields=p?.sections?.flatMap(s=>s.fields)||[];
    const schemaOK=catalog.coverage?.schema==='nxt-tax-coverage/v1'&&catalog.coverage.version==='2026-11-09.1';
    const ready=r=>{const rows=catalog.coverage?.scopes?.filter(x=>x.program==='T1'&&x.tax_year===r.tax_year&&x.jurisdiction===r.jurisdiction)||[];const row=rows.length===1?rows[0]:null;return schemaOK&&row?.engine_scope_available===true&&row.browser_entry_available===true&&row.entry_state==='verified_baseline_schema'&&row.entry_schema_id===p?.entry_schema_id&&row.entry_schema_year===p?.entry_schema_year&&row.tax_year===p?.entry_schema_year;};
    const canWrite=list.write_permitted===true&&(!record||record.write_permitted===true);
    const canPair=canWrite&&record?.paired_entry_available===true&&ready(record.primary)&&ready(record.spouse);
    const candidates=list.candidates.filter(r=>belongs(r.client_ref,scope)),families=list.families.filter(f=>belongs(f.client_ref,scope));
    const options=selected=>'<option value="">Choose a T1 workpaper</option>'+candidates.map(r=>`<option value="${esc(r.id)}"${r.id===selected?' selected':''}>${esc(r.name)} · ${r.tax_year}</option>`).join('');
    const memberForm=()=>`<form data-family-members><h2>${record?'Family members':'Link a family'}</h2><p>Choose two existing T1 workpapers for the same year. This records comparison membership, not legal identity or an automatic spouse-credit claim.</p><fieldset${canWrite?'':' disabled'} style="border:0;padding:0;margin:0"><label>Family name<input name="label" required maxlength="160" value="${esc(record?.family.label||'')}"></label><div style="display:grid;grid-template-columns:repeat(auto-fit,minmax(240px,1fr));gap:16px"><label>Primary<select name="primary" required>${options(record?.primary.id)}</select></label><label>Spouse or partner<select name="spouse" required>${options(record?.spouse.id)}</select></label></div>${canWrite?`<button class="btn primary">${record?'Save family members':'Link family'}</button>`:'<p>Read-only family access. You may compare your assigned returns.</p>'}</fieldset><p data-members-error role="alert"></p></form>`;
    main.innerHTML=`<div class="page-head"><div><h1>${esc(record?.family.label||'Families & planning')}</h1><p>Same-year workpapers · explicit comparison and paired entry</p></div><a href="#worklist">Tax worklist</a></div><div class="card">${memberForm()}</div>`;
    if(!record){main.innerHTML+=`<div class="card"><h2>Linked families</h2>${families.length?'<ul>'+families.map(f=>`<li><a href="#family-workspace/${encodeURIComponent(f.id)}">${esc(f.label)}</a></li>`).join('')+'</ul>':'<p>No families linked in this workspace.</p>'}</div>`;}
    else{
      const editable=canPair;
      const keys=[...new Set([...fields.map(f=>f.key),...Object.keys(record.primary.inputs),...Object.keys(record.spouse.inputs)])];
      function cell(side,key){const r=record[side],f=fields.find(f=>f.key===key),v=r.inputs[key];if(!editable||!f)return `<span>${esc(v===undefined?'Not supplied':JSON.stringify(v))}</span>`;
        const attrs=`data-side="${side}" data-key="${esc(key)}" aria-label="${esc(side+' '+f.label)}"`;
        return f.kind==='boolean'?`<input type="checkbox" ${attrs}${v===true?' checked':''}>`:`<input ${attrs} type="text" value="${esc(v===undefined?'':f.kind==='money'?(v/100).toFixed(2):v)}"${f.kind==='money'?' inputmode="decimal"':''}>`;
      }
      main.innerHTML+=`<div class="card"><h2>Side-by-side data entry</h2><p>${editable?'Save both sides in one transaction. Both calculations and review status will be cleared; calculation and review histories remain. Recalculate each return separately.':!canWrite?'Your staff access permits comparison only. An authorized preparer can update these assigned returns.':'This pair is read-only here. Reopen locked returns and use a verified year-specific entry schema before paired editing.'}</p><p class="form-note">On a small screen, scroll the comparison sideways to see both returns.</p><form data-pair><div style="overflow-x:auto"><table style="width:100%;min-width:620px"><thead><tr><th>Field</th>${['primary','spouse'].map(s=>`<th>${esc(record[s].name)}<br>${record[s].tax_year} · ${esc(record[s].jurisdiction)}<br><a href="#return/${encodeURIComponent(record[s].id)}">Open individual return</a><br><small>${record[s+'_current_supported_calculation']?'Current supported calculation':'Recalculation / scope review needed'}</small></th>`).join('')}</tr></thead><tbody>${keys.map(k=>`<tr><th scope="row">${esc(fields.find(f=>f.key===k)?.label||k)}</th><td>${cell('primary',k)}</td><td>${cell('spouse',k)}</td></tr>`).join('')}</tbody></table></div>${editable?'<button class="btn primary" data-save-pair disabled>Save both returns</button>':''}<p data-pair-error role="alert"></p></form></div><div class="card"><h2>PLAN · optimization readiness</h2><p>Automatic proposal application is unavailable. No pension election, medical/donation pooling or tuition transfer is applied by this workspace.</p><ul>${record.plan_blockers.map(x=>`<li>${esc(x)}</li>`).join('')}</ul><p>Paired manual entry is separate from an optimization proposal. It does not calculate, approve or file either return.</p></div>`;
    }
    const toggleControls=disabled=>{main.querySelectorAll('input,select,button').forEach(el=>el.disabled=disabled);if(!disabled){const b=main.querySelector('[data-save-pair]');if(b)b.disabled=!pairDirty||!canPair;}};
    const mf=main.querySelector('[data-family-members]');let membershipBusy=false,membershipDirty=false,pairDirty=false;
    mf.addEventListener('input',()=>{membershipDirty=true;setDirty(true);});mf.addEventListener('change',()=>{membershipDirty=true;setDirty(true);});
    mf.onsubmit=async e=>{e.preventDefault();if(!canWrite||membershipBusy||!current()||mf.isConnected===false)return;try{
      if(pairDirty)throw Error('Save or discard paired input changes before changing members.');
      const fd=new FormData(mf),body={...pairSelection(candidates,fd.get('primary'),fd.get('spouse'),scope),label:String(fd.get('label')).trim(),...(record?{expected_family_sha256:record.family_sha256}:{})};membershipBusy=true;toggleControls(true);
      const result=await api(BASE+(record?'/'+encodeURIComponent(id):''),{method:record?'PUT':'POST',body});if(!current())return;setDirty(false);location.hash='#family-workspace/'+encodeURIComponent(result.family.id);if(record)await route(id,main,{isCurrent,setDirty});
    }catch(e){if(current())main.querySelector('[data-members-error]').textContent=e.message;}finally{membershipBusy=false;if(current())toggleControls(false);}};
    const pf=main.querySelector('[data-pair]');if(!record||!pf)return;let pairBusy=false;
    pf.addEventListener('input',e=>{e.target.dataset.changed='true';pairDirty=true;setDirty(true);const b=pf.querySelector('[data-save-pair]');if(b)b.disabled=false;});pf.addEventListener('change',e=>{e.target.dataset.changed='true';pairDirty=true;setDirty(true);const b=pf.querySelector('[data-save-pair]');if(b)b.disabled=false;});
    pf.onsubmit=async e=>{e.preventDefault();if(pairBusy||!current()||pf.isConnected===false||!canPair)return;try{
      if(membershipDirty)throw Error('Save or discard family member changes before paired input entry.');
      if(!pairDirty)return;
      const body={expected_family_sha256:record.family_sha256,expected_primary_revision:record.primary.revision,expected_spouse_revision:record.spouse.revision,expected_client_ref:record.client_ref,primary_inputs:inputValues(pf,'primary',record.primary.inputs,fields),spouse_inputs:inputValues(pf,'spouse',record.spouse.inputs,fields)};
      pairBusy=true;toggleControls(true);await api(BASE+'/'+encodeURIComponent(id)+'/inputs',{method:'PUT',body});if(!current())return;setDirty(false);await route(id,main,{isCurrent,setDirty});
    }catch(e){if(current())main.querySelector('[data-pair-error]').textContent=e.message;}finally{pairBusy=false;if(current())toggleControls(false);}};
  }
  window.NXTFamilyWorkflow={route,pairSelection,inputValues,validateWorkspace,discard:()=>discardActive()};
})();
