#!/usr/bin/env bash
# OCR deneyi (M5): metin katmanı olan bir PDF'i görüntüye çevirir, taramayı
# taklit eden bozulmalar ekler, Tesseract ile okur ve sonucu PDF'in kendi
# metniyle karşılaştırır. Ayrıca OCR metni gömülü (aranabilir) PDF üretir.
#
# Kullanım: tools/ocr/deney.sh <belge.pdf> <çıktı klasörü>
# Girdi PDF'i değiştirilmez. Araçlar nix ile gelir (poppler, imagemagick, tesseract).
set -euo pipefail
# Sayılar noktayla yazılsın (Türkçe yerel ayarda ondalık ayırıcı virgüldür).
export LC_ALL=C.UTF-8

if [ "$#" -ne 2 ]; then
    echo "Kullanım: $0 <belge.pdf> <çıktı klasörü>" >&2
    exit 2
fi
pdf=$1
out=$2
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$out"

if [ -z "${DENEY_ICINDE:-}" ]; then
    exec env DENEY_ICINDE=1 nix shell nixpkgs#poppler-utils nixpkgs#imagemagick nixpkgs#tesseract nixpkgs#python3 \
        -c bash "$0" "$@"
fi

pdftotext "$pdf" "$out/dogru.txt"

# Görüntü çeşitleri: ad, çözünürlük, ImageMagick bozulmaları.
render() {
    local name=$1 dpi=$2
    shift 2
    pdftoppm -r "$dpi" -gray -png -singlefile "$pdf" "$out/$name-ham"
    if [ "$#" -gt 0 ]; then
        magick "$out/$name-ham.png" "$@" "$out/$name.png"
        rm "$out/$name-ham.png"
    else
        mv "$out/$name-ham.png" "$out/$name.png"
    fi
}
render temiz-300 300
render temiz-150 150
# Tarama taklidi: hafif eğiklik, bulanıklık, gürültü, gri zemin.
render tarama-300 300 -background white -rotate 1.2 -flatten -blur 0x0.9 \
    -attenuate 0.35 +noise Gaussian -level 8%,92% -colorspace Gray
# Kötü tarama: düşük çözünürlük, daha çok eğiklik, JPEG sıkıştırma.
render kotu-200 200 -background white -rotate 2.5 -flatten -blur 0x1.1 \
    -attenuate 0.6 +noise Gaussian -level 15%,85% -colorspace Gray
magick "$out/kotu-200.png" -quality 35 "$out/kotu-200.jpg"
magick "$out/kotu-200.jpg" "$out/kotu-200.png"
# Aynı kötü tarama, yalnızca eğikliği düzeltilmiş (ön işlemenin etkisi).
magick "$out/kotu-200.png" -deskew 40% "$out/kotu-200-duzeltilmis.png"

printf "görüntü\tdil\tkarakter_hata\tkelime_hata\ttürkçe_kelime_doğru\tkarakter\tsüre_sn\n" | tee "$out/sonuc.tsv"
for img in temiz-300 temiz-150 tarama-300 kotu-200 kotu-200-duzeltilmis; do
    for lang in tur eng; do
        start=$EPOCHREALTIME
        tesseract "$out/$img.png" "$out/$img-$lang" -l "$lang" --psm 3 txt >/dev/null 2>&1
        end=$EPOCHREALTIME
        score=$(python3 "$here/puan.py" "$out/dogru.txt" "$out/$img-$lang.txt")
        seconds=$(python3 -c "print(f'{$end - $start:.2f}')")
        printf "%s\t%s\t%s\t%s\n" "$img" "$lang" "$score" "$seconds" | tee -a "$out/sonuc.tsv"
    done
done

# Aranabilir PDF: görüntü + görünmez metin katmanı. Katmandan geri okunan
# metin de aynı ölçüyle puanlanır.
tesseract "$out/tarama-300.png" "$out/aranabilir" -l tur --psm 3 pdf hocr >/dev/null 2>&1
pdftotext "$out/aranabilir.pdf" "$out/aranabilir.txt"
printf "aranabilir-pdf\ttur\t%s\t-\n" "$(python3 "$here/puan.py" "$out/dogru.txt" "$out/aranabilir.txt")" | tee -a "$out/sonuc.tsv"

# Sayfa düzeni: Tesseract'ın bulduğu paragraf ve satır sayısı (yeniden akıtma için).
printf "\nhOCR: %s paragraf, %s satır (tarama-300, tur)\n" \
    "$(grep -c "class='ocr_par'" "$out/aranabilir.hocr")" \
    "$(grep -c "class='ocr_line'" "$out/aranabilir.hocr")" | tee -a "$out/sonuc.tsv"
