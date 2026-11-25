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

# Boot the `hello` example ELF in PCSX2.
run-pcsx2: build
    ./scripts/run-pcsx2.sh target/mips64r5900el-ps2/release/hello

# Static checks only -- no PCSX2/BIOS needed. Rebuilds (exercising the
# `const`-assertion checks in ps2-rt) then checks the resulting ELF's
# structure (entry point, _gp/_stack_top formulas, bss) against what was
# verified by hand during the initial scaffold.
test: build
    ./scripts/verify-elf.sh target/mips64r5900el-ps2/release/hello
