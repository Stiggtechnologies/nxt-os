-- Rolled-back filing log contract (loopback only, via scripts/check-store-migrations.sh).
-- service_role can append and list through the RPCs, cannot touch the table directly,
-- and nobody (owner included) can update, delete or truncate a row.
begin;

set local role service_role;
do $service$
declare
    listed jsonb;
begin
    if (public.nxt_append_only_append('filing_log_test', 'tax-filing', '{"id":"fl_1","result":"refused"}'::jsonb)->>'result')::bigint < 1 then
        raise exception 'append did not return a position';
    end if;
    perform public.nxt_append_only_append('filing_log_test', 'tax-filing', '{"id":"fl_2","result":"refused"}'::jsonb);
    listed := public.nxt_append_only_list('filing_log_test', 'tax-filing', 10)->'result';
    if pg_catalog.jsonb_array_length(listed) <> 2 or listed->0->>'id' <> 'fl_2' then
        raise exception 'list is not newest-first: %', listed;
    end if;
    begin
        perform public.nxt_append_only_append('filing_log_test', 'other', '{}'::jsonb);
        raise exception 'unknown stream was accepted';
    exception when invalid_parameter_value then null;
    end;
    begin
        update nxt_private.append_only_log set entry = '{}'::jsonb;
        raise exception 'service_role updated the filing log directly';
    exception when insufficient_privilege then null;
    end;
    begin
        delete from nxt_private.append_only_log;
        raise exception 'service_role deleted from the filing log directly';
    exception when insufficient_privilege then null;
    end;
end;
$service$;
reset role;

do $owner$
begin
    begin
        update nxt_private.append_only_log set entry = '{"tampered":true}'::jsonb where namespace = 'filing_log_test';
        raise exception 'owner update was not refused';
    exception when insufficient_privilege then null;
    end;
    begin
        delete from nxt_private.append_only_log where namespace = 'filing_log_test';
        raise exception 'owner delete was not refused';
    exception when insufficient_privilege then null;
    end;
    begin
        truncate nxt_private.append_only_log;
        raise exception 'owner truncate was not refused';
    exception when insufficient_privilege then null;
    end;
    if exists (select 1 from pg_catalog.pg_proc p join pg_catalog.pg_namespace n on n.oid = p.pronamespace
                where n.nspname = 'public' and p.proname ~ '^nxt_append_only_(update|delete|remove|edit)') then
        raise exception 'an update or delete RPC exists for the filing log';
    end if;
    if pg_catalog.has_function_privilege('anon', 'public.nxt_append_only_append(text, text, jsonb)', 'execute')
       or pg_catalog.has_function_privilege('authenticated', 'public.nxt_append_only_list(text, text, integer)', 'execute') then
        raise exception 'filing log RPCs are exposed beyond service_role';
    end if;
end;
$owner$;

rollback;
