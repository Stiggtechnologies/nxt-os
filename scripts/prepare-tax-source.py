#!/usr/bin/env python3
"""Offline opaque evidence preparation and verification; never a tax importer.

prepare --source FILE --out NEW_DIRECTORY --alias opaque-001
verify --pack DIRECTORY [--expected-sha256 SHA256]

One explicit regular file is streamed into deterministic base64 JSON objects. Raw
chunks are <=256 KiB, objects <=360 KiB, the manifest <=128 KiB, and the artifact
<=192 MiB / 768 chunks. The manifest is written last; its existence alone is not
acceptance: strict streaming verification must pass. Empty artifacts are valid
opaque evidence and never imply zero clients, years or families.

Output protection is checked: POSIX owner-only modes, or a Windows protected
directory DACL allowing only the current account and SYSTEM, with private child
ACLs. All existing path ancestors are checked for links/reparse points. Reads
bind path metadata to an open regular-file descriptor and recheck identity,
size, mtime and API-local ctime; preparation also rereads the same source handle.
These are stability checks, not a filesystem snapshot or an adversarial race
proof against a malicious same-account process changing ancestor paths. Use a
quiescent input and trusted parent directories. No source is modified or run.

NXT deterministic JSON uses UTF-8, recursively sorted object keys, compact
separators, unescaped Unicode and no nonfinite numbers. This is an original
encoding contract, not an RFC 8785/JCS or Unicode-normalization claim.
Portable manifests contain an explicit opaque alias, never an automatic source
basename/path or source timestamps. Optional operator metadata is restricted to
note: absent, null and empty text remain distinct. Do not put private identifiers
in aliases/notes. Hashes detect corruption; they are not signatures or proof of
source authenticity. Pin an expected hash independently when one is available.
No network, credentials, vendor code, decoder, upload or admission is involved.
"""
import argparse
import base64
import binascii
from contextlib import contextmanager
import hashlib
import json
import math
import os
from pathlib import Path
import re
import stat
import sys

sys.dont_write_bytecode = True
CHUNK_BYTES = 256 * 1024
MAX_OBJECT_BYTES = 360 * 1024
MAX_MANIFEST_BYTES = 128 * 1024
MAX_ARTIFACT_BYTES = 192 * 1024 * 1024
MAX_CHUNKS = MAX_ARTIFACT_BYTES // CHUNK_BYTES
MAX_METADATA_BYTES = 4096
FORMAT = 'nxt-tax-source-opaque/v1'
CHUNK_FORMAT = 'nxt-tax-source-opaque-chunk/v1'
MISSING = object()


class PreservationError(ValueError):
    pass


def require(condition, code):
    if not condition:
        raise PreservationError(code)


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def encoded(value):
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(',', ':'), allow_nan=False).encode('utf-8')


def digest(value):
    return isinstance(value, str) and re.fullmatch(r'[a-f0-9]{64}', value) is not None


def alias(value):
    require(isinstance(value, str) and re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9_-]{0,63}', value) is not None, 'invalid_opaque_artifact_alias')
    return value


def exact(value, required, optional=()):
    require(isinstance(value, dict) and set(required) <= value.keys() and value.keys() <= set(required) | set(optional), 'unknown_or_missing_object_fields')


def integer(value, minimum, maximum):
    return type(value) is int and minimum <= value <= maximum


def validate_metadata(value):
    exact(value, (), ('note',))
    if 'note' in value:
        require(value['note'] is None or isinstance(value['note'], str), 'invalid_operator_note')
    require(len(encoded(value)) <= MAX_METADATA_BYTES, 'operator_metadata_byte_limit')


def absolute(path):
    return Path(os.path.abspath(os.fspath(path)))


def native(path):
    value = str(absolute(path))
    if os.name == 'nt' and not value.startswith('\\\\?\\'):
        value = '\\\\?\\UNC\\' + value[2:] if value.startswith('\\\\') else '\\\\?\\' + value
    return Path(value)


def no_reparse(info):
    return not stat.S_ISLNK(info.st_mode) and not (getattr(info, 'st_file_attributes', 0) & 0x400) and not getattr(info, 'st_reparse_tag', 0)


def ancestry(path):
    """Inspect explicit existing ancestors; never enumerate source directories."""
    result = []
    path = absolute(path)
    for item in reversed((path, *path.parents)):
        info = native(item).lstat()
        require(no_reparse(info) and stat.S_ISDIR(info.st_mode), 'unsafe_path_ancestor')
        result.append((str(item), info.st_dev, info.st_ino, stat.S_IFMT(info.st_mode)))
    return result


