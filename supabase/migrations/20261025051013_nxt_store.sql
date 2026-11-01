-- Durable server-only Store primitives. No client/company records are seeded.
begin;

create schema if not exists nxt_private;
revoke all on schema nxt_private from public, anon, authenticated, service_role;

create table if not exists nxt_private.kv_values (
    namespace text not null check (namespace ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$'),
    key text not null check (octet_length(key) between 1 and 512),
    value text not null check (octet_length(value) <= 8388608),
    updated_at timestamptz not null default pg_catalog.clock_timestamp(),
    primary key (namespace, key)
);

create table if not exists nxt_private.set_members (
    namespace text not null check (namespace ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$'),
    key text not null check (octet_length(key) between 1 and 512),
    member text not null check (octet_length(member) <= 1024),
    created_at timestamptz not null default pg_catalog.clock_timestamp(),
    primary key (namespace, key, member)
);

create table if not exists nxt_private.list_values (
    namespace text not null check (namespace ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$'),
    key text not null check (octet_length(key) between 1 and 512),
    position bigint generated always as identity,
    value text not null check (octet_length(value) <= 8388608),
    created_at timestamptz not null default pg_catalog.clock_timestamp(),
    primary key (namespace, key, position)
);

alter table nxt_private.kv_values enable row level security;
alter table nxt_private.set_members enable row level security;
alter table nxt_private.list_values enable row level security;
-- No ordinary-role policies. API access is exclusively through the restricted RPC.
revoke all on table nxt_private.kv_values, nxt_private.set_members, nxt_private.list_values
    from public, anon, authenticated, service_role;
revoke all on sequence nxt_private.list_values_position_seq
    from public, anon, authenticated, service_role;

create or replace function public.nxt_store_command(p_namespace text, p_command jsonb)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, nxt_private
as $function$
declare
    op text;
    item jsonb;
    key_name text;
    new_value text;
    old_value text;
    expected text;
    had_value boolean;
    exists_any boolean;
    amount bigint;
    current_number bigint;
    start_index bigint;
    end_index bigint;
    item_count bigint;
    cap bigint;
    changed bigint;
    answer jsonb;
    argc integer;
begin
    if p_namespace is null or p_namespace !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$' then
        raise exception using errcode = '22023', message = 'invalid NXT store namespace';
    end if;
    if p_command is null or pg_catalog.jsonb_typeof(p_command) <> 'array' then
        raise exception using errcode = '22023', message = 'store command must be a JSON array';
    end if;
    argc := pg_catalog.jsonb_array_length(p_command);
    if argc < 2 or argc > 501 or pg_catalog.jsonb_typeof(p_command -> 0) <> 'string'
       or pg_catalog.octet_length(p_command::text) > 41943040 then
        raise exception using errcode = '22023', message = 'invalid store command size or operation';
    end if;
    op := pg_catalog.upper(p_command ->> 0);
    if op not in ('GET', 'SET', 'COMPARE_EXCHANGE', 'INCRBY', 'SADD', 'SMEMBERS',
                  'LPUSH', 'LPUSH_CAPPED', 'LTRIM', 'LRANGE', 'MGET') then
        raise exception using errcode = '22023', message = 'unsupported store operation';
    end if;

    -- MGET is one snapshot query, retains caller order and nulls, and never locks
    -- multiple keys (avoiding lock-order deadlocks between independent requests).
    if op = 'MGET' then
        for item in select value from pg_catalog.jsonb_array_elements(p_command - 0) loop
            if pg_catalog.jsonb_typeof(item) <> 'string'
               or pg_catalog.octet_length(item #>> '{}') not between 1 and 512 then
                raise exception using errcode = '22023', message = 'invalid store key';
            end if;
        end loop;
        select coalesce(pg_catalog.jsonb_agg(v.value order by wanted.ordinality), '[]'::jsonb)
          into answer
          from pg_catalog.jsonb_array_elements_text(p_command - 0) with ordinality as wanted(key, ordinality)
          left join nxt_private.kv_values as v on v.namespace = p_namespace and v.key = wanted.key;
        return pg_catalog.jsonb_build_object('result', answer);
    end if;

    if pg_catalog.jsonb_typeof(p_command -> 1) <> 'string'
       or pg_catalog.octet_length(p_command ->> 1) not between 1 and 512 then
        raise exception using errcode = '22023', message = 'invalid store key';
    end if;
    key_name := p_command ->> 1;

    if (op in ('GET', 'SMEMBERS') and argc <> 2)
       or (op in ('INCRBY', 'SADD', 'LPUSH') and argc <> 3)
       or (op in ('COMPARE_EXCHANGE', 'LPUSH_CAPPED', 'LTRIM', 'LRANGE') and argc <> 4)
       or (op = 'SET' and argc not in (3, 4)) then
        raise exception using errcode = '22023', message = 'wrong number of store arguments';
    end if;

    -- Every single-key operation uses the same transaction lock, including when
    -- there is no row yet. A SELECT FOR UPDATE alone cannot lock a missing key.
    perform pg_catalog.pg_advisory_xact_lock(pg_catalog.hashtextextended(
        pg_catalog.jsonb_build_array('nxt_store', p_namespace, key_name)::text, 0));

    if op in ('GET', 'COMPARE_EXCHANGE', 'INCRBY') and (
        exists(select 1 from nxt_private.set_members s where s.namespace = p_namespace and s.key = key_name)
        or exists(select 1 from nxt_private.list_values l where l.namespace = p_namespace and l.key = key_name)
    ) then
        raise exception using errcode = '22023', message = 'WRONGTYPE store key is not a string';
    end if;
    if op in ('SADD', 'SMEMBERS') and (
        exists(select 1 from nxt_private.kv_values v where v.namespace = p_namespace and v.key = key_name)
        or exists(select 1 from nxt_private.list_values l where l.namespace = p_namespace and l.key = key_name)
    ) then
        raise exception using errcode = '22023', message = 'WRONGTYPE store key is not a set';
    end if;
    if op in ('LPUSH', 'LPUSH_CAPPED', 'LRANGE', 'LTRIM') and (
        exists(select 1 from nxt_private.kv_values v where v.namespace = p_namespace and v.key = key_name)
        or exists(select 1 from nxt_private.set_members s where s.namespace = p_namespace and s.key = key_name)
    ) then
        raise exception using errcode = '22023', message = 'WRONGTYPE store key is not a list';
    end if;

    if op = 'GET' then
        select v.value into old_value from nxt_private.kv_values v
         where v.namespace = p_namespace and v.key = key_name;
        return pg_catalog.jsonb_build_object('result', old_value);
    end if;

    if op in ('SET', 'COMPARE_EXCHANGE', 'LPUSH', 'LPUSH_CAPPED') then
        if op = 'COMPARE_EXCHANGE' then
            if pg_catalog.jsonb_typeof(p_command -> 2) not in ('string', 'null') then
                raise exception using errcode = '22023', message = 'CAS expected must be a string or null for missing';
            end if;
            if pg_catalog.jsonb_typeof(p_command -> 3) <> 'string' then
                raise exception using errcode = '22023', message = 'store value must be a JSON string';
            end if;
            new_value := p_command ->> 3;
        else
            if pg_catalog.jsonb_typeof(p_command -> 2) <> 'string' then
                raise exception using errcode = '22023', message = 'store value must be a JSON string';
            end if;
            new_value := p_command ->> 2;
        end if;
        if pg_catalog.octet_length(new_value) > 8388608 then
            raise exception using errcode = '22023', message = 'store value exceeds 8 MiB';
        end if;
    end if;

    if op = 'SET' then
        if argc = 4 and (pg_catalog.jsonb_typeof(p_command -> 3) <> 'string' or pg_catalog.upper(p_command ->> 3) <> 'NX') then
            raise exception using errcode = '22023', message = 'SET supports only the optional NX flag';
        end if;
        if argc = 4 then
            select exists(select 1 from nxt_private.kv_values v where v.namespace = p_namespace and v.key = key_name)
                or exists(select 1 from nxt_private.set_members s where s.namespace = p_namespace and s.key = key_name)
                or exists(select 1 from nxt_private.list_values l where l.namespace = p_namespace and l.key = key_name)
              into exists_any;
            if exists_any then return pg_catalog.jsonb_build_object('result', null); end if;
        end if;
        -- Redis SET replaces the previous key type.
        delete from nxt_private.set_members s where s.namespace = p_namespace and s.key = key_name;
        delete from nxt_private.list_values l where l.namespace = p_namespace and l.key = key_name;
        insert into nxt_private.kv_values(namespace, key, value) values (p_namespace, key_name, new_value)
        on conflict (namespace, key) do update set value = excluded.value, updated_at = pg_catalog.clock_timestamp();
        return pg_catalog.jsonb_build_object('result', 'OK');
    end if;

    if op = 'COMPARE_EXCHANGE' then
        select v.value into old_value from nxt_private.kv_values v
         where v.namespace = p_namespace and v.key = key_name;
        had_value := found;
        expected := p_command ->> 2;
        if (pg_catalog.jsonb_typeof(p_command -> 2) = 'null' and had_value)
           or (pg_catalog.jsonb_typeof(p_command -> 2) = 'string' and (not had_value or old_value <> expected)) then
            return pg_catalog.jsonb_build_object('result', 0);
        end if;
        insert into nxt_private.kv_values(namespace, key, value) values (p_namespace, key_name, new_value)
        on conflict (namespace, key) do update set value = excluded.value, updated_at = pg_catalog.clock_timestamp();
        return pg_catalog.jsonb_build_object('result', 1);
    end if;

    if op = 'INCRBY' then
        if pg_catalog.jsonb_typeof(p_command -> 2) not in ('number', 'string')
           or (p_command ->> 2) !~ '^[+-]?[0-9]+$' then
            raise exception using errcode = '22023', message = 'INCRBY amount must be a signed integer';
        end if;
        amount := (p_command ->> 2)::bigint;
        select v.value into old_value from nxt_private.kv_values v
         where v.namespace = p_namespace and v.key = key_name;
        if found then
            if old_value !~ '^[+-]?[0-9]+$' then
                raise exception using errcode = '22023', message = 'INCRBY key does not hold an integer';
            end if;
            current_number := old_value::bigint;
        else current_number := 0;
        end if;
        current_number := current_number + amount; -- bigint overflow raises, leaving the row unchanged.
        insert into nxt_private.kv_values(namespace, key, value) values (p_namespace, key_name, current_number::text)
        on conflict (namespace, key) do update set value = excluded.value, updated_at = pg_catalog.clock_timestamp();
        return pg_catalog.jsonb_build_object('result', current_number);
    end if;

    if op = 'SADD' then
        if pg_catalog.jsonb_typeof(p_command -> 2) <> 'string' or pg_catalog.octet_length(p_command ->> 2) > 1024 then
            raise exception using errcode = '22023', message = 'set member must be a JSON string of at most 1024 bytes';
        end if;
        insert into nxt_private.set_members(namespace, key, member) values (p_namespace, key_name, p_command ->> 2)
        on conflict (namespace, key, member) do nothing;
        get diagnostics changed = row_count;
        return pg_catalog.jsonb_build_object('result', changed);
    end if;
    if op = 'SMEMBERS' then
        select coalesce(pg_catalog.jsonb_agg(s.member order by s.member), '[]'::jsonb) into answer
          from nxt_private.set_members s where s.namespace = p_namespace and s.key = key_name;
        return pg_catalog.jsonb_build_object('result', answer);
    end if;

    if op in ('LPUSH_CAPPED', 'LRANGE', 'LTRIM') then
        if pg_catalog.jsonb_typeof(p_command -> 3) not in ('number', 'string') or (p_command ->> 3) !~ '^[+-]?[0-9]+$'
           or (op <> 'LPUSH_CAPPED' and (pg_catalog.jsonb_typeof(p_command -> 2) not in ('number', 'string') or (p_command ->> 2) !~ '^[+-]?[0-9]+$')) then
            raise exception using errcode = '22023', message = 'list indices and caps must be signed integers';
        end if;
    end if;
    if op in ('LPUSH', 'LPUSH_CAPPED') then
        select pg_catalog.count(*) into item_count from nxt_private.list_values l
         where l.namespace = p_namespace and l.key = key_name;
        if op = 'LPUSH_CAPPED' then
            cap := (p_command ->> 3)::bigint;
            if cap < 0 or cap > 10000 then raise exception using errcode = '22023', message = 'list cap must be between 0 and 10000'; end if;
            if cap = 0 then
                delete from nxt_private.list_values l where l.namespace = p_namespace and l.key = key_name;
                return pg_catalog.jsonb_build_object('result', 0);
            end if;
        elsif item_count >= 10000 then
            raise exception using errcode = '22023', message = 'list capacity of 10000 reached; use LPUSH_CAPPED';
        end if;
        insert into nxt_private.list_values(namespace, key, value) values (p_namespace, key_name, new_value);
        if op = 'LPUSH_CAPPED' then
            delete from nxt_private.list_values l where l.namespace = p_namespace and l.key = key_name and l.position in (
                select x.position from nxt_private.list_values x
                 where x.namespace = p_namespace and x.key = key_name order by x.position desc offset cap);
            return pg_catalog.jsonb_build_object('result', least(item_count + 1, cap));
        end if;
        return pg_catalog.jsonb_build_object('result', item_count + 1);
    end if;

    -- Inclusive Redis-style ranges, including negative indices such as 0,-1.
    select pg_catalog.count(*) into item_count from nxt_private.list_values l
     where l.namespace = p_namespace and l.key = key_name;
    start_index := (p_command ->> 2)::bigint;
    end_index := (p_command ->> 3)::bigint;
    start_index := case when start_index < 0 then greatest(item_count + start_index, 0) else start_index end;
    end_index := case when end_index < 0 then item_count + end_index else least(end_index, item_count - 1) end;
    if op = 'LRANGE' then
        if start_index > end_index or end_index < 0 or start_index >= item_count then
            return pg_catalog.jsonb_build_object('result', '[]'::jsonb);
        end if;
        select coalesce(pg_catalog.jsonb_agg(x.value order by x.position desc), '[]'::jsonb) into answer
          from (select l.position, l.value from nxt_private.list_values l
                 where l.namespace = p_namespace and l.key = key_name order by l.position desc
                 offset start_index limit (end_index - start_index + 1)) x;
        return pg_catalog.jsonb_build_object('result', answer);
    end if;
    if start_index > end_index or end_index < 0 or start_index >= item_count then
        delete from nxt_private.list_values l where l.namespace = p_namespace and l.key = key_name;
    else
        delete from nxt_private.list_values l where l.namespace = p_namespace and l.key = key_name and l.position in (
            select ranked.position from (
                select x.position, pg_catalog.row_number() over (order by x.position desc) - 1 as idx
                  from nxt_private.list_values x where x.namespace = p_namespace and x.key = key_name) ranked
            where ranked.idx < start_index or ranked.idx > end_index);
    end if;
    return pg_catalog.jsonb_build_object('result', 'OK');
end;
$function$;

comment on function public.nxt_store_command(text, jsonb) is
    'Server-only NXT Store RPC. Namespace is deployment isolation, not end-user tenant authorization.';

revoke all on function public.nxt_store_command(text, jsonb) from public, anon, authenticated;
grant execute on function public.nxt_store_command(text, jsonb) to service_role;
notify pgrst, 'reload schema';

commit;
