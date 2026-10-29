#!/usr/bin/env bash
# GitHub sürümüne yüklenecek dosyaları hazırlar: cihazdaki güncelleyicinin
# (Araçlar → Güncellemeleri denetle) beklediği `legentia-kobo` (statik ARMv7,
# simgeleri atılmış) ve `legentia-kobo.sha256`.
#
#   tools/kobo/surum.sh <çıktı klasörü>
#
# Hiçbir şey yüklemez. Etiket Cargo sürümüyle aynı olmalı (ör. v0.2.0);
# yüklemek için betiğin sonunda yazdırılan gh komutu kullanılabilir.
# nix develop içinde çalıştırın.
set -euo pipefail

if [ "$#" -ne 1 ]; then
    sed -n '2,10p' "$0" >&2
    exit 2
fi
out=$1
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
target=armv7-unknown-linux-musleabihf

cd "$root"
cargo build --release -p legentia-kobo --target "$target" --config 'profile.release.strip=true'
version=$(cargo metadata --no-deps --format-version 1 |
    sed -n 's/.*"name":"legentia-kobo","version":"\([^"]*\)".*/\1/p')

mkdir -p "$out"
cp "target/$target/release/legentia-kobo" "$out/legentia-kobo"
(cd "$out" && sha256sum legentia-kobo > legentia-kobo.sha256)
echo "Hazır ($out):"
cat "$out/legentia-kobo.sha256"
echo "Yayınlamak için (onaylarsanız): gh release create v${version:-SÜRÜM} \"$out/legentia-kobo\" \"$out/legentia-kobo.sha256\""
