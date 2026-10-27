"""Synthetic opaque bytes only. No real tax sources, vendor or network access."""
import base64
from contextlib import redirect_stderr, redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
from types import SimpleNamespace
import tempfile
import unittest
from unittest.mock import patch

import sys
sys.dont_write_bytecode = True
WORK = Path(__file__).resolve().parents[2]
SCRIPT = Path(__file__).resolve().parents[1] / 'scripts' / 'prepare-tax-source.py'
spec = importlib.util.spec_from_file_location('synthetic_opaque_tax_source', SCRIPT)
prep = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prep)


class OpaquePreservationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='opaque-test-', dir=WORK)
        self.home = Path(self.temporary.name)
        self.sequence = 0

    def tearDown(self):
        target = self.home.resolve()
        self.assertTrue(target.is_relative_to(WORK.resolve()))
        shutil.rmtree(prep.native(target))
        self.temporary.cleanup()

    def source(self, raw=b'\x00synthetic opaque source\xff'):
        self.sequence += 1
        path = self.home / f'PRIVATE-SOURCE-BASENAME-{self.sequence}.bin'
        prep.native(path).write_bytes(raw)
        return path

    def package(self, raw=None, **options):
        raw = raw if raw is not None else bytes(range(256)) * 1024 + b'last\x00\xff'
        source = self.source(raw)
        output = self.home / f'pack-{self.sequence}'
        manifest = prep.prepare(source, output, 'opaque-synthetic', **options)
        return source, output, manifest

    def rewrite(self, path, value):
        prep.native(path).write_bytes(prep.encoded(value))

    def test_streamed_boundaries_exact_hashes_private_files_and_no_basename_leak(self):
        raw = bytes(range(256)) * 2048 + b'last\x00\xff'
        source, pack, manifest = self.package(raw)
        expected = hashlib.sha256(raw).hexdigest()
        self.assertEqual(manifest['artifact'], {'bytes': len(raw), 'sha256': expected})
        self.assertEqual([r['offset'] for r in manifest['chunks']], [0, prep.CHUNK_BYTES, 2 * prep.CHUNK_BYTES])
        self.assertEqual([r['bytes'] for r in manifest['chunks']], [prep.CHUNK_BYTES, prep.CHUNK_BYTES, 6])
        verified = prep.verify(pack, expected, prep.sha(prep.encoded(manifest)))
        self.assertEqual(verified, manifest)
        restored = hashlib.sha256()
        for reference in manifest['chunks']:
            path = pack / 'chunks' / prep.chunk_name(reference)
            payload = prep.native(path).read_bytes()
            self.assertLessEqual(len(payload), prep.MAX_OBJECT_BYTES)
            self.assertNotIn(source.name.encode(), payload)
            obj = json.loads(payload)
            chunk = base64.b64decode(obj['content_base64'], validate=True)
            self.assertLessEqual(len(chunk), prep.CHUNK_BYTES)
            self.assertEqual(hashlib.sha256(chunk).hexdigest(), reference['sha256'])
            restored.update(chunk)
            prep.check_private(path)
        self.assertEqual(restored.hexdigest(), expected)
        self.assertEqual(prep.native(source).read_bytes(), raw)
        self.assertNotIn(source.name.encode(), prep.native(pack / 'manifest.json').read_bytes())
        prep.check_private(pack, protected_root=True)

    def test_empty_artifact_and_metadata_presence_do_not_invent_coverage(self):
        for metadata in (prep.MISSING, {}, {'note': None}, {'note': ''}):
            with self.subTest(metadata=metadata):
                _, pack, manifest = self.package(b'', metadata=metadata)
                self.assertEqual(prep.verify(pack), manifest)
                self.assertEqual(manifest['artifact']['bytes'], 0)
                self.assertIs(type(manifest['artifact']['bytes']), int)
                self.assertEqual(manifest['artifact']['sha256'], hashlib.sha256(b'').hexdigest())
                self.assertEqual(manifest['chunks'], [])
                self.assertEqual(manifest['coverage'], {'clients': None, 'years': None, 'families': None})
                self.assertIs(manifest['decoder_verified'], False)
                self.assertEqual(manifest['tax_cases_imported'], 0)
                if metadata is prep.MISSING:
                    self.assertNotIn('metadata', manifest)
                else:
                    self.assertEqual(manifest['metadata'], metadata)

    def test_existing_output_overlap_and_invalid_alias_never_overwrite(self):
        source = self.source()
        output = self.home / 'existing'
        output.mkdir()
        sentinel = output / 'sentinel'
        sentinel.write_bytes(b'keep')
        with self.assertRaises(prep.PreservationError):
            prep.prepare(source, output, 'opaque')
        self.assertEqual(sentinel.read_bytes(), b'keep')
        with self.assertRaisesRegex(prep.PreservationError, 'overlap'):
            prep.prepare(source, source, 'opaque')
        for value in ('', '../escape', 'not opaque', 'a' * 65, '.hidden', 'opaque/name'):
            with self.subTest(alias=value), self.assertRaises(prep.PreservationError):
                prep.prepare(source, self.home / 'unused', value)
        self.assertFalse((self.home / 'unused').exists())

    def test_resource_budgets_fail_without_manifest_publication(self):
        for constant, limit in (('MAX_ARTIFACT_BYTES', 8), ('MAX_OBJECT_BYTES', 32), ('MAX_MANIFEST_BYTES', 32), ('MAX_CHUNKS', 1)):
            source = self.source(b'x' * (prep.CHUNK_BYTES + 1))
            output = self.home / f'budget-{constant}'
            with self.subTest(constant=constant), patch.object(prep, constant, limit), self.assertRaises(prep.PreservationError):
                prep.prepare(source, output, 'opaque')
            self.assertFalse(prep.native(output / 'manifest.json').exists())

    def test_maximum_artifact_manifest_fits_without_materializing_large_content(self):
        manifest = {'format': prep.FORMAT, 'artifact_alias': 'a' * 64, 'status': 'opaque_preservation_only', 'decoder_verified': False, 'tax_cases_imported': 0, 'coverage': {'clients': None, 'years': None, 'families': None}, 'artifact': {'bytes': prep.MAX_ARTIFACT_BYTES, 'sha256': '0' * 64}, 'chunk_size_bytes': prep.CHUNK_BYTES, 'chunks': [{'index': index, 'offset': index * prep.CHUNK_BYTES, 'bytes': prep.CHUNK_BYTES, 'sha256': '0' * 64} for index in range(prep.MAX_CHUNKS)], 'metadata': {'note': 'x' * 4000}}
        prep.validate_manifest(manifest)
        self.assertLessEqual(len(prep.encoded(manifest)), prep.MAX_MANIFEST_BYTES)

    def test_expected_source_and_manifest_pins_are_enforced(self):
        source = self.source()
        output = self.home / 'wrong-source-pin'
        with self.assertRaisesRegex(prep.PreservationError, 'expected_artifact_hash'):
            prep.prepare(source, output, 'opaque', expected_sha256='0' * 64)
        self.assertFalse((output / 'manifest.json').exists())
        _, pack, _ = self.package()
        with self.assertRaises(prep.PreservationError):
            prep.verify(pack, expected_sha256='0' * 64)
        with self.assertRaises(prep.PreservationError):
            prep.verify(pack, expected_manifest_sha256='0' * 64)

    def test_mutating_source_cannot_publish_a_manifest(self):
        source = self.source(b'a' * (prep.CHUNK_BYTES + 9))
        output = self.home / 'unstable'
        original_write = prep.write_new
        changed = False

        def mutate(path, raw):
            nonlocal changed
            original_write(path, raw)
            if not changed:
                changed = True
                with prep.native(source).open('ab') as target:
                    target.write(b'changed')

        with patch.object(prep, 'write_new', side_effect=mutate), self.assertRaises(prep.PreservationError):
            prep.prepare(source, output, 'opaque')
        self.assertFalse((output / 'manifest.json').exists())

    def test_same_size_change_with_restored_mtime_is_detected_by_source_reread(self):
        source = self.source(b'first synthetic content')
        output = self.home / 'reread'
        before = source.stat()
        original_verify = prep.verify_chunks

        def mutate_after_chunk_verification(*args, **kwargs):
            original_verify(*args, **kwargs)
            prep.native(source).write_bytes(b'other synthetic content')
            os.utime(prep.native(source), ns=(before.st_atime_ns, before.st_mtime_ns))

        with patch.object(prep, 'verify_chunks', side_effect=mutate_after_chunk_verification), self.assertRaises(prep.PreservationError):
            prep.prepare(source, output, 'opaque')
        self.assertFalse((output / 'manifest.json').exists())

    def test_interruption_leaves_an_incomplete_pack_that_verification_rejects(self):
        source = self.source(b'x' * (prep.CHUNK_BYTES + 1))
        output = self.home / 'interrupted'
        original_write = prep.write_new
        calls = 0

        def interrupt(path, raw):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError('synthetic interrupted write')
            original_write(path, raw)

        with patch.object(prep, 'write_new', side_effect=interrupt), self.assertRaises(OSError):
            prep.prepare(source, output, 'opaque')
        self.assertFalse((output / 'manifest.json').exists())
        with self.assertRaises((OSError, prep.PreservationError)):
            prep.verify(output)

    def test_partial_manifest_is_not_an_accepted_completion_marker(self):
        source = self.source()
        output = self.home / 'partial-manifest'
        original_write = prep.write_new

        def interrupt_marker(path, raw):
            original_write(path, b'{"format":' if path.name == 'manifest.json' else raw)
            if path.name == 'manifest.json':
                raise OSError('synthetic marker interruption')

        with patch.object(prep, 'write_new', side_effect=interrupt_marker), self.assertRaises(OSError):
            prep.prepare(source, output, 'opaque')
        with self.assertRaises(prep.PreservationError):
            prep.verify(output)

    def test_changed_missing_reordered_truncated_swapped_and_extra_chunks_reject(self):
        for case in ('changed', 'missing', 'reordered', 'truncated', 'swapped', 'extra-chunk', 'extra-root'):
            with self.subTest(case=case):
                _, pack, manifest = self.package()
                paths = [pack / 'chunks' / prep.chunk_name(ref) for ref in manifest['chunks']]
                if case == 'changed':
                    obj = json.loads(prep.native(paths[0]).read_bytes())
                    obj['content_base64'] = base64.b64encode(b'z' * prep.CHUNK_BYTES).decode('ascii')
                    self.rewrite(paths[0], obj)
                elif case == 'missing':
                    prep.native(paths[0]).unlink()
                elif case == 'reordered':
                    manifest['chunks'].reverse()
                    self.rewrite(pack / 'manifest.json', manifest)
                elif case == 'truncated':
                    prep.native(paths[0]).write_bytes(prep.native(paths[0]).read_bytes()[:-1])
                elif case == 'swapped':
                    first, second = (prep.native(p).read_bytes() for p in paths)
                    prep.native(paths[0]).write_bytes(second)
                    prep.native(paths[1]).write_bytes(first)
                elif case == 'extra-chunk':
                    prep.write_new(pack / 'chunks' / 'unexpected.json', b'{}')
                else:
                    prep.write_new(pack / 'unexpected.txt', b'extra')
                with self.assertRaises((prep.PreservationError, OSError)):
                    prep.verify(pack)

    def test_strict_schema_duplicate_keys_and_false_coverage_reject(self):
        for case in ('unknown-manifest', 'unknown-chunk', 'duplicate-field', 'coverage-zero', 'imported-boolean', 'offset', 'whole-hash'):
            with self.subTest(case=case):
                _, pack, manifest = self.package(b'synthetic')
                if case == 'unknown-manifest':
                    manifest['source_basename'] = 'forbidden'
                elif case == 'unknown-chunk':
                    path = pack / 'chunks' / prep.chunk_name(manifest['chunks'][0])
                    obj = json.loads(prep.native(path).read_bytes())
                    obj['untrusted'] = True
                    self.rewrite(path, obj)
                elif case == 'duplicate-field':
                    raw = prep.encoded(manifest)
                    prep.native(pack / 'manifest.json').write_bytes(b'{"format":"duplicate",' + raw[1:])
                elif case == 'coverage-zero':
                    manifest['coverage']['clients'] = 0
                elif case == 'imported-boolean':
                    manifest['tax_cases_imported'] = False
                elif case == 'offset':
                    manifest['chunks'][0]['offset'] = 1
                else:
                    manifest['artifact']['sha256'] = '0' * 64
                if case not in ('unknown-chunk', 'duplicate-field'):
                    self.rewrite(pack / 'manifest.json', manifest)
                with self.assertRaises(prep.PreservationError):
                    prep.verify(pack)

    def test_reordered_manifest_keys_are_not_a_second_accepted_encoding(self):
        _, pack, manifest = self.package(b'', metadata={'note': ''})
        canonical = prep.encoded(manifest)
        reordered = dict(reversed(list(json.loads(canonical).items())))
        noncanonical = json.dumps(reordered, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
        self.assertNotEqual(noncanonical, canonical)
        self.assertEqual(prep.encoded(reordered), canonical)
        prep.native(pack / 'manifest.json').write_bytes(noncanonical)
        with self.assertRaisesRegex(prep.PreservationError, 'noncanonical'):
            prep.verify(pack)

    def test_nested_manifest_metadata_and_chunk_keys_use_recursive_sorted_order(self):
        for nested in ('artifact', 'coverage', 'chunk-reference', 'chunk-object'):
            with self.subTest(nested=nested):
                _, pack, manifest = self.package(b'synthetic nested metadata')
                manifest = json.loads(prep.encoded(manifest))
                target = pack / 'manifest.json'
                if nested == 'chunk-object':
                    target = pack / 'chunks' / prep.chunk_name(manifest['chunks'][0])
                    value = json.loads(prep.native(target).read_bytes())
                    value = dict(reversed(list(value.items())))
                else:
                    value = manifest
                    if nested == 'chunk-reference':
                        value['chunks'][0] = dict(reversed(list(value['chunks'][0].items())))
                    else:
                        value[nested] = dict(reversed(list(value[nested].items())))
                noncanonical = json.dumps(value, ensure_ascii=False, separators=(',', ':')).encode('utf-8')
                self.assertNotEqual(noncanonical, prep.encoded(value))
                prep.native(target).write_bytes(noncanonical)
                with self.assertRaisesRegex(prep.PreservationError, 'noncanonical'):
                    prep.verify(pack)

    def test_reparse_ancestors_fail_before_source_read_or_output_creation(self):
        source = self.source()
        original_lstat = Path.lstat
        for ancestor in (source.parent, source.parent.parent, source):
            def reparse(path, *args, **kwargs):
                info = original_lstat(path, *args, **kwargs)
                if str(path) == str(prep.native(ancestor)):
                    return SimpleNamespace(st_mode=info.st_mode, st_dev=info.st_dev, st_ino=info.st_ino, st_file_attributes=0x400, st_reparse_tag=1)
                return info
            output = self.home / 'reparse-rejected'
            with self.subTest(ancestor=ancestor.name), patch.object(Path, 'lstat', reparse), self.assertRaises(prep.PreservationError):
                prep.prepare(source, output, 'opaque')
            self.assertFalse(output.exists())

    def test_real_leaf_symlink_is_rejected_when_host_allows_creation(self):
        source = self.source()
        link = self.home / 'linked-source'
        try:
            os.symlink(source, link)
        except OSError:
            self.skipTest('Host does not permit synthetic symlink creation')
        with self.assertRaises(prep.PreservationError):
            prep.prepare(link, self.home / 'linked-output', 'opaque')

    def test_unverifiable_output_protection_fails_closed(self):
        source = self.source()
        output = self.home / 'unprotected'
        with patch.object(prep, 'protect_directory', side_effect=prep.PreservationError('synthetic_acl_unavailable')), self.assertRaises(prep.PreservationError):
            prep.prepare(source, output, 'opaque')
        self.assertFalse((output / 'manifest.json').exists())

    def test_broad_output_permissions_are_rejected_by_verification(self):
        _, pack, _ = self.package(b'synthetic privacy check')
        if os.name == 'nt':
            c, w, advapi, kernel = prep.windows_api()
            descriptor = c.c_void_p()
            sddl = 'D:P(A;OICI;FA;;;' + prep.windows_sid() + ')(A;OICI;FA;;;SY)(A;OICI;FR;;;WD)'
            self.assertTrue(advapi.ConvertStringSecurityDescriptorToSecurityDescriptorW(sddl, 1, c.byref(descriptor), None))
            try:
                self.assertTrue(advapi.SetFileSecurityW(str(prep.native(pack)), 0x80000004, descriptor))
            finally:
                kernel.LocalFree(descriptor)
        else:
            os.chmod(pack, 0o755)
        try:
            with self.assertRaises(prep.PreservationError):
                prep.verify(pack)
        finally:
            prep.protect_directory(pack)

    def test_verifier_enforces_object_and_manifest_limits_before_decoding(self):
        _, pack, manifest = self.package(b'synthetic bounded decode')
        path = pack / 'chunks' / prep.chunk_name(manifest['chunks'][0])
        obj = json.loads(prep.native(path).read_bytes())
        obj['content_base64'] = 'A' * 1000
        self.rewrite(path, obj)
        with patch.object(prep.base64, 'b64decode', side_effect=AssertionError('oversized data must not be decoded')), self.assertRaises(prep.PreservationError):
            prep.verify(pack)
        prep.native(path).write_bytes(b'x' * (prep.MAX_OBJECT_BYTES + 1))
        with self.assertRaises(prep.PreservationError):
            prep.verify(pack)
        prep.native(pack / 'manifest.json').write_bytes(b'x' * (prep.MAX_MANIFEST_BYTES + 1))
        with self.assertRaises(prep.PreservationError):
            prep.verify(pack)

    def test_cli_reports_only_opaque_evidence_and_hides_source_basename(self):
        source = self.source()
        output = self.home / 'cli'
        stdout, stderr = io.StringIO(), io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            status = prep.main(['prepare', '--source', str(source), '--out', str(output), '--alias', 'opaque-cli', '--note', ''])
        self.assertEqual(status, 0, stderr.getvalue())
        result = json.loads(stdout.getvalue())
        self.assertEqual(result['status'], 'opaque_preservation_only')
        self.assertFalse(result['decoder_verified'])
        self.assertEqual(result['tax_cases_imported'], 0)
        self.assertNotIn(source.name, stdout.getvalue() + stderr.getvalue())
        self.assertEqual(prep.verify(output)['metadata'], {'note': ''})


if __name__ == '__main__':
    unittest.main()
