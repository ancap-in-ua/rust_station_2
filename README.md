# rust_station_2

Bare-metal Rust tooling for the PS2's Emotion Engine (MIPS R5900). No
ps2sdk/ee-gcc dependency — pure Rust + a hand-rolled boot shim, targeting a
custom LLVM target spec since rustc has no built-in PS2 target.

## Layout

- `targets/mips64r5900el-ps2.json` — custom rustc target spec for the EE.
- `ps2-rt/` — runtime support crate: entry point (`_start`), linker script
  (`link.ld`), bss zeroing, panic handler, bump allocator, GS/BIOS-syscall
  plumbing. Every tool depends on this.
- `tools/hello/` — minimal example tool / template for new tools.
  `tools/alloc-hello/` exercises the allocator; `tools/solid-color/`
  attempts the solid-red-screen milestone.
- `scripts/` — `verify-elf.sh` + `verify-solid-color.sh` (static ELF
  checks), `verify-qemu.sh` + `verify-solid-color-qemu.sh` (dynamic, run
  the ELF under qemu), `lib-qemu.sh` (shared qemu/gdb plumbing, and the
  authoritative list of what emulation does and doesn't prove),
  `run-pcsx2.sh` / `debug-qemu.sh` (launchers).
- `.cargo/config.toml` — pins the custom target as default and configures
  `build-std` (core/alloc have no prebuilt artifacts for this target).
- `rust-toolchain.toml` — pins nightly + `rust-src` + `llvm-tools-preview`,
  auto-applied by rustup when you're in this directory.
- `flake.nix` — `nix develop` shell with PCSX2, qemu, cargo-binutils, gdb,
  just. Deliberately does **not** provide rustc/cargo — those come from
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

To wire a new tool into `just test`, add it to the `test-static` and
`test-qemu` recipes in the `justfile` — the latter takes the
proof-of-life word you expect it to write to `0x1000`.

## Building / running

```
nix develop
just build           # cargo build --release
just test            # static + dynamic checks -- see "Testing" below
just run-pcsx2       # boot tools/hello in PCSX2 (pcsx2-qt under the hood)
just debug-qemu      # boot tools/hello under qemu + interactive gdb
just size            # section sizes (cargo-binutils)
just objdump         # disassembly (cargo-binutils)
```

PCSX2 needs a legally-obtained BIOS dump configured (first-run setup wizard,
or `pcsx2-qt -setupwizard`) before it can boot anything, ELF included —
that's on you to provide, nixpkgs obviously doesn't ship one.

Cargo commands must be run from the workspace root — the linker script path
in `.cargo/config.toml` is relative.

## Testing

`just test` runs two layers, neither of which needs a BIOS dump.

**`just test-static`** confirms each built ELF's structure (entry point,
`_gp`/`_stack_top` formulas, bss size, `.got` contents) still matches
what was manually verified with `readelf`/`nm` when this was scaffolded,
plus tool-specific checks (`scripts/verify-solid-color.sh` pins the
hand-written BIOS syscall stub's compiled bytes).

**`just test-qemu`** actually executes the unmodified release ELFs under
`qemu-system-mips64el` and asserts what they did: that they reach their
terminal state rather than panicking or hanging, that `$gp`/`$sp` match
the linker script, that `boot.rs`'s `zero_bss` really zeroes bss (checked
by dirtying the whole range *before* boot, so the check can't pass
vacuously), and that each tool's proof-of-life word lands correctly —
which for `alloc-hello` means the bump allocator handed out usable
memory. `just debug-qemu <tool>` is the interactive version.

This works because `link.ld` loads at `0x00100000` with
`PhysAddr == VirtAddr`, and a MIPS CPU at reset (`Status.ERL = 1`)
identity-maps kuseg to physical RAM, so qemu can run the image with no
test-only build.

> **Do not read a green `just test-qemu` as "this would run on a PS2".**
> It currently would not: the ELFs are ELF64, and PCSX2's loader is
> ELF32-only — it segfaults on them. qemu's generic ELF loader is
> simply more permissive than any real PS2 loader. See docs/progress.md
> 2026-11-27 ("a real BIOS arrives") for that and two other assumptions
> that running on actual PCSX2 invalidated.

**qemu is not a PS2, and a green `just test-qemu` is not evidence that
anything works on one.** It has no GS (so `solid-color`'s actual claim —
the screen turning red — is still unverified), no BIOS (so no syscall can
complete), and none of the R5900's specifics: the 128-bit MMI
instructions, the non-IEEE-754 FPU, scratchpad RAM, cache behaviour, DMA,
the GIF, VU0/VU1, SIF/IOP. Code relying on any of those passes here and
fails on hardware. The header of `scripts/lib-qemu.sh` has the full list;
`docs/next-steps.md` #1 covers what's left.

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

- **No display/console output yet.** `tools/hello` only proves the CPU
  reached Rust code, by writing a magic value to a fixed RAM address
  (`0x1000`) — `just test-qemu` asserts it landed, or check by hand via
  PCSX2's memory viewer or a GDB `x` command. `tools/solid-color`
  attempts a solid red screen without touching the GIF at all (by
  forcing the PCRTC's merge circuit to take 100% of its color from
  `BGCOLOR`), but whether that actually works is unverified — it needs a
  BIOS. Driving the GS properly (writing GIF packets to set up a display
  buffer) is real, nontrivial work and intentionally not faked here.
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
