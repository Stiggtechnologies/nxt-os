-- Server-only identity projection. Hash the original sealed bytes in PostgreSQL,
-- but do not transfer journal/trial-balance bodies merely to build a company menu.
begin;
create table nxt_private.company_preview_identities (
  namespace text not null,
  key text not null,
  identity jsonb not null,
  primary key(namespace,key),
  foreign key(namespace,key) references nxt_private.kv_values(namespace,key) on delete cascade on update cascade
);
alter table nxt_private.company_preview_identities enable row level security;
revoke all on table nxt_private.company_preview_identities from public,anon,authenticated,service_role;

create function nxt_private.company_preview_identity(p_value text) returns jsonb
language plpgsql immutable set search_path=''
as $payload$
declare document jsonb;
begin
  begin document:=p_value::jsonb; exception when invalid_text_representation then document:=null;end;
  return pg_catalog.jsonb_build_object(
    'sha256',pg_catalog.encode(pg_catalog.sha256(pg_catalog.convert_to(p_value,'UTF8')),'hex'),
    'bytes',pg_catalog.octet_length(p_value),'format',document->'format',
    'company_id',document->'company_id','company',document#>'{input,company}',
    'controls_passed',document#>'{analysis,controls_passed}');
end;
$payload$;
revoke all on function nxt_private.company_preview_identity(text) from public,anon,authenticated,service_role;

create function nxt_private.sync_company_preview_identity() returns trigger
language plpgsql security definer set search_path=''
as $sync$
begin
  if new.key ~ '^nxt:company-books:[A-Za-z0-9_.-]{1,96}:preview:[a-f0-9]{64}$' then
    insert into nxt_private.company_preview_identities(namespace,key,identity)
    values(new.namespace,new.key,nxt_private.company_preview_identity(new.value))
    on conflict(namespace,key) do update set identity=excluded.identity;
  elsif tg_op='UPDATE' and old.key ~ '^nxt:company-books:[A-Za-z0-9_.-]{1,96}:preview:[a-f0-9]{64}$' then
    delete from nxt_private.company_preview_identities where namespace=new.namespace and key=new.key;
  end if;
  return new;
end;
$sync$;
revoke all on function nxt_private.sync_company_preview_identity() from public,anon,authenticated,service_role;
create trigger sync_company_preview_identity after insert or update of value,key,namespace
on nxt_private.kv_values for each row execute function nxt_private.sync_company_preview_identity();

-- Derived identity rows only. Original KV values and client/tax records are untouched.
insert into nxt_private.company_preview_identities(namespace,key,identity)
select namespace,key,nxt_private.company_preview_identity(value)
from nxt_private.kv_values
where key ~ '^nxt:company-books:[A-Za-z0-9_.-]{1,96}:preview:[a-f0-9]{64}$';

create or replace function public.nxt_company_identity_previews(p_namespace text, p_keys jsonb)
returns jsonb language plpgsql security definer set search_path = ''
as $identity$
declare item jsonb; answer jsonb;
begin
  if p_namespace is null or p_namespace !~ '^[A-Za-z0-9][A-Za-z0-9_.:-]{0,63}$'
     or pg_catalog.jsonb_typeof(p_keys) is distinct from 'array'
     or pg_catalog.jsonb_array_length(p_keys) not between 1 and 100 then
    raise exception using errcode='22023', message='invalid identity projection scope';
  end if;
  for item in select value from pg_catalog.jsonb_array_elements(p_keys) loop
    if pg_catalog.jsonb_typeof(item) <> 'string'
       or (item #>> '{}') !~ '^nxt:company-books:[A-Za-z0-9_.-]{1,96}:preview:[a-f0-9]{64}$' then
      raise exception using errcode='22023', message='identity projection key is outside company previews';
    end if;
    perform nxt_private.note_company_book_access(p_namespace,item #>> '{}','MGET',false);
  end loop;
  select pg_catalog.jsonb_agg(v.identity order by wanted.ordinality)
    into answer
    from pg_catalog.jsonb_array_elements_text(p_keys) with ordinality as wanted(key,ordinality)
    left join nxt_private.company_preview_identities v on v.namespace=p_namespace and v.key=wanted.key;
  return pg_catalog.jsonb_build_object('result',answer);
end;
$identity$;
revoke all on function public.nxt_company_identity_previews(text,jsonb) from public,anon,authenticated;
grant execute on function public.nxt_company_identity_previews(text,jsonb) to service_role;
notify pgrst,'reload schema';
commit;
