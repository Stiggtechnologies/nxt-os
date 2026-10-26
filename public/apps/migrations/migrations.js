/* Original NXT read-only parallel review. Source formula text is never executed. */
(() => {
  'use strict';
  const {api,esc,toast}=NXT, API='/api/migrations', PAGE_SIZE=100;
  const $=selector=>document.querySelector(selector), main=$('#main'), enc=encodeURIComponent;
  const S={version:0,snapshots:[],detail:null,report:null,page:null,headers:[],search:'',downloadBusy:false};
  const list=value=>Array.isArray(value)?value:[];
  const stamp=value=>{const time=new Date(value);return value&&!Number.isNaN(time.getTime())?time.toLocaleString('en-CA'):'Not recorded';};
  const hash=value=>/^[a-f0-9]{64}$/i.test(String(value||''))?String(value):'Not recorded';
  const pretty=value=>String(value??'').replace(/_/g,' ');
  const sourceName=value=>({quickbooks:'QuickBooks',dtmax:'DT Max',webtax:'WebTax',caseware:'Caseware'})[String(value).toLowerCase()]||String(value||'Source system');
  const request=(path='')=>api(API+path);
  function routeInfo(){
    const [path,query='']=location.hash.replace(/^#\/?/,'').split('?'), parts=path.split('/');
    if(parts[0]!=='snapshot')return {view:'worklist'};
    const rowText=new URLSearchParams(query).get('row')||'1';
    if(!/^[1-9]\d{0,6}$/.test(rowText)||Number(rowText)>1048576)throw Error('Enter a worksheet row between 1 and 1,048,576.');
    return {view:'snapshot',id:decodeURIComponent(parts[1]||''),reportId:decodeURIComponent(parts[2]||''),row:Number(rowText)};
  }
  const viewLink=(id,reportId='',row=1)=>`#snapshot/${enc(id)}${reportId?'/'+enc(reportId):''}?row=${row}`;
  function openApp(app){const view=({home:'',firm:'projects',ledger:'dashboard',engagements:'worklist'})[app]||'';if(window.parent!==window)window.parent.postMessage({nxt:'open-app',app,view},location.origin);else location.href='/#/'+app+(view?'/'+view:'');}
  function banner(identity){return `<section class="banner" aria-label="Parallel test boundary"><strong>Parallel test · original systems remain the source of truth</strong><p>These are frozen source snapshots for checking and comparison. They do not update NXT Ledger, replace live books or authorize cutover. Use the original system for current transactions.</p><p class="help">${esc(identity||'Shared workspace key; reviewer labels are metadata, not verified identities.')} A named reviewer has not approved a snapshot merely because their name appears here.</p></section>`;}
  function assertBoundary(value){if(value?.shadow_only!==true||value.live_cutover_allowed!==false||value.source_authoritative!==true)throw Error('The source review boundary could not be confirmed. Reload before reviewing this snapshot.');}
  function assertRows(value,columns){
    if(!Array.isArray(value.rows))throw Error('The saved source rows are incomplete.');
    for(const row of value.rows){
      if(!Array.isArray(row)||row.length!==2||!Number.isSafeInteger(row[0])||row[0]<1||row[0]>1048576||!Array.isArray(row[1]))throw Error('An original worksheet row has an unsupported shape.');
      const seen=new Set();
      for(const cell of row[1]){
        if(!Array.isArray(cell)||cell.length!==5||typeof cell[0]!=='string'||!columns.includes(cell[0])||seen.has(cell[0])||!['n','s','inlineStr','str'].includes(cell[1])||!(cell[2]===null||typeof cell[2]==='string')||!(cell[3]===null||typeof cell[3]==='string')||!(cell[4]===null||typeof cell[4]==='string'))throw Error('A saved source cell has an unsupported representation.');
        seen.add(cell[0]);
      }
    }
  }
  function page(title,sub,body,action=''){main.innerHTML=`<div class="page"><div class="page-head"><div><div class="eyebrow">NXT · Parallel source review</div><h1>${esc(title)}</h1><p>${esc(sub)}</p></div>${action}</div>${body}</div>`;}
  function loading(){main.innerHTML='<div class="loading" role="status">Loading the saved source evidence…</div>';}
  function errorView(error){
    const unauthorized=error.status===401;
    page(unauthorized?'Sign in to review source evidence':'Source evidence could not be loaded','No empty or partial result is being presented as a completed review.',`<section class="card error-state" role="alert"><h2>${unauthorized?'Workspace sign-in required':'Review unavailable'}</h2><p>${esc(error.message||'The request failed. Try again.')}</p>${unauthorized&&window.parent===window?'<form class="signin-form" id="signin-form"><label>Workspace key<input class="signin-input" name="workspace_key" type="password" autocomplete="current-password" required></label><button class="btn primary" type="submit">Sign in</button></form>':'<div class="row"><button class="btn" data-action="refresh">Retry</button><button class="btn" data-os="home">NXT OS home</button></div>'}</section>`);
  }
  function fact(label,value){return `<div class="fact"><span>${esc(label)}</span><strong>${esc(value??'Not recorded')}</strong></div>`;}
  function worklistMarkup(){
    const query=S.search.toLowerCase().trim();
    const filtered=S.snapshots.filter(s=>`${s.company_name||''} ${s.cohort_ref||''} ${s.source_system||''}`.toLowerCase().includes(query));
    const host=$('#snapshot-list');if(!host)return;
    host.innerHTML=filtered.length?`<div class="snapshot-grid">${filtered.map(s=>`<article class="card snapshot-card"><span class="badge readonly">Read-only source snapshot</span><h2>${esc(s.company_name||s.cohort_ref||'Source snapshot')}</h2><p class="help">${esc(sourceName(s.source_system))} · ${esc(s.cohort_ref||'')}</p><div class="facts">${fact('Cutoff',s.cutoff_date)}${fact('Reports',s.report_count??'Not recorded')}${fact('Flagged exceptions',s.exception_count??'Not recorded')}${fact('Captured',stamp(s.captured_at))}${fact('Reviewer label',s.reviewer_label||'Unassigned')}${fact('Authority','Original system')}</div><a class="btn primary" href="${viewLink(s.id)}">Review saved reports</a></article>`).join('')}</div>`:`<section class="card empty"><h2>${S.snapshots.length?'No matching snapshots':'No published source snapshots'}</h2><p>${S.snapshots.length?'Change the company or source search.':'An authorized source snapshot must be published before it can be reviewed here. No source books have been loaded into Ledger by this screen.'}</p></section>`;
  }
  async function worklist(version){
    const data=await request();if(version!==S.version)return;assertBoundary(data);
    if(!Array.isArray(data.snapshots))throw Error('The snapshot list response is incomplete.');
    S.snapshots=data.snapshots;S.detail=null;S.report=null;S.page=null;
    $('#storage-label').textContent=data.storage==='supabase'?'Persistent storage · Supabase':'Shared workspace access';
    const exceptions=S.snapshots.reduce((total,s)=>total+(Number.isSafeInteger(s.exception_count)?s.exception_count:0),0);
    page('Parallel migration review','Inspect the preserved source reports, trace exceptions and download the original source package.',banner(data.identity_limit)+`<div class="stats"><div class="card"><span>Published snapshots</span><strong>${S.snapshots.length}</strong></div><div class="card"><span>Flagged exceptions</span><strong>${exceptions}</strong></div><div class="card"><span>Live cutover</span><strong>No-go</strong></div></div><label class="search">Find company or source<input type="search" id="snapshot-search" value="${esc(S.search)}" placeholder="Company, cohort or source system"></label><div id="snapshot-list"></div>`);
    worklistMarkup();
  }
  function formatCents(value){
    if(typeof value!=='string'||! /^-?\d{1,40}$/.test(value))return null;
    const cents=BigInt(value),absolute=cents<0n?-cents:cents;
    return `${cents<0n?'-':''}${(absolute/100n).toString().replace(/\B(?=(\d{3})+(?!\d))/g,',')}.${String(absolute%100n).padStart(2,'0')}`;
  }
  function sourceCell(cell,address){
    if(!cell)return '<span class="blank" title="No source cell value is present at this worksheet location.">Blank</span>';
    const [,type,raw,formula,cents]=cell;
    const rawText=raw===null||raw===undefined?'':String(raw), interpreted=formatCents(cents), hasFormula=typeof formula==='string';
    const detail=`<details class="cell-detail"><summary>Source ${esc(address)}</summary><dl><dt>Source type</dt><dd>${esc(type)}</dd><dt>${hasFormula?'Raw cached source value':'Raw source value'}</dt><dd>${rawText===''?'Blank':esc(rawText)}</dd>${hasFormula?`<dt>Source formula · text only</dt><dd>${esc(formula)}</dd>`:''}${interpreted!==null?`<dt>Restricted interpretation · exact cents</dt><dd>${esc(String(cents))}</dd>`:''}</dl></details>`;
    if(hasFormula)return `${interpreted!==null?`<div class="numeric">${esc(interpreted)}<span class="derived-label">Derived · restricted interpretation</span></div>`:'<span class="formula-unresolved">Formula · not interpreted</span>'}${detail}`;
    if(rawText==='')return `<span class="blank">Blank</span>${detail}`;
    return `${interpreted!==null?`<div class="numeric">${esc(interpreted)}<span class="derived-label">Interpreted amount</span></div>`:`<div class="cell-value${type==='n'?' numeric':''}">${esc(rawText)}</div>`}${detail}`;
  }
  function headerText(column){const row=list(S.headers).find(r=>r[0]===5),cell=row&&list(row[1]).find(c=>c[0]===column);return cell&&cell[3]===null?String(cell[2]??''):'';}
  function headerContext(){return list(S.headers).filter(r=>r[0]<=5).map(([row,cells])=>`Row ${row}: ${list(cells).filter(c=>c[2]!==''&&c[2]!==null&&c[3]===null).map(c=>`${c[0]}: ${String(c[2])}`).join(' | ')}`).join('\n');}
  function tableMarkup(rows,columns,exceptions,targetRow){
    const flagged=new Set(exceptions.map(e=>e.row));
    return `<div class="table-scroll" tabindex="0" aria-label="Source worksheet rows"><table class="source-table"><thead><tr><th class="row-number" scope="col">Row</th>${columns.map(c=>`<th scope="col">${esc(headerText(c)||c)}<small>Source column ${esc(c)}</small></th>`).join('')}</tr></thead><tbody>${rows.map(([row,cells])=>{const byColumn=new Map(list(cells).map(c=>[c[0],c]));return `<tr id="source-row-${row}" class="${flagged.has(row)?'flagged ':''}${row===targetRow?'target-row':''}"><td class="row-number" scope="row">${row}${flagged.has(row)?'<br><span class="badge warning">Flag</span>':''}</td>${columns.map(c=>`<td>${sourceCell(byColumn.get(c),c+row)}</td>`).join('')}</tr>`;}).join('')}</tbody></table></div>`;
  }
  function snapshotMarkup(route){
    const {manifest:m,identity_limit,published_at,manifest_sha256}=S.detail, report=S.report, data=S.page;
    const allExceptions=list(m.exceptions), exceptions=allExceptions.filter(e=>e.report_id===report.id), rows=list(data.rows), columns=list(report.columns);
    const lastRow=rows.at(-1)?.[0], firstRow=rows[0]?.[0];
    const exceptionMarkup=allExceptions.length?`<details class="card"${exceptions.length?' open':''}><summary><strong>Exception queue · ${allExceptions.length} flagged source locations</strong></summary><p class="help">Flags remain unresolved until supported by source evidence and independent review. A blank amount does not establish zero, void or cancellation.</p><div class="exception-list">${allExceptions.map(e=>`<article class="exception"><div><strong>${esc(list(m.reports).find(r=>r.id===e.report_id)?.title||e.report_id)} · worksheet row ${esc(e.row??'Not recorded')}</strong><p><code>${esc(e.code)}</code></p><p>${esc(e.note||'Review this location against the original source transaction and report controls.')}</p></div>${Number.isSafeInteger(e.row)&&e.row>0?`<a class="btn sm" href="${viewLink(m.id,e.report_id,e.row)}">Go to source row</a>`:''}</article>`).join('')}</div></details>`:'';
    const reportNav=list(m.reports).map(r=>`<a href="${viewLink(m.id,r.id)}"${r.id===report.id?' aria-current="page"':''}>${esc(r.title||r.id)}<small>${esc(r.row_count)} saved source rows</small></a>`).join('');
    page(m.company_name||m.cohort_ref,'Frozen source reports for parallel checking; original systems remain authoritative.',`<a class="back" href="#worklist">← All source snapshots</a>${banner(identity_limit)}<section class="card"><div class="facts">${fact('Source',sourceName(m.source_system))}${fact('Cutoff date',m.cutoff_date)}${fact('Captured',stamp(m.captured_at))}${fact('Reviewer label',m.reviewer_label||'Unassigned')}${fact('Review mode','Parallel test · read-only')}${fact('Published',stamp(published_at))}</div><details style="margin-top:15px"><summary>Integrity references and review limitations</summary><p class="integrity">Manifest SHA-256 ${esc(hash(manifest_sha256))}<br>Original source ZIP SHA-256 ${esc(hash(m.source_archive?.sha256))}<br>Selected workbook SHA-256 ${esc(hash(report.source_sha256))}</p><ul class="limitation-list">${list(m.limitations).map(l=>`<li>${esc(l)}</li>`).join('')}</ul><p class="help">Reviewer names are routing metadata; no sign-off is recorded here. Source labels and row locators are not durable transaction identities. The report snapshots do not establish complete company migration or filing readiness.</p></details></section>${exceptionMarkup}<div class="review-layout"><aside class="card report-side"><h2>Source reports</h2><nav class="report-nav" aria-label="Saved source reports">${reportNav}</nav><p class="help">Report values and formulas are preserved as source text. Missing cells stay blank.</p></aside><section class="card report-content"><div class="report-title"><div><h2>${esc(report.title||report.id)}</h2><p class="help">Worksheet: ${esc(report.sheet_name||'Not recorded')} · ${esc(report.row_count)} saved source rows · ${exceptions.length} flags</p></div><button class="btn sm" data-action="download"${S.downloadBusy?' disabled':''}>${S.downloadBusy?'Checking source ZIP…':'Download original ZIP'}</button></div><details class="context"><summary>Original report headings · source rows 1–5</summary><pre>${esc(headerContext()||'No heading cells were present in source rows 1–5.')}</pre></details><p class="help">Formula cells show a restricted derived value when available. Open “Source” beneath a cell to inspect the original formula and cached value. Derived values are separate from the raw source; no general Excel calculation runs here.</p>${rows.length?tableMarkup(rows,columns,exceptions,route.row):'<div class="empty"><h2>No saved rows at or after this location</h2><p>The requested worksheet location is beyond the available rows. Return to the first row or enter another worksheet row.</p></div>'}<div class="pager"><div><p class="help">${rows.length?`Source rows ${firstRow}–${lastRow} · ${rows.length} displayed`:'No source rows displayed'} · page limit ${PAGE_SIZE}</p><div class="row" style="margin-top:8px"><a class="btn sm" href="${viewLink(m.id,report.id,1)}">First row</a>${route.row>1?`<a class="btn sm" href="${viewLink(m.id,report.id,Math.max(1,route.row-PAGE_SIZE))}">Earlier rows</a>`:''}${data.has_more&&Number.isSafeInteger(data.next_from_row)?`<a class="btn sm" href="${viewLink(m.id,report.id,data.next_from_row)}">Next source rows →</a>`:'<span class="help">End of saved rows</span>'}</div></div><form class="jump" id="jump-form"><label for="jump-row">Worksheet row</label><input id="jump-row" name="row" inputmode="numeric" pattern="[1-9][0-9]{0,6}" value="${route.row}" aria-label="Jump to original worksheet row" required><button class="btn sm" type="submit">Go</button></form></div></section></div>`,`<span class="badge readonly">Read-only · no live posting</span>`);
    const focus=document.getElementById('source-row-'+route.row);if(focus&&route.row>5)focus.scrollIntoView({block:'nearest',inline:'nearest'});
  }
  async function snapshot(route,version){
    if(!route.id)throw Error('A saved source snapshot must be selected.');
    const detail=await request('/'+enc(route.id));if(version!==S.version)return;assertBoundary(detail);
    const manifest=detail.manifest;if(manifest?.format!=='nxt-shadow-snapshot/v1'||manifest.id!==route.id||manifest.source_of_truth!=='original_system'||manifest.mode!=='parallel_test'||!list(manifest.reports).length)throw Error('The saved snapshot manifest is incomplete or unsupported.');
    const report=route.reportId?list(manifest.reports).find(r=>r.id===route.reportId):manifest.reports[0];if(!report)throw Error('That source report is not present in this saved snapshot.');
    if(!Array.isArray(report.columns)||!report.columns.length||!report.columns.every(c=>typeof c==='string'&&/^[A-Z]{1,3}$/.test(c))||new Set(report.columns).size!==report.columns.length)throw Error('The saved source column inventory is incomplete.');
    const path='/'+enc(route.id)+'/reports/'+enc(report.id)+'/rows';
    const [rows,headings]=await Promise.all([request(path+'?from_row='+route.row+'&limit='+PAGE_SIZE),request(path+'?from_row=1&limit=5')]);if(version!==S.version)return;assertBoundary(rows);assertBoundary(headings);
    if(rows.report_id!==report.id||rows.snapshot_id!==route.id||headings.report_id!==report.id||headings.snapshot_id!==route.id)throw Error('The source row response does not match the selected report.');
    assertRows(rows,report.columns);assertRows(headings,report.columns);
    S.detail=detail;S.report=report;S.page=rows;S.headers=headings.rows;snapshotMarkup(route);
  }
  async function downloadArchive(){
    if(S.downloadBusy||!S.detail)return;S.downloadBusy=true;const detail=S.detail,version=S.version,m=detail.manifest;
    const button=$('[data-action="download"]');if(button){button.disabled=true;button.textContent='Checking source ZIP…';}
    try{
      const response=await fetch(API+'/'+enc(m.id)+'/archive',{headers:{authorization:'Bearer '+NXT.key.get()},cache:'no-store'});
      if(!response.ok){const body=await response.json().catch(()=>({}));const error=new Error(body.error||'Source ZIP download failed ('+response.status+').');error.status=response.status;if(response.status===401&&window.parent!==window)window.parent.postMessage({nxt:'signed-out'},location.origin);throw error;}
      const expected=m.source_archive;if(!Number.isSafeInteger(expected?.bytes)||expected.bytes<1||expected.bytes>64*1024*1024)throw Error('The original ZIP size cannot be verified.');
      const bytes=await response.arrayBuffer();if(bytes.byteLength!==expected.bytes)throw Error('The source ZIP byte count does not match the saved manifest.');
      const actual=Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),n=>n.toString(16).padStart(2,'0')).join('');if(actual!==String(expected.sha256).toLowerCase())throw Error('The source ZIP checksum does not match the saved manifest.');
      const blobUrl=URL.createObjectURL(new Blob([bytes],{type:'application/zip'})),link=document.createElement('a');link.href=blobUrl;link.download=String(expected.file_name||'nxt-source-snapshot.zip').replace(/[\\/\x00-\x1f\x7f]/g,'_').slice(0,180);document.body.append(link);link.click();link.remove();setTimeout(()=>URL.revokeObjectURL(blobUrl),30000);toast('Original source ZIP checked against its saved size and SHA-256.');
    }catch(error){if(error.status===401&&version===S.version)errorView(error);else toast(error.message||'The original source ZIP could not be downloaded.',true);}
    finally{S.downloadBusy=false;if(version===S.version){const current=$('[data-action="download"]');if(current){current.disabled=false;current.textContent='Download original ZIP';}}}
  }
  async function route(){const version=++S.version;loading();try{const info=routeInfo();if(info.view==='snapshot')await snapshot(info,version);else await worklist(version);if(version===S.version&&window.parent!==window)window.parent.postMessage({nxt:'view',view:location.hash.replace(/^#/,'')||'worklist'},location.origin);}catch(error){if(version===S.version)errorView(error);}}
  document.addEventListener('click',event=>{if(event.target.closest('.skip-link')){event.preventDefault();main.focus();return;}const control=event.target.closest('[data-action],[data-os]');if(!control)return;if(control.dataset.os)return openApp(control.dataset.os);if(control.dataset.action==='refresh')route();if(control.dataset.action==='download')downloadArchive();});
  document.addEventListener('input',event=>{if(event.target.id==='snapshot-search'){S.search=event.target.value;worklistMarkup();}});
  document.addEventListener('submit',event=>{if(event.target.id==='jump-form'){event.preventDefault();const row=String(new FormData(event.target).get('row')||'');if(!/^[1-9]\d{0,6}$/.test(row)||Number(row)>1048576)return toast('Enter a worksheet row between 1 and 1,048,576.',true);location.hash=viewLink(S.detail.manifest.id,S.report.id,Number(row));}if(event.target.id==='signin-form'){event.preventDefault();const key=String(new FormData(event.target).get('workspace_key')||'').trim();if(key){NXT.key.set(key);event.target.reset();route();}}});
  window.addEventListener('hashchange',route);route();
})();
