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

## 2026-11-26 — GS privileged register plumbing (next-steps.md #2, partial)

Still blocked on #1 (no BIOS), so this is necessarily all static: code
that's internally consistent and sourced, not dynamically confirmed to
produce anything on a real display. Scoped deliberately narrow — see
"what's still missing" below before treating this as the milestone.

### What was researched (sourced, not guessed)

- EE virtual/physical memory map, via
  [ps2tek](https://psi-rockin.github.io/ps2tek/): unlike main RAM (which
  has separate cached/uncached kseg0/kseg1-style aliases), the
  `0x10000000`-`0x13FFFFFF` region (I/O registers, VU0/VU1 memory, GS
  privileged registers) is identity-mapped — virtual address equals
  physical address directly, reachable from kuseg with no segment
  prefix. This resolves the exact open question
  next-steps.md previously posed ("the exact bit-pattern for R5900's
  64-bit address segments was not worked out") for this specific address
  range: no translation helper is needed, a plain pointer write to
  `0x12000000` is correct as-is (assuming kernel-mode execution, which
  the BIOS/ELF loader is responsible for — not re-verified here).
- GS privileged register addresses and bit layouts, from ps2sdk's
  `common/include/gs_privileged.h` (`GS_REG_*`/`GS_SET_*`), cross-checked
  against ps2tek's privileged-register address table. Implemented in the
  new `ps2-rt/src/gs.rs`: `REG_PMODE`/`REG_SMODE2`/`REG_DISPFB1`/
  `REG_DISPLAY1`/`REG_DISPFB2`/`REG_DISPLAY2`/`REG_BGCOLOR` address
  constants, plus `pmode`/`bgcolor`/`dispfb`/`display` — `const fn`
  bit-packing helpers mirroring ps2sdk's `GS_SET_*` macros.
- A possible shortcut for next-steps.md #2's "get a solid color on
  screen" milestone that avoids needing the GIF/DMA primitive-draw path
  entirely: PMODE's blend fields (`MMOD` = use a fixed alpha instead of
  circuit 1's per-pixel alpha, `SLBG` = blend against `BGCOLOR` instead
  of circuit 2, `ALP` = the fixed alpha value, 0xff = 1.0) mean
  `pmode(en1: true, en2: false, use_fixed_alpha: true,
  out1_alpha_from_out2: false, select_bgcolor: true, alpha: 0)` should
  make the *entire* PCRTC output equal to `BGCOLOR`, regardless of
  whatever (even garbage/uninitialized) data sits in circuit 1's
  framebuffer — no VRAM write, no GS primitive, no GIF/DMA needed at
  all. This is a hypothesis built from the field semantics, not
  something confirmed against the primary source (GS Users Manual's
  PCRTC blend-equation section) or run anywhere. Flagging it as
  promising, not as done.

### What's still missing before attempting to boot this

- Video sync timing: `SMODE1`/`SYNCH1`/`SYNCH2`/`SYNCHV` need real
  NTSC/PAL timing constants (scanline counts, blanking intervals, pixel
  clock dividers) that are empirically-tuned hardware values, not
  something to derive from first principles. Not sourced yet — ps2tek's
  public docs don't tabulate them, so this needs the EE/GS Users Manual
  directly or a close reading of a known-working ps2sdk video-mode
  preset. Deliberately not guessed/fabricated here.
- The PCRTC blend-equation hypothesis above, against the GS Users
  Manual's actual equation (not just field-name semantics pulled from
  secondary sources).
- Everything in `gs.rs` is unused by any tool so far — no binary calls
  `write_reg` yet, so none of this has even been linked into a built ELF,
  let alone run.

## 2026-11-26 — primary-source PCRTC registers + the real sync-timing answer

Follow-up to the same day's earlier entry, asked to source the video
sync-timing constants specifically from the EE Users Manual. That source
turned out not to have them — tracked down what actually does, which
changed the plan rather than just filling in the blanks.

### Where the primary sources actually live, and what's in each

