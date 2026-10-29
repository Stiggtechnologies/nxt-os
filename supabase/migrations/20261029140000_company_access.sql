-- Per-company access for client books before any real company goes live.
-- Staff sessions see only assigned companies. nxt_company_admin sees all.
-- The company-books HTTP API keeps using service_role through nxt_store_command.
-- That service-role path is the intended server credential. It does not bypass
-- the audit: every company-books or company-source-lists key it touches is appended
-- to company_access_audit. Financial payloads stay in nxt_private and are not copied
-- into the audit or the import catalog.
begin;

do $company_admin_role$
begin
  if not exists (select 1 from pg_catalog.pg_roles where rolname = 'nxt_company_admin') then
    create role nxt_company_admin nologin nosuperuser nocreatedb nocreaterole noinherit nobypassrls;
  end if;
end;
$company_admin_role$;

create table if not exists public.staff_principals (
    staff_id text primary key,
    is_admin boolean not null default false,
    created_at timestamptz not null default pg_catalog.clock_timestamp(),
    constraint staff_principals_id_chk check (staff_id ~ '^[A-Za-z0-9_.@:-]{1,128}$')
);

create table if not exists public.client_companies (
    company_id text primary key,
    display_name text not null,
    created_at timestamptz not null default pg_catalog.clock_timestamp(),
    constraint client_companies_id_chk check (company_id ~ '^[A-Za-z0-9_.-]{1,96}$'),
    constraint client_companies_name_chk check (char_length(display_name) between 1 and 200)
);

create table if not exists public.staff_company_assignments (
    staff_id text not null references public.staff_principals (staff_id),
    company_id text not null references public.client_companies (company_id),
    assigned_at timestamptz not null default pg_catalog.clock_timestamp(),
    assigned_by text not null,
    primary key (staff_id, company_id),
    constraint staff_company_assignments_by_chk check (assigned_by ~ '^[A-Za-z0-9_.@:-]{1,128}$')
);

