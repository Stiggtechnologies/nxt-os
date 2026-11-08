-- Source preservation counters only. No client rows, indexes, roles or settings change.
begin;

create or replace function public.nxt_source_set_counts(p_namespace text, p_keys jsonb)
returns jsonb
language plpgsql
stable
security definer
set search_path = ''
as $counts$
declare
    item jsonb;
    answer jsonb;
begin
    if p_namespace is null or p_namespace !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$'
       or pg_catalog.jsonb_typeof(p_keys) is distinct from 'array'
       or pg_catalog.jsonb_array_length(p_keys) not between 1 and 16 then
        raise exception using errcode = '22023', message = 'invalid source counter namespace or key count';
    end if;
    for item in select value from pg_catalog.jsonb_array_elements(p_keys) loop
        if pg_catalog.jsonb_typeof(item) <> 'string'
           or pg_catalog.octet_length(item #>> '{}') > 512
           or (item #>> '{}') !~ '^nxt:dtmaxtc:[A-Za-z0-9][A-Za-z0-9_.:-]{0,79}:(family_files|family_member_links|historical_lines|historical_lines_pending|afr_occurrences|afr_contents|afr_missing_year)$' then
            raise exception using errcode = '22023', message = 'counter key is outside preserved source indexes';
        end if;
    end loop;
    if exists(select 1 from nxt_private.kv_values v
              where v.namespace = p_namespace and v.key in (select pg_catalog.jsonb_array_elements_text(p_keys)))
       or exists(select 1 from nxt_private.list_values l
                 where l.namespace = p_namespace and l.key in (select pg_catalog.jsonb_array_elements_text(p_keys))) then
        raise exception using errcode = '22023', message = 'WRONGTYPE source counter key is not a set';
    end if;
    -- One database snapshot; retain order, missing sets and repeated requested keys.
    select pg_catalog.jsonb_agg(
        (select count(*) from nxt_private.set_members s
         where s.namespace = p_namespace and s.key = wanted.key)
        order by wanted.ordinality)
      into answer
      from pg_catalog.jsonb_array_elements_text(p_keys) with ordinality as wanted(key, ordinality);
    return pg_catalog.jsonb_build_object('result', answer);
end;
$counts$;

revoke all on function public.nxt_source_set_counts(text, jsonb) from public, anon, authenticated;
grant execute on function public.nxt_source_set_counts(text, jsonb) to service_role;

-- Refresh function metadata in place: https://supabase.com/kb/troubleshooting/refresh-postgrest-schema
notify pgrst, 'reload schema';
commit;
