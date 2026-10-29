-- Rolled-back company access contract. Refuses nothing against production because
-- the caller (scripts/check-store-migrations.sh) only points it at loopback.
-- Assigned staff see their company, unassigned staff see nothing, nxt_company_admin
-- and an is_admin principal see every company, and both staff reads and the
-- service-role store path append audit rows.
begin;

do $bypass$
begin
    if (select rolbypassrls from pg_catalog.pg_roles where rolname = 'authenticated') then
        raise exception 'authenticated bypasses row level security';
    end if;
    if (select rolbypassrls from pg_catalog.pg_roles where rolname = 'nxt_company_admin') then
        raise exception 'nxt_company_admin bypasses row level security';
    end if;
    if exists (
        select 1 from pg_catalog.pg_class
         where oid in (
            'public.staff_principals'::regclass,
            'public.client_companies'::regclass,
            'public.staff_company_assignments'::regclass,
            'public.company_book_imports'::regclass,
            'public.company_access_audit'::regclass
         ) and not relrowsecurity
    ) then
        raise exception 'company access table is missing row level security';
    end if;
end;
$bypass$;

set local role service_role;
do $service$
declare
    ns text := 'company_access_test';
    chunk text := pg_catalog.repeat('a', 64);
    capture text := pg_catalog.repeat('b', 64);
    chunk_key text;
begin
    chunk_key := 'nxt:company-books:acme:chunk:' || chunk;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array('SET', chunk_key, 'x', 'NX'))
        <> '{"result":"OK"}'::jsonb then
        raise exception 'service role chunk upload failed under row level security';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array('SET', chunk_key, 'y', 'NX'))
        <> '{"result":null}'::jsonb then
        raise exception 'service role idempotent chunk upload changed the book';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array('GET', chunk_key))
        <> '{"result":"x"}'::jsonb then
        raise exception 'service role chunk read failed under row level security';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array(
        'COMPARE_EXCHANGE', 'nxt:company-books:acme:head:v1', null, 'head-body'))
        <> '{"result":1}'::jsonb then
        raise exception 'service role head publish failed under row level security';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array('GET', 'nxt:company-books:acme:head:v1'))
        <> '{"result":"head-body"}'::jsonb then
        raise exception 'service role head read failed under row level security';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array(
        'SET', 'nxt:company-books:acme:preview-request:preview-acme-1', 'preview'))
        <> '{"result":"OK"}'::jsonb then
        raise exception 'service role preview request failed under row level security';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array(
        'SET', 'nxt:company-source-lists:acme:capture:' || capture, 'capture'))
        <> '{"result":"OK"}'::jsonb then
        raise exception 'service role source-list write failed under row level security';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array(
        'SET', 'nxt:company-books:bravo:head:v1', 'bravo-head'))
        <> '{"result":"OK"}'::jsonb then
        raise exception 'service role second company write failed under row level security';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array('SET', 'nxt:firm:state:v1', 'ok'))
        <> '{"result":"OK"}'::jsonb then
        raise exception 'service role non-company write failed';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array('SET', 'nxt:company-books:roster:head:v1', 'roster'))
        <> '{"result":"OK"}'::jsonb then
        raise exception 'service role roster write failed';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array('SET', 'nxt:company-books:companies:v1', '[]'))
        <> '{"result":"OK"}'::jsonb then
        raise exception 'service role company index write failed';
    end if;
    if public.nxt_store_command(ns, pg_catalog.jsonb_build_array('MGET', chunk_key, 'nxt:firm:state:v1'))
        <> pg_catalog.jsonb_build_object('result', pg_catalog.jsonb_build_array('x', 'ok')) then
        raise exception 'service role company MGET failed under row level security';
    end if;
end;
$service$;
reset role;

do $catalog$
begin
    if (select pg_catalog.count(*) from public.client_companies) <> 2 then
        raise exception 'service role did not catalog both client companies';
    end if;
    if (select pg_catalog.count(*) from public.company_book_imports where company_id = 'acme') <> 4 then
        raise exception 'acme import catalog mismatch';
    end if;
    if (select pg_catalog.count(*) from public.company_book_imports where company_id = 'bravo' and object_kind = 'head') <> 1 then
        raise exception 'bravo import catalog mismatch';
    end if;
    if exists (select 1 from public.company_book_imports where company_id in ('roster', 'companies')) then
        raise exception 'global company-books key was catalogued as a client company';
    end if;
    if exists (select 1 from public.company_access_audit where company_id in ('roster', 'companies')) then
        raise exception 'global company-books key was audited as a client company';
    end if;
    if exists (select 1 from public.company_access_audit where object_ref = 'nxt:firm:state:v1') then
        raise exception 'non-company store key was audited';
    end if;
    if exists (select 1 from public.company_access_audit where pg_catalog.to_jsonb(company_access_audit)::text like '%head-body%') then
        raise exception 'audit stored a book payload';
    end if;
    if (select pg_catalog.count(*) from public.company_access_audit
         where source = 'service_role_store' and actor = 'service_role' and actor_role = 'service_role'
           and outcome = 'allowed' and company_id = 'acme' and occurred_at is not null) <> 8 then
        raise exception 'service role acme accesses were not all audited';
    end if;
    if (select pg_catalog.count(*) from public.company_access_audit
         where source = 'service_role_store' and actor = 'service_role' and company_id = 'bravo'
           and action = 'SET' and outcome = 'allowed') <> 1 then
        raise exception 'service role bravo access was not audited';
    end if;
