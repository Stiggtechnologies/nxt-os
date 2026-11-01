-- Each client's own Ledger keeps its records under nxt:company-ledger:{company}:...
-- Writes and collection reads are appended to company_access_audit. MGET rows are
-- not: a report fetches every entry after one audited collection read, and one
-- row per entry would add nothing but volume. These keys are not imports, so the
-- import catalog and client_companies are untouched. Payloads stay in nxt_private.
begin;

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
    if p_key ~ '^nxt:company-ledger:[A-Za-z0-9_.-]{1,96}:' then
        if p_action = 'MGET' then
            return;
        end if;
        v_company := (pg_catalog.regexp_match(p_key, '^nxt:company-ledger:([A-Za-z0-9_.-]{1,96}):'))[1];
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
        return;
    end if;
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

revoke all on function nxt_private.note_company_book_access(text, text, text, boolean) from public;

commit;