def common(info):
    return info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns


@contextmanager
def regular(path):
    path = absolute(path)
    parents = ancestry(path.parent)
    before = native(path).lstat()
    require(no_reparse(before) and stat.S_ISREG(before.st_mode), 'source_or_member_is_not_regular')
    flags = os.O_RDONLY | getattr(os, 'O_BINARY', 0) | getattr(os, 'O_NOFOLLOW', 0)
    descriptor = os.open(native(path), flags)
    with os.fdopen(descriptor, 'rb') as stream:
        opened = os.fstat(stream.fileno())
        require(no_reparse(opened) and stat.S_ISREG(opened.st_mode) and common(before) == common(opened), 'file_changed_before_read')

        def stable():
            current = native(path).lstat()
            after = os.fstat(stream.fileno())
            require(no_reparse(current) and no_reparse(after) and stat.S_ISREG(current.st_mode)
                    and common(before) == common(opened) == common(after) == common(current)
                    and before.st_ctime_ns == current.st_ctime_ns
                    and opened.st_ctime_ns == after.st_ctime_ns
                    and parents == ancestry(path.parent), 'file_changed_during_read')

        yield stream, opened, stable
        stable()


def windows_api():
    import ctypes as c
    from ctypes import wintypes as w
    advapi = c.WinDLL('advapi32', use_last_error=True)
    kernel = c.WinDLL('kernel32', use_last_error=True)
    kernel.GetCurrentProcess.restype = w.HANDLE
    kernel.CloseHandle.argtypes = [w.HANDLE]
    kernel.LocalFree.argtypes = [c.c_void_p]
    advapi.OpenProcessToken.argtypes = [w.HANDLE, w.DWORD, c.POINTER(w.HANDLE)]
    advapi.GetTokenInformation.argtypes = [w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD)]
    advapi.ConvertSidToStringSidW.argtypes = [c.c_void_p, c.POINTER(w.LPWSTR)]
    advapi.ConvertStringSecurityDescriptorToSecurityDescriptorW.argtypes = [w.LPCWSTR, w.DWORD, c.POINTER(c.c_void_p), c.POINTER(w.DWORD)]
    advapi.SetFileSecurityW.argtypes = [w.LPCWSTR, w.DWORD, c.c_void_p]
    advapi.GetNamedSecurityInfoW.argtypes = [w.LPWSTR, c.c_int, w.DWORD, c.POINTER(c.c_void_p), c.c_void_p, c.POINTER(c.c_void_p), c.c_void_p, c.POINTER(c.c_void_p)]
    advapi.GetSecurityDescriptorControl.argtypes = [c.c_void_p, c.POINTER(w.WORD), c.POINTER(w.DWORD)]
    advapi.ConvertSecurityDescriptorToStringSecurityDescriptorW.argtypes = [c.c_void_p, w.DWORD, w.DWORD, c.POINTER(w.LPWSTR), c.POINTER(w.DWORD)]
    return c, w, advapi, kernel


def windows_sid():
    c, w, advapi, kernel = windows_api()
    token, size, text = w.HANDLE(), w.DWORD(), w.LPWSTR()
    require(advapi.OpenProcessToken(kernel.GetCurrentProcess(), 8, c.byref(token)), 'cannot_verify_current_windows_account')
    try:
        advapi.GetTokenInformation(token, 1, None, 0, c.byref(size))
        require(0 < size.value <= 65536, 'invalid_windows_token_metadata')
        buffer = c.create_string_buffer(size.value)
        require(advapi.GetTokenInformation(token, 1, buffer, size, c.byref(size)), 'cannot_read_current_windows_account')
        sid = c.cast(buffer, c.POINTER(c.c_void_p))[0]
        require(advapi.ConvertSidToStringSidW(sid, c.byref(text)), 'cannot_identify_windows_account')
        return text.value
    finally:
        if text:
            kernel.LocalFree(c.cast(text, c.c_void_p))
        kernel.CloseHandle(token)


def protect_directory(path):
    if os.name == 'nt':
        c, w, advapi, kernel = windows_api()
        descriptor = c.c_void_p()
        sddl = 'D:P(A;OICI;FA;;;' + windows_sid() + ')(A;OICI;FA;;;SY)'
        require(advapi.ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, 1, c.byref(descriptor), None), 'cannot_build_private_directory_acl')
        try:
            require(advapi.SetFileSecurityW(str(native(path)), 0x80000004, descriptor), 'cannot_protect_output_directory')
        finally:
            kernel.LocalFree(descriptor)
    else:
        os.chmod(path, 0o700)
    check_private(path, protected_root=True)


