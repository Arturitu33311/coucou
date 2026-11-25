# Banking apps and the island

Some banking apps (Nu, and others) refuse to open while an accessibility service from an app **not installed by a
store** is active. Android reports who installed a package (`installer`). A plain `adb install` leaves it empty,
and the bank app then treats the island as a possible overlay-fraud tool.

## The rule

Always install the phone app with the store marked as the installer:

```
coucou-setup phone --serial <ip:port>        # from the laptop, any phone
# or directly:
android/scripts/install-phone.sh <ip:port> [--build] [--fix-a11y]
```

Both run `adb install -r -i com.android.vending …` and then **verify**: the installer must read
`com.android.vending` and the accessibility service must be enabled. If the installer is wrong the script exits 2
and says so; never use a bare `adb install`.

## Getting the serial

Settings → Developer options → Wireless debugging shows `ip:port` (the port changes each time it is switched on).
`adb mdns services` lists phones that are advertising it on the same network. The IP is the phone's Tailscale or
LAN address.

## If something is off afterwards

- The in-app Ajustes screen warns when the app was installed without a store ("Instalación sin tienda").
- Island missing after an install: run the installer again with `--fix-a11y`.
- Right after `install -r`, wait a few seconds before using the assistant gesture (the script already waits).
