"""Minimal Source RCON client (stdlib)."""
import socket
import struct
import time


class Rcon:
    def __init__(self, host, port, password, timeout=120):
        self.s = socket.create_connection((host, port), timeout=timeout)
        self.rid = 0
        self._send(3, password)
        rid, typ, _ = self._recv()
        if rid == -1:
            raise RuntimeError('rcon auth failed')

    def _send(self, typ, body):
        self.rid += 1
        b = body.encode('utf-8')
        pkt = struct.pack('<iii', len(b) + 10, self.rid, typ) + b + b'\0\0'
        self.s.sendall(pkt)
        return self.rid

    def _recvn(self, n):
        buf = b''
        while len(buf) < n:
            chunk = self.s.recv(n - len(buf))
            if not chunk:
                raise ConnectionError('rcon closed')
            buf += chunk
        return buf

    def _recv(self):
        (ln,) = struct.unpack('<i', self._recvn(4))
        data = self._recvn(ln)
        rid, typ = struct.unpack('<ii', data[:8])
        return rid, typ, data[8:-2].decode('utf-8', 'replace')

    def cmd(self, command):
        """Sends a command and returns its whole reply, however many packets it takes.

        Vanilla's RconClient reads one packet per socket read and drops the connection when a read
        holds two, so nothing is pipelined: the end-of-reply marker (an unknown request type, which
        vanilla answers with the same id) goes out only after the first reply packet is back. The
        server writes every packet of a reply before it reads the next request.
        """
        rid = self._send(2, command)
        out = []
        while True:
            r, typ, body = self._recv()
            if r == rid:
                out.append(body)
                break
        marker = self._send(0, '')
        while True:
            r, typ, body = self._recv()
            if r == marker:
                break
            if r == rid:
                out.append(body)
        return ''.join(out)

    def close(self):
        try:
            self.s.close()
        except OSError:
            pass


def connect_retry(host, port, password, deadline_s=600):
    t0 = time.time()
    while True:
        try:
            return Rcon(host, port, password)
        except (OSError, ConnectionError):
            if time.time() - t0 > deadline_s:
                raise
            time.sleep(2)
