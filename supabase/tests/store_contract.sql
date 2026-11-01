-- Run after the migration with an administrative database connection:
-- psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f supabase/tests/store_contract.sql
-- Every synthetic write is rolled back, including on assertion failure.
begin;

do $test$
declare
    ns text := 'nxt_test_' || pg_catalog.md5(pg_catalog.clock_timestamp()::text || pg_catalog.random()::text);
    v jsonb;
begin
    if public.nxt_store_command(ns, '["GET","missing"]') <> '{"result":null}'::jsonb then raise exception 'missing GET'; end if;
    if public.nxt_store_command(ns, '["COMPARE_EXCHANGE","cas",null,""]') <> '{"result":1}'::jsonb then raise exception 'CAS missing -> empty'; end if;
    if public.nxt_store_command(ns, '["COMPARE_EXCHANGE","cas",null,"wrong"]') <> '{"result":0}'::jsonb then raise exception 'CAS must distinguish missing from empty'; end if;
    if public.nxt_store_command(ns, '["COMPARE_EXCHANGE","cas","","approved"]') <> '{"result":1}'::jsonb then raise exception 'CAS empty -> approved'; end if;
    if public.nxt_store_command(ns, '["COMPARE_EXCHANGE","cas","","rejected"]') <> '{"result":0}'::jsonb then raise exception 'CAS stale must not overwrite'; end if;
    if public.nxt_store_command(ns, '["GET","cas"]') <> '{"result":"approved"}'::jsonb then raise exception 'CAS final value'; end if;
    if public.nxt_store_command(ns || '_other', '["GET","cas"]') <> '{"result":null}'::jsonb then raise exception 'namespace isolation'; end if;

    if public.nxt_store_command(ns, '["SET","value","","NX"]') <> '{"result":"OK"}'::jsonb then raise exception 'NX new key'; end if;
    if public.nxt_store_command(ns, '["SET","value","changed","NX"]') <> '{"result":null}'::jsonb then raise exception 'NX existing empty string'; end if;
    if public.nxt_store_command(ns, '["SET","value","hello"]') <> '{"result":"OK"}'::jsonb then raise exception 'SET'; end if;
    if public.nxt_store_command(ns, '["MGET","value","missing","value","cas"]') <> '{"result":["hello",null,"hello","approved"]}'::jsonb then raise exception 'MGET ordering/null/duplicates'; end if;

    if public.nxt_store_command(ns, '["INCRBY","count",5]') <> '{"result":5}'::jsonb then raise exception 'INCRBY create'; end if;
    if public.nxt_store_command(ns, '["INCRBY","count","-2"]') <> '{"result":3}'::jsonb then raise exception 'INCRBY signed argument'; end if;
    perform public.nxt_store_command(ns, '["SET","max","9223372036854775807"]');
    begin
        perform public.nxt_store_command(ns, '["INCRBY","max",1]');
        raise exception 'INCRBY overflow unexpectedly succeeded';
    exception when numeric_value_out_of_range then null;
    end;
    if public.nxt_store_command(ns, '["GET","max"]') <> '{"result":"9223372036854775807"}'::jsonb then raise exception 'overflow mutated value'; end if;

    if public.nxt_store_command(ns, '["SADD","members","b"]') <> '{"result":1}'::jsonb then raise exception 'SADD new'; end if;
    if public.nxt_store_command(ns, '["SADD","members","b"]') <> '{"result":0}'::jsonb then raise exception 'SADD duplicate'; end if;
    perform public.nxt_store_command(ns, '["SADD","members","a"]');
    if public.nxt_store_command(ns, '["SMEMBERS","members"]') <> '{"result":["a","b"]}'::jsonb then raise exception 'set contents'; end if;
    if public.nxt_store_command(ns, '["MGET","members","missing"]') <> '{"result":[null,null]}'::jsonb then raise exception 'MGET non-string'; end if;
    begin
        perform public.nxt_store_command(ns, '["GET","members"]');
        raise exception 'WRONGTYPE GET unexpectedly succeeded';
    exception when invalid_parameter_value then null;
    end;

    if public.nxt_store_command(ns, '["LPUSH","events","one"]') <> '{"result":1}'::jsonb then raise exception 'LPUSH create'; end if;
    perform public.nxt_store_command(ns, '["LPUSH","events","two"]');
    perform public.nxt_store_command(ns, '["LPUSH","events","three"]');
    if public.nxt_store_command(ns, '["LRANGE","events",0,-1]') <> '{"result":["three","two","one"]}'::jsonb then raise exception 'newest-first list order'; end if;
    if public.nxt_store_command(ns, '["LTRIM","events",0,1]') <> '{"result":"OK"}'::jsonb then raise exception 'LTRIM'; end if;
    if public.nxt_store_command(ns, '["LPUSH_CAPPED","events","four",2]') <> '{"result":2}'::jsonb then raise exception 'atomic capped list'; end if;
    if public.nxt_store_command(ns, '["LRANGE","events",0,-1]') <> '{"result":["four","three"]}'::jsonb then raise exception 'capped contents'; end if;
    if public.nxt_store_command(ns, '["LRANGE","events",-1,-1]') <> '{"result":["three"]}'::jsonb then raise exception 'negative list indices'; end if;
    perform public.nxt_store_command(ns, '["LTRIM","events",9,10]');
    if public.nxt_store_command(ns, '["LRANGE","events",0,-1]') <> '{"result":[]}'::jsonb then raise exception 'out-of-range trim'; end if;
    perform public.nxt_store_command(ns, '["LPUSH_CAPPED","events","discard",0]');
    if public.nxt_store_command(ns, '["LRANGE","events",0,-1]') <> '{"result":[]}'::jsonb then raise exception 'zero list cap'; end if;
    perform public.nxt_store_command(ns, '["LPUSH","events","old list"]');
    perform public.nxt_store_command(ns, '["SET","events","replaced"]');
    if public.nxt_store_command(ns, '["GET","events"]') <> '{"result":"replaced"}'::jsonb then raise exception 'SET type replacement'; end if;

    begin
        perform public.nxt_store_command('../invalid', '["GET","value"]');
        raise exception 'invalid namespace unexpectedly accepted';
    exception when invalid_parameter_value then null;
    end;
    begin
        perform public.nxt_store_command(ns, '["EVAL","return 1"]');
        raise exception 'arbitrary command unexpectedly accepted';
    exception when invalid_parameter_value then null;
    end;
    begin
        perform public.nxt_store_command(ns, '["SET","value",false]');
        raise exception 'non-string SET unexpectedly accepted';
    exception when invalid_parameter_value then null;
    end;
    begin
        perform public.nxt_store_command(ns, '["COMPARE_EXCHANGE","value",false,"wrong"]');
        raise exception 'non-string CAS expected unexpectedly accepted';
    exception when invalid_parameter_value then null;
    end;
    if public.nxt_store_command(ns, '["GET","value"]') <> '{"result":"hello"}'::jsonb then raise exception 'invalid requests mutated state'; end if;

    if pg_catalog.has_function_privilege('anon', 'public.nxt_store_command(text,jsonb)', 'EXECUTE') then raise exception 'anon RPC execution granted'; end if;
    if pg_catalog.has_function_privilege('authenticated', 'public.nxt_store_command(text,jsonb)', 'EXECUTE') then raise exception 'authenticated RPC execution granted'; end if;
    if not pg_catalog.has_function_privilege('service_role', 'public.nxt_store_command(text,jsonb)', 'EXECUTE') then raise exception 'service_role RPC execution missing'; end if;
    if pg_catalog.has_schema_privilege('anon', 'nxt_private', 'USAGE') then raise exception 'private schema exposed to anon'; end if;
    if pg_catalog.has_table_privilege('service_role', 'nxt_private.kv_values', 'SELECT') then raise exception 'direct table access should use RPC instead'; end if;
    if exists(select 1 from pg_catalog.pg_class c join pg_catalog.pg_namespace n on n.oid = c.relnamespace
               where n.nspname = 'nxt_private' and c.relname in ('kv_values','set_members','list_values') and not c.relrowsecurity) then
        raise exception 'RLS missing from private store table';
    end if;
end;
$test$;

-- Actual denied calls, not only catalog ACL inspection.
set local role anon;
do $test$
begin
    begin
        perform public.nxt_store_command('nxt_denied_probe', '["GET","value"]');
        raise exception 'anon unexpectedly executed the RPC';
    exception when insufficient_privilege then null;
    end;
end;
$test$;
reset role;

set local role authenticated;
do $test$
begin
    begin
        perform public.nxt_store_command('nxt_denied_probe', '["GET","value"]');
        raise exception 'authenticated unexpectedly executed the RPC';
    exception when insufficient_privilege then null;
    end;
end;
$test$;
reset role;

rollback;