end;
$catalog$;

set local role service_role;
do $service_read$
begin
    if (select pg_catalog.count(*) from public.client_companies) <> 2 then
        raise exception 'service role cannot see client companies under row level security';
    end if;
    if (select pg_catalog.count(*) from public.company_book_imports) <> 5 then
        raise exception 'service role cannot see company book imports under row level security';
    end if;
end;
$service_read$;
reset role;

insert into public.staff_principals (staff_id, is_admin) values
    ('staff-assigned', false),
    ('staff-other', false);
insert into public.staff_company_assignments (staff_id, company_id, assigned_by)
values ('staff-assigned', 'acme', 'test-admin');

set local role authenticated;
select pg_catalog.set_config('nxt.staff_id', 'staff-assigned', true);
do $assigned$
declare
    seen integer;
begin
    select pg_catalog.count(*) into seen from public.client_companies;
    if seen <> 1 then raise exception 'assigned staff saw % companies', seen; end if;
    if not exists (select 1 from public.client_companies where company_id = 'acme') then
        raise exception 'assigned staff cannot see their company';
    end if;
    if exists (select 1 from public.client_companies where company_id = 'bravo') then
        raise exception 'assigned staff can see an unassigned company';
    end if;
    select pg_catalog.count(*) into seen from public.company_book_imports;
    if seen <> 4 then raise exception 'assigned staff saw % imports', seen; end if;
    if exists (select 1 from public.company_book_imports where company_id <> 'acme') then
        raise exception 'assigned staff saw another company import';
    end if;
    select pg_catalog.count(*) into seen from public.nxt_read_company_books('acme');
    if seen <> 4 then raise exception 'assigned company read returned % rows', seen; end if;
    select pg_catalog.count(*) into seen from public.nxt_read_company_books('bravo');
    if seen <> 0 then raise exception 'assigned staff read of another company returned rows'; end if;
    select pg_catalog.count(*) into seen from public.nxt_read_company_books('acme');
    if seen <> 4 then raise exception 'second assigned read failed'; end if;
    if (select pg_catalog.count(*) from public.company_access_audit
         where actor = 'staff-assigned' and actor_role = 'authenticated' and company_id = 'acme'
           and action = 'read' and outcome = 'allowed' and source = 'company_books_read') <> 2 then
        raise exception 'assigned reads were not appended to the audit';
    end if;
    if not exists (
        select 1 from public.company_access_audit
         where actor = 'staff-assigned' and actor_role = 'authenticated' and company_id = 'bravo'
           and action = 'read' and outcome = 'denied' and source = 'company_books_read'
           and occurred_at is not null
    ) then
        raise exception 'denied company read was not audited';
    end if;
    if exists (select 1 from public.company_access_audit where actor = 'service_role') then
        raise exception 'staff can see another principal audit row';
    end if;
    begin
        insert into public.staff_company_assignments (staff_id, company_id, assigned_by)
        values ('staff-assigned', 'bravo', 'staff-assigned');
        raise exception 'staff self-assignment succeeded';
    exception when insufficient_privilege then null;
    end;
    begin
        perform public.nxt_store_command('company_access_test', '["GET","nxt:company-books:acme:head:v1"]');
        raise exception 'authenticated executed the store RPC';
    exception when insufficient_privilege then null;
    end;
end;
$assigned$;
reset role;

set local role authenticated;
select pg_catalog.set_config('nxt.staff_id', 'staff-other', true);
do $unassigned$
declare
    seen integer;
begin
    select pg_catalog.count(*) into seen from public.client_companies;
    if seen <> 0 then raise exception 'unassigned staff saw % companies', seen; end if;
    select pg_catalog.count(*) into seen from public.company_book_imports;
    if seen <> 0 then raise exception 'unassigned staff saw % imports', seen; end if;
    select pg_catalog.count(*) into seen from public.nxt_read_company_books('acme');
    if seen <> 0 then raise exception 'unassigned staff read returned rows'; end if;
    select pg_catalog.count(*) into seen from public.nxt_read_company_books('bravo');
    if seen <> 0 then raise exception 'unassigned staff read of the other company returned rows'; end if;
    if (select pg_catalog.count(*) from public.company_access_audit
         where actor = 'staff-other' and actor_role = 'authenticated' and outcome = 'denied'
           and source = 'company_books_read' and company_id in ('acme', 'bravo')) <> 2 then
        raise exception 'unassigned denials were not audited';
    end if;
end;
$unassigned$;
reset role;

