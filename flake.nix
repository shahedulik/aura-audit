{
  description = "Aura-Audit Forensic Supercomputer Environment";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, rust-overlay }:
    let
      system = "x86_64-linux";
      overlays = [ (import rust-overlay) ];
      pkgs = import nixpkgs { inherit system overlays; };
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        nativeBuildInputs = with pkgs; [
          (rust-bin.nightly.latest.default.override {
            extensions = [ "rust-src" "clippy" "rustfmt" ];
            targets = [ "x86_64-unknown-linux-gnu" ];
          })
          cmake
          gcc
          pkg-config
          perl
          openssl
          llvmPackages.libclang.dev
        ];

        buildInputs = with pkgs; [
          arrow-cpp
          openssl
          libclang
        ];

        shellHook = ''
          export LIBCLANG_PATH="${pkgs.llvmPackages.libclang.lib}/lib"
          export LLVM_CONFIG="${pkgs.llvmPackages.libllvm}/bin/llvm-config"
          export CMAKE_PREFIX_PATH="${pkgs.arrow-cpp}/lib/cmake/Arrow:$CMAKE_PREFIX_PATH"
          export PKG_CONFIG_PATH="${pkgs.openssl.out}/lib/pkgconfig:$PKG_CONFIG_PATH"
          
          # FIX: Allow older CMake policies for kuzu's bundled re2 library
          export CMAKE_POLICY_VERSION_MINIMUM=3.5
          
          echo "AURA-AUDIT GOD-LEVEL SHELL ACTIVATED"
          echo "CMake policy fix: CMAKE_POLICY_VERSION_MINIMUM=3.5"
        '';
      };
    };
}
