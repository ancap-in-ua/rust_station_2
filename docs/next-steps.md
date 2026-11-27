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
reach: a real console. And PCSX2 is not a console — that distinction
has stopped being theoretical, see #2: the solid-colour milestone is
now blocked on an emulation gap PCSX2 documents in its own source.

A BIOS dump goes in `bios_dump/` (gitignored — copyrighted, not
redistributable). Without one, `test-static` and `test-qemu` still run
and `test-pcsx2` skips loudly.

## 2. Minimal GS output — the real first milestone

**Status: `solid-color` is as correct as PCSX2 can tell, and PCSX2
structurally cannot render what it asks for.** Deciding whether the
approach actually works now needs real hardware — or a different
approach. See progress.md 2026-11-27 (third entry).

What is known:

- The tool runs end to end on PCSX2 with a real BIOS: `SetGsCrt`
  returns, CSR/IMR/SMODE2/DISPFB1/DISPLAY1/BGCOLOR/PMODE are all
  written in ps2sdk's own order, it publishes `0x5011d000` and halts.
  `just test-pcsx2` asserts all of that.
- The screen is nevertheless 100% black, and **that is a PCSX2
  limitation, not evidence about the approach.** `GSRenderer::Merge`
  returns without presenting a frame when neither read circuit has a
  framebuffer texture, which is exactly our case since we deliberately
  never write a pixel to VRAM. PCSX2's own comment on that early-out
  reads "Both circuits off still outputs BGCOLOR on real hardware."
  Also tried with both circuits disabled: still black.

Fixed along the way (progress.md has the detail):

- **`SetGsCrt`'s mode argument was wrong** — we passed ps2sdk's
  `GRAPH_MODE_NTSC` (1), which is an *index* into `graph_mode[]`. The
  value the syscall wants is that row's `.mode` field, `0x02`
  (= libgs's `GS_MODE_NTSC`). Now correct.
- **CSR reset, IMR and SMODE2 were never written.** ps2sdk's
  `graph_set_mode` does all three before/around `SetGsCrt`; we now do
  the same.

What got resolved earlier and shouldn't be re-litigated:

- "How does the EE even address `0x12000000`?" — identity-mapped, no
  segment prefix needed (ps2tek's EE memory map, re-confirmed on PCSX2:
  a raw store there from a user-mode ELF takes no TLB miss).
- "Where do NTSC/PAL sync-timing constants come from?" — they don't;
  the GS Users Manual says that's a BIOS kernel service
  (`ps2_rt::syscall::set_gs_crt`), not something application code
  hand-derives.
- PMODE/BGCOLOR/DISPFB/DISPLAY bit layouts — confirmed against the GS
  Users Manual directly. Note the *bit layouts* are confirmed; "they
  add up to a red screen" is precisely what remains unconfirmed.

Two ways forward, answering different questions:

1. **Run it on a real console** (needs #5, deployment). The only thing
   that can settle whether the PMODE/BGCOLOR trick works. Cheap to try
   if hardware and a loader are available — the ELF is loadable now.
2. **Put actual pixels in VRAM via GIF/DMA.** This was filed as the
   "fallback"; it is now the only route to a visible result *under
   emulation*, because PCSX2 needs a real framebuffer texture before it
   composites anything. It is also what every real PS2 program does, so
   it has to happen eventually regardless.

   Substantially bigger than anything so far: GIF packet construction
   (a GIFtag plus register writes), driving a DMA channel to the GIF,
   and VRAM addressing — with no C library to crib conventions from.
   ps2sdk's `ee/draw` and `ee/dma` are the reference to read first, and
   the lesson from the mode-number bug applies: read the *call site*,
   not just the constant next to it.

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
isn't something to guess), and `tools/alloc-hello` exercises it. This
is checked at all three layers now — static (including a bss-size
check), qemu, and real PCSX2, where the heap read back over PINE shows
the bump allocator's signature directly: the dead 4-element `Vec`
unreclaimed, then the live 8-element one.

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
