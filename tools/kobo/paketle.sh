#!/usr/bin/env bash
# Kobo paketi: cihaz programını (statik ARMv7, simgeleri atılmış) derler ve
# Kobo'nun kök klasörüne aynen kopyalanacak ağacı kurar:
#
#   <çıktı>/.adds/legentia/legentia-kobo   program
#   <çıktı>/.adds/legentia/legentia.sh     başlatma betiği
#   <çıktı>/.adds/legentia/fonts/          fontlar ve OFL lisans metinleri
#   <çıktı>/.adds/legentia/heceleme-lisanslari.md  gömülü heceleme desenlerinin lisansları
#   <çıktı>/.adds/nm/legentia              NickelMenu satırı
#
# Kullanım (nix develop içinde): tools/kobo/paketle.sh <çıktı klasörü>
# Cihaza hiçbir şey yazmaz; kopyalama ayrı ve elle yapılır (docs/device-install.md).
set -euo pipefail

if [ "$#" -ne 1 ]; then
    echo "Kullanım: $0 <çıktı klasörü>" >&2
    exit 2
fi
out=$1
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
target=armv7-unknown-linux-musleabihf

: "${LEGENTIA_FONTS:?nix develop içinde çalıştırın (LEGENTIA_FONTS yok)}"

cd "$root"
cargo build --release -p legentia-kobo --target "$target" --config 'profile.release.strip=true'

app="$out/.adds/legentia"
rm -rf "$out"
mkdir -p "$app/fonts" "$out/.adds/nm"
cp "target/$target/release/legentia-kobo" "$app/"
install -m 755 "$here/legentia.sh" "$app/legentia.sh"
install -m 755 "$here/wifi.sh" "$app/wifi.sh"
install -m 644 "$LEGENTIA_FONTS"/* "$app/fonts/"
install -m 644 "$here"/font-lisanslari/* "$app/fonts/"
# Programa gömülü heceleme desenlerinin telif ve lisans notları.
install -m 644 "$root/crates/core/hyph/LISANSLAR.md" "$app/heceleme-lisanslari.md"
install -m 644 "$here/nm-legentia" "$out/.adds/nm/legentia"

echo "Paket hazır: $out"
find "$out" -type f -printf '%10s  %P\n' | sort -k2