def check_private(path, protected_root=False):
    info = native(path).lstat()
    require(no_reparse(info), 'unsafe_output_reparse_point')
    if os.name != 'nt':
        require(info.st_uid == os.geteuid() and stat.S_IMODE(info.st_mode) & 0o077 == 0, 'output_is_not_owner_private')
        return
    c, w, advapi, kernel = windows_api()
    owner, dacl, descriptor, owner_text, sddl = c.c_void_p(), c.c_void_p(), c.c_void_p(), w.LPWSTR(), w.LPWSTR()
    require(advapi.GetNamedSecurityInfoW(str(native(path)), 1, 5, c.byref(owner), None, c.byref(dacl), None, c.byref(descriptor)) == 0, 'cannot_verify_output_acl')
    try:
        require(owner and dacl and advapi.ConvertSidToStringSidW(owner, c.byref(owner_text)), 'cannot_verify_output_owner')
        current_sid = windows_sid()
        require(owner_text.value == current_sid, 'output_owner_is_not_current_account')
        control, revision = w.WORD(), w.DWORD()
        require(advapi.GetSecurityDescriptorControl(descriptor, c.byref(control), c.byref(revision)), 'cannot_verify_acl_control')
        require(not protected_root or control.value & 0x1000, 'output_root_acl_inheritance_is_not_protected')
        require(advapi.ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, 1, 4, c.byref(sddl), None), 'cannot_verify_acl_entries')
        entries = re.findall(r'\(([^()]*)\)', sddl.value)
        require(1 <= len(entries) <= 2, 'unexpected_output_acl_entries')
        trustees = []
        for entry in entries:
            fields = entry.split(';')
            require(len(fields) == 6 and fields[0] == 'A' and re.fullmatch(r'(?:OI|CI|ID)*', fields[1]) is not None
                    and fields[2] in ('FA', '0x1f01ff') and fields[3:5] == ['', '']
                    and fields[5] in (current_sid, 'SY'), 'output_acl_is_not_private')
            trustees.append(fields[5])
        require(current_sid in trustees or (current_sid == 'S-1-5-18' and 'SY' in trustees), 'output_acl_missing_current_account')
    finally:
        if sddl:
            kernel.LocalFree(c.cast(sddl, c.c_void_p))
        if owner_text:
            kernel.LocalFree(c.cast(owner_text, c.c_void_p))
        kernel.LocalFree(descriptor)


def write_new(path, raw):
    descriptor = os.open(native(path), os.O_WRONLY | os.O_CREAT | os.O_EXCL | getattr(os, 'O_BINARY', 0), 0o600)
    with os.fdopen(descriptor, 'wb') as target:
        target.write(raw)
        target.flush()
        os.fsync(target.fileno())
    check_private(path)


