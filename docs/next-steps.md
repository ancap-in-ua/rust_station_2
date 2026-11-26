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
end-to-end is: **get a solid color on screen.** Progress so far (see
progress.md 2026-11-26): GS privileged register addresses/bit-packing
are implemented in `ps2-rt/src/gs.rs`, sourced from ps2sdk, and the
"how does the EE even address `0x12000000`" question from the previous
version of this doc is answered (identity-mapped, no segment prefix
needed) — but nothing has been wired up into a tool or run. What's left:

- Source real NTSC/PAL video-timing constants for `SMODE1`/`SYNCH1`/
  `SYNCH2`/`SYNCHV` — not guessable, needs the EE/GS Users Manual or a
  known-working reference (ps2sdk video-mode preset), not fabricated.
- Verify the PCRTC blend-equation hypothesis in progress.md (using
  `PMODE`'s `MMOD`/`SLBG`/`ALP=0` to force the whole display to
  `BGCOLOR`, skipping DMA/GIF entirely) against the GS Users Manual's
  actual blend equation — it's currently inferred from field semantics,
  not confirmed.
- If that hypothesis holds, a solid-color tool needs no GIF/DMA at all;
  if it doesn't, fall back to the original plan: issue a DMA transfer
  with a GIF packet to push an actual primitive (even a full-screen
  rectangle) to the GS. The latter is real hardware programming with no
  C library to crib conventions from — budget real time for it.
- Either way, this still needs #1 (an actual BIOS/PCSX2 run) to confirm
  anything beyond "it compiles and the addresses match a secondary
  source."

## 3. Validate the target spec dynamically, not just statically

`just test` now statically re-checks the ELF invariants (entry point,
`_gp`/`_stack_top` formulas, bss) on every build — see progress.md
2026-11-25. That only guards against *regressions* in things already
confirmed once; it's not a substitute for #1/#2 below. Things stated as
"unverified" in progress.md that should still get burned down once
something actually boots:

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
