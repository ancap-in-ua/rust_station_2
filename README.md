# rust_station_2

Bare-metal Rust tooling for the PS2's Emotion Engine (MIPS R5900). No
ps2sdk/ee-gcc dependency — pure Rust + a hand-rolled boot shim, targeting a
custom LLVM target spec since rustc has no built-in PS2 target.

## Layout

- `targets/mipsel-sony-ps2.json` — custom rustc target spec for the EE:
  32-bit MIPS, o32 ABI, `mips3` CPU, hardware single-float. ELF32 is
  not optional — PCSX2's loader segfaults on ELF64 (docs/progress.md
  2026-11-27).
- `ps2-rt/` — runtime support crate: entry point (`_start`), linker script
  (`link.ld`), bss zeroing, panic handler, bump allocator, BIOS syscalls,
  and graphics — `gs` (PCRTC display registers *and* the GS drawing
  registers), `gif` (GIF packets + DMA to the GIF), `draw` (framebuffer
  fills). Every tool depends on this.
- `tools/hello/` — minimal example tool / template for new tools.
  `tools/alloc-hello/` exercises the allocator. For graphics:
  `tools/fb-fill/` fills the screen by drawing a sprite through a GIF
  packet over DMA (this is the one that actually puts a colour on
  screen), `tools/color-cycle/` animates red/green/blue on the same
  path and is the one to run on real hardware, and
  `tools/solid-color/` keeps the PCRTC merge-circuit shortcut that
  PCSX2 can't render.
- `scripts/` — three layers of checks: `verify-elf.sh` +
  `verify-solid-color.sh` (static), `verify-qemu.sh` +
  `verify-solid-color-qemu.sh` (run the ELF under qemu, no BIOS needed),
  and `verify-pcsx2.sh` (run it on a real PS2). Plus `lib-qemu.sh` /
  `lib-pcsx2.sh` (shared plumbing, and the authoritative notes on what
  each emulator does and doesn't prove), `pine.py` (reads EE memory out
  of a running PCSX2), and the `run-pcsx2.sh` / `debug-qemu.sh`
  launchers.
- `bios_dump/` — where a PS2 BIOS dump goes (gitignored; see "Testing").
- `.cargo/config.toml` — pins the custom target as default and configures
  `build-std` (core/alloc have no prebuilt artifacts for this target).
- `rust-toolchain.toml` — pins nightly + `rust-src` + `llvm-tools-preview`,
  auto-applied by rustup when you're in this directory.
- `flake.nix` — `nix develop` shell with PCSX2, qemu, cargo-binutils, gdb,
  python3, just. Deliberately does **not** provide rustc/cargo — those come from
  your existing rustup install, version-pinned by `rust-toolchain.toml`.

## Adding a new tool

Copy `tools/hello`, rename the package, add it to the root `Cargo.toml`
workspace `members`. Every tool crate needs:

```rust
#![no_std]
#![no_main]
use ps2_rt as _;

#[no_mangle]
extern "C" fn rust_main() -> ! {
    // ...
    ps2_rt::ps2_rt_halt()
}
```

End in `ps2_rt::ps2_rt_halt()` rather than a bare `loop {}`: it's the
same infinite spin, but it's a named symbol, which is how
`scripts/verify-qemu.sh` knows the tool finished (and how it tells
"finished" apart from "panicked", which lands in `ps2_rt_panicked`).
A tool whose `rust_main` ends in `loop {}` can't be checked by it.

To wire a new tool into `just test`, add it to the `test-static`,
`test-qemu` and `test-pcsx2` recipes in the `justfile` — the latter two
take the proof-of-life word you expect it to publish via
`ps2_rt::set_proof`.

## Building / running

```
nix develop
just build           # cargo build --release
just test            # static + dynamic checks -- see "Testing" below
just run-pcsx2       # boot tools/hello in PCSX2 (needs a BIOS dump)
just debug-qemu      # boot tools/hello under qemu + interactive gdb
just size            # section sizes (cargo-binutils)
just objdump         # disassembly (cargo-binutils)
```

