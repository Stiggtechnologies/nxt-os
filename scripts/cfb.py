"""Minimal read-only OLE2 / Compound File Binary reader (stdlib only).

Used by the Caseware .cvw inventory and the payroll .xls BIFF reader. Reads
the header, FAT, mini FAT and directory; returns stream names, sizes and
bytes. Never writes. Malformed chains fail visibly with CfbError.
"""
import struct

MAGIC = bytes([0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1])
ENDOFCHAIN, FREESECT = 0xFFFFFFFE, 0xFFFFFFFF
MAX_BYTES = 256 * 1024 * 1024


class CfbError(Exception):
    pass


class Cfb:
    def __init__(self, data):
        if len(data) < 512 or data[:8] != MAGIC:
            raise CfbError("not an OLE2 compound file")
        if len(data) > MAX_BYTES:
            raise CfbError("compound file too large")
        self.d = data
        ss = 1 << struct.unpack_from("<H", data, 30)[0]
        mss = 1 << struct.unpack_from("<H", data, 32)[0]
        if ss not in (512, 4096) or mss != 64:
            raise CfbError("unsupported sector size")
        self.ss, self.mss = ss, mss
        nfat, dir1, _, self.cutoff, mfat1, nmfat, difat1, ndifat = struct.unpack_from("<IIIIIIII", data, 44)
        difat = list(struct.unpack_from("<109I", data, 76))
        s, guard = difat1, 0
        while ndifat and s not in (ENDOFCHAIN, FREESECT):
            guard += 1
            if guard > 100000: raise CfbError("DIFAT loop")
            vals = struct.unpack_from(f"<{ss // 4}I", self._sec(s))
            difat += vals[:-1]; s = vals[-1]
        fat = []
        for s in difat[:nfat]:
            fat += struct.unpack_from(f"<{ss // 4}I", self._sec(s))
        self.fat = fat
        self.dirs = self._entries(self._chain(dir1))
        root = self.dirs[0]
        self.ministream = self._chain(root["start"])[: root["size"]] if root["size"] else b""
        mf = self._chain(mfat1) if nmfat else b""
        self.minifat = list(struct.unpack_from(f"<{len(mf) // 4}I", mf))

    def _sec(self, n):
        off = (n + 1) * self.ss
        if off >= len(self.d):  # a short final sector is tolerated; past EOF is not
            raise CfbError(f"sector {n} beyond end of file")
        return self.d[off: off + self.ss]

    def _walk(self, start, table, limit):
        out, s, seen = [], start, set()
        while s not in (ENDOFCHAIN, FREESECT):
            if s in seen or s >= len(table) or len(out) > limit:
                raise CfbError("broken sector chain")
            seen.add(s); out.append(s); s = table[s]
        return out

    def _chain(self, start):
        return b"".join(self._sec(s) for s in self._walk(start, self.fat, len(self.fat)))

    def _entries(self, raw):
        out = []
        for i in range(len(raw) // 128):
            e = raw[i * 128:(i + 1) * 128]
            nl = struct.unpack_from("<H", e, 64)[0]
            name = e[: max(nl - 2, 0)].decode("utf-16-le", "replace")
            typ = e[66]
            start, size = struct.unpack_from("<IQ", e, 116)
            if self.ss == 512: size &= 0xFFFFFFFF
            out.append({"name": name, "type": typ, "start": start, "size": size})
        return out

    def streams(self):
        """[(name, size)] for every stream entry (type 2), directory order."""
        return [(e["name"], e["size"]) for e in self.dirs if e["type"] == 2]

    def read(self, name):
        for e in self.dirs:
            if e["type"] == 2 and e["name"] == name:
                if e["size"] < self.cutoff:
                    secs = self._walk(e["start"], self.minifat, len(self.minifat))
                    b = b"".join(self.ministream[s * self.mss:(s + 1) * self.mss] for s in secs)
                else:
                    b = self._chain(e["start"])
                if len(b) < e["size"]:
                    raise CfbError(f"stream {name} truncated")
                return b[: e["size"]]
        raise CfbError(f"stream {name} not found")
