# Windows-only, non-production Store recovery rehearsal. This does not restore a
# whole Supabase project, object storage, permissions, secrets or deployment config.
# ExecuteQuery must execute the supplied SQL against the explicitly chosen project
# and return its result rows. It must not print SQL, credentials or record values.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][scriptblock]$ExecuteQuery,
    [Parameter(Mandatory)][ValidatePattern('^verification:[A-Za-z0-9_.:-]{1,48}$')][string]$SourceNamespace,
    [Parameter(Mandatory)][string]$ProjectRef,
    [Parameter(Mandatory)][string]$OutputDirectory
)
$ErrorActionPreference = 'Stop'
if ($env:OS -ne 'Windows_NT') { throw 'DPAPI encryption requires Windows.' }
if ($ProjectRef -notmatch '^[a-z0-9]{20}$') { throw 'Invalid project reference.' }
$runId = [DateTime]::UtcNow.ToString('yyyyMMddHHmmss') + '-' + [Guid]::NewGuid().ToString('N').Substring(0,8)
$targetNamespace = 'verification:restore-' + $runId
$directory = Join-Path ([IO.Path]::GetFullPath($OutputDirectory)) $runId
if (Test-Path -LiteralPath $directory) { throw 'Evidence destination must be new.' }
[void](New-Item -ItemType Directory -Path $directory)
$backupPath = Join-Path $directory 'namespace-backup.dpapi.xml'

function Get-NxtSnapshot([string]$Namespace) {
    # All three collections are read in ONE statement and therefore one MVCC snapshot.
    $sql = @"
select jsonb_build_object(
  'format', 'nxt-store-namespace/v1',
  'kv', coalesce((select jsonb_agg(jsonb_build_object('key',key,'value',value,'updated_at',updated_at) order by key collate "C") from nxt_private.kv_values where namespace='$Namespace'),'[]'::jsonb),
  'sets', coalesce((select jsonb_agg(jsonb_build_object('key',key,'member',member,'created_at',created_at) order by key collate "C",member collate "C") from nxt_private.set_members where namespace='$Namespace'),'[]'::jsonb),
  'lists', coalesce((select jsonb_agg(jsonb_build_object('key',key,'position',position,'value',value,'created_at',created_at) order by key collate "C",position) from nxt_private.list_values where namespace='$Namespace'),'[]'::jsonb)
)::text as snapshot;
"@
    $rows = @(& $ExecuteQuery $sql)
    if ($rows.Count -ne 1 -or -not $rows[0].snapshot) { throw 'Snapshot query did not return exactly one document.' }
    return [string]$rows[0].snapshot
}
function Get-NxtDigest([string]$Text) {
    $sha = [Security.Cryptography.SHA256]::Create()
    try { return ([BitConverter]::ToString($sha.ComputeHash([Text.Encoding]::UTF8.GetBytes($Text)))).Replace('-','').ToLowerInvariant() }
    finally { $sha.Dispose() }
}
$snapshot = Get-NxtSnapshot $SourceNamespace
$data = $snapshot | ConvertFrom-Json
if ($data.format -ne 'nxt-store-namespace/v1') { throw 'Unsupported snapshot format.' }
if ((@($data.kv).Count + @($data.sets).Count + @($data.lists).Count) -eq 0) { throw 'Empty source is not a meaningful recovery test.' }
$digest = Get-NxtDigest $snapshot
$secureSnapshot = ConvertTo-SecureString -String $snapshot -AsPlainText -Force
$secureSnapshot | Export-Clixml -LiteralPath $backupPath
$secureSnapshot = $null
$restoredSnapshot = [Net.NetworkCredential]::new('', (Import-Clixml -LiteralPath $backupPath)).Password
if ((Get-NxtDigest $restoredSnapshot) -ne $digest) { throw 'Decrypted backup checksum does not match source.' }
$encoded = [Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($restoredSnapshot))
# The target is generated here, restricted to verification:, and must be empty.
# No UPDATE/DELETE/TRUNCATE, source writes, production restore or overwrite exists.
$restoreSql = @"
do `$recovery`$
declare payload jsonb := convert_from(decode('$encoded','base64'),'UTF8')::jsonb;
begin
  perform pg_advisory_xact_lock(hashtextextended('$targetNamespace',71924));
  if exists(select 1 from nxt_private.kv_values where namespace='$targetNamespace')
     or exists(select 1 from nxt_private.set_members where namespace='$targetNamespace')
     or exists(select 1 from nxt_private.list_values where namespace='$targetNamespace') then
    raise exception 'Recovery target must be empty';
  end if;
  insert into nxt_private.kv_values(namespace,key,value,updated_at)
    select '$targetNamespace',r.key,r.value,r.updated_at from jsonb_to_recordset(payload->'kv') as r(key text,value text,updated_at timestamptz);
  insert into nxt_private.set_members(namespace,key,member,created_at)
    select '$targetNamespace',r.key,r.member,r.created_at from jsonb_to_recordset(payload->'sets') as r(key text,member text,created_at timestamptz);
  insert into nxt_private.list_values(namespace,key,position,value,created_at) overriding system value
    select '$targetNamespace',r.key,r.position,r.value,r.created_at from jsonb_to_recordset(payload->'lists') as r(key text,position bigint,value text,created_at timestamptz);
