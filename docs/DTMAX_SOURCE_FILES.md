# DT Max original client file preservation

Each original DT Max client file is kept as immutable bytes linked to its NXT OS client, inside one company scope. This lane does not decode DT Max, import tax cases, file returns or write back. The DT Max export reader is a separate lane.

All routes sit behind the existing admin gate and are scoped by company, as in the QuickBooks source lists. A file stored under one company is not visible from another.

| Route | Purpose |
| --- | --- |
| `POST /api/source-imports/dtmax/companies/{company}/clients` | Register `{client_id, dtmax_keys}`. A key belongs to one client and cannot move. |
| `POST /api/source-imports/dtmax/companies/{company}/files` | Preserve `{synthetic, original_path, year, file_type, sha256, content_hex}`. |
| `GET .../files/{sha256}` and `.../files/{sha256}/content` | Read the record, or the bytes after the hash is checked again. |
| `GET .../files/unmatched` | Files the linkage rule could not link. |
| `POST .../files/{sha256}/link` | A reviewer links an unmatched file to a registered client. This can happen once. |
| `GET .../clients/{client}/files` | Preserved files linked to one client. |

Rules:

- **Hash:** the server computes SHA-256 and rejects a declared hash that does not match. Bytes and records are write-once (`set_nx`). This follows the engagement attachment pattern.
- **Idempotent, deduped by hash:** sending the same bytes again returns the existing record with `duplicate_hash: true`. A new path for the same bytes is added to `observed_paths`. The original record does not change.
- **Metadata:** original path (up to 1,024 characters), year (2013 to 2024), and file type. The file type must equal the file extension.
- **Client linkage rule (`dtmax_key_exact`):** the key is the file-name stem in upper case, for example `Y:\DTMax\2019\smithj.t19` gives `SMITHJ`. A file links only when that key is registered for the company. Otherwise it joins the unmatched queue. Once a link exists it never changes.
- **Bounds:** 4 MiB per file.
- **Real data:** only requests with `synthetic: true` are accepted. A real cohort of about 16,448 files from the Y: drive stays blocked (`real_sample_required`) until Orville approves a load.
