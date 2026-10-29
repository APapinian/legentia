#!/bin/sh
# Legentia'yı Kobo'da Nickel'ın yerine çalıştırır; NickelMenu'den başlatılır
# (.adds/nm/legentia). Nickel ve yardımcıları durdurulur, Legentia çalışır,
# program nasıl biterse bitsin (Çık, güç tuşu, çökme, bekçi) cihaz yeniden
# başlatılır. Nickel'ı elle geri başlatmak Wi-Fi ve USB durumuna bağlı
# ayrıntılar istiyor; ilk denemelerde en güvenli dönüş yolu yeniden başlatmak.
#
# Bekçi: DENEME_SURESI saniye sonra program kendiliğinden durdurulur
# (ekran ya da dokunma çalışmazsa cihaz kendini kurtarsın diye). 0 = sınırsız.
DENEME_SURESI=0

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

    # Cihazda indirilip doğrulanan güncelleme (Araçlar → Güncellemeleri
    # denetle) burada kurulur; eski program yeni açılamazsa diye saklanır.
    yeni=
    if [ -f legentia-kobo.yeni ]; then
        echo "$(date) güncelleme kuruluyor"
        if mv -f legentia-kobo legentia-kobo.eski && mv -f legentia-kobo.yeni legentia-kobo; then
            chmod 755 legentia-kobo
            yeni=1
        fi
    fi
    basla=$(date +%s)

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
    kod=$?
    echo "$(date) program bitti, çıkış kodu $kod"
    # Yeni sürüm 20 saniye içinde hatayla kapandıysa eskisine dön.
    if [ -n "$yeni" ] && [ "$kod" -ne 0 ] && [ $(($(date +%s) - basla)) -lt 20 ]; then
        echo "$(date) yeni sürüm açılamadı; eski sürüme dönülüyor"
        mv -f legentia-kobo.eski legentia-kobo
    fi
    [ -n "$bekci" ] && kill "$bekci" 2>/dev/null
    sync
    # İlk denemede `reboot` cihazı yeniden başlatmadı; komut bu bloğun
    # dışında olduğu için hatası da kaydedilmedi. Artık tam yolla çağrılıyor
    # ve sonucu yazılıyor; 20 sn içinde olmazsa zorla yeniden başlatılır.
    echo "$(date) yeniden başlatılıyor"
    /sbin/reboot
    echo "$(date) reboot döndü: $?"
    sleep 20
    echo "$(date) hâlâ açık; zorla yeniden başlatılıyor"
    sync
    /sbin/reboot -f
} >>"$DIR/betik.log" 2>&1
