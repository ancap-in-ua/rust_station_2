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

## 2026-11-27 — code actually executes: a qemu-based dynamic test harness

Every entry above ends with the same caveat — "not dynamically verified,
no BIOS dump in this environment." next-steps.md #1 (run it under PCSX2)
blocks #2 and #3, and no amount of static checking substitutes for it.
This entry removes most of that caveat without a BIOS, by executing the
unmodified release ELFs under `qemu-system-mips64el`.

### Why an unmodified PS2 ELF runs under qemu at all

Not a workaround, a coincidence of two designs lining up. `link.ld`
places the image at `0x0010_0000` with `PhysAddr == VirtAddr`, because
PS2 loaders hand control over with low addresses identity-mapped to
physical RAM. That is also precisely how a MIPS CPU behaves coming out of
reset: `Status.ERL = 1` makes kuseg unmapped and identity-mapped to
physical addresses. So the same bytes load and run in both places, for
the same reason — no separate test build, no relinking, no `#[cfg(test)]`
variant. What `just test-qemu` boots is what you'd hand a PS2 loader.

Two things had to be discovered empirically:

- **`-M malta -kernel` cannot be used.** It rejects any kernel linked
  below `0x8000_0000` with "Trap-and-Emul kernels (Linux
  CONFIG_KVM_GUEST) are not supported" — which is every PS2 ELF.
  `-device loader,file=...,cpu-num=0` loads the segments and sets the
  CPU's PC from the ELF entry with no such check, and works.
- **`-cpu R4000`**, chosen over `5Kc`: both run these tools identically,
  but R4000 is a 64-bit MIPS III part, matching the `mips3` ELF flag
  verify-elf.sh asserts, so it can't quietly accept a post-MIPS-III
  instruction the EE would reject. `-m 32` matches the PS2's RAM exactly,
  so `_stack_top = 0x200_0000` sits at the real end of RAM rather than
  somewhere forgiving.

### What it proved (first dynamic results in this project)

`scripts/verify-qemu.sh <elf> <expected-proof-word>` boots a tool, drives
it over qemu's gdbstub, and asserts. All of the following were previously
assumed or statically inferred, and are now observed:

