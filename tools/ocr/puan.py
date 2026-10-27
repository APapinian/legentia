#!/usr/bin/env python3
"""OCR çıktısını doğru metinle karşılaştırır.

Kullanım: puan.py <doğru.txt> <ocr.txt>
Çıktı (tek satır, sekmeyle ayrılmış): karakter hata oranı, kelime hata oranı,
Türkçe harfli kelime doğruluğu, karşılaştırılan karakter sayısı.

Karşılaştırmadan önce iki metin de NFC'ye çevrilir, boşluklar tek boşluğa
indirilir ve tırnak/kesme işaretleri birleştirilir (’ ' ‘ → ', “ ” → ").
"""

import sys
import unicodedata
from collections import Counter

TURKISH = set("çğıöşüÇĞİÖŞÜ")


def normalize(text: str) -> str:
    text = unicodedata.normalize("NFC", text)
    for a, b in (("’", "'"), ("‘", "'"), ("“", '"'), ("”", '"'), ("–", "-"), ("—", "-")):
        text = text.replace(a, b)
    return " ".join(text.split())


def distance(a, b) -> int:
    """Levenshtein uzaklığı (ekleme, silme, değiştirme)."""
    prev = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        cur = [i]
        for j, y in enumerate(b, 1):
            cur.append(min(prev[j] + 1, cur[j - 1] + 1, prev[j - 1] + (x != y)))
        prev = cur
    return prev[-1]


def main() -> None:
    truth = normalize(open(sys.argv[1], encoding="utf-8").read())
    ocr = normalize(open(sys.argv[2], encoding="utf-8").read())
    cer = distance(truth, ocr) / max(len(truth), 1)
    tw, ow = truth.split(), ocr.split()
    wer = distance(tw, ow) / max(len(tw), 1)
    turkish_words = [w for w in tw if TURKISH & set(w)]
    found = Counter(ow)
    hits = 0
    for w in turkish_words:
        if found[w] > 0:
            found[w] -= 1
            hits += 1
    turkish = hits / max(len(turkish_words), 1)
    print(f"{cer:.4f}\t{wer:.4f}\t{turkish:.4f}\t{len(truth)}")


if __name__ == "__main__":
    main()
