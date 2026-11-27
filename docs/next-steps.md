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
has stopped being theoretical: the PCRTC merge-circuit approach in
`tools/solid-color` cannot be rendered by PCSX2 at all (#2), which is
what forced the GIF/DMA drawing path. That path then worked, so the gap
cost a detour rather than the milestone.

A BIOS dump goes in `bios_dump/` (gitignored — copyrighted, not
redistributable). Without one, `test-static` and `test-qemu` still run
and `test-pcsx2` skips loudly.

## 2. Minimal GS output — DONE under emulation (2026-11-27)

**The screen is red.** `tools/fb-fill` draws a full-screen sprite via a
GIF packet over DMA and PCSX2 renders it; `tools/color-cycle` animates
red/green/blue on the same path. See progress.md 2026-11-27 (fifth
entry).

This went the long way round. The PCRTC merge-circuit shortcut
(`tools/solid-color`) turned out to be unrenderable in PCSX2 —
`GSRenderer::Merge` returns without presenting a frame when no read
circuit has a framebuffer texture, which is exactly what a tool that
never writes VRAM produces. Drawing real pixels was filed as the
"fallback" and was in fact the only route.

Fixed along the way (progress.md has the detail): `SetGsCrt`'s mode
argument was a table index rather than the mode value; CSR reset, IMR
and SMODE2 were never written; and every GS register write was
compiling to a pair of 32-bit `sw`s instead of one `sd`.

What's left here:

- **Run `color-cycle` on a console.** Much more meaningful now: it no
  longer depends on the unverifiable BGCOLOR path, so it exercises the
  same code that demonstrably works under emulation. Needs #5.
- `tools/solid-color` keeps the PCRTC trick. Still unjudgeable on
  PCSX2, still worth a look if anyone runs it on hardware.
- **Double buffering and VSync.** Nothing currently waits for a
  vertical blank; `color-cycle` just busy-loops. Fine for flat fills,
  not for anything that moves.

## 2a. Text output on screen (next, and the thing that unblocks the rest)

With a framebuffer that demonstrably draws, text is a font blitted into
it. Two routes: a glyph-per-sprite loop using the primitives already in
`ps2_rt::draw`, or uploading a font texture to VRAM and drawing
textured sprites. The first needs no new GIF modes and is the obvious
starting point.

This matters more than it looks. Debugging is currently "write one u32
to `PS2_RT_PROOF` and read it over PINE" — fine for one value, useless
for anything structured. On-screen text is also the only output channel
that works on the user's actual console (no serial port on a Slim, see
2b). Everything after this — a menu, a file browser, a launcher — needs
it.

## 2b. Text output over EE SIO (small, unblocks everything else)

The EE has a debug UART — bytes written to `SIO_TXFIFO` (`0x1000_F180`)
are printed by PCSX2 (`EnableEEConsole` / `EnableEESIOInput` in its
config). It is a plain memory-mapped register write, the same mechanism
as `gs::write_reg`, so it needs no GS, no DMA, no VRAM and no BIOS
call.

This would give a host-side log channel, which is complementary to #2a
rather than a substitute: on-screen text is what works on the console,
SIO is what's convenient while iterating in PCSX2. Lower priority than
#2a for that reason.

Two caveats: one source suggests PCSX2 only prints SIO in debug builds
or interpreter mode (unverified — cheap to test). And it does **not**
work on a real console: the Slim has no exposed serial port, so SIO
there needs a hardware cable mod. On hardware, output means the screen,
or `ps2link` over Ethernet with the DECI2 protocol implemented.

## 3. Validate the target spec dynamically, not just statically

`just test` runs `test-static` (ELF invariants — entry point,
`_gp`/`_stack_top` formulas, bss, `.got`) and then `test-qemu`, which
executes each ELF and asserts what it did. Code generated from the
hand-written target spec demonstrably runs on a MIPS III CPU model.

**That is a much weaker result than it sounded on the morning of
2026-11-27.** Two separate defects that day were invisible to every
layer of the suite and obvious in one disassembly:

- the ELF class (ELF64, which no real PS2 loader accepts) — which
  `verify-elf.sh` was actively asserting as *correct*;
- every GS register write compiling to a pair of 32-bit `sw`s instead
  of one `sd`, after the o32 re-target, because
  `write_volatile::<u64>` does not emit a 64-bit store on a 32-bit
  target.

Both were "the source says the right thing, the compiler did something
else." Standing habit worth keeping: **when a register has a documented
access width, check the emitted instruction, not the source.** Treat
"the checks are green" as "nothing changed since we last chose what to
look at."

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