set local role authenticated;
select pg_catalog.set_config('request.jwt.claim.sub', 'staff-assigned', true);
select pg_catalog.set_config('nxt.staff_id', 'staff-other', true);
do $jwt_assigned$
declare
    seen integer;
begin
    if public.nxt_current_staff_id() <> 'staff-assigned' then
        raise exception 'JWT subject did not win over nxt.staff_id';
    end if;
    select pg_catalog.count(*) into seen from public.client_companies;
    if seen <> 1 then raise exception 'JWT assigned staff saw % companies', seen; end if;
    if exists (select 1 from public.client_companies where company_id <> 'acme') then
        raise exception 'JWT assigned staff saw another company';
    end if;
end;
$jwt_assigned$;
reset role;

set local role authenticated;
select pg_catalog.set_config('request.jwt.claim.sub', 'staff-other', true);
select pg_catalog.set_config('nxt.staff_id', 'staff-assigned', true);
do $jwt_denied$
declare
    seen integer;
begin
    if public.nxt_current_staff_id() <> 'staff-other' then
        raise exception 'unassigned JWT subject was replaced by nxt.staff_id';
    end if;
    select pg_catalog.count(*) into seen from public.client_companies;
    if seen <> 0 then raise exception 'spoofed nxt.staff_id saw % companies', seen; end if;
    select pg_catalog.count(*) into seen from public.company_book_imports;
    if seen <> 0 then raise exception 'spoofed nxt.staff_id saw % imports', seen; end if;
end;
$jwt_denied$;
reset role;
select pg_catalog.set_config('request.jwt.claim.sub', '', true);

set local role nxt_company_admin;
select pg_catalog.set_config('nxt.staff_id', '', true);
do $admin$
declare
    seen integer;
begin
    select pg_catalog.count(*) into seen from public.client_companies;
    if seen <> 2 then raise exception 'admin role saw % companies', seen; end if;
    select pg_catalog.count(*) into seen from public.company_book_imports;
    if seen <> 5 then raise exception 'admin role saw % imports', seen; end if;
    select pg_catalog.count(*) into seen from public.nxt_read_company_books('bravo');
    if seen <> 1 then raise exception 'admin role read of bravo returned % rows', seen; end if;
    select pg_catalog.count(*) into seen from public.nxt_read_company_books('acme');
    if seen <> 4 then raise exception 'admin role read of acme returned % rows', seen; end if;
    if not exists (
        select 1 from public.company_access_audit
         where actor = 'nxt_company_admin' and actor_role = 'nxt_company_admin'
           and company_id = 'bravo' and outcome = 'allowed' and source = 'company_books_read'
    ) then
        raise exception 'admin role read was not audited';
    end if;
    if (select pg_catalog.count(*) from public.company_access_audit
         where source = 'service_role_store' and company_id = 'acme') <> 8 then
        raise exception 'admin role cannot see the service role audit';
    end if;
end;
$admin$;
reset role;

update public.staff_principals set is_admin = true where staff_id = 'staff-other';
set local role authenticated;
select pg_catalog.set_config('nxt.staff_id', 'staff-other', true);
do $admin_flag$
declare
    seen integer;
begin
    if not public.nxt_is_company_admin() then
        raise exception 'is_admin principal was not treated as an admin';
    end if;
    select pg_catalog.count(*) into seen from public.client_companies;
    if seen <> 2 then raise exception 'admin principal saw % companies', seen; end if;
    select pg_catalog.count(*) into seen from public.company_book_imports;
    if seen <> 5 then raise exception 'admin principal saw % imports', seen; end if;
end;
$admin_flag$;
reset role;

do $append$
declare
    before_count bigint;
begin
    select pg_catalog.count(*) into before_count from public.company_access_audit;
    if before_count < 1 then raise exception 'audit log is empty'; end if;
    begin
        update public.company_access_audit set action = 'tamper' where company_id = 'acme';
        raise exception 'audit update succeeded';
    exception when insufficient_privilege then null;
    end;
    begin
        delete from public.company_access_audit where company_id = 'acme';
        raise exception 'audit delete succeeded';
    exception when insufficient_privilege then null;
    end;
    begin
        truncate public.company_access_audit;
        raise exception 'audit truncate succeeded';
    exception when insufficient_privilege then null;
    end;
    if (select pg_catalog.count(*) from public.company_access_audit) <> before_count then
        raise exception 'append-only audit changed row count';
    end if;
end;
$append$;

set local role anon;
do $anon$
begin
    begin
        perform 1 from public.client_companies;
        raise exception 'anon read client companies';
    exception when insufficient_privilege then null;
    end;
    begin
        perform public.nxt_read_company_books('acme');
        raise exception 'anon read company books';
    exception when insufficient_privilege then null;
    end;
    begin
        perform public.nxt_store_command('company_access_test', '["GET","nxt:company-books:acme:head:v1"]');
        raise exception 'anon executed the store RPC';
    exception when insufficient_privilege then null;
    end;
end;
$anon$;
reset role;

rollback;
