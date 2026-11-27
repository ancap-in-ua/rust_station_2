# Next steps

Ordered roughly by what unblocks the most follow-on work. See
[progress.md](progress.md) for what's already in place and what's been
verified vs. just assumed.

## 1. Run it on a real PS2 — DONE (2026-11-27)

The tools boot on PCSX2 with a real BIOS and do what they claim.
`just test-pcsx2` asserts it on every run, over PCSX2's PINE socket:
`hello` publishes `0xc0ffee42`, `alloc-hello` publishes `0xa110008c`
with the bump allocator's signature visible in the heap, `solid-color`
runs its whole GS sequence, and none of them trip a TLB miss.

Getting there needed two fixes and produced one retraction — see
progress.md 2026-11-27 (both entries):

- The target spec emitted ELF64, which PCSX2's ELF32-only loader
  *segfaults* on. Re-targeted to `targets/mipsel-sony-ps2.json`
  (`mips`/o32/32-bit pointers, `mips3` CPU retained).
- The `0x1000` proof-of-life address was unwritable from a user-mode
  ELF (TLB miss — the first MiB is the BIOS kernel's). Replaced with
  `ps2_rt::PS2_RT_PROOF`, a static in the tool's own image.
- **Retracted:** "`set_gs_crt` hangs and never returns." It doesn't.
  The hand-assembled probe that produced that result kept its marker
  registers in `$t0`/`$t1` across the syscall, which the BIOS handler
  is entitled to clobber. The 2026-11-26 syscall research was right.

What's left for this item is only the hardware this environment can't
reach: a real console. PCSX2 is not a PS2 either — see #2's open
questions for a case where that distinction may already matter.

A BIOS dump goes in `bios_dump/` (gitignored — copyrighted, not
redistributable). Without one, `test-static` and `test-qemu` still run
and `test-pcsx2` skips loudly.

## 2. Minimal GS output — the real first milestone

**This is now the only thing standing between the project and its first
visible result, and it is genuinely unsolved.**

`solid-color` runs end to end on a real PS2 as of 2026-11-27: the
`SetGsCrt` syscall returns, all four GS registers are written, the tool
reaches its terminal state and publishes `0x5011d000`. And the screen
is **100% black**, not red.

So the PMODE-blend approach is finally being *tested* rather than
blocked, and it isn't doing what the GS User's Manual reading
predicted. Verified the capture isn't lying by booting the BIOS itself
and capturing that — it comes back with the boot screen's colors.

What got resolved along the way, so it's not re-litigated:

- "How does the EE even address `0x12000000`?" — identity-mapped, no
  segment prefix needed (confirmed against ps2tek's EE memory map, and
  since re-confirmed *on PCSX2* on 2026-11-27: a raw store to
  `0x12000000` from a user-mode ELF takes no TLB miss. Re-tested
  because the neighbouring assumption about low RAM turned out to be
  wrong — see progress.md).
- "Where do NTSC/PAL sync-timing constants come from?" — they don't;
  the GS Users Manual says that's a BIOS kernel service
  (`ps2_rt::syscall::set_gs_crt`, BIOS syscall `0x02`), not something
  application code hand-derives. Sourced from ps2tek + ps2sdk, and the
  compiled stub's raw bytes are checked against the hand-assembled
  expectation in `scripts/verify-solid-color.sh`.
- PMODE/BGCOLOR/DISPFB/DISPLAY's bit layouts and the PCRTC blend
  equation — confirmed against the GS Users Manual directly (not just
  ps2sdk macro names), so the "force the whole display to `BGCOLOR` via
  `MMOD`/`SLBG`/`ALP=0`, skipping GIF/DMA entirely" approach is no longer
  a hypothesis.

What's actually still open, roughly in order of suspicion:

- **`SMODE2` is never written.** `ps2_rt::gs` defines `REG_SMODE2`
  ("interlace, field mode, **display enable**") and `solid-color`
  doesn't touch it. If `SetGsCrt` doesn't leave the display enabled,
  nothing reaches the CRTC no matter what the merge circuit computes.
  Cheapest thing to try first.
- **Whether PCSX2's PCRTC emulates this path at all.** The whole trick
  depends on `SLBG`/`MMOD`/`ALP` producing output from a circuit whose
  framebuffer points at VRAM nobody ever wrote. That is an exotic
  corner, and an emulator is allowed to get it wrong where real
  hardware wouldn't. Distinguishing "our registers are wrong" from
  "PCSX2 doesn't model this" may need real hardware — the first thing
  in this project where PCSX2 being not-a-PS2 actually bites.
- **Whether the `ALP = 0` reading of the blend equation is inverted.**
  Re-derive from the GS User's Manual rather than from our own note.
- If the PCRTC approach turns out to be a dead end, the fallback is the
  original plan: issue a DMA transfer with a GIF packet to push an
  actual primitive (even a full-screen rectangle) to the GS. That's
  real hardware programming with no C library to crib conventions from
  — budget real time for it if it comes to that.

Note the "no longer a hypothesis" claim in the bullet list above was
written before anything had run. The bit layouts and the blend equation
really are confirmed against the primary source; what was never
confirmed is that they add up to a red screen, and so far they don't.

## 3. Validate the target spec dynamically, not just statically

`just test` runs `test-static` (ELF invariants — entry point,
`_gp`/`_stack_top` formulas, bss, `.got`) and then `test-qemu`, which
executes each ELF and asserts what it did. Code generated from the
hand-written target spec demonstrably runs on a MIPS III CPU model.

**That is a much weaker result than it sounded on the morning of
2026-11-27.** The spec's biggest defect — emitting ELF64, which no real
PS2 loader accepts — was invisible to both layers, and `verify-elf.sh`
was actively asserting it as correct. Treat "the checks are green" as
"nothing changed since we last chose what to look at", and keep in mind
that those choices were all made before anything had run on a PS2. The
spec work itself is now #1.

What's left is specifically the parts of the spec qemu's generic MIPS
models can't speak to:

- Narrow `max-atomic-width` to whatever the R5900 actually supports if
  any tool ends up needing atomics. (qemu's R4000 would happily execute
  64-bit `ll`/`sc`-based atomics that the EE may not implement, so this
  cannot be settled here — it needs hardware or PCSX2.)
- If floating point correctness ever matters for a ported tool, write a
  small test that compares EE FPU results against expected IEEE-754
  values for known tricky cases (NaN handling, denormals) — the target
  declares `+single-float` but the EE's documented non-compliance hasn't
  been checked against actual LLVM codegen. Note this *specifically*
  cannot be done under qemu: its FPU is more standards-compliant than
  the EE's, so it would mask exactly the discrepancy being looked for.
  PCSX2 or hardware only.
- Anything touching MMI (128-bit paired GPRs), scratchpad RAM, or EE
  cache behaviour — absent from qemu entirely.

## 4. Pick the second tool to port — done for `alloc`, see progress.md 2026-11-26

Dynamically confirmed working as of progress.md 2026-11-27 — the
allocator returns usable, non-aliasing memory and `alloc-hello` produces
its expected result under qemu.

`ps2_rt::heap` now provides a `#[global_allocator]` (a bump allocator
backed by a fixed 64 KiB static array — intentionally not a
freelist, and intentionally not sized from `link.ld`'s
`_heap_start`/`_stack_top` gap, since deciding how much of the
remaining RAM is safe to hand to the heap vs. leave as stack headroom
isn't something to guess), and `tools/alloc-hello` exercises it. Like
everything else, this is statically checked (`just test` — including a
bss-size check now that this is the first tool with non-empty `.bss`)
but not dynamically run (#1).

64 KiB was picked as "clearly enough for a `Vec<u32>` of 8 elements,
clearly not so much it's a real resource decision" — revisit if a
future tool's actual allocation needs make that the wrong default, and
consider whether a freelist becomes necessary once something frees
memory in a loop (a pure bump allocator will exhaust the 64 KiB fast
under that pattern).

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
