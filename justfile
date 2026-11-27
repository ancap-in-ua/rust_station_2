default:
    @just --list

# Build all tools (release).
build:
    cargo build --release

# Build all tools (debug, faster iteration, bigger/slower binaries).
build-debug:
    cargo build

# Disassemble the `hello` example.
objdump:
    cargo objdump --release --bin hello -- -d

# Print section sizes for the `hello` example.
size:
    cargo size --release --bin hello

# Boot a tool's ELF in PCSX2 (default: hello). e.g. `just run-pcsx2 solid-color`.
run-pcsx2 tool="hello": build
    ./scripts/run-pcsx2.sh target/mipsel-sony-ps2/release/{{tool}}

# Boot a tool under qemu with an interactive gdb attached, frozen at the
# entry point. Not a PS2 -- no GS, no BIOS syscalls; see
# scripts/lib-qemu.sh.
#
# Boot a tool's ELF under qemu + gdb (default: hello). e.g. `just debug-qemu alloc-hello`.
debug-qemu tool="hello": build
    ./scripts/debug-qemu.sh target/mipsel-sony-ps2/release/{{tool}}

# test-static and test-qemu need no BIOS dump; test-pcsx2 skips loudly
# without one.
#
# All three check layers: static, qemu, and real PCSX2.
test: test-static test-qemu test-pcsx2

# Rebuilds (exercising the `const`-assertion checks in ps2-rt and each
# tool) then checks the resulting ELFs' structure (entry point,
# _gp/_stack_top formulas, bss) against what was verified by hand during
# the initial scaffold, plus tool-specific structural checks where a tool
# has any (see scripts/verify-solid-color.sh).
#
# Static ELF structure checks -- no emulator at all.
test-static: build
    # 4 bytes of bss: ps2_rt's PS2_RT_PROOF, the proof-of-life slot.
    ./scripts/verify-elf.sh target/mipsel-sony-ps2/release/hello 4
    ./scripts/verify-elf.sh target/mipsel-sony-ps2/release/solid-color 4
    # color-cycle / fb-fill: PS2_RT_PROOF (4) + a 12-quadword GIF packet
    # (192 bytes) in bss for the DMAC to read.
    ./scripts/verify-elf.sh target/mipsel-sony-ps2/release/color-cycle "$((4 + 192))"
    ./scripts/verify-elf.sh target/mipsel-sony-ps2/release/fb-fill "$((4 + 192))"
    ./scripts/verify-solid-color.sh target/mipsel-sony-ps2/release/solid-color
    # alloc-hello: PS2_RT_PROOF (4) + 12 bytes padding to align
    # ps2_rt::heap's HEAP to 16 + HEAP itself (64 KiB) + BumpAllocator's
    # `next` cursor (one usize = 4 bytes under o32).
    ./scripts/verify-elf.sh target/mipsel-sony-ps2/release/alloc-hello "$((4 + 12 + 64 * 1024 + 4))"

# Actually executes each ELF under qemu (needs qemu + gdb, but no PS2
# BIOS dump) and asserts what it did -- boot shim, $gp/$sp, bss zeroing,
# the allocator, proof-of-life values. Does NOT cover the GS or BIOS
# syscalls; see scripts/lib-qemu.sh for the full list of limits.
#
# Dynamic checks: run each ELF under qemu and assert what it did.
test-qemu: build
    ./scripts/verify-qemu.sh target/mipsel-sony-ps2/release/hello 0xc0ffee42
    # 0xA110_0000 | 140, where 140 = sum of i*i for i in 0..8 -- computed
    # from heap memory the bump allocator actually handed out.
    ./scripts/verify-qemu.sh target/mipsel-sony-ps2/release/alloc-hello 0xa110008c
    # solid-color can't run to completion under qemu (its BIOS syscall
    # has no implementation there and hangs), so it gets the subset of
    # its behaviour that is checkable instead.
    ./scripts/verify-solid-color-qemu.sh target/mipsel-sony-ps2/release/solid-color

# Runs each ELF on an actual PS2: PCSX2 with a real BIOS. The only layer
# that proves anything about a PlayStation 2 -- it's what caught the
# ELF-class and proof-of-life bugs that static and qemu checks were both
# green on. Needs a dump in bios_dump/; skips loudly without one.
#
# Dynamic checks: run each ELF on real PCSX2 and assert what it did.
test-pcsx2: build
    ./scripts/verify-pcsx2.sh target/mipsel-sony-ps2/release/hello 0xc0ffee42
    ./scripts/verify-pcsx2.sh target/mipsel-sony-ps2/release/alloc-hello 0xa110008c
    # solid-color gets this far: the BIOS SetGsCrt call returns and all
    # four GS registers are written. What it does NOT yet do is turn the
    # screen red -- see docs/next-steps.md #2.
    ./scripts/verify-pcsx2.sh target/mipsel-sony-ps2/release/solid-color 0x5011d000
    # color-cycle never halts (it cycles forever by design), so only
    # its pre-loop setup is assertable -- which is the point: reaching
    # the loop at all means the PCRTC bring-up ran.
    ./scripts/verify-pcsx2.sh target/mipsel-sony-ps2/release/color-cycle 0xc0c00000
    # fb-fill actually draws: a full-screen sprite via GIF/DMA. Its
    # proof value is published only after the DMA has drained, so this
    # also covers the transfer completing.
    ./scripts/verify-pcsx2.sh target/mipsel-sony-ps2/release/fb-fill 0xfbf11100
