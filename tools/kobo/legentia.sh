#!/bin/sh
# Legentia'yı Kobo'da Nickel'ın yerine çalıştırır; NickelMenu'den başlatılır
# (.adds/nm/legentia). Nickel ve yardımcıları durdurulur, Legentia çalışır;
# program nasıl biterse bitsin (Çık, güç tuşu, çökme) Nickel yeniden
# başlatılır, başlamazsa cihaz yeniden başlatılır.
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
    # Nickel'a dönüş (yeniden başlatmadan): donanım borusu yeniden kurulur,
    # hindenburg ve nickel Nickel'ın kendi ortamıyla (NickelMenu'den gelen)
    # başlatılır. 20 sn içinde Nickel ayağa kalkmazsa yeniden başlatılır.
    echo "$(date) Nickel başlatılıyor"
    cd /
    rm -f /tmp/nickel-hardware-status
    mkfifo /tmp/nickel-hardware-status
    (/usr/local/Kobo/hindenburg >/dev/null 2>&1 &)
    LIBC_FATAL_STDERR_=1 /usr/local/Kobo/nickel -platform kobo -skipFontLoad >/dev/null 2>&1 &
    n=0
    while ! pidof nickel >/dev/null && [ "$n" -lt 20 ]; do
        sleep 1
        n=$((n + 1))
    done
    sleep 5
    if pidof nickel >/dev/null; then
        echo "$(date) Nickel çalışıyor"
        exit 0
    fi
    echo "$(date) Nickel başlamadı; yeniden başlatılıyor"
    sync
    /sbin/reboot
    sleep 20
    /sbin/reboot -f
} >>"$DIR/betik.log" 2>&1