create table if not exists public.company_book_imports (
    namespace text not null,
    company_id text not null references public.client_companies (company_id),
    lane text not null,
    object_kind text not null,
    object_id text not null,
    store_key text not null,
    updated_at timestamptz not null default pg_catalog.clock_timestamp(),
    primary key (namespace, store_key),
    constraint company_book_imports_namespace_chk check (namespace ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$'),
    constraint company_book_imports_lane_chk check (lane in ('company_books', 'company_source_lists')),
    constraint company_book_imports_kind_chk check (object_kind ~ '^[A-Za-z0-9_-]{1,64}$'),
    constraint company_book_imports_object_chk check (object_id ~ '^[A-Za-z0-9_.-]{1,128}$'),
    constraint company_book_imports_key_chk check (octet_length(store_key) between 1 and 512)
);

create table if not exists public.company_access_audit (
    audit_id bigint generated always as identity primary key,
    occurred_at timestamptz not null default pg_catalog.clock_timestamp(),
    actor text not null,
    actor_role text not null,
    company_id text not null,
    namespace text,
    action text not null,
    outcome text not null,
    source text not null,
    object_ref text,
    constraint company_access_audit_actor_chk check (char_length(actor) between 1 and 128),
    constraint company_access_audit_role_chk check (char_length(actor_role) between 1 and 64),
    constraint company_access_audit_company_chk check (company_id ~ '^[A-Za-z0-9_.-]{1,96}$'),
    constraint company_access_audit_namespace_chk check (
        namespace is null or namespace ~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$'
    ),
    constraint company_access_audit_action_chk check (char_length(action) between 1 and 64),
    constraint company_access_audit_outcome_chk check (outcome in ('allowed', 'denied')),
    constraint company_access_audit_source_chk check (source in ('company_books_read', 'service_role_store')),
    constraint company_access_audit_object_chk check (object_ref is null or octet_length(object_ref) between 1 and 512)
);

create index if not exists staff_company_assignments_company_idx
    on public.staff_company_assignments (company_id);
create index if not exists company_book_imports_company_idx
    on public.company_book_imports (company_id);
create index if not exists company_access_audit_company_idx
    on public.company_access_audit (company_id, occurred_at);
create index if not exists company_access_audit_actor_idx
    on public.company_access_audit (actor, occurred_at);

alter table public.staff_principals enable row level security;
alter table public.client_companies enable row level security;
alter table public.staff_company_assignments enable row level security;
alter table public.company_book_imports enable row level security;
alter table public.company_access_audit enable row level security;

comment on table public.staff_company_assignments is
    'Staff-to-client-company assignments. Staff cannot grant these to themselves.';
comment on table public.company_book_imports is
    'Catalog of company-books and company-source-lists objects written by the service-role store RPC. Payloads remain in nxt_private.';
comment on table public.company_access_audit is
    'Append-only log of who accessed which client company and when. Updates, deletes, and truncates are rejected.';
comment on role nxt_company_admin is
    'Override role that can read every client company. It does not bypass row level security.';

create or replace function public.nxt_current_staff_id()
returns text
language plpgsql
stable
security invoker
set search_path = pg_catalog
as $function$
declare
    claim text;
begin
    if exists (
        select 1
          from pg_catalog.pg_proc as p
          join pg_catalog.pg_namespace as n on n.oid = p.pronamespace
         where n.nspname = 'auth'
           and p.proname = 'uid'
           and pg_catalog.pg_get_function_identity_arguments(p.oid) = ''
    ) then
        begin
            execute 'select auth.uid()::text' into claim;
        exception when others then
            claim := null;
        end;
        if claim is not null and claim <> '' then
            return claim;
        end if;
    end if;
    claim := nullif(pg_catalog.current_setting('request.jwt.claim.sub', true), '');
    if claim is not null then
        return claim;
    end if;
    return nullif(pg_catalog.current_setting('nxt.staff_id', true), '');
end;
$function$;

create or replace function public.nxt_staff_is_admin(p_staff_id text)
returns boolean
language sql
stable
security definer
set search_path = pg_catalog, public
as $function$
    select exists (
        select 1
          from public.staff_principals as p
         where p.staff_id = p_staff_id
           and p.is_admin
    );
$function$;

create or replace function public.nxt_is_company_admin()
returns boolean
language plpgsql
stable
security invoker
set search_path = pg_catalog, public
as $function$
begin
    if pg_catalog.pg_has_role(current_user, 'nxt_company_admin', 'member') then
        return true;
    end if;
    return public.nxt_staff_is_admin(public.nxt_current_staff_id());
end;
$function$;

create or replace function nxt_private.note_company_book_access(
    p_namespace text,
    p_key text,
    p_action text,
    p_write boolean
) returns void
language plpgsql
security definer
set search_path = pg_catalog
as $function$
declare
    parts text[];
    v_lane text;
    v_company text;
    v_kind text;
    v_object text;
    v_actor text;
begin
    if p_key ~ '^nxt:company-books:[A-Za-z0-9_.-]{1,96}:[A-Za-z0-9_-]{1,64}:[A-Za-z0-9_.-]{1,128}$' then
        v_lane := 'company_books';
    elsif p_key ~ '^nxt:company-source-lists:[A-Za-z0-9_.-]{1,96}:[A-Za-z0-9_-]{1,64}:[A-Za-z0-9_.-]{1,128}$' then
        v_lane := 'company_source_lists';
    else
        return;
    end if;
    parts := pg_catalog.regexp_match(
        p_key,
        '^nxt:company-(?:books|source-lists):([A-Za-z0-9_.-]{1,96}):([A-Za-z0-9_-]{1,64}):([A-Za-z0-9_.-]{1,128})$'
    );
    if parts is null then
        return;
    end if;
    v_company := parts[1];
    v_kind := parts[2];
    v_object := parts[3];
    -- Global roster and company-index keys are not client companies.
    if v_company = 'companies' or (v_company = 'roster' and v_kind in ('head', 'capture')) then
        return;
    end if;
    v_actor := nullif(pg_catalog.current_setting('nxt.actor_id', true), '');
    if v_actor is null then
        v_actor := 'service_role';
    elsif v_actor !~ '^[A-Za-z0-9_.@:-]{1,128}$' then
        raise exception using errcode = '22023', message = 'invalid company access actor';
    end if;
    insert into public.company_access_audit (
        actor, actor_role, company_id, namespace, action, outcome, source, object_ref
    ) values (
        v_actor, 'service_role', v_company, p_namespace, p_action, 'allowed', 'service_role_store', p_key
    );
    if not p_write then
        return;
    end if;
    insert into public.client_companies (company_id, display_name)
    values (v_company, v_company)
    on conflict (company_id) do nothing;
    insert into public.company_book_imports (namespace, company_id, lane, object_kind, object_id, store_key)
    values (p_namespace, v_company, v_lane, v_kind, v_object, p_key)
    on conflict (namespace, store_key) do update
        set updated_at = pg_catalog.clock_timestamp();
end;
$function$;

create or replace function public.nxt_read_company_books(p_company_id text)
returns table (
    company_id text,
    display_name text,
    namespace text,
    lane text,
    object_kind text,
    object_id text,
    store_key text
)
language plpgsql
security invoker
set search_path = pg_catalog, public
as $function$
declare
    v_actor text;
    v_role text;
    v_visible boolean;
begin
    if p_company_id is null or p_company_id !~ '^[A-Za-z0-9_.-]{1,96}$' then
        raise exception using errcode = '22023', message = 'invalid company id';
    end if;
    if current_user not in ('authenticated', 'nxt_company_admin', 'service_role')
       and not public.nxt_is_company_admin() then
        raise exception using errcode = '42501', message = 'company access denied';
    end if;
    v_role := current_user;
    v_actor := coalesce(public.nxt_current_staff_id(), v_role);
    select exists (
        select 1 from public.client_companies as c where c.company_id = p_company_id
    ) into v_visible;
    if not coalesce(v_visible, false) then
        insert into public.company_access_audit (actor, actor_role, company_id, action, outcome, source)
        values (v_actor, v_role, p_company_id, 'read', 'denied', 'company_books_read');
        return;
    end if;
    insert into public.company_access_audit (actor, actor_role, company_id, action, outcome, source)
    values (v_actor, v_role, p_company_id, 'read', 'allowed', 'company_books_read');
    return query
        select c.company_id, c.display_name, i.namespace, i.lane, i.object_kind, i.object_id, i.store_key
          from public.client_companies as c
          left join public.company_book_imports as i on i.company_id = c.company_id
         where c.company_id = p_company_id;
end;
$function$;

create or replace function public.reject_company_access_audit_mutation()
returns trigger
language plpgsql
security definer
set search_path = pg_catalog
as $function$
begin
    raise exception using errcode = '42501', message = 'company access audit is append-only';
end;
$function$;

drop trigger if exists company_access_audit_append_only on public.company_access_audit;
create trigger company_access_audit_append_only
    before update or delete on public.company_access_audit
    for each row execute function public.reject_company_access_audit_mutation();

drop trigger if exists company_access_audit_no_truncate on public.company_access_audit;
create trigger company_access_audit_no_truncate
    before truncate on public.company_access_audit
    for each statement execute function public.reject_company_access_audit_mutation();

revoke all on table
    public.staff_principals,
    public.client_companies,
    public.staff_company_assignments,
    public.company_book_imports,
    public.company_access_audit
    from public, anon, authenticated, service_role, nxt_company_admin;

grant select on public.staff_principals, public.client_companies,
    public.staff_company_assignments, public.company_book_imports
    to authenticated, nxt_company_admin, service_role;
grant select, insert on public.company_access_audit
    to authenticated, nxt_company_admin, service_role;
grant insert, update, delete on public.staff_principals, public.client_companies,
    public.staff_company_assignments, public.company_book_imports
    to nxt_company_admin, service_role;
grant usage, select on sequence public.company_access_audit_audit_id_seq
    to authenticated, nxt_company_admin, service_role;

revoke all on function public.nxt_current_staff_id() from public;
revoke all on function public.nxt_staff_is_admin(text) from public;
revoke all on function public.nxt_is_company_admin() from public;
revoke all on function public.nxt_read_company_books(text) from public;
revoke all on function nxt_private.note_company_book_access(text, text, text, boolean) from public;
revoke all on function public.reject_company_access_audit_mutation() from public;

grant execute on function public.nxt_current_staff_id() to authenticated, nxt_company_admin, service_role;
grant execute on function public.nxt_staff_is_admin(text) to authenticated, nxt_company_admin, service_role;
grant execute on function public.nxt_is_company_admin() to authenticated, nxt_company_admin, service_role;
grant execute on function public.nxt_read_company_books(text) to authenticated, nxt_company_admin, service_role;
grant execute on function public.reject_company_access_audit_mutation()
    to authenticated, nxt_company_admin, service_role;

drop policy if exists staff_principals_select on public.staff_principals;
create policy staff_principals_select on public.staff_principals
    for select to authenticated
    using (staff_id = public.nxt_current_staff_id() or public.nxt_is_company_admin());
drop policy if exists staff_principals_admin on public.staff_principals;
create policy staff_principals_admin on public.staff_principals
    for all to nxt_company_admin
    using (true) with check (true);
drop policy if exists staff_principals_service on public.staff_principals;
create policy staff_principals_service on public.staff_principals
    for all to service_role
    using (true) with check (true);

drop policy if exists client_companies_select on public.client_companies;
create policy client_companies_select on public.client_companies
    for select to authenticated
    using (
        public.nxt_is_company_admin()
        or exists (
            select 1
              from public.staff_company_assignments as a
             where a.company_id = client_companies.company_id
               and a.staff_id = public.nxt_current_staff_id()
        )
    );
drop policy if exists client_companies_admin on public.client_companies;
create policy client_companies_admin on public.client_companies
    for all to nxt_company_admin
    using (true) with check (true);
drop policy if exists client_companies_service on public.client_companies;
create policy client_companies_service on public.client_companies
    for all to service_role
    using (true) with check (true);

drop policy if exists staff_company_assignments_select on public.staff_company_assignments;
create policy staff_company_assignments_select on public.staff_company_assignments
    for select to authenticated
    using (staff_id = public.nxt_current_staff_id() or public.nxt_is_company_admin());
drop policy if exists staff_company_assignments_admin on public.staff_company_assignments;
create policy staff_company_assignments_admin on public.staff_company_assignments
    for all to nxt_company_admin
    using (true) with check (true);
drop policy if exists staff_company_assignments_service on public.staff_company_assignments;
create policy staff_company_assignments_service on public.staff_company_assignments
    for all to service_role
    using (true) with check (true);

drop policy if exists company_book_imports_select on public.company_book_imports;
create policy company_book_imports_select on public.company_book_imports
    for select to authenticated
    using (
        public.nxt_is_company_admin()
        or exists (
            select 1
              from public.staff_company_assignments as a
             where a.company_id = company_book_imports.company_id
               and a.staff_id = public.nxt_current_staff_id()
        )
    );
drop policy if exists company_book_imports_admin on public.company_book_imports;
create policy company_book_imports_admin on public.company_book_imports
    for all to nxt_company_admin
    using (true) with check (true);
drop policy if exists company_book_imports_service on public.company_book_imports;
create policy company_book_imports_service on public.company_book_imports
    for all to service_role
    using (true) with check (true);

drop policy if exists company_access_audit_select_staff on public.company_access_audit;
create policy company_access_audit_select_staff on public.company_access_audit
    for select to authenticated
    using (actor = public.nxt_current_staff_id() or public.nxt_is_company_admin());
drop policy if exists company_access_audit_insert_staff on public.company_access_audit;
create policy company_access_audit_insert_staff on public.company_access_audit
    for insert to authenticated
    with check (
        actor = public.nxt_current_staff_id()
        and actor_role = current_user
        and public.nxt_current_staff_id() is not null
    );
drop policy if exists company_access_audit_select_admin on public.company_access_audit;
create policy company_access_audit_select_admin on public.company_access_audit
    for select to nxt_company_admin
    using (true);
drop policy if exists company_access_audit_insert_admin on public.company_access_audit;
create policy company_access_audit_insert_admin on public.company_access_audit
    for insert to nxt_company_admin
    with check (
        actor_role = current_user
        and (actor = current_user or actor = public.nxt_current_staff_id())
    );
drop policy if exists company_access_audit_select_service on public.company_access_audit;
create policy company_access_audit_select_service on public.company_access_audit
    for select to service_role
    using (true);
drop policy if exists company_access_audit_insert_service on public.company_access_audit;
create policy company_access_audit_insert_service on public.company_access_audit
    for insert to service_role
    with check (actor_role = current_user and actor in (current_user, 'service_role'));

-- Row level security is enabled and not forced. The table owner is the
-- security-definer store RPC, which must append audit rows and catalog writes.
-- authenticated and nxt_company_admin are not the owner, so their policies apply.
-- Hosted service_role may bypass row level security; that is the intended API
-- credential, and note_company_book_access still appends the audit row.

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
            perform nxt_private.note_company_book_access(p_namespace, item #>> '{}', 'MGET', false);
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

    perform nxt_private.note_company_book_access(
        p_namespace,
        key_name,
        op,
        op in ('SET', 'COMPARE_EXCHANGE', 'INCRBY', 'SADD', 'LPUSH', 'LPUSH_CAPPED', 'LTRIM'));

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
    'Server-only NXT Store RPC. service_role is the intended executor for the company-books API. Company-books and company-source-lists keys are catalogued and appended to company_access_audit. Namespace remains deployment isolation.';

revoke all on function public.nxt_store_command(text, jsonb) from public, anon, authenticated;
grant execute on function public.nxt_store_command(text, jsonb) to service_role;
notify pgrst, 'reload schema';

commit;
