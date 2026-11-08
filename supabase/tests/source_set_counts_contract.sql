-- Synthetic local contract, always rolled back. No real source records.
begin;
do $contract$
declare
    ns text := 'source-count-contract';
    k1 text := 'nxt:dtmaxtc:counter-test:family_files';
    k2 text := 'nxt:dtmaxtc:counter-test:afr_contents';
    k3 text := 'nxt:dtmaxtc:counter-test:historical_lines';
    bad_keys jsonb;
begin
    perform public.nxt_store_command(ns, pg_catalog.jsonb_build_array('SADD',k1,'synthetic-a'));
    perform public.nxt_store_command(ns, pg_catalog.jsonb_build_array('SADD',k1,'synthetic-b'));
    if public.nxt_source_set_counts(ns,pg_catalog.jsonb_build_array(k1,k2,k1)) <> '{"result":[2,0,2]}'::jsonb then
        raise exception 'source counts lost missing sets, repeated keys or order';
    end if;
    if public.nxt_source_set_counts(ns || '-isolated',pg_catalog.jsonb_build_array(k1)) <> '{"result":[0]}'::jsonb then
        raise exception 'source count namespace leaked';
    end if;
    if public.nxt_store_command(ns,pg_catalog.jsonb_build_array('SMEMBERS',k1)) <> '{"result":["synthetic-a","synthetic-b"]}'::jsonb then
        raise exception 'source members changed while counting';
    end if;
    for bad_keys in select value from pg_catalog.jsonb_array_elements(
        pg_catalog.jsonb_build_array('[]'::jsonb,'{}'::jsonb,'[null]'::jsonb,
            '["nxt:company:ledger"]'::jsonb,'["nxt:dtmaxtc:counter-test:return_pdf"]'::jsonb,
            '["nxt:dtmaxtc: counter-test:family_files"]'::jsonb)) loop
        begin
            perform public.nxt_source_set_counts(ns,bad_keys);
            raise exception 'invalid source counter request accepted';
        exception when sqlstate '22023' then null;
        end;
    end loop;
    begin
        perform public.nxt_source_set_counts('bad namespace',pg_catalog.jsonb_build_array(k1));
        raise exception 'invalid namespace accepted';
    exception when sqlstate '22023' then null;
    end;
    begin
        perform public.nxt_source_set_counts(ns,(select pg_catalog.jsonb_agg(k1)from generate_series(1,17)));
        raise exception 'unbounded source count accepted';
    exception when sqlstate '22023' then null;
    end;
    perform public.nxt_store_command(ns,pg_catalog.jsonb_build_array('SET',k3,'synthetic-string'));
    begin
        perform public.nxt_source_set_counts(ns,pg_catalog.jsonb_build_array(k3));
        raise exception 'wrong-type source index accepted as empty';
    exception when sqlstate '22023' then null;
    end;
    if pg_catalog.has_function_privilege('anon','public.nxt_source_set_counts(text,jsonb)','EXECUTE')
       or pg_catalog.has_function_privilege('authenticated','public.nxt_source_set_counts(text,jsonb)','EXECUTE')
       or not pg_catalog.has_function_privilege('service_role','public.nxt_source_set_counts(text,jsonb)','EXECUTE') then
        raise exception 'source counter permissions differ from server-only boundary';
    end if;
    raise notice 'source count contract passed: exact cardinalities, isolation, type/bound checks and server-only execution';
end;
$contract$;
rollback;
