import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import {fileURLToPath} from 'node:url';
const {chromium}=await import(process.env.NXT_PLAYWRIGHT_MODULE||'playwright');
const root=fileURLToPath(new URL('../public/',import.meta.url)),origin='https://nxt-pdf.invalid';
const browser=await chromium.launch({headless:true,executablePath:process.env.NXT_CHROME_EXECUTABLE||undefined,chromiumSandbox:true});
try{
 const context=await browser.newContext({serviceWorkers:'block'}),requests=[],errors=[];
 await context.route('**/*',async route=>{
  const u=new URL(route.request().url());requests.push(u.pathname);
  if(u.origin!==origin)return route.abort();
  if(u.pathname==='/')return route.fulfill({contentType:'text/html',body:'<!doctype html><title>PDF test workspace</title><h1>Documents</h1><p role="status">Draft saved successfully</p><label>Mail password<input type="password" value="private-password"></label><script src="/nxt/workspace-pdf.js"></script><script src="/nxt/workspace-control.js"></script>'});
  const file=path.resolve(root,'.'+u.pathname);assert.ok(file.startsWith(root));
  if(!fs.existsSync(file))return route.fulfill({status:404,body:''});
  return route.fulfill({contentType:/\.m?js$/.test(file)?'text/javascript':file.endsWith('.wasm')?'application/wasm':'application/octet-stream',body:fs.readFileSync(file)});
 });
 const page=await context.newPage();page.on('pageerror',e=>errors.push(e.message));
 await page.goto(origin);
 await page.evaluate(()=>{
  window.signedIn=true;window.controller=NXTWorkspace.create({allowed:()=>signedIn,onSignature:pending=>{window.stagedPDF=pending;return {ok:true,say:'Draft preparation opened; no email sent.'};}});
  window.makePDF=(scan=false)=>{
   let image='';
   if(scan){const canvas=document.createElement('canvas');canvas.width=600;canvas.height=800;const ctx=canvas.getContext('2d');ctx.fillStyle='white';ctx.fillRect(0,0,600,800);ctx.fillStyle='black';ctx.font='28px sans-serif';ctx.fillText('Scanned total: 123.45',45,80);image=atob(canvas.toDataURL('image/jpeg',.8).split(',')[1]);}
   const content=scan?'q 600 0 0 800 0 0 cm /Im0 Do Q':'BT /F1 18 Tf 45 700 Td (Synthetic annual PDF statement income 123.45) Tj ET';
   const objects=['<< /Type /Catalog /Pages 2 0 R >>','<< /Type /Pages /Kids [3 0 R] /Count 1 >>',`<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources ${scan?'<< /XObject << /Im0 5 0 R >> >>':'<< /Font << /F1 5 0 R >> >>'} /Contents 4 0 R >>`,`<< /Length ${content.length} >>\nstream\n${content}\nendstream`,scan?`<< /Type /XObject /Subtype /Image /Width 600 /Height 800 /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode /Length ${image.length} >>\nstream\n${image}\nendstream`:'<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>'];
   let result='%PDF-1.4\n',offsets=[0];for(let i=0;i<objects.length;i++){offsets.push(result.length);result+=`${i+1} 0 obj\n${objects[i]}\nendobj\n`;}
   const xref=result.length;result+=`xref\n0 6\n0000000000 65535 f \n`+offsets.slice(1).map(n=>String(n).padStart(10,'0')+' 00000 n \n').join('')+`trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF`;
   return new Uint8Array([...result].map(c=>c.charCodeAt(0)));
  };
  window.openPDF=scan=>{document.querySelectorAll('iframe,a').forEach(e=>e.remove());const url=URL.createObjectURL(new Blob([makePDF(scan)],{type:'application/pdf'})),link=document.createElement('a');link.href=url;link.download='synthetic.pdf';link.textContent='Download PDF';document.body.append(link);const frame=document.createElement('iframe');frame.src=url+'#page=1';frame.title=scan?'Scanned synthetic return':'Digital synthetic return';frame.style='width:600px;height:400px';document.body.append(frame);return url;};
 });
 assert.equal(requests.some(p=>p.includes('/vendor/pdfjs/')),false,'reader must be lazy');
 const text=await page.evaluate(()=>controller.read());assert.match(JSON.stringify(text),/Draft saved successfully/);assert.doesNotMatch(JSON.stringify(text),/private-password/);
 await page.evaluate(()=>openPDF(false));
 let listing=await page.evaluate(()=>controller.inspect());assert.equal(listing.documents.length,1);assert.match(listing.documents[0].title,/Digital/);
 const id=listing.documents[0].id;
 let digital=await page.evaluate(id=>controller.readPdf({document_id:id}),id);
 assert.equal(digital.ok,true,JSON.stringify(digital));assert.match(digital.pages[0].text,/income 123.45/);assert.equal(digital._images.length,0);
 const staged=await page.evaluate(id=>controller.act({action:'prepare_signature',target:id}),id);assert.equal(staged.ok,true);assert.deepEqual(await page.evaluate(()=>({type:stagedPDF.blob.type,pages:stagedPDF.pages})),{type:'application/pdf',pages:1});
 const invalid=await page.evaluate(()=>controller.readPdf({page:2}));assert.equal(invalid.ok,false);
 listing=await page.evaluate(()=>controller.inspect());const download=listing.controls.find(c=>c.label==='Download PDF');
 const done=page.waitForEvent('download');done.catch(()=>{});const clicked=await page.evaluate(id=>controller.act({action:'click',target:id}),download.id);assert.equal(clicked.ok,true,JSON.stringify(clicked));await(await done).cancel();
 const visual=await page.evaluate(()=>controller.readPdf({mode:'image'}));assert.equal(visual.ok,true,JSON.stringify(visual));assert.ok(visual._images.length>0);assert.ok(visual._images.every(i=>i.image_url.length<48000));
 await page.evaluate(()=>openPDF(true));
 assert.equal((await page.evaluate(id=>controller.readPdf({document_id:id}),id)).ok,false,'old document handle must expire');
 const scan=await page.evaluate(()=>controller.readPdf());assert.equal(scan.ok,true,JSON.stringify(scan));assert.equal(scan.pages[0].has_text,false);assert.ok(scan._images.length>0,'scans need actual vision inputs');
 await page.evaluate(()=>{signedIn=false;});assert.equal((await page.evaluate(()=>controller.readPdf())).ok,false);
 assert.deepEqual(errors,[]);
 console.log(JSON.stringify({passed:true,digital_text:true,scanned_page_images:true,images_bounded:true,pdf_download:true,stale_document_rejected:true,signed_out_rejected:true,lazy_library:true}));
}finally{await browser.close();}
