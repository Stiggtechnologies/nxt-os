# Caseware Working Papers DBF reader

`scripts/read-caseware-engagement.py` reads a **copy** of one Caseware engagement folder
(`NAME.ac` + FoxPro tables) offline and writes a new directory with:

- `original/` byte-identical copies of every file (.ac, .cvw, .dbf, .cdx, .fpt, …), re-hashed after copy;
- `preservation.json` (`nxt-caseware-preservation-batch/v1` rows, `engagement_file`, sha256, size, `synthetic:false`);
- `trial-balance.json` in `nxt-engagement-tb/v1`, ready for `POST /{id}/import/preview`;
- `mapping.json` (account → Caseware map number, GIFI, class, kind; plus the MP mapping table);
- `report.json` (balance checks, counts, warnings).

`verify --out DIR` re-hashes preserved files.

Table semantics were established read-only against 24 engagement/template folders on R: (2026-10-29):
AM = account master; BL `ID=O, YEAR=0` = current unadjusted balance; `ID=P, YEAR=1` = prior-year final;
`ID=X` (computed rollups) and `ID=L` are ignored; AMT_YTD is signed debit-positive. Every client
engagement with balances produced balanced current and prior TBs. Kind is taken from GIFI range,
then Caseware class, then (with a warning) balance sign.

Not done: adjusting entries (GL.dbf) are counted, not applied; CaseView `.cvw` bodies are preserved
opaque only; sign-offs/annotations/history are not mapped. Caseware's own template folders with
duplicate account numbers fail visibly.
