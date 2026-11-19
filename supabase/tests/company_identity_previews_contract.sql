-- Synthetic and rolled back, including access-audit rows.
begin;
do $contract$
declare ns text:='identity-preview-contract'; body text; k text; missing text; result jsonb; bad jsonb;
begin
  body := '{"format":"nxt-company-book-preview/v1","company_id":"synthetic-identity","input":{"company":{"name":"Synthetic Company","source_company_id":"synthetic-source","currency":"CAD","identity_verified":true,"currency_verified":true},"private_journal":"PRIVATE_JOURNAL_MUST_NOT_LEAVE_DATABASE"},"analysis":{"controls_passed":true}}';
  k := 'nxt:company-books:synthetic-identity:preview:'||pg_catalog.encode(pg_catalog.sha256(pg_catalog.convert_to(body,'UTF8')),'hex');
  missing := 'nxt:company-books:synthetic-missing:preview:'||repeat('a',64);
  perform public.nxt_store_command(ns,pg_catalog.jsonb_build_array('SET',k,body));
  result:=public.nxt_company_identity_previews(ns,pg_catalog.jsonb_build_array(k,missing,k));
  if pg_catalog.jsonb_array_length(result->'result')<>3 or result#>'{result,1}'<>'null'::jsonb
     or result#>'{result,0}'<>result#>'{result,2}' then raise exception 'identity order/missing/duplicates differ'; end if;
  if result#>>'{result,0,sha256}'<>pg_catalog.encode(pg_catalog.sha256(pg_catalog.convert_to(body,'UTF8')),'hex')
     or result#>'{result,0,company}'<>body::jsonb#>'{input,company}'
     or result#>>'{result,0,company_id}'<>'synthetic-identity' then raise exception 'identity projection or sealed byte hash differs'; end if;
  if result::text like '%PRIVATE_JOURNAL_MUST_NOT_LEAVE_DATABASE%' then raise exception 'journal body leaked into identity projection'; end if;
  if public.nxt_company_identity_previews(ns||'-other',pg_catalog.jsonb_build_array(k))<>'{"result":[null]}'::jsonb then raise exception 'identity namespace leaked'; end if;
  if public.nxt_store_command(ns,pg_catalog.jsonb_build_array('GET',k))->>'result'<>body then raise exception 'identity read changed source'; end if;
  if not exists(select 1 from public.company_access_audit where namespace=ns and company_id='synthetic-identity' and action='MGET') then raise exception 'identity read lost access audit'; end if;
  perform public.nxt_store_command(ns,pg_catalog.jsonb_build_array('SET',k,replace(body,'Synthetic Company','Synthetic Changed')));
  result:=public.nxt_company_identity_previews(ns,pg_catalog.jsonb_build_array(k));
  if result#>>'{result,0,company,name}'<>'Synthetic Changed' or result#>>'{result,0,sha256}'=split_part(k,':',5) then raise exception 'identity trigger retained stale bytes';end if;
  perform public.nxt_store_command(ns,pg_catalog.jsonb_build_array('SET',k,'corrupt JSON'));
  result:=public.nxt_company_identity_previews(ns,pg_catalog.jsonb_build_array(k));
  if result#>'{result,0,format}'<>'null'::jsonb then raise exception 'corrupt source retained valid metadata';end if;
  perform public.nxt_store_command(ns,pg_catalog.jsonb_build_array('SET',k,body));
  for bad in select value from pg_catalog.jsonb_array_elements(pg_catalog.jsonb_build_array('[]'::jsonb,'{}'::jsonb,'[null]'::jsonb,'["nxt:firm:state:v1"]'::jsonb,pg_catalog.jsonb_build_array(replace(k,':preview:',':receipt:')))) loop
    begin perform public.nxt_company_identity_previews(ns,bad);raise exception 'invalid identity key accepted';exception when sqlstate '22023' then null;end;
  end loop;
  begin perform public.nxt_company_identity_previews('bad namespace',pg_catalog.jsonb_build_array(k));raise exception 'invalid namespace accepted';exception when sqlstate '22023' then null;end;
  begin perform public.nxt_company_identity_previews(ns,(select pg_catalog.jsonb_agg(k)from generate_series(1,101)));raise exception 'unbounded identity projection accepted';exception when sqlstate '22023' then null;end;
  if pg_catalog.has_function_privilege('anon','public.nxt_company_identity_previews(text,jsonb)','EXECUTE')
     or pg_catalog.has_function_privilege('authenticated','public.nxt_company_identity_previews(text,jsonb)','EXECUTE')
     or not pg_catalog.has_function_privilege('service_role','public.nxt_company_identity_previews(text,jsonb)','EXECUTE') then raise exception 'identity RPC must remain server-only';end if;
  if pg_catalog.has_table_privilege('service_role','nxt_private.company_preview_identities','SELECT')
     or pg_catalog.has_table_privilege('anon','nxt_private.company_preview_identities','SELECT')
     or pg_catalog.has_table_privilege('authenticated','nxt_private.company_preview_identities','SELECT') then raise exception 'identity table exposed outside its RPC';end if;
end;
$contract$;
rollback;
