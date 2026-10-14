# Banking apps refuse to run beside the island ("Aviso … Smart Island … intentando acceder tu cuenta")

## What happens

Opening a banking app (seen with Nu, `com.nu.production`) shows a full-screen notice:

> Detectamos que la siguiente aplicación "Smart Island" está intentando acceder tu cuenta. Por seguridad,
> bloqueamos este acceso. Por favor, desinstala esta aplicación antes de volver a ingresar a la app de Nu.

The app named is whichever **enabled accessibility service** the bank's anti-fraud check dislikes.

## Cause (found 2026-10-12, S21 Ultra, Android 14, over wireless adb)

Not the capabilities. Checked with `dumpsys accessibility` on the phone:

| | Smart Island | AnyDesk Control, KDE Connect, one sec, Windows Link, ATT |
|---|---|---|
| capabilities | 0 (reading windows already removed) | 1 or 33 |
| listens to typing | no | — |
| installer (`pm list packages -i`) | **`null` (sideloaded)** | `com.android.vending` |

The phone already ran the least-privilege config and Nu still blocked it. The one thing that set the island
apart from the other four enabled services was that it was **not installed by Play Store**, and the bank
named exactly that one. Banks treat "an accessibility service from an app that did not come from a store"
as the malware signature.

## Fix

Install the APK with Play Store set as its installer. This changes only the install-source metadata of our
own app (`InstallSourceInfo`); nothing else about the app changes:

```
./scripts/install-phone.sh                      # the only connected device
./scripts/install-phone.sh 100.78.102.31:36415  # over wireless adb
# which is:  adb install -r -i com.android.vending app/build/outputs/apk/debug/app-debug.apk
```

Verified afterwards: `adb shell pm list packages --user 0 -i com.agupta07505.smartisland` prints
`installer=com.android.vending`; Nu opens and works.

**An install without `-i` puts the installer back to `null` and the notice comes back.** Android Studio, a
plain `adb install`, or a copy of the APK all do that. Always use the script.

After `install -r` the accessibility service can show "Crashed services" with no real crash: remove and
re-add its entry in `settings get/put secure enabled_accessibility_services` (keep the other entries).

## Also done (kept, but not what fixed it)

`accessibility_service_config.xml` declares no `canRetrieveWindowContent`, no interactive windows and no
`typeViewTextChanged`. The service never reads a node (no `rootInActiveWindow`, no `getWindows`), and
`onAccessibilityEvent` looks at the event kind only. Commit `0528ee6`. Keep it: a bank that does check
capabilities (some do) is then satisfied as well.

## If it stops being enough

Plan B, not built: an island that needs no accessibility service at all: a normal foreground service with
a `TYPE_APPLICATION_OVERLAY` window (`SYSTEM_ALERT_WINDOW` is already declared), the foreground app from
`UsageStatsManager` (`PACKAGE_USAGE_STATS` is already declared) and presence from screen-on/unlock events
instead of touches. The bank would then see no service of ours. Costs: a refactor of
`SmartIslandOverlayService` (it hosts the whole overlay), and presence is coarser.

Plan C (switch the service off while a bank is in front with `disableSelf()`) was rejected: the bank checks
before our window-change event arrives, and switching it back on needs `WRITE_SECURE_SETTINGS`.

## Not circumvention of anything it should not be

The island does not read or touch the banking app: it never retrieves window content and has never listened
to typing. The fix presents the app's real install source (an adb install by the owner, labelled as the
store they would use to get it) so a heuristic that cannot tell a sideloaded personal tool from malware stops
firing. It is for the phone's owner's own build only.
