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

      # OCR deneyi (yalnızca NixOS'ta; cihaz için ayrı karar verilecek). Apache-2.0.
      tesseract = pkgs.tesseract.override { enableLanguages = [ "tur" "eng" "osd" ]; };

      # Uygulamanın kullandığı fontlar (hepsi SIL OFL 1.1), tek klasörde.
      # Dosya adları crates/core/src/text/fonts.rs ile aynı olmalı.
      fonts = pkgs.runCommand "legentia-fonts" { } ''
        mkdir -p $out
        for s in Regular Italic Bold BoldItalic; do
          cp ${pkgs.literata}/share/fonts/truetype/Literata-$s.ttf $out/
          cp ${pkgs.gentium}/share/fonts/truetype/Gentium-$s.ttf $out/
        done
        for s in Regular It Bold BoldIt; do
          cp ${pkgs.source-serif}/share/fonts/opentype/SourceSerif4-$s.otf $out/
          cp ${pkgs.source-sans}/share/fonts/opentype/SourceSans3-$s.otf $out/
        done
      '';
    in
    {
      devShells.${system}.default = pkgs.mkShell {
        packages = [
          rust
          # ARMv7 test ikililerini masaüstünde çalıştırmak için (cargo test --target ...).
          pkgs.qemu-user
        ];

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath simLibs;

        LEGENTIA_FONTS = "${fonts}";
        LEGENTIA_TESSERACT = "${tesseract}/bin/tesseract";

        CARGO_TARGET_ARMV7_UNKNOWN_LINUX_MUSLEABIHF_RUNNER = "qemu-arm";

        # ARMv7 için C/assembly kodu (ring: TLS şifrelemesi) clang ile derlenir.
        # ring'in C kodu libc'ye ihtiyaç duymaz; çapraz GCC/musl araç zinciri gerekmez.
        CC_armv7_unknown_linux_musleabihf = "${pkgs.llvmPackages.clang-unwrapped}/bin/clang";
        CFLAGS_armv7_unknown_linux_musleabihf = "--target=armv7-unknown-linux-musleabihf -ffreestanding";
        AR_armv7_unknown_linux_musleabihf = "${pkgs.llvmPackages.llvm}/bin/llvm-ar";
      };
    };
}
