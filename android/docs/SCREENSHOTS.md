# Screenshots of Coucou's screens without a phone

The Coucou app's screens (Jinx · Pendientes · Ajustes, `coucou/CoucouHome.kt`) can be drawn on the JVM with
[Paparazzi](https://github.com/cashapp/paparazzi) (Android's layoutlib), e.g. in a cloud session with no emulator.
Paparazzi is **not** part of the build: it is added to a scratch copy of `android/` only.

1. Copy `android/` somewhere else (without `build/`, `.gradle/`).
2. In the copy: root `build.gradle.kts` → `classpath("app.cash.paparazzi:paparazzi-gradle-plugin:2.0.0-alpha05.1")`;
   `app/build.gradle.kts` → `id("app.cash.paparazzi")` in `plugins`.
3. Put `screenshots/AfterShots.kt.txt` and `TallShots.kt.txt` (renamed `.kt`) under `app/src/test/java/shots/`.
4. `./gradlew :app:recordPaparazziDebug --tests "*AfterShots" --tests "*TallShots"`; the PNGs land in
   `app/src/test/snapshots/images/`. Where Maven Central rate-limits (HTTP 429), add
   `--init-script maven-central-mirror.gradle.txt` (Google's mirror of Central).

The tabs' contents are stateless (`AssistContent`, `TasksContent`, `CoucouScaffold`) so they draw from sample data.
`Setup` reads the real settings; `TallShots` puts a throwaway key in place so the connection cards show. Paparazzi
shrinks tall images to 1000 px, hence the four offset screens of Ajustes. Dates follow the JVM's locale (English
there, the phone's language on the phone).
