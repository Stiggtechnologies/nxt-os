-- Append-only filing log for the NXT Tax Filing Desk.
-- One row per filer confirmation: who confirmed, when, which return revision, the return
-- fingerprint and the recorded result. Rows can be added and read; they can never be
-- changed or removed. UPDATE, DELETE and TRUNCATE raise for every role (owner included),
-- and the only entry points are the two RPCs below (service_role only). There is no
-- update or delete RPC. Nothing here contacts CRA; NETFILE stays disabled.

create table if not exists nxt_private.append_only_log (
    position bigint generated always as identity primary key,
    namespace text not null,
    stream text not null check (stream in ('tax-filing')),
    entry jsonb not null check (pg_catalog.jsonb_typeof(entry) = 'object'),
    entry_sha256 text not null,
    created_at timestamptz not null default pg_catalog.now()
);

create index if not exists append_only_log_stream_idx
    on nxt_private.append_only_log (namespace, stream, position desc);

revoke all on table nxt_private.append_only_log from public, anon, authenticated, service_role;
revoke all on sequence nxt_private.append_only_log_position_seq from public, anon, authenticated, service_role;

create or replace function nxt_private.append_only_log_refuse()
returns trigger
language plpgsql
set search_path = pg_catalog
as $refuse$
begin
    raise exception 'the filing log is append-only; % is not allowed', tg_op
        using errcode = '42501';
end;
$refuse$;

drop trigger if exists append_only_log_no_update on nxt_private.append_only_log;
create trigger append_only_log_no_update
    before update or delete on nxt_private.append_only_log
    for each row execute function nxt_private.append_only_log_refuse();

drop trigger if exists append_only_log_no_truncate on nxt_private.append_only_log;
create trigger append_only_log_no_truncate
    before truncate on nxt_private.append_only_log
    for each statement execute function nxt_private.append_only_log_refuse();

create or replace function public.nxt_append_only_append(p_namespace text, p_stream text, p_entry jsonb)
returns jsonb
language plpgsql
security definer
set search_path = pg_catalog, nxt_private
as $append$
declare
    new_position bigint;
begin
    if p_namespace is null or p_namespace !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$' then
        raise exception 'invalid namespace' using errcode = '22023';
    end if;
    if p_stream is distinct from 'tax-filing' then
        raise exception 'unknown append-only stream' using errcode = '22023';
    end if;
    if p_entry is null or pg_catalog.jsonb_typeof(p_entry) <> 'object' then
        raise exception 'entry must be a JSON object' using errcode = '22023';
    end if;
    if pg_catalog.octet_length(p_entry::text) > 65536 then
        raise exception 'entry is too large' using errcode = '22023';
    end if;
    insert into nxt_private.append_only_log (namespace, stream, entry, entry_sha256)
    values (p_namespace, p_stream, p_entry,
            pg_catalog.encode(pg_catalog.sha256(pg_catalog.convert_to(p_entry::text, 'UTF8')), 'hex'))
    returning position into new_position;
    return pg_catalog.jsonb_build_object('result', new_position);
end;
$append$;

create or replace function public.nxt_append_only_list(p_namespace text, p_stream text, p_limit integer)
returns jsonb
language plpgsql
stable
security definer
set search_path = pg_catalog, nxt_private
as $list$
declare
    listed_rows jsonb;
begin
    if p_namespace is null or p_namespace !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$' then
        raise exception 'invalid namespace' using errcode = '22023';
    end if;
    if p_stream is distinct from 'tax-filing' then
        raise exception 'unknown append-only stream' using errcode = '22023';
    end if;
    select coalesce(pg_catalog.jsonb_agg(l.entry order by l.position desc), '[]'::jsonb) into listed_rows
      from (select a.entry, a.position from nxt_private.append_only_log a
             where a.namespace = p_namespace and a.stream = p_stream
             order by a.position desc
             limit greatest(1, least(coalesce(p_limit, 500), 5000))) l;
    return pg_catalog.jsonb_build_object('result', listed_rows);
end;
$list$;

revoke all on function nxt_private.append_only_log_refuse() from public, anon, authenticated, service_role;
revoke all on function public.nxt_append_only_append(text, text, jsonb) from public, anon, authenticated;
revoke all on function public.nxt_append_only_list(text, text, integer) from public, anon, authenticated;
grant execute on function public.nxt_append_only_append(text, text, jsonb) to service_role;
grant execute on function public.nxt_append_only_list(text, text, integer) to service_role;
