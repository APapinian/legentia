{
  description = "Legentia: Kobo Libra 2 için Türkçe e-kitap okuyucu";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { nixpkgs, rust-overlay, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs {
        inherit system;
        overlays = [ rust-overlay.overlays.default ];
      };

      # Masaüstü + Kobo hedefi. Kobo için statik musl, linker rust-lld
      # (.cargo/config.toml); cihazdaki glibc'ye bağımlılık yok.
      rust = pkgs.rust-bin.stable.latest.default.override {
        extensions = [ "rust-src" "clippy" "rustfmt" ];
        targets = [ "armv7-unknown-linux-musleabihf" ];
      };

      # Simülatör penceresinin çalışma zamanında yüklediği kütüphaneler.
      simLibs = with pkgs; [ libx11 libxcursor libxrandr libxi libxkbcommon ];
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rust
          # ARMv7 test ikililerini masaüstünde çalıştırmak için (cargo test --target ...).
          pkgs.qemu-user
        ];

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath simLibs;

        # Geçici test fontu (Bitstream Vera lisansı, Türkçe karakterleri kapsar).
        # Asıl font seçimi M3'te yapılacak.
        LEGENTIA_FONT = "${pkgs.dejavu_fonts}/share/fonts/truetype/DejaVuSans.ttf";

        CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER = "qemu-arm";
      };
    };
}