- `_start`'s hand-written assembly runs, and `$gp` at halt equals
  `link.ld`'s `_gp` exactly (`0x1081a0` for `hello`) — the "defensive,
  possibly unnecessary" `$gp` setup at least demonstrably does what it
  intended. `$sp` lands inside the stack region (`0x1fffff0`, i.e.
  `_stack_top - 16` after `rt_start`'s frame).
- `hello` writes `0xc0ffee42` to `0x1000`. The proof-of-life value the
  README has been telling people to go look for in PCSX2's memory viewer
  for four days is, in fact, the value that gets written.
- **`boot.rs`'s `zero_bss()` works, checked non-vacuously.** This
  mattered more than expected. Fresh qemu RAM reads back as zero, so a
  naive "is bss zero after boot" check would pass even if `zero_bss` were
  a no-op — exactly the sort of vacuous test this log has been careful
  about elsewhere. Instead the harness writes `0xd5` over the entire bss
  range *while the guest is still frozen at its entry point* (qemu `-S`
  plus gdb `restore`), then asserts not one `0xd5` byte survives. It also
  asserts the pre-boot dirtying itself took effect, so the check can't
  silently become vacuous if `restore` ever stops working.
- **The bump allocator hands out real, usable, non-aliasing memory.**
  `alloc-hello` writes `0xa110008c` — `0xA110_0000 | 140`, the sum
  computed from heap memory it actually allocated and wrote. The heap
  contents at halt tell the whole story and match a bump allocator
  exactly: the dead 4-element `Vec` (`0,1,4,9`) still sitting at offset 0
  unreclaimed, the live 8-element one (`0,1,4,9,0x10,0x19,0x24,0x31`)
  after it, cursor at `0x30` = 48 bytes for 16 + 32 bytes of allocation.
  Also the first time `zero_bss` ran with a non-zero length at all (every
  earlier tool had `__bss_start == __bss_end`).

### `solid-color`: what's checkable, and what still isn't

It cannot run to completion here. Its first act is BIOS syscall `0x02`,
and qemu has no PS2 BIOS — the `syscall` instruction traps to an
exception vector holding no handler and never returns (confirmed by hand:
stepping over that single instruction hangs). So `verify-qemu.sh` can't
be used on it.

But the part that its static checks structurally *could not* reach is
checkable: `scripts/verify-solid-color-qemu.sh` breaks on `set_gs_crt`
and reads the argument registers. verify-solid-color.sh pins the stub's
compiled bytes and main.rs const-asserts the register values, yet
neither can show execution actually *arrives* there with the right
arguments in the right registers — and "the arguments are already in
`$a0`-`$a2` by the ordinary calling convention" is the entire reason
that stub can be four instructions with no marshalling (see 2026-11-26).
That's a compiler-dependent assumption the source only implies. Now
observed: `$a0 = 0` (non-interlaced), `$a1 = 1` (NTSC), `$a2 = 0`
(field), with `syscall` confirmed as the very next instruction, so those
really are the arguments as a BIOS would see them.

Still unproven, and unprovable this way: **whether the screen turns
red.** That remains next-steps.md #1.

### Non-vacuity, checked the usual way

Each new check was shown to fail on a real regression, not just to pass:

- Broke `zero_bss` (`write_bytes(start, 0, len * 0)`), rebuilt: caught
  three ways at once — 65544 surviving `0xd5` bytes, proof word `0x0`,
  and `FAIL terminal state: the tool PANICKED`. That last one is worth
  noting: with bss left dirty, the allocator's cursor came up as
  `0xd5d5...`, the allocation failed, and the panic path ran. The
  halt-vs-panic distinction earned its keep on its first real failure.
- Pointed `solid-color` at `syscall::PAL` instead of `NTSC`, rebuilt:
  `FAIL $a1 (display_mode = GRAPH_MODE_NTSC): got 2, want 1`.
- Made a tool spin without reaching halt: reported "the ELF never
  reached its terminal state under qemu (gdb timed out — the guest is
  hung, faulting, or looping somewhere that is neither ps2_rt_halt nor
  ps2_rt_panicked)" rather than hanging `just test` forever.
- Removed the `ps2_rt_halt` call entirely: the symbol gets
  garbage-collected, and the script says so by name instead of failing
  obscurely.

### `ps2_rt_halt` / `ps2_rt_panicked`, and an ICF surprise

Each tool's terminal `loop {}` is now `ps2_rt::ps2_rt_halt()`, and the
panic handler ends in `ps2_rt_panicked()`. The point is a stable symbol
to break on: a bare `loop {}` is an anonymous backwards branch at
whatever address LLVM picks, so a test would have to hardcode an address
that moves on every unrelated change.

First attempt had both as `#[inline(never)] #[no_mangle]` functions whose
body was `loop {}` — and they came out **at the same address** in all
three tools, folded together as identical code. A distinction by name
only is no distinction at all once the linker is allowed to merge
bodies. Fixed by having each pass a distinct tag (`"OKOK"` / `"PAN!"`) to
a shared `spin_forever`, which differs by one immediate and so can't be
folded; the tag is also readable in `$a0` from a debugger with no symbols
loaded. Worth remembering for any future "two functions that differ only
in meaning" pattern.

Note `ps2_rt_panicked` is garbage-collected out of tools with no
reachable panic path (`hello`, `solid-color` — confirmed with `nm`), so
anything breaking on it must treat the symbol as optional.

### What this does NOT cover

Listed in full at the top of `scripts/lib-qemu.sh`, because the risk here
is someone mistaking a green `just test-qemu` for "it works on a PS2":

- Anything GS/PCRTC — qemu has no PS2 graphics hardware; writes to
  `0x1200_0000` go nowhere observable.
- Anything BIOS — no syscall can complete.
- Anything R5900-specific — the 128-bit MMI instructions, the non-IEEE-754
  single-precision FPU, scratchpad RAM, and EE cache behaviour are all
  absent from qemu's generic MIPS models. Code relying on them passes
  here and fails on hardware. This is why next-steps.md #3's float-
  correctness item stays open: qemu's FPU is *more* standards-compliant
  than the EE's, so it cannot reveal that class of bug.
- DMA, the GIF, VU0/VU1, SIF/IOP — nothing of the PS2 beyond the CPU core
  and RAM exists here.

`just test` now runs `test-static` then `test-qemu`; `just debug-qemu
<tool>` boots a tool and drops into an interactive gdb attached to it.
qemu was added to flake.nix (it had been present only by accident, via
the host system, which would have made `just test` fail inside a clean
`nix develop`).

## 2026-11-27 — a real BIOS arrives, and three of our assumptions die

A BIOS dump (`bios_dump/ps2-0220e-20060210.bin`, Europe v02.20
(10/02/2006), recognized by PCSX2 as `Console 20060210-142424`) finally
unblocked next-steps.md #1. PCSX2 is driven here with `-datapath` so all
of its state lives in a scratch dir rather than the user's
`~/.config/PCSX2`, and read back over **PINE**, PCSX2's IPC socket
(`EnablePINE = true`, unix socket at `$XDG_RUNTIME_DIR/pcsx2.sock`),
which allows scripted EE memory reads — no screenshots-and-squinting
needed for memory claims.

Nothing we built actually works on a PS2. Three separate assumptions,
all previously "verified", turned out to be wrong, and the qemu harness
from the entry above could not have caught any of them.

### 1. The ELF is the wrong class. PCSX2's loader *crashes* on it.

Booting `hello`:

```
(ELF) Size of section headers is not standard
*************** Unhandled SIGSEGV at 0x5d158c5da8fe ***************
  0x005d158c5da8fe _ZN9ElfObject18LoadSectionHeadersEv
  0x005d158c69daf9 _ZN9VMManagerL13UpdateELFInfoE...
```

PCSX2's ELF loader is strictly **ELF32**: it reads the section header
table assuming 40-byte ELF32 entries, our ELF64 entries are 64 bytes, it
walks off the end and segfaults the emulator. Exit code 134 (SIGABRT via
the crash handler). This is not a graceful "unsupported file" — our
binary takes the host process down with it.

PS2 ELFs are ELF32 by universal convention (ee-gcc/ps2sdk build
`mips-r5900-elf` o32 objects). Our target spec asks for `mips64` + n64,
which produces ELF64. `scripts/verify-elf.sh` has been *asserting* `ELF
class == ELF64` as if it were a desired property since the initial
scaffold. It was asserting the bug.

Confirmed it is the only structural blocker by hand-assembling a minimal
ELF32 with the same load address, entry point, and program header layout
(`lui/ori/sw` of `0xC0FFEE42`, then spin — 5 instructions):

```
ELF Loading: host:min32.elf, Game CRC = D27C853B, EntryPoint = 0x00100000
ELF host:min32.elf with entry point at 0x00100000 is executing.
```

It loads and runs. So: load address, entry convention, and program
header layout are all fine; the ELF class is not.

**Why qemu missed it:** `-device loader` is a generic ELF loader that
handles ELF64 perfectly well. The previous entry's claim — "the same
bytes load and run in both places, for the same reason" — is false. The
same bytes run under qemu *because qemu is more permissive than any real
PS2 loader*, not because the image is PS2-correct. That was the one
thing the qemu harness most needed to be right about, and it was the
thing it was structurally incapable of testing.

### 2. The proof-of-life address `0x1000` is not writable. Never was.

The very first thing the minimal ELF32 did was fault:

```
TLB Miss, pc=0x100000 addr=0x1000 [store]
```

The first MiB belongs to the BIOS kernel, and a user-mode ELF cannot
reach it through kuseg — the EE kernel simply has no TLB entry mapping
it for us. `link.ld`'s comment ("the MMU already set up so that low
addresses map directly to physical RAM") is true only for the region the
loader mapped for *our* image, not for arbitrary low addresses.

The same store through kseg0 works:

```
0x1000     = 0x00000000     (via PINE -- unmapped, reads as zero)
0x80001000 = 0xc0ffee42     (via PINE -- the store landed)
```

So `hello` and `alloc-hello`'s entire proof-of-life mechanism is
invalid on hardware, and the README's standing instruction to "check
address 0x00001000 in PCSX2's memory viewer" would have shown zero even
if the ELF had loaded. Writes to a fixed physical address need a kseg0
(`0x8000_0000`, cached) or kseg1 (`0xA000_0000`, uncached) prefix; the
simpler fix is to write somewhere the program actually owns. Markers
written into our own image at `0x100800` read back fine.

### 3. The BIOS syscall never returns, so `solid-color` can't even start

This one is **unresolved**, and it blocks the whole solid-color
milestone.

Traced by storing progress markers into our own image around the call:
`0x1111` before the `syscall`, `0x2222` immediately after. With a
hand-assembled ELF32 doing exactly what `ps2_rt::syscall::set_gs_crt`
does (`li $v1, 2` / `syscall`):

```
0x100800 = 0x00001111   <- reached the syscall
0x100804 = 0x00000000   <- never came back
0x100808 = 0x00000000
```

Still zero after **25 seconds**, so it is hung, not slow. Things ruled
out:

- **Not a missing stack.** An early attempt never set `$sp`; setting it
  to `0x01FFFFF0` (matching link.ld's `_stack_top`) changed nothing.
- **Not `-fastboot` skipping kernel init.** `-slowboot` behaves
  identically.
- **Not a bad CPU state.** Read CP0 at the entry point:
  `Status = 0x70030c11` → `KSU = 2` (user mode), `EXL = 0`, `ERL = 0`,
  `IE = 1`, `CU0 = 1`; `EPC = 0x00100000`; `PRId = 0x00002e20` (R5900);
  `Config = 0x00073443`. That is a normal user-mode state in which
  `syscall` should dispatch to the kernel handler. The nested-exception
  theory (a `syscall` taken with `EXL` already set looping in the
  vector) is dead.
- **Not the kernel-mode variant.** `li $v1, -2` (the negative-number
  convention) hangs identically.
- **Not the surrounding code.** With the `syscall` instruction removed
  and nothing else changed, the program runs to completion — both
  markers land (`0x1111`, `0x3333`) and all four GS register writes
  execute.

So `set_gs_crt` is broken in a way that the primary-source research
(ps2tek's syscall table, ps2sdk's `kernel.S`, 2026-11-26) did not
predict, and the byte-level check in `verify-solid-color.sh` cannot see,
because the stub's bytes are exactly what we intended them to be. The
intent was right and the encoding was right; the call still doesn't
work.

### 4. GS privileged registers *are* reachable from kuseg (this one held)

Worth recording as a non-finding, since it was the obvious suspect after
#2: a store to a raw `0x12000000` produces **no** TLB miss. The EE
kernel maps the hardware register region, so `ps2_rt::gs::write_reg`'s
no-segment-prefix addressing is correct as written. The 2026-11-26
conclusion on this point stands — tested, not assumed, because #2 made
it look doubtful.

### 5. The solid-color hypothesis is still untested, not refuted

With the syscall removed and all four GS registers (DISPFB1, DISPLAY1,
BGCOLOR, PMODE) written with the values `tools/solid-color`
const-asserts, PCSX2 fullscreen is **100% black** (checked by capturing
the framebuffer and decoding it: every sampled pixel `rgb(0,0,0)`).

That is *not* evidence against the PMODE-blend approach. `SetGsCrt` is
what programs the PCRTC's video timings; without it there is plausibly
no video signal for the merge circuit to blend into, so a black screen
is the expected outcome either way. The hypothesis can't be evaluated
until #3 is fixed.

### Where this leaves the project

`just test` (static + qemu) is still green, and still green for things
that are now known to be wrong — `verify-elf.sh` asserts the ELF class
that crashes PCSX2. Green does not mean correct; it means "unchanged
since the last time we looked", and what we were looking at was chosen
before anything had ever run on a PS2.

The target spec needs to emit ELF32 before any of this can run on real
hardware. That is a pointer-width change (64 → 32), not a flag tweak,
and it touches the target JSON, `link.ld`'s assumptions, the proof-of-
life convention from #2, and every expectation in `verify-elf.sh`.

## 2026-11-27 (later) — re-targeted to ELF32/o32; it runs on a real PS2

Acted on the previous entry. The target is now 32-bit, the tools boot on
PCSX2 with a real BIOS, and `just test-pcsx2` asserts it. Also:
**one of the three findings in that entry was wrong**, and the
correction is the most interesting part of this one.

### Correction: the BIOS syscall works fine

The previous entry concluded that `ps2_rt::syscall::set_gs_crt` hangs
and never returns, based on a hand-assembled ELF32 probe that wrote
`0x1111` before the `syscall` and `0x2222` after, and only ever showed
the first.

That probe was broken. It kept the marker base address in `$t0` and the
marker value in `$t1` across the `syscall` — **both caller-saved** in
o32. The BIOS handler clobbers them, as it is entitled to, so the
"after" store went to whatever garbage address `$t0` held by then. The
syscall had returned normally every time; the instruction meant to
record that fact is what failed.

Caught it by accident: once the real Rust `solid-color` was running
(the Rust `_start`/ABI code saves and restores registers properly, as
compiled code does), its post-syscall proof value appeared immediately.

Worth keeping in mind for future hand-written probes: anything written
in raw assembly around a BIOS call must treat `$t0`-`$t9`, `$a0`-`$a3`
and `$v0`-`$v1` as destroyed. Compiled Rust gets this right for free,
which is exactly why the bug only showed up in the hand-written test
and not in the real artifact it was supposed to be modelling.

So the 2026-11-26 syscall research was correct after all: syscall `2` in
`$v1`, arguments in `$a0`-`$a2`, four-instruction stub.

### The target spec is now ELF32/o32

`targets/mips64r5900el-ps2.json` → `targets/mipsel-sony-ps2.json`
(renamed to match the built-in `mipsel-sony-psx` naming, and because
"mips64" was the bug). Built from rustc's `mipsel-unknown-none` rather
than from the old n64 spec:

```
 arch                  mips64 -> mips
 cpu                   mips3     (unchanged -- EE has no MIPS32r2 ext/ins/rotr)
 features              +mips3,+single-float,+noabicalls
 llvm-abiname          n64    -> o32
 target-pointer-width  64     -> 32
 data-layout           e-m:m-p:32:32-i8:8:32-i16:16:32-i64:64-n32-S64
```

LLVM accepts `mips3` with o32 — the same combination ee-gcc uses, and
the same shape as N64 toolchains (64-bit CPU, 32-bit pointers). The
resulting ELF header is exactly what a PS2 expects:

```
Class: ELF32   Flags: 0x20001101, noreorder, 32bitmode, o32, mips3
```

Knock-on changes: `#[cfg(target_arch = "mips64")]` → `"mips"` in
`boot.rs`/`syscall.rs` (these silently compiled *everything* out
otherwise — the first build after the switch produced an "unused import:
global_asm" warning and an ELF with no `_start`, which is the only
reason it got noticed); `usize` is now 4 bytes, so the allocator cursor
shrank; `.got` is two 4-byte reserved entries (`0x80000000`) instead of
two 8-byte ones; `.MIPS.options` is gone.

### Proof-of-life now lives in the tool's own image

`ps2_rt::PS2_RT_PROOF`, a `static` written through
`ps2_rt::set_proof()`, replaces the hardcoded `0x1000`. The loader maps
the image it loaded, so this is memory we are actually entitled to
write — no kseg0/kseg1 prefix, nothing to collide with, and it works
identically under qemu and PCSX2. Harnesses resolve it by symbol with
`nm` rather than hardcoding an address.

### It runs

`scripts/verify-pcsx2.sh` (+ `lib-pcsx2.sh`, `pine.py`, `just
test-pcsx2`) boots each ELF on PCSX2 with the real BIOS and asserts over
PINE. All three tools pass:

- `hello` — `PS2_RT_PROOF = 0xc0ffee42`.
- `alloc-hello` — `0xa110008c`, and the heap read back over PINE shows
  the bump allocator's signature exactly as it did under qemu: the dead
  4-element `Vec` (`0,1,4,9`) unreclaimed, then the live 8-element one
  (`0,1,4,9,0x10,0x19,0x24,0x31`). The allocator works on a PS2.
- `solid-color` — `0x5011d000`, meaning the `SetGsCrt` syscall returned
  and all four GS register writes executed.

No TLB misses anywhere, which the harness now checks explicitly: it is
the signal that a tool touched memory the EE kernel never mapped for it,
and it is what the old `0x1000` convention tripped on every run.

`just test` is now three layers — `test-static`, `test-qemu`,
`test-pcsx2` — 62 checks. The PCSX2 layer skips loudly (not silently)
when `bios_dump/` is empty, so the repo still works for anyone without
a dump.

Also fixed: `scripts/run-pcsx2.sh` was `pcsx2-qt -elf "$elf" -fastboot`,
which boots **nothing**. `-elf` only overrides the boot ELF; the file
must be a positional argument, and PCSX2 ignores any file not ending in
`.elf` (both confirmed by testing). It had been wrong since the initial
scaffold, invisibly, because there was never a BIOS to run it with. A
symlink to a `.elf` name is not sufficient either — PCSX2 boots garbage
from one and ends up executing at address 0; the harness copies the
bytes.

### What's still not working: the screen is black

`solid-color` now runs end to end on a real PS2 and the display stays
**100% black** — not red.

This is finally a real test of the PMODE-blend hypothesis rather than a
blocked one, and the hypothesis is not behaving as the GS User's Manual
reading predicted. Checked that the capture pipeline isn't lying by
booting the BIOS itself and capturing that: it comes back with the boot
screen's colors (center pixel `rgb(0,95,140)`), so black means black.

Open questions for whoever picks this up, roughly in order of
suspicion:

- `SMODE2` is never written. `ps2_rt::gs` defines `REG_SMODE2`
  ("interlace, field mode, display enable") and `solid-color` doesn't
  touch it. If `SetGsCrt` doesn't leave the display enabled, nothing
  reaches the CRTC regardless of what the merge circuit computes.
- Whether PCSX2's PCRTC emulation implements the `SLBG`/`MMOD`/`ALP`
  path when circuit 1 points at VRAM that was never written. A real GS
  might behave differently; this is exactly the kind of exotic corner
  an emulator is allowed to get wrong.
- Whether the `ALP = 0` reading of the blend equation is inverted.

## 2026-11-27 (later still) — ps2sdk says we had the mode number wrong; PCSX2 says it can't judge the result

Went to the sources the brief named — ps2dev.github.io, ps2sdk, ps2tek —
to work out why `solid-color` draws a black screen. Found one genuine
bug in our code and one hard limit in PCSX2 that makes the milestone
unjudgeable there.

### Bug: `SetGsCrt`'s mode argument was the table index, not the mode

ps2sdk `ee/graph/src/graph_mode.c`:

```c
static const GRAPH_MODE graph_mode[22] =
{
    {   0,   0,    0,    0,    0 }, // AUTO
    { 652,  26, 2560,  224, 0x02 }, // NTSC-NI
    { 680,  37, 2560,  256, 0x03 }, // PAL-NI
    ...
```

and `graph.h`'s `GRAPH_MODE_NTSC` is **1** — an index into that table.
The call is `SetGsCrt(graph_interlace, graph_mode[mode].mode,
graph_ffmd)`, so what reaches the syscall is the row's `.mode` field,
`0x02`. ps2sdk's libgs names the same value directly:
`GS_MODE_NTSC = 0x02`, `GS_MODE_PAL = 0x03`.

`ps2_rt::syscall::NTSC` was 1 and `PAL` was 2 — the indices. We were
telling the BIOS to configure video mode 1, which is `AUTO`'s neighbour
in an index space the syscall knows nothing about.

The sharp part: `tools/solid-color`'s geometry constants (652, 26,
2560, 224) were read correctly out of **that same table row**, while
the mode number was taken from the enum sitting next to it. Half the
line was sourced properly. The 2026-11-26 note even quotes the
`SetGsCrt(graph_interlace, graph_mode[mode].mode, graph_ffmd)` call
verbatim — the correct value was written down at the time and then not
used.

Fixed to `NTSC = 0x02` / `PAL = 0x03`.
`scripts/verify-solid-color-qemu.sh` caught the change immediately
(`FAIL $a1: got 2, want 1`), which is the first time that check has
earned its keep on a real edit rather than a deliberately broken one.

### Also added, from ps2sdk's actual bring-up order

`graph_set_mode` does more than call the syscall, and we were doing
none of it:

```c
*GS_REG_CSR = (u64)1<<9;                      // GS reset
*GS_REG_CSR = GS_SET_CSR(0,0,0,0,0,0,0,0,0,0,0,0);
GsPutIMR(0x00007700);                         // mask GS interrupts
SetGsCrt(graph_interlace, graph_mode[mode].mode, graph_ffmd);
```

Added `gs::REG_CSR` (`0x1200_1000`), `gs::REG_IMR` (`0x1200_1010`),
`gs::CSR_RESET`, `gs::IMR_MASK_ALL`, and a `gs::smode2(INT, FFMD, DPMS)`
packer matching ps2sdk's `GS_SET_SMODE2`. `solid-color` now follows
ps2sdk's order: CSR reset → CSR clear → IMR → `SetGsCrt` → SMODE2 →
DISPFB1 → DISPLAY1 → BGCOLOR → PMODE.

SMODE2 is worth calling out because PCSX2 reads it to decide whether a
display is live at all — `PCRTCDisplays.EnableDisplays(m_regs->PMODE,
m_regs->SMODE2, ...)`. `SetGsCrt` is documented as setting the *timing*
registers; SMODE2 is a separate write that ps2sdk makes separately, and
we had never made it.

### None of it changes the screen, and PCSX2 explains why

Still black after all of the above. The reason is in PCSX2's own
source, `GSRenderer::Merge`:

```cpp
if (!tex[0] && !tex[1])
{
    ...
    // Both circuits off still outputs BGCOLOR on real hardware.
    if (PCRTCDisplays.PCRTCDisplays[0].enabled || PCRTCDisplays.PCRTCDisplays[1].enabled)
    {
        m_real_size = GSVector2i(0, 0);
        return false;
    }
}
```

`tex[0]`/`tex[1]` are the read circuits' framebuffer textures, fetched
from the texture cache. `solid-color` never writes a pixel to VRAM —
that is the entire point of the BGCOLOR trick — so there is no render
target for either circuit, both are null, and **PCSX2 returns without
presenting a frame**. The comment immediately above the early-out
concedes that real hardware would still output BGCOLOR here.

Tested the obvious escape too: with `EN1 = EN2 = 0` the
`enabled || enabled` guard is false, so that specific early-out is
skipped — still black.

So: `solid-color` is *correct as far as PCSX2 can tell* — it loads,
the syscall returns, every register write executes, it publishes its
proof-of-life — and PCSX2 structurally cannot render what it asks for.
The black screen is not evidence against the approach. It is also not
evidence for it.

This is the first point in the project where PCSX2 being an emulator
rather than a console actually blocks progress, which next-steps.md #2
had flagged as a possibility before it was confirmed.

### What that means for the milestone

Two ways forward, and they answer different questions:

1. **Real hardware.** The only thing that can settle whether the
   PMODE/BGCOLOR trick works. Everything needed is built — the ELF is
   loadable now — but deployment isn't wired up (next-steps.md #5).
2. **Put actual pixels in VRAM**, i.e. the GIF/DMA path. This stops
   being the "fallback" and becomes the only route to a visible result
   *under emulation*, since PCSX2 needs a real framebuffer texture
   before it will composite anything at all. It also happens to be what
   every real PS2 program does.

Option 2 is the one that unblocks iteration here, and it is a
substantially bigger piece of work than anything so far: GIF packet
construction, a DMA channel, and VRAM addressing, with no C library to
crib conventions from.

## 2026-11-27 (fourth) — the o32 switch silently broke every GS write

Prep for a hardware test, which turned up a regression the whole
check suite was blind to.

### `write_volatile::<u64>` does not emit `sd` under o32

GS privileged registers are 64-bit and documented as not byte/word
addressable; `gs.rs` has said so since it was written. Disassembling
`color-cycle` showed what the compiler was actually emitting:

```
10005c: sw $2, 0x0($1)      # $1 = 0x12001000 -- CSR low word
100064: sw $zero, 0x0($2)   # $2 = 0x12001004 -- CSR high word
```

Every GS register write had become a **pair of 32-bit stores**, to
`addr` and `addr + 4` — precisely the access pattern the register docs
rule out. Under o32 (32-bit pointers) LLVM lowers a volatile 64-bit
store by splitting it; it had been emitting a real `sd` only because
the target used to be n64/ELF64. Re-targeting to o32 earlier today
broke every GS write in the project, with no source change and nothing
in `just test` able to see it.

ps2sdk gets `sd` for the same C because ee-gcc emits one for `volatile
u64 *` writes even under o32 — the R5900 has 64-bit stores regardless
of pointer width, LLVM just won't use them here.

Fixed by hand-writing the store in `gs::write_reg`:

```
.set noat
dsll32 {lo}, {lo}, 0
dsrl32 {lo}, {lo}, 0     # zero-extend: a 32-bit value in a MIPS64
dsll32 {hi}, {hi}, 0     # register is SIGN-extended, which would
or     {lo}, {lo}, {hi}  # corrupt the upper half
sd     {lo}, 0({addr})
.set at
```

Rust's `reg` asm class is 32-bit on this target, so there is no way to
hand `asm!` a `u64` — hence assembling the halves. `.set noat` because
the allocator is free to hand us `$at` and the assembler warns
otherwise (same bracket `boot.rs` uses around its `$gp` setup).

`verify-solid-color.sh` now counts `sd` opcodes in `.text` and asserts
8 (CSR twice, IMR, SMODE2, DISPFB1, DISPLAY1 via `init_display`, then
BGCOLOR and PMODE). Counted from raw bytes rather than a disassembler,
so it needs nothing `readelf` doesn't already provide: MIPS opcode 0x3F
(SD) occupies bits 31-26, so a little-endian instruction word is an
`sd` exactly when its most significant byte is 0xFC-0xFF (opcode 62
would be 0xF8-0xFB — no overlap). A count of 0 means `write_reg`
regressed to split stores.

**This is the second defect in a row that was invisible to every check
and visible in one disassembly.** The first was the ELF class. Both
were "the source says the right thing, the compiler did something
else." Worth a standing habit: when a register write has a documented
*access width*, check the emitted instruction, not the source.

### PAL, and a video-mode table

The only console available is European, as is the only BIOS dump, so
running NTSC made a black screen ambiguous — "approach doesn't work"
vs. "TV didn't sync". Added `gs::VideoMode` carrying one row of
ps2sdk's `graph_mode[]` (geometry *plus* the `SetGsCrt` mode value, so
the two can't be taken from different places again — that was this
morning's bug) with `NTSC_NONINTERLACED` and `PAL_NONINTERLACED`
presets, and `gs::init_display(&mode, width)` doing ps2sdk's bring-up
order. `solid-color` dropped from ~40 lines of register arithmetic to
three calls, and switched to PAL.

### `tools/color-cycle`

A static screen is a weak test result on hardware: black could mean the
approach fails, or that nothing booted. This cycles red/green/blue
forever, so *motion* means booted + syscall returned + PCRTC live +
BGCOLOR reaching the display, and nothing means the problem is upstream
of the blend equation. It's the tool to run first on a console.

Publishes its proof-of-life before entering the loop, so
`just test-pcsx2` can still assert the setup ran even though it never
halts. Excluded from `test-qemu` for the same reason `solid-color` is.

### Unexplained: `%` makes it fail on PCSX2, and only on PCSX2

With `idx = (idx + 1) % COLORS.len()` in the loop, `color-cycle`
**never reaches `rust_main`** on PCSX2 — the proof-of-life value stays
0 even with `set_proof` moved to the first statement of the function.
The identical ELF runs correctly under qemu: `rust_main` reached,
`ra = 0x10022c`, `sp = 0x1ffffe8`, proof published, `set_gs_crt`
reached with `a1 = 3`. So the binary is sound.

What was ruled out:

- **Not `.rodata` access.** A `black_box`-indexed read of `COLORS`
  published the right byte on PCSX2.
- **Not the loop or the spin delay.** A loop writing a constant colour
  with the same delay passes.
- **Not segment layout.** `.bss` sits inside the RW `PT_LOAD`'s
  `memsz` in both the passing and failing builds.
- **Not PCSX2's CRC patching.** No patch, cheat or game-fix lines in
  the log for any of these CRCs.
- **Not the EE recompiler.** With `EnableEE = false` the TLB miss
  disappears but the proof still never appears, after 30+ seconds.
- **Not code size** in any monotonic sense: builds with `.bss` at
  `0x100308` and `0x100358` pass, `0x100318` and `0x1002a8` fail.

With the recompiler on, PCSX2 logs `TLB Miss, pc=<memset+0xc8>
addr=0x0 [load]` — memset's `jr $ra`, i.e. a return to address 0,
reported as a load because it's an instruction fetch. But memset is
called exactly once, from `rt_start`, and qemu shows `ra = 0x100224`
there, so that is a symptom rather than a cause.

Sidestepped by advancing the index with a compare-and-reset instead of
a division, which also avoids a divide in a bare-metal loop. Recorded
rather than solved; the comment in `color-cycle` warns anyone who
reintroduces `%`.

### Harness: regenerate PCSX2's config, don't patch it

`pcsx2_write_config` used to run `-testconfig` (a no-op when the ini
exists) and then `sed` three keys, so anything changed by hand while
debugging persisted into every later run. Setting `EnableEE = false`
during the investigation above would have silently become part of what
`just test-pcsx2` tests. It now deletes the ini first so every run
starts from PCSX2's defaults plus exactly our three overrides.

### Noted for later: `EnableEEConsole`

PCSX2's config has `EnableEEConsole` and `EnableEESIOInput`, which is
the lead for the text-output work (EE SIO at `0x1000_F180`). Not
touched yet. Worth knowing it won't help on real hardware: the Slim has
no exposed serial port, so SIO output there needs a physical cable mod.