Pulled both manuals (Sony, 2002, "SCE Confidential") from
[DarrenRainey/PS2-Programming-Docs](https://github.com/DarrenRainey/PS2-Programming-Docs),
converted to text with `pdftotext -layout` to search them directly
rather than relying on secondary sources:

- **EE Users Manual v6.0**, section 2.2.3 "GS Privileged Registers":
  only an address table (confirms the `0x1200_00xx` addresses already in
  `gs.rs`, nothing more). No bit layouts, no timing values — `grep -i
  pcrtc` on the extracted text matches exactly once, the words "Various
  PCRTC modes" in that same address table. The PCRTC register
  *definitions* live in a different manual entirely.
- **GS Users Manual v6.0**, chapter 5 "CRTC": this is the real source.
  Confirmed, by reading the actual `BIT ASSIGN`/`FIELD` tables (not
  inferring from ps2sdk macro names):
  - `PMODE`, `BGCOLOR`, `DISPFB1`/`DISPFB2`, `DISPLAY1`/`DISPLAY2`'s bit
    layouts match `gs.rs`'s `pmode`/`bgcolor`/`dispfb`/`display` exactly
    — every field position and width. `gs.rs`'s module doc now cites
    this directly instead of "not yet checked against the primary
    source."
  - The Merge Circuit's actual blend equation (section 5.2.3): `Ro = Rs
    × A + Rd × (0xff − A)` (and same for G/B), where `Rs`/etc. is circuit
    1's output, `Rd`/etc. is circuit 2's output or `BGCOLOR` (selected by
    `SLBG`), and `A` is either circuit 1's alpha or the fixed `PMODE.ALP`
    value (selected by `MMOD`). With `MMOD=1` and `ALP=0`, `A=0`, so
    `Ro=Rd` — **confirms** the previous entry's PMODE-blend hypothesis
    outright, not just "promising."
  - Section 5.2.1 "Video Clock" states outright: "the VCK frequency
    predetermined for each video mode is set by **the kernel service**."
    This is the actual answer to "source the NTSC sync timing constants"
    — they're not meant to be hand-derived or hand-set by application
    code at all; a BIOS kernel call does it.

### The kernel service, and why that's a better answer than raw constants

[ps2tek](https://psi-rockin.github.io/ps2tek/)'s BIOS EE Syscalls
reference: syscall `0x02`, `void SetGsCrt(bool interlaced, int
display_mode, bool frame)`, "Initializes the PCRTC." Cross-checked
against ps2sdk:

- `ee/kernel/include/syscallnr.h`: `__NR_SetGsCrt` = 2, confirming the
  number.
- `ee/kernel/src/kernel.S`: the non-kmode `SYSCALL_SPECIAL` stub is
  exactly `li $3, N; syscall; jr $31; nop` — args already sit in
  `$a0`-`$a2` by the ordinary calling convention, so the whole function
  body is those four instructions.
- `ee/graph/src/graph_mode.c`'s `graph_set_mode`: confirms argument
  order and meaning (`SetGsCrt(interlace, graph_mode[mode].mode,
  ffmd)`), and `ee/graph/include/graph.h`'s `GRAPH_MODE_NTSC`=1/`_PAL`=2/
  `_INTERLACED`=1/`_NONINTERLACED`=0/`_FIELD`=0/`_FRAME`=1.

Implemented as `ps2_rt::syscall::set_gs_crt` (`ps2-rt/src/syscall.rs`):
the raw 4-instruction stub via `global_asm!` (same pattern as
`boot.rs`'s `_start`), an `extern "C"` declaration, and the mode
constants. This is a materially better outcome than what was asked for:
rather than fabricating/guessing `SMODE1`/`SYNCH1`/`SYNCH2`/`SYNCV`
register values (which the primary source explicitly says isn't how
application code is supposed to arrive at them), the real hardware
design delegates that entirely to a one-line BIOS call.

### Wired into an actual tool, and checked structurally

Added `tools/solid-color`, a full (static-only) implementation of the
PMODE-blend hypothesis: calls `set_gs_crt(NON_INTERLACED, NTSC, FIELD)`,
then writes `DISPFB1`/`DISPLAY1` (ps2sdk's own `graph_mode[NTSC]` preset
— `{x:652, y:26, width:2560, height:224}` from `graph_mode.c` — run
through its documented `graph_set_screen`/`graph_set_framebuffer`
formulas for a non-interlaced 640×224 PSMCT32 screen, not hand-derived),
`BGCOLOR` (red), and finally `PMODE` to enable the forced-background
blend.

This is where the "unused by any tool" gap from the previous entry
actually got closed, and with it, something real to check:

- `set_gs_crt` now appears in `solid-color`'s linked ELF (confirmed with
  `nm` — it's absent from `hello`'s, correctly GC'd there since nothing
  references it, same as `gs.rs`'s functions still are).
- `scripts/verify-solid-color.sh`: asserts the compiled `set_gs_crt`
  symbol's raw bytes equal the hand-assembled stub exactly
  (`02000324 0c000000 0800e003 00000000`, i.e. `li $3,2 / syscall / jr
  $31 / nop`). Checked this actually catches a regression, not just
  compiles: changed the syscall number to 3 in the source, rebuilt,
  watched it fail with a clear byte mismatch, reverted, watched it pass
  again.
- `tools/solid-color/src/main.rs` names every magic number from the
  `graph_mode[NTSC]` preset and `const`-asserts the four packed register
  values they produce (`DISPFB1`/`DISPLAY1`/`BGCOLOR`/`PMODE`). Checked
  the same way: broke `DISPFB1`'s expected value by one, rebuilt, saw
  `E0080` fail the build, reverted. Deliberately *not* done by scanning
  compiled instruction bytes for these — that location is compiler-choice-
  dependent (register allocation, immediate materialization strategy)
  and would be testing LLVM's codegen decisions, not the actual claim.
- `scripts/verify-elf.sh` runs against `solid-color` too now (it was
  already generic enough — no changes needed) and `just test` runs all
  three checks.

### Still not run anywhere

Same caveat as every entry today: no BIOS dump in this environment, so
none of this has been dynamically confirmed. What's checked is
internal consistency (the numbers match their stated formulas, the
hand-written assembly matches what was intended) and consistency with
secondary sources (ps2sdk) that are themselves known-working — not "the
screen turns red." That's still gated on next-steps.md #1.

## 2026-11-26 — `alloc` support (next-steps.md #4)

\#1 and #2 are now both entirely blocked on getting a BIOS dump into
this environment — nothing left to do there without one. #4 ("pick the
second tool to port... something that exercises `alloc`") isn't blocked
on that, so moved to it instead.

- `.cargo/config.toml`: `build-std = ["core", "alloc"]` (was just
  `["core"]`). Built cleanly on the pinned nightly (`1.102.0-nightly
  (137d1d494 2026-11-22)`) with no `#[alloc_error_handler]` needed —
  checked by actually building with none defined rather than assuming;
  this nightly provides a default one. Worth re-checking if the
  toolchain pin ever moves, the same way `panic_immediate_abort` broke
  across a nightly boundary (see 2026-11-23's bug log).
- `ps2-rt/src/heap.rs`: a bump allocator (`#[global_allocator]`) backed
  by a fixed 64 KiB `static` array, not a region carved out of `link.ld`
  between `_heap_start` and `_stack_top` — deliberately avoids having to
  decide how much of the remaining RAM is safe to give the heap vs.
  leave as stack headroom, which isn't something to size from first
  principles. `dealloc` is a no-op (bump allocators don't reclaim
  individual allocations); next-steps.md already flagged this as an
  acceptable simplification over a freelist.
- `tools/alloc-hello`: builds a `Vec<u32>`, sums values derived from
  actually allocated/written memory (not a hardcoded constant — a
  broken allocator returning garbage or aliased memory would plausibly
  change the result), writes it to the same proof-of-life address
  pattern as `hello`.
- This is the **first tool with non-empty `.bss`** (`HEAP` plus the
  allocator's `next` cursor, 64 KiB + 8 bytes). Every tool before this
  one had `__bss_start == __bss_end`, so `boot.rs`'s `zero_bss()` was
  only ever exercised as a zero-length no-op — this is the first ELF
  where it actually has bytes to zero. Confirmed via `nm`: `HEAP` and
  `ALLOCATOR`'s inner cursor both land in `.bss`, sized exactly as
  expected (0x10008 = 0x10000 + 8).
- `scripts/verify-elf.sh`'s bss check was hard-coded to assume
  `__bss_start == __bss_end` for every tool — no longer true, so it now
  takes an optional expected-size argument (default 0, so `hello`/
  `solid-color` need no changes) instead of assuming equality. Checked
  it still catches a real mismatch (passed a wrong size by hand, saw it
  fail with the actual vs. expected values, removed the override).
- `heap.rs`'s `align_up` is const-asserted the same way `boot.rs`'s
  `bss_len` and `gs.rs`'s bit-packing functions are — checked these
  assertions aren't vacuous (broke one, rebuilt, saw `E0080`, reverted).

Same caveat as everything else today: builds and link-time facts
(symbol presence, section sizes) are checked; whether the allocator
actually produces correct, non-aliasing pointers on real EE hardware is
not — that's gated on next-steps.md #1 like everything else.
