"""Minimal Anvil region and NBT reader (stdlib only) for the OmegaSrv deploy rehearsal.

Reads region files (.mca), returns each chunk's raw sector payload and a parsed NBT view.
The NBT parser builds Python objects but can skip named top-level subtrees cheaply.
"""
import struct
import zlib
import gzip
import os

TAG_END, TAG_BYTE, TAG_SHORT, TAG_INT, TAG_LONG, TAG_FLOAT, TAG_DOUBLE = 0, 1, 2, 3, 4, 5, 6
TAG_BYTE_ARRAY, TAG_STRING, TAG_LIST, TAG_COMPOUND, TAG_INT_ARRAY, TAG_LONG_ARRAY = 7, 8, 9, 10, 11, 12

_FIXED = {TAG_BYTE: 1, TAG_SHORT: 2, TAG_INT: 4, TAG_LONG: 8, TAG_FLOAT: 4, TAG_DOUBLE: 8}
_FMT = {TAG_BYTE: '>b', TAG_SHORT: '>h', TAG_INT: '>i', TAG_LONG: '>q', TAG_FLOAT: '>f', TAG_DOUBLE: '>d'}


class Reader:
    __slots__ = ('b', 'p')

    def __init__(self, b, p=0):
        self.b = b
        self.p = p

    def u8(self):
        v = self.b[self.p]
        self.p += 1
        return v

    def u16(self):
        v = (self.b[self.p] << 8) | self.b[self.p + 1]
        self.p += 2
        return v

    def i32(self):
        v = struct.unpack_from('>i', self.b, self.p)[0]
        self.p += 4
        return v

    def string(self):
        n = self.u16()
        s = self.b[self.p:self.p + n].decode('utf-8', 'replace')
        self.p += n
        return s

    def skip(self, t):
        b = self.b
        if t in _FIXED:
            self.p += _FIXED[t]
        elif t == TAG_BYTE_ARRAY:
            n = self.i32()
            self.p += n
        elif t == TAG_STRING:
            n = self.u16()
            self.p += n
        elif t == TAG_INT_ARRAY:
            n = self.i32()
            self.p += 4 * n
        elif t == TAG_LONG_ARRAY:
            n = self.i32()
            self.p += 8 * n
        elif t == TAG_LIST:
            et = self.u8()
            n = self.i32()
            if et in _FIXED:
                self.p += _FIXED[et] * max(n, 0)
            else:
                for _ in range(n):
                    self.skip(et)
        elif t == TAG_COMPOUND:
            while True:
                ct = b[self.p]
                self.p += 1
                if ct == TAG_END:
                    break
                n = self.u16()
                self.p += n
                self.skip(ct)
        else:
            raise ValueError('bad tag %d at %d' % (t, self.p))

    def value(self, t, skip_keys=None):
        if t in _FMT:
            v = struct.unpack_from(_FMT[t], self.b, self.p)[0]
            self.p += _FIXED[t]
            return v
        if t == TAG_STRING:
            return self.string()
        if t == TAG_BYTE_ARRAY:
            n = self.i32()
            v = bytes(self.b[self.p:self.p + n])
            self.p += n
            return v
        if t == TAG_INT_ARRAY:
            n = self.i32()
            v = struct.unpack_from('>%di' % n, self.b, self.p)
            self.p += 4 * n
            return list(v)
        if t == TAG_LONG_ARRAY:
            n = self.i32()
            self.p += 8 * n
            return ('longarray', n)
        if t == TAG_LIST:
            et = self.u8()
            n = self.i32()
            return [self.value(et) for _ in range(max(n, 0))]
        if t == TAG_COMPOUND:
            d = {}
            while True:
                ct = self.u8()
                if ct == TAG_END:
                    return d
                k = self.string()
                if skip_keys is not None and k in skip_keys:
                    self.skip(ct)
                else:
                    d[k] = self.value(ct)
        raise ValueError('bad tag %d' % t)


def parse_nbt(data, skip_keys=None):
    r = Reader(data)
    t = r.u8()
    if t != TAG_COMPOUND:
        raise ValueError('root is not a compound')
    r.string()  # root name (empty)
    return r.value(TAG_COMPOUND, skip_keys)


def read_nbt_file(path):
    with open(path, 'rb') as f:
        raw = f.read()
    if raw[:2] == b'\x1f\x8b':
        raw = gzip.decompress(raw)
    return parse_nbt(raw)


def region_coords(name):
    # r.X.Z.mca
    parts = name.split('.')
    return int(parts[1]), int(parts[2])


def iter_region(path):
    """Yields (index, cx, cz, timestamp, raw_payload, compression, external) for each chunk.

    raw_payload is the bytes after the 5-byte header (length, compression), exactly as stored, or
    None for an external (.mcc) chunk whose payload lives in a separate file.
    """
    with open(path, 'rb') as f:
        data = f.read()
    if len(data) < 8192:
        return
    rx, rz = region_coords(os.path.basename(path))
    for i in range(1024):
        loc = struct.unpack_from('>I', data, i * 4)[0]
        if loc == 0:
            continue
        off = (loc >> 8) * 4096
        cnt = loc & 0xFF
        ts = struct.unpack_from('>I', data, 4096 + i * 4)[0]
        if off + 5 > len(data):
            continue
        length = struct.unpack_from('>I', data, off)[0]
        comp = data[off + 4]
        payload = data[off + 5:off + 4 + length]
        cx = rx * 32 + (i & 31)
        cz = rz * 32 + (i >> 5)
        yield i, cx, cz, ts, payload, comp, bool(comp & 0x80)


def decompress(payload, comp, mcc_path=None):
    c = comp & 0x7F
    if comp & 0x80:
        with open(mcc_path, 'rb') as f:
            payload = f.read()
    if c == 2:
        return zlib.decompress(payload)
    if c == 1:
        return gzip.decompress(payload)
    if c == 3:
        return payload
    raise ValueError('compression %d not supported' % c)


def write_region(path, chunks):
    """chunks: list of (index, timestamp, comp_byte, payload). Writes a fresh region file."""
    header_loc = bytearray(4096)
    header_ts = bytearray(4096)
    body = bytearray()
    sector = 2
    for i, ts, comp, payload in sorted(chunks):
        blob = struct.pack('>I', len(payload) + 1) + bytes([comp]) + payload
        pad = (-len(blob)) % 4096
        blob += b'\0' * pad
        n = len(blob) // 4096
        if n > 255:
            raise ValueError('chunk too large for inline storage')
        struct.pack_into('>I', header_loc, i * 4, (sector << 8) | n)
        struct.pack_into('>I', header_ts, i * 4, ts)
        body += blob
        sector += n
    with open(path, 'wb') as f:
        f.write(header_loc)
        f.write(header_ts)
        f.write(body)
