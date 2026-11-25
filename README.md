# rust_station_2

Bare-metal Rust tooling for the PS2's Emotion Engine (MIPS R5900). No
ps2sdk/ee-gcc dependency — pure Rust + a hand-rolled boot shim, targeting a
custom LLVM target spec since rustc has no built-in PS2 target.

## Layout

- `targets/mips64r5900el-ps2.json` — custom rustc target spec for the EE.
- `ps2-rt/` — runtime support crate: entry point (`_start`), linker script
  (`link.ld`), bss zeroing, panic handler. Every tool depends on this.
- `tools/hello/` — minimal example tool / template for new tools.
- `.cargo/config.toml` — pins the custom target as default and configures
  `build-std` (core has no prebuilt artifacts for this target).
- `rust-toolchain.toml` — pins nightly + `rust-src` + `llvm-tools-preview`,
  auto-applied by rustup when you're in this directory.
- `flake.nix` — `nix develop` shell with PCSX2, cargo-binutils, gdb, just.
  Deliberately does **not** provide rustc/cargo — those come from your
  existing rustup install, version-pinned by `rust-toolchain.toml`.

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
}
```

## Building / running

```
nix develop
just build          # cargo build --release
just run-pcsx2       # boot tools/hello in PCSX2 (pcsx2-qt under the hood)
just size            # section sizes (cargo-binutils)
just objdump          # disassembly (cargo-binutils)
just test            # static checks only -- see "Testing" below
```

PCSX2 needs a legally-obtained BIOS dump configured (first-run setup wizard,
or `pcsx2-qt -setupwizard`) before it can boot anything, ELF included —
that's on you to provide, nixpkgs obviously doesn't ship one.

Cargo commands must be run from the workspace root — the linker script path
in `.cargo/config.toml` is relative.

## Testing

`just test` (= `scripts/verify-elf.sh`) is a static check only: it builds
`hello` and confirms the resulting ELF's structure (entry point,
`_gp`/`_stack_top` formulas, bss) still matches what was manually verified
with `readelf`/`nm` when this was scaffolded. It does **not** prove
anything boots — that needs PCSX2 with a BIOS configured, see below.

`cargo test` does not work in this workspace: `.cargo/config.toml`'s
global `build-std = ["core"]` (required for the no-std PS2 target)
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
  (`0x1000`) — check it landed via PCSX2's memory viewer or a GDB `x`
  command. Driving the GS (writing GIF packets to set up a display buffer)
  is real, nontrivial work and intentionally not faked here.
- **`$gp`/`_gp` setup is defensive, possibly unnecessary.** With
  `+noabicalls` and a static relocation model, generated code should mostly
  use absolute addressing; this is just a safety net.
- **`max-atomic-width` is conservatively set to 32** pending confirmation
  of which atomic RMW widths the R5900 actually implements.
- **Floating point is declared `+single-float`** (hardware single only,
  `f64` falls back to softfloat via compiler-builtins) to match the EE's
  FPU, but the EE's non-IEEE-754 rounding/exception behavior isn't modeled
  — don't trust exact float results without testing.
- **Real-hardware deployment isn't wired up.** The flake only gets you
  PCSX2. Getting a built `.elf` onto real hardware (network boot via
  ps2link, USB via OPL, etc.) needs ps2dev-side client tools that aren't
  packaged in nixpkgs — for now, build the ELF here and hand it to
  whatever loader you're already using.
- `scripts/run-pcsx2.sh` hardcodes a `-elf` flag that may not match your
  installed PCSX2 version's CLI — check `pcsx2 --help` if it doesn't boot.

Treat the first real milestone as: get *something* visibly different on
screen (e.g. a solid color via a minimal GS/GIF driver), since that's the
first point where boot correctness, `$gp`/`$sp` setup, and bss zeroing are
all actually exercised end to end.
