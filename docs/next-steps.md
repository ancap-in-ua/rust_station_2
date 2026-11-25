# Next steps

Ordered roughly by what unblocks the most follow-on work. See
[progress.md](progress.md) for what's already in place and what's been
verified vs. just assumed.

## 1. Actually run `hello` under PCSX2 (highest priority)

Nothing has been dynamically executed yet — only statically inspected via
`readelf`/`nm`. This needs a legally-obtained PS2 BIOS dump, which isn't
something this repo or the flake can provide; run
`pcsx2-qt -setupwizard` once to configure one, then:

```
just run-pcsx2
```

Check the magic value (`0xC0FFEE42`) landed at RAM address `0x1000` via
PCSX2's memory viewer (Debug menu) or `gdb` against its gdbstub. If it
doesn't:
- Check whether PCSX2 even reaches the entry point (`-debugger` flag
  breaks on entry).
- Suspect `$gp`/`$sp` setup in `ps2-rt/src/boot.rs` first — that's the
  least-tested part of the boot path.

## 2. Minimal GS output — the real first milestone

`hello` currently proves nothing visible. The first milestone that
actually exercises boot correctness, `$sp`/`$gp` setup, and bss zeroing
end-to-end is: **get a solid color on screen.** This means:

- Writing GS privileged registers (memory-mapped, physical base
  `0x12000000`) to configure a basic framebuffer / display mode.
- Understanding how the EE addresses that region from kernel mode on
  boot (likely an uncached segment, analogous to MIPS `kseg1` — the exact
  bit-pattern for R5900's 64-bit address segments was *not* worked out
  during the initial scaffold; this is real research, not a known
  quantity to just copy in).
- Issuing a DMA transfer with a GIF packet to actually push a primitive
  (even a full-screen rectangle) to the GS.

This is genuinely nontrivial embedded/hardware work — budget real time
for it, and expect to lean on PS2 hardware references (EE Users Manual,
GS Users Manual) rather than Rust-specific resources, since this is pure
hardware programming with no C library to crib conventions from.

## 3. Validate the target spec dynamically, not just statically

`just test` now statically re-checks the ELF invariants (entry point,
`_gp`/`_stack_top` formulas, bss) on every build — see progress.md
2026-11-25. That only guards against *regressions* in things already
confirmed once; it's not a substitute for #1/#2 below. Things stated as
"unverified" in progress.md that should still get burned down once
something actually boots:

- Confirm `+noabicalls` + static relocation isn't silently relying on the
  `.got`/`_gp` setup for correctness (it's present and non-empty in the
  built ELF — find out why, and whether it matters).
- Narrow `max-atomic-width` to whatever the R5900 actually supports if
  any tool ends up needing atomics.
- If floating point correctness ever matters for a ported tool, write a
  small test that compares EE FPU results against expected IEEE-754
  values for known tricky cases (NaN handling, denormals) — the target
  declares `+single-float` but the EE's documented non-compliance hasn't
  been checked against actual LLVM codegen.

## 4. Pick the second tool to port

The workspace is structured for multiple tools (`tools/<name>/`) but only
has the one placeholder. Once GS output exists, a reasonable second
target is something that exercises `alloc` (the target spec currently
only builds `core` via `build-std` — adding a simple bump/freelist
allocator in `ps2-rt` and switching `build-std = ["core", "alloc"]` would
be a prerequisite for anything beyond the most trivial no-heap tool).

## 5. Real hardware deployment (lower priority, deliberately deferred)

The flake only gets you PCSX2. Getting a built `.elf` onto real hardware
needs a loader already — network boot via `ps2link`, USB via OPL, etc.
None of those client-side tools are packaged in nixpkgs, so for now:
build the ELF here (`target/mips64r5900el-ps2/release/<tool>`) and hand
it to whatever loader setup already exists outside this repo. Revisit
packaging a loader client in the flake only if PCSX2 iteration stops
being sufficient.

## 6. If bare-metal turns out to be too limiting

This was a deliberate tradeoff (see progress.md) to avoid packaging
`ee-gcc`/`ps2sdk` in nix. If writing everything from scratch (console,
pad input, file I/O) becomes the actual bottleneck rather than an
interesting part of the project, the fallback is FFI against ps2sdk's C
libraries — that would mean fetching prebuilt ps2dev toolchain releases
from GitHub inside the flake (not building `ee-gcc` from source) rather
than continuing pure-Rust. Worth re-raising explicitly before committing
more time to hand-rolled drivers if that point is reached.
