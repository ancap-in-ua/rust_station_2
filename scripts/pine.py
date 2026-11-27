#!/usr/bin/env python3
"""Minimal PINE client -- read EE memory out of a running PCSX2.

PINE is PCSX2's IPC interface (`[EmuCore] EnablePINE = true`). It is
what makes the PCSX2 checks scriptable: without it, "did the tool write
its proof-of-life value" needs a human squinting at the debugger's
memory viewer.

Framing: request = <u32 LE total_len><u8 opcode><payload>
         reply   = <u32 LE total_len><u8 result><payload>
with result 0 = OK and 0xFF = failure.

Limits worth knowing (both found the hard way, see docs/progress.md
2026-11-27):

  - It reads EE *memory*. GS privileged registers (0x1200_0000..) read
    back as zero through it whether or not a write landed, so it cannot
    be used to check those.
  - Every request fails with 0xFF until a VM is actually running, which
    is a useful "has it booted yet?" signal and is what `wait_for_vm`
    below polls on.
"""
import argparse
import os
import socket
import struct
import sys
import time

MSG_READ8, MSG_READ16, MSG_READ32, MSG_READ64 = 0, 1, 2, 3
READ_SIZES = {MSG_READ8: 1, MSG_READ16: 2, MSG_READ32: 4, MSG_READ64: 8}

DEFAULT_SLOT = 28011


def socket_path(slot=DEFAULT_SLOT):
    runtime = os.environ.get("XDG_RUNTIME_DIR")
    base = f"{runtime}/pcsx2.sock" if runtime else "/tmp/pcsx2.sock"
    return base if slot == DEFAULT_SLOT else f"{base}.{slot}"


class Pine:
    def __init__(self, path=None, slot=DEFAULT_SLOT, timeout=5.0):
        self.path = path or socket_path(slot)
        self.sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.sock.settimeout(timeout)
        self.sock.connect(self.path)

    def _recv_exactly(self, n):
        buf = b""
        while len(buf) < n:
            chunk = self.sock.recv(n - len(buf))
            if not chunk:
                raise RuntimeError("PINE connection closed mid-reply")
            buf += chunk
        return buf

    def _request(self, opcode, payload=b""):
        body = bytes([opcode]) + payload
        self.sock.sendall(struct.pack("<I", 4 + len(body)) + body)
        (total,) = struct.unpack("<I", self._recv_exactly(4))
        rest = self._recv_exactly(total - 4)
        if rest[0] != 0:
            raise RuntimeError(f"PINE opcode {opcode:#x} failed (result {rest[0]:#x})")
        return rest[1:]

    def read(self, addr, opcode=MSG_READ32):
        raw = self._request(opcode, struct.pack("<I", addr))
        return int.from_bytes(raw[: READ_SIZES[opcode]], "little")

    def close(self):
        self.sock.close()


def wait_for_vm(deadline, probe_addr, slot=DEFAULT_SLOT):
    """Block until PCSX2 has a VM running and answering reads."""
    last = "never connected"
    while time.monotonic() < deadline:
        try:
            p = Pine(slot=slot)
            try:
                p.read(probe_addr)
                return p
            finally:
                pass
        except (OSError, RuntimeError) as exc:
            last = str(exc)
            time.sleep(0.25)
    raise TimeoutError(f"PCSX2 never started answering PINE reads ({last})")


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("addresses", nargs="+", help="addresses to read, e.g. 0x100188")
    ap.add_argument("--slot", type=int, default=DEFAULT_SLOT)
    ap.add_argument(
        "--wait",
        type=float,
        default=0.0,
        help="seconds to wait for a VM to come up before reading",
    )
    args = ap.parse_args()

    first = int(args.addresses[0], 0)
    if args.wait:
        p = wait_for_vm(time.monotonic() + args.wait, first, args.slot)
    else:
        p = Pine(slot=args.slot)

    for a in args.addresses:
        addr = int(a, 0)
        print(f"{addr:#010x} {p.read(addr):#010x}")
    p.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
