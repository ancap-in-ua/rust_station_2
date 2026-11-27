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
    ./scripts/run-pcsx2.sh target/mips64r5900el-ps2/release/{{tool}}

# Boot a tool under qemu with an interactive gdb attached, frozen at the
# entry point. Not a PS2 -- no GS, no BIOS syscalls; see
# scripts/lib-qemu.sh.
#
# Boot a tool's ELF under qemu + gdb (default: hello). e.g. `just debug-qemu alloc-hello`.
debug-qemu tool="hello": build
    ./scripts/debug-qemu.sh target/mips64r5900el-ps2/release/{{tool}}

# All checks, static and dynamic. No BIOS dump needed for any of it.
test: test-static test-qemu

# Rebuilds (exercising the `const`-assertion checks in ps2-rt and each
# tool) then checks the resulting ELFs' structure (entry point,
# _gp/_stack_top formulas, bss) against what was verified by hand during
# the initial scaffold, plus tool-specific structural checks where a tool
# has any (see scripts/verify-solid-color.sh).
#
# Static ELF structure checks -- no emulator at all.
test-static: build
    ./scripts/verify-elf.sh target/mips64r5900el-ps2/release/hello
    ./scripts/verify-elf.sh target/mips64r5900el-ps2/release/solid-color
    ./scripts/verify-solid-color.sh target/mips64r5900el-ps2/release/solid-color
    # alloc-hello's bss isn't empty: ps2_rt::heap's static HEAP (64 KiB)
    # plus BumpAllocator's `next` cursor (one usize, 8 bytes).
    ./scripts/verify-elf.sh target/mips64r5900el-ps2/release/alloc-hello "$((64 * 1024 + 8))"

# Actually executes each ELF under qemu (needs qemu + gdb, but no PS2
# BIOS dump) and asserts what it did -- boot shim, $gp/$sp, bss zeroing,
# the allocator, proof-of-life values. Does NOT cover the GS or BIOS
# syscalls; see scripts/lib-qemu.sh for the full list of limits.
#
# Dynamic checks: run each ELF under qemu and assert what it did.
test-qemu: build
    ./scripts/verify-qemu.sh target/mips64r5900el-ps2/release/hello 0xc0ffee42
    # 0xA110_0000 | 140, where 140 = sum of i*i for i in 0..8 -- computed
    # from heap memory the bump allocator actually handed out.
    ./scripts/verify-qemu.sh target/mips64r5900el-ps2/release/alloc-hello 0xa110008c
    # solid-color can't run to completion under qemu (its BIOS syscall
    # has no implementation there and hangs), so it gets the subset of
    # its behaviour that is checkable instead.
    ./scripts/verify-solid-color-qemu.sh target/mips64r5900el-ps2/release/solid-color
