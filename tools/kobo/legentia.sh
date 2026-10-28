#!/bin/sh
# Legentia'yı Kobo'da Nickel'ın yerine çalıştırır; NickelMenu'den başlatılır
# (.adds/nm/legentia). Nickel ve yardımcıları durdurulur, Legentia çalışır,
# program nasıl biterse bitsin (Çık, güç tuşu, çökme, bekçi) cihaz yeniden
# başlatılır. Nickel'ı elle geri başlatmak Wi-Fi ve USB durumuna bağlı
# ayrıntılar istiyor; ilk denemelerde en güvenli dönüş yolu yeniden başlatmak.
#
# Bekçi: DENEME_SURESI saniye sonra program kendiliğinden durdurulur
# (ekran ya da dokunma çalışmazsa cihaz kendini kurtarsın diye). 0 = sınırsız.
DENEME_SURESI=300

DIR=/mnt/onboard/.adds/legentia
cd "$DIR" || exit 1

{
    echo "$(date) başlıyor"
    # Nickel'ın veritabanı yarım yazılmasın.
    sync
    killall -q -TERM nickel hindenburg sickel fickel strickel fontickel \
        adobehost foxitpdf iink dhcpcd-dbus dhcpcd fmon
    # Nickel kapanana kadar en çok 4 saniye bekle.
    n=0
    while pidof nickel >/dev/null && [ "$n" -lt 16 ]; do
        usleep 250000
        n=$((n + 1))
    done
    # Nickel'ın donanım bildirim borusu; açık kalırsa udev betikleri takılabilir.
    rm -f /tmp/nickel-hardware-status

    ./legentia-kobo /mnt/onboard &
    app=$!
    bekci=
    if [ "$DENEME_SURESI" -gt 0 ]; then
        (
            sleep "$DENEME_SURESI"
            echo "$(date) bekçi: süre doldu, program durduruluyor"
            kill -TERM "$app" 2>/dev/null
            sleep 5
            kill -KILL "$app" 2>/dev/null
        ) &
        bekci=$!
    fi
    wait "$app"
    echo "$(date) program bitti, çıkış kodu $?"
    [ -n "$bekci" ] && kill "$bekci" 2>/dev/null
    sync
} >>"$DIR/betik.log" 2>&1

reboot
