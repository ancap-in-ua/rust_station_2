{
  description = "Bare-metal Rust tools for the PS2 Emotion Engine (MIPS R5900)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = { self, nixpkgs }:
    let
      system = "x86_64-linux";
      pkgs = nixpkgs.legacyPackages.${system};
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        name = "ps2-rust-dev";

        # Deliberately NOT providing rustc/cargo here: the Rust toolchain
        # comes from rustup, pinned per-project by rust-toolchain.toml
        # (nightly + rust-src + llvm-tools-preview, for build-std and
        # cargo-binutils respectively). This shell only supplies the
        # auxiliary tooling nixpkgs actually has for PS2 dev; ps2dev/
        # ps2sdk (the EE GCC toolchain) are not packaged in nixpkgs and
        # are intentionally unused by this bare-metal setup.
        packages = with pkgs; [
          cargo-binutils # cargo objdump / nm / size, via llvm-tools-preview
          pcsx2 # emulator, for fast iteration
          gdb # can attach to PCSX2's gdbstub or a real-hardware target
          just
        ];

        shellHook = ''
          echo "ps2-rust-dev shell: pcsx2, cargo-binutils, gdb, just available."
          echo "Rust itself comes from rustup via rust-toolchain.toml."
        '';
      };
    };
}
