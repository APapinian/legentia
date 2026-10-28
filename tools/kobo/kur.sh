#!/usr/bin/env bash
# Legentia'yı USB ile bağlı Kobo'ya kurar, günceller ya da kaldırır.
#
#   tools/kobo/kur.sh <Kobo klasörü>                          kur / güncelle
#   tools/kobo/kur.sh --kaldir <Kobo klasörü>                 kaldır (veri/ kalır)
#   tools/kobo/kur.sh --kaldir --verileri-de-sil <Kobo klasörü>
#
# Yalnızca Legentia'nın kendi yollarına dokunur: .adds/legentia/ ve
# .adds/nm/legentia. Güncellemede okuma konumu ve notların durduğu
# .adds/legentia/veri/ korunur. Kurulumdan sonra kopyalanan her dosya sha256 ile
# doğrulanır. Ayrıntılar: docs/device-install.md. nix develop içinde çalıştırın.
set -euo pipefail

remove=false
purge=false
while [ "$#" -gt 1 ]; do
    case "$1" in
    --kaldir) remove=true ;;
    --verileri-de-sil) purge=true ;;
    *) break ;;
    esac
    shift
done
if [ "$#" -ne 1 ]; then
    sed -n '2,12p' "$0" >&2
    exit 2
fi
kobo=$1
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)

# Bağlanan aygıt gerçekten Kobo mu? (Cihaz kodu 388 = Libra 2.)
version="$kobo/.kobo/version"
if [ ! -f "$version" ]; then
    echo "Kobo bulunamadı: $version yok. Kobo'yu USB ile bağlayıp klasörünü verin." >&2
    exit 1
fi
model=$(cut -d, -f6 "$version" | sed 's/^[0-]*//')
if [ "$model" != "388" ]; then
    echo "Uyarı: bu cihaz Libra 2 değil (cihaz kodu $model). Legentia yalnızca Libra 2'de denendi."
    read -r -p "Yine de devam edilsin mi? [e/H] " answer
    [ "$answer" = "e" ] || exit 1
fi

app="$kobo/.adds/legentia"
menu="$kobo/.adds/nm/legentia"

if $remove; then
    if $purge; then
        echo "Siliniyor: $app (okuma konumu ve notlar dahil), $menu"
        rm -rf -- "$app"
    else
        echo "Siliniyor: $app (veri/ hariç), $menu"
        if [ -d "$app" ]; then
            find "$app" -mindepth 1 -maxdepth 1 ! -name veri -exec rm -rf -- {} +
            rmdir --ignore-fail-on-non-empty "$app"
        fi
    fi
    rm -f -- "$menu"
    sync
    echo "Kaldırıldı. Kobo'yu güvenle çıkarın; NickelMenu'deki satır yeniden başlatınca kaybolur."
    exit 0
fi

out="$root/target/kobo-paket"
"$here/paketle.sh" "$out" >/dev/null
echo "Paket hazırlandı: $out"

# Yalnızca paketteki dosyalar kopyalanır (veri/ ve günlüklere dokunulmaz).
mkdir -p "$app/fonts" "$kobo/.adds/nm"
(cd "$out/.adds" && find . -type f) | while read -r file; do
    cp -- "$out/.adds/$file" "$kobo/.adds/$file"
done
sync

bad=0
while read -r file; do
    if ! cmp -s "$out/.adds/$file" "$kobo/.adds/$file"; then
        echo "Doğrulanamadı: .adds/$file" >&2
        bad=1
    fi
done < <(cd "$out/.adds" && find . -type f)
if [ "$bad" -ne 0 ]; then
    echo "Kopyalanan dosyalar paketten farklı; Kobo'yu çıkarmadan yeniden deneyin." >&2
    exit 1
fi
count=$(cd "$out/.adds" && find . -type f | wc -l)
echo "Kuruldu ve doğrulandı ($count dosya). Kobo'yu güvenle çıkarın; menüde"
echo "\"Legentia\" görünmezse cihazı bir kez yeniden başlatın."