Anything involving PCSX2 needs a legally-obtained BIOS dump — drop one in
`bios_dump/` (gitignored, since it's copyrighted Sony code and not
redistributable; nixpkgs obviously doesn't ship one). The scripts
configure PCSX2 themselves from there, into a `.pcsx2/` directory in the
repo, so none of this touches your own `~/.config/PCSX2`.

Cargo commands must be run from the workspace root — the linker script path
in `.cargo/config.toml` is relative.

## Testing

`just test` runs three layers, each of which can see things the one
before it can't. The first two need no BIOS dump; the third skips
loudly without one.

**`just test-static`** checks each built ELF's structure — ELF class,
entry point, `_gp`/`_stack_top` formulas, bss size, `.got` contents —
plus tool-specific checks (`verify-solid-color.sh` pins the hand-written
BIOS syscall stub's compiled bytes).

**`just test-qemu`** executes the unmodified release ELFs under
`qemu-system-mips64el` and asserts what they did: that they reach their
terminal state rather than panicking or hanging, that `$gp`/`$sp` match
the linker script, that `boot.rs`'s `zero_bss` really zeroes bss
(checked by dirtying the whole range *before* boot, so it can't pass
vacuously), and that each tool's proof-of-life word lands. This works
because `link.ld` loads at `0x00100000` with `PhysAddr == VirtAddr`, and
a MIPS CPU at reset (`Status.ERL = 1`) identity-maps kuseg to physical
RAM — so qemu runs the real image with no test-only build.
`just debug-qemu <tool>` is the interactive version.

**`just test-pcsx2`** runs each ELF on an actual PS2: PCSX2 with a real
BIOS. It asserts PCSX2's loader accepts the ELF, that it starts at the
expected entry point, that the proof-of-life value arrives (read over
PCSX2's PINE socket, not by eye), and that the BIOS kernel reports no
TLB miss.

**Only the third layer proves anything about a PlayStation 2.** This
isn't theoretical: on 2026-11-27 the first two were fully green while
the ELFs were ELF64 — unloadable by every real PS2 loader, and enough
to *segfault* PCSX2 — and wrote their results to an address that faults
on hardware. qemu's generic ELF loader is simply more permissive than a
PS2's.

qemu also has no GS, no BIOS (so no syscall can complete), and none of
the R5900's specifics: 128-bit MMI instructions, the non-IEEE-754 FPU,
scratchpad RAM, cache behaviour, DMA, the GIF, VU0/VU1, SIF/IOP. The
header of `scripts/lib-qemu.sh` has the full list. And PCSX2 is not a
console either — that distinction is no longer theoretical: the
solid-colour milestone is blocked on an emulation gap PCSX2 documents
in its own source. See `docs/next-steps.md` #2.

`cargo test` does not work in this workspace: `.cargo/config.toml`'s
global `build-std = ["core", "alloc"]` (required for the no-std PS2 target)
conflicts with linking the host's prebuilt `std` that a test harness
needs, and Cargo has no supported way to scope `build-std` to one target
only (array config values merge across sources instead of letting an
inner one override — confirmed, several approaches tried; see
docs/progress.md 2026-11-25). `ps2-rt`'s pure logic is instead checked with
`const` assertions, which run as part of the normal build.

## Why bare-metal instead of ps2sdk

`ps2dev`/`ps2sdk` (the EE GCC toolchain + homebrew C libraries — stdio,
pad, gsKit graphics, memory card, etc.) aren't packaged in nixpkgs, and
building `ee-gcc` from source in a flake is slow and fragile. This setup
avoids that entirely: nightly Rust + `rust-lld` + `-Z build-std` is enough
to produce a working ELF, at the cost of having no libc — everything
(console output, graphics, controller input, file I/O) has to be
implemented from scratch against the hardware/BIOS directly.

If this gets limiting, the FFI-on-ps2sdk approach is the fallback — ask to
restructure if you hit that wall.

## Known caveats / open work

This target spec and boot shim are a best-effort starting point, not a
validated reference implementation — the EE is a nonstandard MIPS III
variant (128-bit "paired" GPRs for multimedia instructions, FPU is
single-precision only and not IEEE-754 compliant) and LLVM's generic MIPS
backend knows nothing about any of that. Specifically unverified/TODO:

- **No display/console output yet — and this is now the main open
  problem.** `tools/hello` only proves the CPU reached Rust code, by
  publishing a magic value through `ps2_rt::set_proof`.
  `tools/solid-color` attempts a solid red screen without touching the
  GIF at all (forcing the PCRTC's merge circuit to take 100% of its
  color from `BGCOLOR`, configured for PAL). As of 2026-11-27 it runs
  end to end on PCSX2 with a real BIOS — syscall returns, every
  register written — and the screen stays black, because **PCSX2
  cannot render what it asks for**:
  its merge circuit returns without presenting a frame when no read
  circuit has a framebuffer texture, which is exactly the situation the
  BGCOLOR trick creates. PCSX2's own comment there reads "Both circuits
  off still outputs BGCOLOR on real hardware." Settling it needs a real
  console; getting a visible result *under emulation* needs the GIF/DMA
  path. See `docs/next-steps.md` #2.
- **`$gp`/`_gp` setup is defensive, possibly unnecessary.** With
  `+noabicalls` and a static relocation model, generated code should mostly
  use absolute addressing; this is just a safety net. (It does at least
  demonstrably work: `just test-qemu` confirms `$gp` holds `link.ld`'s
  `_gp` after boot. Whether anything ever *reads* it is the open part.)
- **`max-atomic-width` is conservatively set to 32** pending confirmation
  of which atomic RMW widths the R5900 actually implements.
- **Floating point is declared `+single-float`** (hardware single only,
  `f64` falls back to softfloat via compiler-builtins) to match the EE's
  FPU, but the EE's non-IEEE-754 rounding/exception behavior isn't modeled
  — don't trust exact float results without testing.
- **Real-hardware deployment isn't wired up.** The flake only gets you
  PCSX2 and qemu. Getting a built `.elf` onto real hardware (network boot
  via ps2link, USB via OPL, etc.) needs ps2dev-side client tools that
  aren't packaged in nixpkgs — for now, build the ELF here and hand it to
  whatever loader you're already using.
- `scripts/run-pcsx2.sh` hardcodes a `-elf` flag that may not match your
  installed PCSX2 version's CLI — check `pcsx2 --help` if it doesn't boot.

Boot correctness, `$gp`/`$sp` setup, and bss zeroing *are* now exercised
end to end — under qemu, which needs no BIOS (see "Testing"). The
remaining milestone is the one qemu can't reach: get something visibly
different on screen on an actual PS2 (PCSX2 or hardware), which is the
first point where the GS and the BIOS are exercised at all.