def unique_fields(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate_json_field')
        result[key] = value
    return result


def read_json(path, maximum):
    check_private(path)
    with regular(path) as (source, info, stable):
        require(0 < info.st_size <= maximum, 'encoded_object_byte_limit')
        raw = source.read(maximum + 1)
        require(len(raw) == info.st_size and len(raw) <= maximum, 'encoded_object_changed_or_oversized')
        stable()
    try:
        value = json.loads(raw.decode('utf-8'), object_pairs_hook=unique_fields,
                           parse_constant=lambda _: require(False, 'nonfinite_json_value'))
        require(encoded(value) == raw, 'noncanonical_or_changed_json_object')
        return value, raw
    except (UnicodeError, RecursionError, json.JSONDecodeError) as exc:
        raise PreservationError('invalid_bounded_json_object') from exc


def chunk_name(reference):
    return f"{reference['index']:06d}-{reference['sha256']}.json"


def closed_members(path, expected, maximum):
    ancestry(path)
    found = set()
    with os.scandir(native(path)) as entries:
        for entry in entries:
            require(len(found) < maximum and entry.name in expected and entry.name not in found, 'unexpected_or_extra_pack_member')
            found.add(entry.name)
    require(found == expected, 'missing_pack_member')


def validate_manifest(manifest):
    exact(manifest, ('format', 'artifact_alias', 'status', 'decoder_verified', 'tax_cases_imported', 'coverage', 'artifact', 'chunk_size_bytes', 'chunks'), ('metadata',))
    require(manifest['format'] == FORMAT and manifest['status'] == 'opaque_preservation_only'
            and manifest['decoder_verified'] is False and type(manifest['tax_cases_imported']) is int
            and manifest['tax_cases_imported'] == 0, 'not_opaque_preservation_only')
    alias(manifest['artifact_alias'])
    exact(manifest['coverage'], ('clients', 'years', 'families'))
    require(all(value is None for value in manifest['coverage'].values()), 'source_coverage_must_remain_unknown')
    if 'metadata' in manifest:
        validate_metadata(manifest['metadata'])
    exact(manifest['artifact'], ('bytes', 'sha256'))
    total = manifest['artifact']['bytes']
    require(integer(total, 0, MAX_ARTIFACT_BYTES) and digest(manifest['artifact']['sha256']), 'invalid_artifact_size_or_hash')
    require(type(manifest['chunk_size_bytes']) is int and manifest['chunk_size_bytes'] == CHUNK_BYTES, 'unsupported_chunk_size')
    references = manifest['chunks']
    require(isinstance(references, list) and len(references) <= MAX_CHUNKS
            and len(references) == math.ceil(total / CHUNK_BYTES), 'invalid_chunk_count')
    offset = 0
    for index, reference in enumerate(references):
        exact(reference, ('index', 'offset', 'bytes', 'sha256'))
        require(type(reference['index']) is int and reference['index'] == index
                and type(reference['offset']) is int and reference['offset'] == offset
                and type(reference['bytes']) is int and reference['bytes'] == min(CHUNK_BYTES, total - offset)
                and digest(reference['sha256']), 'invalid_chunk_order_offset_size_or_hash')
        offset += reference['bytes']
    require(offset == total and len(encoded(manifest)) <= MAX_MANIFEST_BYTES, 'artifact_or_manifest_byte_limit')


def verify_chunks(pack, manifest, published):
    validate_manifest(manifest)
    check_private(pack, protected_root=True)
    closed_members(pack, {'chunks', 'manifest.json'} if published else {'chunks'}, 2)
    chunk_dir = pack / 'chunks'
    check_private(chunk_dir)
    closed_members(chunk_dir, {chunk_name(ref) for ref in manifest['chunks']}, MAX_CHUNKS)
    whole, count = hashlib.sha256(), 0
    for reference in manifest['chunks']:
        obj, _ = read_json(chunk_dir / chunk_name(reference), MAX_OBJECT_BYTES)
        exact(obj, ('format', 'artifact_alias', 'index', 'offset', 'bytes', 'sha256', 'content_base64'))
        require(obj['format'] == CHUNK_FORMAT and obj['artifact_alias'] == manifest['artifact_alias'], 'chunk_artifact_binding_mismatch')
        for key in ('index', 'offset', 'bytes', 'sha256'):
            require(type(obj[key]) is type(reference[key]) and obj[key] == reference[key], 'chunk_reference_binding_mismatch')
        text = obj['content_base64']
        require(isinstance(text, str) and len(text) == 4 * math.ceil(reference['bytes'] / 3), 'base64_byte_limit_or_length_mismatch')
        try:
            raw = base64.b64decode(text, validate=True)
        except (ValueError, binascii.Error) as exc:
            raise PreservationError('invalid_base64_content') from exc
        require(len(raw) == reference['bytes'] and len(raw) <= CHUNK_BYTES
                and base64.b64encode(raw).decode('ascii') == text and sha(raw) == reference['sha256'], 'chunk_content_integrity_failed')
        whole.update(raw)
        count += len(raw)
    require(count == manifest['artifact']['bytes'] and whole.hexdigest() == manifest['artifact']['sha256'], 'whole_artifact_integrity_failed')


def verify(pack, expected_sha256=None, expected_manifest_sha256=None):
    pack = absolute(pack)
    ancestry(pack)
    manifest, raw = read_json(pack / 'manifest.json', MAX_MANIFEST_BYTES)
    validate_manifest(manifest)
    if expected_sha256 is not None:
        require(digest(expected_sha256) and manifest.get('artifact', {}).get('sha256') == expected_sha256, 'expected_artifact_hash_mismatch')
    if expected_manifest_sha256 is not None:
        require(digest(expected_manifest_sha256) and sha(raw) == expected_manifest_sha256, 'expected_manifest_hash_mismatch')
    verify_chunks(pack, manifest, published=True)
    return manifest


def prepare(source, output, artifact_alias, metadata=MISSING, expected_sha256=None):
    artifact_alias = alias(artifact_alias)
    if metadata is not MISSING:
        validate_metadata(metadata)
    if expected_sha256 is not None:
        require(digest(expected_sha256), 'invalid_expected_artifact_hash')
    source, output = absolute(source), absolute(output)
    require(source != output and not source.is_relative_to(output) and not output.is_relative_to(source), 'source_output_overlap')
    ancestry(output.parent)
    require(not os.path.lexists(native(output)), 'fresh_output_directory_required')
    with regular(source) as (stream, before, stable):
        require(0 <= before.st_size <= MAX_ARTIFACT_BYTES, 'artifact_byte_limit')
        native(output).mkdir(mode=0o700)
        protect_directory(output)
        chunk_dir = output / 'chunks'
        native(chunk_dir).mkdir(mode=0o700)
        # Windows Python may install extra explicit ACEs for mkdir(0o700);
        # replace them with the same verified private DACL as the pack root.
        protect_directory(chunk_dir)
        whole, references, offset = hashlib.sha256(), [], 0
        while True:
            raw = stream.read(CHUNK_BYTES)
            if not raw:
                break
            require(len(references) < MAX_CHUNKS and offset + len(raw) <= before.st_size, 'source_grew_or_chunk_budget_exceeded')
            reference = {'index': len(references), 'offset': offset, 'bytes': len(raw), 'sha256': sha(raw)}
            obj = {'format': CHUNK_FORMAT, 'artifact_alias': artifact_alias, **reference, 'content_base64': base64.b64encode(raw).decode('ascii')}
            payload = encoded(obj)
            require(len(payload) <= MAX_OBJECT_BYTES, 'chunk_object_byte_limit')
            write_new(chunk_dir / chunk_name(reference), payload)
            references.append(reference)
            whole.update(raw)
            offset += len(raw)
        require(offset == before.st_size, 'source_truncated_during_read')
        stable()
        manifest = {'format': FORMAT, 'artifact_alias': artifact_alias, 'status': 'opaque_preservation_only', 'decoder_verified': False, 'tax_cases_imported': 0, 'coverage': {'clients': None, 'years': None, 'families': None}, 'artifact': {'bytes': offset, 'sha256': whole.hexdigest()}, 'chunk_size_bytes': CHUNK_BYTES, 'chunks': references}
        if metadata is not MISSING:
            manifest['metadata'] = metadata
        validate_manifest(manifest)
        if expected_sha256 is not None:
            require(manifest['artifact']['sha256'] == expected_sha256, 'expected_artifact_hash_mismatch')
        verify_chunks(output, manifest, published=False)
        stream.seek(0)
        reread, reread_bytes = hashlib.sha256(), 0
        while True:
            raw = stream.read(CHUNK_BYTES)
            if not raw:
                break
            reread_bytes += len(raw)
            require(reread_bytes <= before.st_size, 'source_changed_during_reread')
            reread.update(raw)
        require(reread_bytes == offset and reread.hexdigest() == manifest['artifact']['sha256'], 'source_changed_during_reread')
        stable()
    # The context's final source/ancestor check must pass before the completion
    # marker is created. No check that can fail is deferred until after it.
    write_new(output / 'manifest.json', encoded(manifest))
    return manifest


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    create = sub.add_parser('prepare')
    create.add_argument('--source', type=Path, required=True)
    create.add_argument('--out', type=Path, required=True)
    create.add_argument('--alias', required=True)
    create.add_argument('--note', default=MISSING)
    create.add_argument('--expected-sha256')
    check = sub.add_parser('verify')
    check.add_argument('--pack', type=Path, required=True)
    check.add_argument('--expected-sha256')
    check.add_argument('--expected-manifest-sha256')
    args = parser.parse_args(argv)
    try:
        if args.command == 'prepare':
            metadata = MISSING if args.note is MISSING else {'note': args.note}
            manifest = prepare(args.source, args.out, args.alias, metadata, args.expected_sha256)
        else:
            manifest = verify(args.pack, args.expected_sha256, args.expected_manifest_sha256)
        print(json.dumps({'status': manifest['status'], 'verified_exact_reconstruction': True, 'artifact_bytes': manifest['artifact']['bytes'], 'artifact_sha256': manifest['artifact']['sha256'], 'manifest_sha256': sha(encoded(manifest)), 'chunks': len(manifest['chunks']), 'decoder_verified': False, 'tax_cases_imported': 0, 'coverage': manifest['coverage']}))
        return 0
    except (PreservationError, OSError, ValueError, TypeError, KeyError, RecursionError) as exc:
        print('Opaque preservation rejected: ' + (str(exc) if isinstance(exc, PreservationError) else type(exc).__name__), file=sys.stderr)
        return 2


if __name__ == '__main__':
    raise SystemExit(main())
