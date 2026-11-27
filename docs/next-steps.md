# Next steps

Ordered roughly by what unblocks the most follow-on work. See
[progress.md](progress.md) for what's already in place and what's been
verified vs. just assumed.

## 1. Make the ELF loadable by a real PS2 loader (highest priority)

**Status changed completely on 2026-11-27.** A BIOS dump arrived, the
ELFs were booted on PCSX2 for the first time, and they don't work. Read
progress.md 2026-11-27 ("a real BIOS arrives") in full before touching
anything here. In short:

- **The target spec emits ELF64; PCSX2's loader is ELF32-only and
  segfaults on our binaries.** This is the blocker for everything else.
  A hand-assembled ELF32 with the same load address and entry point
  loads and runs, so nothing else about the image layout is wrong.
- **`0x1000` (the proof-of-life address) is not writable** from a
  user-mode ELF — TLB miss. The first MiB is the BIOS kernel's.
- **`ps2_rt::syscall::set_gs_crt` hangs and never returns**, cause
  unknown (see #2).

The work, in order:

1. **Re-target to ELF32.** This is a pointer-width change (64 → 32),
   not a flag tweak. `targets/mips64r5900el-ps2.json` currently asks
   for `mips64` + n64, which is what produces ELF64; PS2 toolchains
   (ee-gcc/ps2sdk) use 32-bit `mips-r5900-elf` with the **o32** ABI.
   Touches the target JSON, `link.ld`'s assumptions, `boot.rs`, and
   every expectation in `verify-elf.sh` — which currently *asserts*
   `ELF class == ELF64`, i.e. asserts the bug.
2. **Replace the proof-of-life convention.** Writing to a fixed low
   address can't work. Either write into memory the program owns (a
   `static` in its own image — markers at `0x100800` read back fine) or
   go through kseg0 (`0x8000_0000`, cached) / kseg1 (`0xA000_0000`,
   uncached) for a genuinely fixed physical address.
3. **Then** re-run everything on PCSX2 and see what the next layer of
   wrong assumptions turns out to be.

Harness notes for whoever picks this up: PCSX2 is scriptable. Run it
with `-datapath <dir>` to keep its state out of `~/.config/PCSX2`, set
`[UI] SetupWizardIncomplete = false` (otherwise it silently refuses to
boot anything), `[Filenames] BIOS = <file>`, and `[EmuCore] EnablePINE =
true`. PINE then gives scripted EE memory reads over a unix socket at
`$XDG_RUNTIME_DIR/pcsx2.sock` — enough to assert proof-of-life values
without a human looking at a memory viewer. Note PINE reads EE RAM
only; GS privileged registers read back as zero through it whether or
not a write landed, so it can't be used to check those. The ELF must be
passed as a positional argument (`-elf <file>` alone boots nothing) and
needs a `.elf` extension.

## 2. Minimal GS output — the real first milestone

`hello` currently proves nothing visible. The first milestone that
actually exercises boot correctness, `$sp`/`$gp` setup, and bss zeroing
end-to-end is: **get a solid color on screen.** As of progress.md
2026-11-26 (both entries), this is *implemented*, sourced, and
statically checked (`tools/solid-color`, `ps2-rt/src/gs.rs`,
`ps2-rt/src/syscall.rs`, `scripts/verify-solid-color.sh`) — what's left
is entirely #1: nothing here has run anywhere.

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

What's actually still open — **and the third bullet above is no longer
"resolved", see below**:

- **`set_gs_crt` hangs and never returns on PCSX2.** Tested on
  2026-11-27 with a hand-assembled ELF32 doing exactly what the Rust
  stub does, with progress markers either side of the `syscall`:
  execution reaches it and never comes back, still hung after 25
  seconds. Ruled out: missing stack, `-fastboot` vs `-slowboot`, bad
  CP0 state (user mode, `EXL = 0`, `CU0 = 1` — all normal), and the
  negative/kernel-mode syscall number. Removing the `syscall`
  instruction lets everything else run to completion. Cause unknown.
  **This is the blocker for the whole milestone**, and no amount of
  static byte-checking can see it — `verify-solid-color.sh` passes,
  because the stub's bytes really are what we intended.
- The PMODE-blend approach is therefore **untested, not confirmed**,
  despite what the bullet list above used to say. With the syscall
  removed and all four GS registers written with the const-asserted
  values, PCSX2 shows a 100% black screen — but `SetGsCrt` is what
  programs the PCRTC's video timings, so with no timings there is
  plausibly no signal to blend and black is expected either way. The
  hypothesis can't be judged until the syscall works.
- Worth reconsidering whether `SetGsCrt` is the right mechanism at all.
  If the syscall can't be made to work, the alternatives are writing
  `SMODE1`/`SYNCH1`/`SYNCH2`/`SYNCV` directly (which the GS Users
  Manual explicitly says is not how application code is meant to do it
  — see 2026-11-26) or finding how ps2sdk's `graph_set_mode` actually
  reaches the kernel on a booted-from-ELF system.
- If the PCRTC approach turns out not to work once it can be judged,
  the fallback is the original plan: issue a DMA transfer with a GIF
  packet to push an actual primitive (even a full-screen rectangle) to
  the GS. That's real hardware programming with no C library to crib
  conventions from — budget real time for it if it comes to that.

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
