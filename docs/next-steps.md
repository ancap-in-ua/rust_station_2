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

Once `hello` is confirmed, run `just run-pcsx2 solid-color` to check #2
below, and `just run-pcsx2 alloc-hello` to check #4's allocator (watch
for `0xA110_0000 | 140` = `0xA110008C` at `0x1000`, same proof-of-life
pattern as `hello`).

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

- Run it. Until #1 is unblocked (a BIOS dump), there is no way to tell
  whether `solid-color` really shows red, or hangs, or shows something
  else — everything checked so far is internal consistency (the numbers
  match their stated formulas) and consistency with ps2sdk, not a
  hardware/emulator result.
- If it turns out *not* to work once run, the fallback is the original
  plan: issue a DMA transfer with a GIF packet to push an actual
  primitive (even a full-screen rectangle) to the GS. That's real
  hardware programming with no C library to crib conventions from —
  budget real time for it if it comes to that.

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

## 4. Pick the second tool to port — done for `alloc`, see progress.md 2026-11-26

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