end;
`$recovery`$;
"@
$timer = [Diagnostics.Stopwatch]::StartNew()
$ignored = & $ExecuteQuery $restoreSql
$timer.Stop()
$target = Get-NxtSnapshot $targetNamespace
if ((Get-NxtDigest $target) -ne $digest) { throw 'Restored data checksum mismatch.' }
$probeEncoded=[Convert]::ToBase64String([Text.Encoding]::UTF8.GetBytes($restoreSql))
$probeSql=@"
do `$probe`$
begin
  begin
    execute convert_from(decode('$probeEncoded','base64'),'UTF8');
    raise exception using errcode='ZX001',message='Recovery guard unexpectedly allowed overwrite';
  exception when sqlstate 'P0001' then
    if sqlerrm <> 'Recovery target must be empty' then raise; end if;
    perform set_config('nxt.recovery_guard_probe','expected_nonempty_target',true);
  end;
end;
`$probe`$;
select current_setting('nxt.recovery_guard_probe',true) as guard_outcome;
"@
$probeRows=@(& $ExecuteQuery $probeSql)
$guardRejected=$probeRows.Count -eq 1 -and $probeRows[0].guard_outcome -eq 'expected_nonempty_target'
if (-not $guardRejected) { throw 'Nonempty-target restore guard was not proven.' }
if ((Get-NxtDigest (Get-NxtSnapshot $targetNamespace)) -ne $digest) { throw 'Rejected replay changed the restored data.' }
$report = [ordered]@{
    format='nxt-namespace-recovery-evidence/v1'; executed_at=[DateTime]::UtcNow.ToString('o');
    project_ref=$ProjectRef; source_namespace=$SourceNamespace; target_namespace=$targetNamespace;
    source_digest_sha256=$digest; restored_digest_sha256=(Get-NxtDigest $target);
    counts=@{kv=@($data.kv).Count;set_members=@($data.sets).Count;list_values=@($data.lists).Count};
    backup_encryption='Windows DPAPI CurrentUser; local user/machine bound; not an off-site DR backup';
    backup_file='namespace-backup.dpapi.xml'; decrypted_integrity_verified=$true;
    restore_to_empty_namespace_verified=$true; nonempty_target_rejected=$guardRejected; guard_outcome='expected_nonempty_target';
    restore_request_elapsed_ms=$timer.ElapsedMilliseconds; source_modified=$false;
    production_data_migrated=$false; live_cutover_allowed=$false;
    limitations=@('Synthetic/non-production namespace rehearsal only.','ExecuteQuery must be independently bound to ProjectRef; the parameter alone does not attest the executor destination.','No whole-project restore, cross-region recovery, off-site copy or production RPO/RTO proof.','Target is an independent point-in-time copy; later source writes are not replicated.','Schema/RPC/grants/identity sequence/configuration/secrets/storage objects are outside this archive.','Keep both evidence namespaces for inspection; no cleanup or rollback of live records is implemented.')
}
$report | ConvertTo-Json -Depth 10 | Set-Content -LiteralPath (Join-Path $directory 'recovery-evidence.json') -Encoding UTF8
$snapshot=$null; $restoredSnapshot=$null; $target=$null; $encoded=$null; $restoreSql=$null; $probeSql=$null; $probeEncoded=$null; $data=$null
$report | ConvertTo-Json -Depth 10
