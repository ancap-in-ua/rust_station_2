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
