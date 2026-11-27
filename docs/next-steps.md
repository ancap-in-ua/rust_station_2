# Next steps

Ordered roughly by what unblocks the most follow-on work. See
[progress.md](progress.md) for what's already in place and what's been
verified vs. just assumed.

## 1. Run it on something that is actually a PS2 (highest priority)

**Partly done as of progress.md 2026-11-27** — read that entry before
picking this up, because what's left is now much narrower than it was.

`just test-qemu` executes the unmodified release ELFs under
`qemu-system-mips64el`, which needs no BIOS dump. That settled everything
CPU- and RAM-level: `_start`'s assembly runs, `$gp`/`$sp` come out
matching `link.ld`, `zero_bss` genuinely zeroes bss (checked
non-vacuously, by dirtying it before boot), the bump allocator hands out
usable non-aliasing memory, and both proof-of-life values land where the
README says they do. "Suspect `$gp`/`$sp` setup first" is no longer the
right advice — that path is now the *best*-tested part of the boot
sequence, not the least.

What qemu cannot touch, and therefore what this item is now *only*
about: the GS, BIOS syscalls, and anything R5900-specific (MMI, the
non-IEEE FPU, scratchpad, cache). See the end of progress.md 2026-11-27,
or the header of `scripts/lib-qemu.sh`, for the exhaustive list. A green
`just test-qemu` is not evidence that anything works on a PS2.

So this still needs a legally-obtained PS2 BIOS dump, which isn't
something this repo or the flake can provide. Run `pcsx2-qt -setupwizard`
once to configure one, then:

```
just run-pcsx2 solid-color
```

and see whether the screen turns red (#2). If it doesn't, the thing to
suspect is the PMODE-blend hypothesis or the register values — *not* the
boot path, which qemu has already cleared. `just run-pcsx2` (hello) and
`just run-pcsx2 alloc-hello` are worth a sanity pass too, but qemu has
already confirmed both write their expected words (`0xC0FFEE42` and
`0xA110008C` at `0x1000`), so a failure there would specifically mean
"PCSX2/real loader differs from qemu", which is itself the useful signal.

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
  segment prefix needed (confirmed against ps2tek's EE memory map).
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

What's actually still open:

- Run it on a PS2. Until #1 is unblocked (a BIOS dump), there is no way
  to tell whether `solid-color` really shows red, or hangs, or shows
  something else. qemu (progress.md 2026-11-27) got as far as it can
  here: `just test-qemu` confirms execution reaches `set_gs_crt` with
  the right arguments in the right registers — dynamically, which the
  static checks couldn't — and then stops, because the BIOS syscall
  can't complete without a BIOS. Everything past that instruction is
  still only internal consistency (the numbers match their stated
  formulas) and consistency with ps2sdk.
- If it turns out *not* to work once run, the fallback is the original
  plan: issue a DMA transfer with a GIF packet to push an actual
  primitive (even a full-screen rectangle) to the GS. That's real
  hardware programming with no C library to crib conventions from —
  budget real time for it if it comes to that.

## 3. Validate the target spec dynamically, not just statically

Largely done as of progress.md 2026-11-27: `just test` runs
`test-static` (ELF invariants — entry point, `_gp`/`_stack_top`
formulas, bss, `.got`) and then `test-qemu`, which executes each ELF and
asserts what it did. Code generated from the hand-written target spec
demonstrably runs on a MIPS III CPU model, which was the core open
question here.

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
