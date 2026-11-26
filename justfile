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

# Static checks only -- no PCSX2/BIOS needed. Rebuilds (exercising the
# `const`-assertion checks in ps2-rt and each tool) then checks the
# resulting ELFs' structure (entry point, _gp/_stack_top formulas, bss)
# against what was verified by hand during the initial scaffold, plus
# tool-specific structural checks where a tool has any (see
# scripts/verify-solid-color.sh).
test: build
    ./scripts/verify-elf.sh target/mips64r5900el-ps2/release/hello
    ./scripts/verify-elf.sh target/mips64r5900el-ps2/release/solid-color
    ./scripts/verify-solid-color.sh target/mips64r5900el-ps2/release/solid-color
