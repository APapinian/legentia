#!/bin/sh
# Legentia'nın Wi-Fi betiği (Libra 2: PLATFORM=mx6sll-ntx, WIFI_MODULE=8723ds,
# INTERFACE=wlan0; değerler Nickel'ın ortamından gelir, yoksa bunlar).
#   wifi.sh kapat   wpa_supplicant ve DHCP durur, arabirim iner, modüller kalkar
#   wifi.sh ac      modüller yüklenir, Nickel'ın kayıtlı ağlarıyla bağlanılır
PLATFORM=${PLATFORM:-mx6sll-ntx}
WIFI_MODULE=${WIFI_MODULE:-8723ds}
INTERFACE=${INTERFACE:-wlan0}
KMOD=/drivers/$PLATFORM/wifi

case "$1" in
kapat)
    killall -q udhcpc dhcpcd
    wpa_cli -i "$INTERFACE" terminate >/dev/null 2>&1
    killall -q wpa_supplicant
    ifconfig "$INTERFACE" down 2>/dev/null
    grep -q "^$WIFI_MODULE " /proc/modules && rmmod "$WIFI_MODULE"
    grep -q "^sdio_wifi_pwr " /proc/modules && rmmod sdio_wifi_pwr
    ;;
ac)
    grep -q "^sdio_wifi_pwr " /proc/modules || insmod "$KMOD/sdio_wifi_pwr.ko"
    usleep 250000
    grep -q "^$WIFI_MODULE " /proc/modules || insmod "$KMOD/$WIFI_MODULE.ko"
    n=0
    while [ ! -e "/sys/class/net/$INTERFACE" ] && [ "$n" -lt 20 ]; do
        usleep 250000
        n=$((n + 1))
    done
    ifconfig "$INTERFACE" up
    pidof wpa_supplicant >/dev/null || wpa_supplicant -D wext -s -i "$INTERFACE" \
        -c /etc/wpa_supplicant/wpa_supplicant.conf -C /var/run/wpa_supplicant -B
    udhcpc -S -i "$INTERFACE" -s /etc/udhcpc.d/default.script -b -q -t 30 >/dev/null 2>&1 &
    ;;
*)
    echo "kullanım: $0 ac|kapat" >&2
    exit 2
    ;;
esac
