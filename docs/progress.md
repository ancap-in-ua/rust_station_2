# Progress log

## 2026-11-23 — initial scaffold

### Decision: bare-metal Rust, no ps2sdk

Checked nixpkgs first: `pcsx2` (the emulator) is packaged, `ps2dev`/`ps2sdk`
(the EE GCC toolchain + homebrew C libraries — stdio, pad, gsKit graphics,
memory card, etc.) are not. Building `ee-gcc` from source inside a flake
would be slow and fragile, so the project goes pure bare-metal instead:
nightly Rust + `rust-lld` + `-Z build-std`, no C dependency at all. Cost:
no libc, so console output, graphics, controller input, and file I/O all
have to be built from scratch against the hardware — see
[next-steps.md](next-steps.md).

Also decided: PCSX2 for fast iteration, with real hardware as a secondary
target (not yet wired up — see next steps); general workspace scaffold for
multiple future tools, not a single specific port.

### What exists now

- `targets/mips64r5900el-ps2.json` — custom rustc target spec for the
  Emotion Engine (MIPS R5900). Rust has no built-in PS2 target (unlike the
  PS1's `mipsel-sony-psx`), so this had to be hand-built from rustc's
  built-in `mips64el-unknown-linux-gnuabi64` and `mipsel-unknown-none`
  specs: `mips64` arch, `n64`/`abi64` ABI, little-endian, `mips3` CPU
  (plain MIPS III instruction set — EE lacks MIPS32r2+ instructions like
  `ext`/`ins`/`rotr`), `+single-float` (EE's FPU is hardware single
  precision only, no double), `+noabicalls` + static relocation (no
  position-independent code), `panic-strategy: abort`, `os: none`.
- `ps2-rt/` — the runtime crate every tool depends on:
  - `link.ld` — loads at `0x00100000` (the conventional PS2 homebrew load
    address, leaving the first MiB for BIOS/kernel), lays out
    `.text`/`.rodata`/`.data`/`.sdata`/`.got`/`.bss`, defines `_gp`,
    `__bss_start`/`__bss_end`, `_stack_top` (top of the 32 MiB - 1 MiB RAM
    region).
  - `src/boot.rs` — `_start`, written in raw MIPS assembly via
    `global_asm!` (needs `#![feature(asm_experimental_arch)]`, inline asm
    isn't stabilized for MIPS). Sets up `$gp` and `$sp` from the linker
    symbols, jumps to `rt_start`. `rt_start` zeroes `.bss` then calls the
    binary crate's `rust_main`.
  - `src/lib.rs` — the `#[panic_handler]` (infinite loop).
- `tools/hello/` — template/proof-of-life tool. Writes a magic value
  (`0xC0FFEE42`) to a fixed RAM address (`0x1000`) and spins. No
  display/console output exists yet (see next steps) — this is literally
  the smallest possible "did the CPU reach Rust code" check.
- `.cargo/config.toml` + `rust-toolchain.toml` — pin nightly (rustup, not
  nix — see below) with `rust-src`/`llvm-tools-preview`, set the custom
  target as default, and turn on the unstable cargo features actually
  required (`build-std`, `build-std-features = compiler-builtins-mem`,
  `json-target-spec`).
- `flake.nix` — `nix develop` shell. Deliberately does **not** provide
  rustc/cargo (those come from the system's existing rustup install,
  version-pinned by `rust-toolchain.toml`); only supplies tooling nixpkgs
  actually has: `pcsx2`, `cargo-binutils`, `gdb`, `just`.
- `justfile`, `scripts/run-pcsx2.sh` — build/inspect/run helpers.

### What was actually verified (not just written and hoped)

- `cargo build --release` and plain `cargo build` both succeed from a
  clean `target/` directory.
- The resulting ELF was inspected with `readelf`/`nm`:
  - `ELF64 LSB executable, MIPS, MIPS-III`, flags `noreorder, mips3` — ISA
    level matches the target spec.
  - Entry point `0x100000` matches the `_start` symbol and `.text`'s
    address, both matching `link.ld`'s intended load address.
  - `_gp` = `0x108170` = `.text` end (`0x100170`) rounded up + `0x8000`,
    matching the linker script's `_gp = . + 0x8000` formula exactly.
  - `_stack_top` = `0x2000000` = `0x100000 + (32M - 1M)`, matching
    `link.ld`'s memory region math exactly.
  - `__bss_start == __bss_end` (no statics in `hello` yet, so an empty
    region is correct, not a bug).
- `nix develop` resolves and actually provides working binaries for
  `cargo-objdump`, `rust-objdump`, `gdb`, `just`.

### Bugs caught and fixed during setup (recorded so they aren't re-hit)

- `target-pointer-width` in the target JSON must be a bare integer (`64`),
  not a string (`"64"`) — rustc's deserializer rejected the string form.
- The target spec needs an explicit `"abi": "abi64"` field alongside
  `"llvm-abiname": "n64"` — without it rustc fails with "invalid MIPS ABI
  name and `cfg(target_abi)` combination".
- `.json` target specs require `json-target-spec = true` under
  `[unstable]` in `.cargo/config.toml` on this cargo version (passing
  `-Z json-target-spec` by hand works too, but the config key is what lets
  plain `cargo build` work without extra flags).
- `build-std-features = ["panic_immediate_abort"]` does **not** work on
  this nightly (2026-11-22) — `panic_immediate_abort` changed from a cargo
  build-std feature flag to a real `-C panic=` strategy value upstream.
  Removed it; `panic = "abort"` in the workspace `Cargo.toml` profiles
  plus the target spec's `panic-strategy: abort` is sufficient.
- `core::ptr::write_bytes` (used for bss zeroing) lowers to a `memset`
  call that doesn't exist for a from-scratch `build-std` build unless
  `build-std-features = ["compiler-builtins-mem"]` is set — without it,
  linking fails with `undefined symbol: memset`.
- `global_asm!` on MIPS needs `#![feature(asm_experimental_arch)]` even
  though `global_asm!` has no per-architecture operand matching — MIPS
  just isn't on the stabilized architecture list yet.
- nixpkgs' `pcsx2` package installs the binary as `pcsx2-qt`, not
  `pcsx2`. `scripts/run-pcsx2.sh` was written assuming the latter and had
  to be corrected after actually running `nix develop` and checking.
  Confirmed working flag: `pcsx2-qt -elf <file> -fastboot`.

## 2026-11-25 — automated static checks (`just test`)

Still blocked on dynamic verification (next-steps.md #1 — no BIOS dump
configured in this environment), so this burns down the "statically
verified by hand" gap instead: turned the manual `readelf`/`nm` inspection
from the initial scaffold into something `just test` re-checks on every
change.

- `ps2-rt/src/boot.rs`: split the bss-zeroing length arithmetic out into a
  standalone `const fn bss_len`, checked with `const _: () = assert!(...)`
  items (equal bounds, normal span, and the defensive save-from-underflow
  case). Verified these actually catch regressions, not just compile: a
  deliberately wrong expected value in one of the assertions failed the
  build with `E0080 evaluation panicked`, as intended.
- The MIPS-specific pieces of `boot.rs` (`global_asm!`, the `rt_start`
  extern block, `zero_bss`) are now `#[cfg(target_arch = "mips64")]`-gated
  so the pure arithmetic stands alone.
- `scripts/verify-elf.sh` + `just test`: builds `hello` and asserts, from
  `readelf -S -W`/`nm` output, the invariants progress.md previously
  recorded as manually checked once — ELF64/MIPS/`noreorder,mips3` flags,
  entry point `0x100000`, `_gp == end-of-.text/.rodata/.data + 0x8000`,
  `_stack_top == 0x2000000`, `__bss_start == __bss_end`. Also verified this
  catches real regressions: a one-character edit to `link.ld`'s `_gp`
  formula made the script fail with a clear mismatch, and passed again
  after reverting.
- `.got` size is reported as an `info` line, not asserted on — its
  non-empty size is still an open question (see known-unverified below),
  not a settled invariant to lock in.

### Why not `cargo test`

Tried first, abandoned after confirming (not just suspecting) it can't
work as the workspace is currently structured: `.cargo/config.toml` sets
`build-std = ["core"]` globally (needed for the no-std custom PS2 target),
and that's incompatible with linking the host's prebuilt `std` — a
`cargo test` harness for `x86_64-unknown-linux-gnu` needs `std`, but it was
built against a *different* `core` than the one `build-std` rebuilds from
source for the same invocation, so linking fails with `E0152: duplicate
lang item`. Tried to scope `build-std` to just the embedded target via
(a) `--config unstable.build-std=[]`, (b) a `CARGO_UNSTABLE_BUILD_STD=`
env override, (c) a nested `ps2-rt/.cargo/config.toml` with `build-std = []`,
and (d) the same with a type-mismatched value to force a hard error instead
of a silent no-op. All four failed: array-valued Cargo config keys *merge*
across config sources (CLI, env, and directory layers) rather than letting
an inner/later one replace an outer/earlier one, so `build-std` can't be
unset short of removing it from the root config entirely — which would
break the embedded build it exists for. `const` assertions sidestep this
entirely (same compiler invocation as the real build, no second target, no
`std`), at the cost of only working for pure, no-I/O logic — fine for now
since that's all `ps2-rt` has.

### Known-unverified (stated, not swept under the rug)

Everything below is plausible-but-untested. Flagging explicitly rather
than implying it's been checked:

- Whether PCSX2 (or real hardware) actually boots `hello` and the magic
  value lands at `0x1000` — the ELF structure was statically verified,
  not dynamically run under an emulator/debugger yet.
- `max-atomic-width: 32` — picked conservatively, not confirmed against
  R5900 documentation for which atomic widths it actually implements.
- Exact FPU rounding/exception behavior under `+single-float` — declared
  to match "hardware has single precision, no double", not validated
  against the EE's documented non-IEEE-754 quirks.

## 2026-11-26 — resolved: why `.got` is non-empty under `+noabicalls`

Still no BIOS dump in this environment, so dynamic boot verification
(next-steps.md #1) stays blocked. This burns down one of the
known-unverified items from 2026-11-25 instead, since it's answerable
statically.

Checked `hello`'s built ELF directly:

- `readelf -r`: "There are no relocations in this file." — fully static,
  nothing left to resolve at load time.
- `rust-objdump -d`: `$gp` is written exactly twice, both in `_start`'s
  prologue (`lui`/`addiu` to compute it from `_gp`), and never read back
  as a base register anywhere in the disassembly — zero `$gp`-relative
  loads/stores in the whole binary.
- `readelf -x .got`: exactly 16 bytes — `0x0000000000000000` followed by
  `0x8000000000000000`.

That second value is the tell: it's the MIPS psABI's two *mandatory*
reserved GOT header entries (GOT[0] reserved for a lazy-resolution stub
address, zero here since there's no dynamic linker; GOT[1] with its sign
bit set, the ABI's "not lazily bound" marker) — emitted by the linker
unconditionally whenever a `.got` output section exists at all, not
something generated code asked for. Combined with zero relocations and
zero `$gp`-relative accesses, this confirms `+noabicalls` is doing
exactly what it's documented to do: the `.got`/`_gp` setup in `boot.rs`
really is pure defensive boilerplate for this binary, not something
silently load-bearing.

Turned this into a regression check rather than leaving it as a one-time
reading: `scripts/verify-elf.sh` now asserts zero relocations and the
exact 16-byte/known-content `.got`, instead of only reporting its size as
an `info` line.
