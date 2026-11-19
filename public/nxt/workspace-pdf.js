// Read only PDFs currently displayed or linked in the signed-in NXT workspace.
// Mozilla PDF.js is loaded on demand; original files remain unchanged.
(function(root){
  "use strict";
  const clean=v=>String(v||"").replace(/\s+/g," ").trim();
  const MAX_BYTES=64*1024*1024, BASE="/vendor/pdfjs/";
  let library;
  function safeUrl(value){
    try { const u=new URL(value,root.location.href); return u.origin===root.location.origin && ["https:","http:","blob:"].includes(u.protocol) ? u : null; } catch { return null; }
  }
  async function pdfjs(){
    if(!library) library=import(BASE+"build/pdf.mjs").then(m=>{m.GlobalWorkerOptions.workerSrc=BASE+"build/pdf.worker.mjs";return m;}).catch(e=>{library=null;throw e;});
    return library;
  }
  function create({allowed,documents,visible,signature}){
    let handles=new Map(),serial=0,cache=null;
    function list(){
      if(!allowed()) { reset(); return []; }
      const current=new Map(), rows=[];
      for(const entry of documents()) for(const el of entry.doc.querySelectorAll('iframe[src],embed[src],object[data],a[href]')){
        if(!visible(el))continue;
        const raw=el.getAttribute(el.tagName==="OBJECT"?"data":el.tagName==="A"?"href":"src"),url=safeUrl(raw);
        if(!url)continue;
        const link=el.tagName==="A";
        if(link && !(/\.pdf(?:$|[?#])/i.test(el.download||url.pathname)||el.dataset.nxtPdf==="true"))continue;
        if(!link && !(el.type==="application/pdf"||url.protocol==="blob:"||/\.pdf$/i.test(url.pathname)||el.dataset.nxtPdf==="true"))continue;
        url.hash="";
        const key=entry.doc.URL+"|"+url.href;
        if(current.has(key)){
          if(!link){const record=current.get(key),row=rows.find(r=>r.id===record.id);record.el=el;record.current_page=Number(new URLSearchParams(raw.split('#')[1]||'').get('page'))||1;record.title=clean(el.title||record.title).slice(0,200);Object.assign(row,{current_page:record.current_page,title:record.title});}
          continue;
        }
        const previous=handles.get(key),id=previous?.id||"pdf-"+(++serial);
        const page=Number(new URLSearchParams((raw.split("#")[1]||"")).get("page"))||1;
        const record={id,key,url:url.href,el,owner:entry.doc,scope:entry.scope,title:clean(el.title||el.getAttribute("aria-label")||el.download||el.textContent||"Current PDF").slice(0,200),current_page:page};
        current.set(key,record);rows.push({id,title:record.title,scope:record.scope,current_page:page});
      }
      handles=current;
      if(cache && ![...current.values()].some(r=>r.id===cache.id)){cache.task?.destroy();cache=null;}
      return rows;
    }
    function reset(){handles.clear();cache?.task?.destroy();cache=null;}
    function selected(id){list();return [...handles.values()].find(r=>r.id===id);}
    function fresh(record){const current=selected(record.id);return allowed()&&current?.url===record.url&&current.owner===record.owner&&visible(current.el);}
    async function load(record){
      if(cache?.id===record.id)return cache.promise;
      cache?.task?.destroy();
      const slot={id:record.id,task:null};cache=slot;
      slot.promise=(async()=>{
        const headers=record.url.startsWith("blob:")?{}:{authorization:"Bearer "+(root.NXT?.key?.get()||"")};
        const response=await root.fetch(record.url,{headers,credentials:"same-origin",redirect:"error"});
        if(!response.ok)throw Error("The PDF could not be opened ("+response.status+").");
        if(!/application\/pdf/i.test(response.headers.get("content-type")||""))throw Error("The selected link did not return a PDF.");
        if(Number(response.headers.get("content-length"))>MAX_BYTES)throw Error("This PDF exceeds the 64 MiB reader limit.");
        const reader=response.body.getReader(),parts=[];let length=0;
        try { while(true){const {done,value}=await reader.read();if(done)break;length+=value.length;if(length>MAX_BYTES)throw Error("This PDF exceeds the 64 MiB reader limit.");parts.push(value);} } finally { await reader.cancel().catch(()=>{}); }
        const bytes=new Uint8Array(length);let at=0;for(const p of parts){bytes.set(p,at);at+=p.length;}
        slot.blob=new Blob([bytes],{type:'application/pdf'});
        if(!fresh(record))throw Error("The PDF or workspace changed. Inspect again.");
        const m=await pdfjs();
        slot.task=m.getDocument({data:bytes,isEvalSupported:false,enableXfa:false,cMapUrl:BASE+"cmaps/",cMapPacked:true,standardFontDataUrl:BASE+"standard_fonts/",wasmUrl:BASE+"wasm/"});
        const document=await slot.task.promise;
        if(document.numPages>10000){slot.task.destroy();throw Error("This PDF has too many pages.");}
        return document;
      })().catch(e=>{if(cache===slot)cache=null;slot.task?.destroy();throw e;});
      return slot.promise;
    }
    async function tiles(page,maximum=48000){
      const native=page.getViewport({scale:1}), scale=Math.min(2,1500/Math.max(native.width,native.height));
      const viewport=page.getViewport({scale}),canvas=root.document.createElement("canvas");
      canvas.width=Math.ceil(viewport.width);canvas.height=Math.ceil(viewport.height);
      await page.render({canvasContext:canvas.getContext("2d"),viewport,background:"white"}).promise;
      const images=[];
      for(let y=0;y<canvas.height;y+=750)for(let x=0;x<canvas.width;x+=750){
        const tile=root.document.createElement("canvas");tile.width=Math.min(750,canvas.width-x);tile.height=Math.min(750,canvas.height-y);
        const ctx=tile.getContext("2d");ctx.drawImage(canvas,x,y,tile.width,tile.height,0,0,tile.width,tile.height);
        let quality=.78,url;
        do {url=tile.toDataURL("image/jpeg",quality);quality-=.12;}while(url.length>maximum&&quality>.18);
        let ratio=.8;
        while(url.length>maximum&&ratio>.2){const small=root.document.createElement("canvas");small.width=Math.ceil(tile.width*ratio);small.height=Math.ceil(tile.height*ratio);small.getContext("2d").drawImage(tile,0,0,small.width,small.height);url=small.toDataURL("image/jpeg",.5);ratio-=.15;}
        if(url.length>maximum)throw Error("The PDF page image exceeds the voice message limit.");
        images.push({image_url:url,x,y,width:tile.width,height:tile.height,page_width:canvas.width,page_height:canvas.height});
        tile.width=tile.height=0;
      }
      canvas.width=canvas.height=0;return images;
    }
    async function read(args={}){
      if(!allowed())return {ok:false,error:"Sign in with workspace access first."};
      const rows=list(),record=selected(args.document_id || (rows.length===1?rows[0].id:""));
      if(!record)return {ok:false,error:rows.length?"Choose a current PDF id from this list.":"Open the PDF in NXT first, then inspect again.",documents:rows};
      try{
        const document=await load(record),start=args.page===undefined?record.current_page:Number(args.page),count=args.count===undefined?1:Number(args.count);
        if(!Number.isInteger(start)||start<1||start>document.numPages||!Number.isInteger(count)||count<1||count>2||!['text','image',undefined].includes(args.mode))throw Error("Choose one or two valid pages and text or image mode.");
        const pages=[],images=[];
        for(let number=start;number<=Math.min(document.numPages,start+count-1);number++){
          if(!fresh(record))throw Error("The PDF or workspace changed. Inspect again.");
          const page=await document.getPage(number),content=await page.getTextContent();
          const text=content.items.map(item=>item.str+(item.hasEOL?"\n":" ")).join("").slice(0,18000);
          const imageMode=args.mode==='image'||text.trim().length<30;
          const maximum=Math.max(3000,Math.min(48000,Number(args._image_max)||48000));
          const pageImages=imageMode?await tiles(page,maximum):[];
          pages.push({page:number,text,has_text:text.trim().length>0,text_truncated:content.items.reduce((n,i)=>n+(i.str||'').length,0)>18000,image_tiles:pageImages.length});
          images.push(...pageImages.map((tile,index)=>({...tile,page:number,tile:index+1})));
          page.cleanup();
        }
        if(!fresh(record))throw Error("The PDF or workspace changed. Inspect again.");
        return {ok:true,document_id:record.id,title:record.title,total_pages:document.numPages,pages,next_page:start+count<=document.numPages?start+count:null,_images:images,say:"Read "+pages.length+" PDF page(s)."};
      }catch(e){return {ok:false,error:e.name==="PasswordException"?"This PDF is password protected. Ask its owner to unlock it; Nixon cannot bypass encryption.":clean(e.message||"PDF reading failed.").slice(0,300)};}
    }
    function permitsBlob(el){return el.tagName==='A' && safeUrl(el.href)?.protocol==='blob:' && [...handles.values()].some(r=>r.url===el.href.split('#')[0]);}
    async function stage(id){
      const rows=list(),record=selected(id||(rows.length===1?rows[0].id:''));
      if(!record)return {ok:false,error:'Choose an open PDF from inspection first.',documents:rows};
      try{const document=await load(record);if(!fresh(record))throw Error('The document changed. Inspect again.');if(cache.blob.size>3*1024*1024||document.numPages>20)throw Error('Signature requests accept PDFs up to 3 MiB and 20 pages. This document exceeds that limit.');return signature?signature({blob:cache.blob,title:record.title,pages:document.numPages}):{ok:false,error:'Signature preparation is unavailable.'};}catch(e){return {ok:false,error:clean(e.message||'Unable to prepare this PDF.').slice(0,300)};}
    }
    return {list,read,stage,reset,permitsBlob};
  }
  root.NXTWorkspacePDF={create,safeUrl};
})(typeof window!=="undefined"?window:globalThis);
