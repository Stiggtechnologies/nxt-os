-- Read-only export principal. Provision its password out of band; never in Git.
-- RLS remains enabled. The user-approved backup job reads all NXT namespaces.
begin;
do $backup_role$
begin
  if not exists(select 1 from pg_roles where rolname='nxt_backup_reader') then
    create role nxt_backup_reader nologin nosuperuser nocreatedb nocreaterole noinherit noreplication nobypassrls connection limit 2;
  end if;
end;
$backup_role$;
grant usage on schema nxt_private to nxt_backup_reader;
grant select on nxt_private.kv_values,nxt_private.set_members,nxt_private.list_values to nxt_backup_reader;
grant select on sequence nxt_private.list_values_position_seq to nxt_backup_reader;
do $backup_policies$
declare table_name text;
begin
  foreach table_name in array array['kv_values','set_members','list_values'] loop
    if not exists(select 1 from pg_policies where schemaname='nxt_private' and tablename=table_name and policyname='nxt_backup_readonly') then
      execute format('create policy nxt_backup_readonly on nxt_private.%I for select to nxt_backup_reader using (true)',table_name);
    end if;
  end loop;
end;
$backup_policies$;
alter role nxt_backup_reader set default_transaction_read_only=on;
alter role nxt_backup_reader set statement_timeout='5min';
alter role nxt_backup_reader set lock_timeout='10s';
commit;
