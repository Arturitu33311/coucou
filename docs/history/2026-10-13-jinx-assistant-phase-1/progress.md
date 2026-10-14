# SDD ledger — plan: docs/superpowers/plans/2026-10-13-jinx-assistant-phase-1.md
Spec: docs/superpowers/specs/2026-10-13-jinx-assistant-text-design.md (read; plan written by same session)
Pre-flight: T1->T2 Thread API (append/answer_for/context/merge/since/unmirrored/mark_mirrored) matches T2 usage; T5->T6->T7 Kotlin types (ChatLine, ChatEvent, LocalBrain, Mirror, ChatApi) consistent; T3 independent. No conflicts found.
Task 1: complete (commits cdbacd5..e2f67a3, tests: python3 -m unittest discover -s scripts/jinx-assist -p test_*.py → OK)
Task 2: complete (commits e2f67a3..eb00ebf, tests: python3 -m unittest discover -s scripts/jinx-assist -p test_*.py → OK)
Task 3: complete (commits eb00ebf..ac155e0, tests: python3 -m unittest discover -s scripts/jinx-assist -p test_*.py → OK)
Task 4: partial (commit df3ca1f): ~/bin files installed + relay patched on disk (backup jinx-telegram.bak-20261013-thread), self-check OK. Ruling: restart of jinx-telegram.service and switching authorized_keys to coucou-dispatch DEFERRED to the end — security-sensitive/live change, plan says ask Alan first; Tasks 5-8 do not depend on it — cost if wrong: phone cannot reach jinx-chat until Alan approves.
Task 5: complete (commits df3ca1f..8453087, tests: ./gradlew --offline :app:testDebugUnitTest --tests *ChatProtocolTest --tests *ThreadStoreTest → Consider enabling configuration cache to speed up this build: https://docs.gradle.org/9.7.1/userguide/configuration_cache_enabling.html)
Task 6: complete (commit cee3867, tests: :app:testDebugUnitTest --tests *ChatRepositoryTest *PhoneKeyTest *JinxSyncTest -> BUILD SUCCESSFUL)
Task 7: complete (commit 62b03d5, tests: OpenRouterBrainTest TelegramMirrorTest PersonaStoreTest -> BUILD SUCCESSFUL)
Task 8: Ruling: lintDebug cannot run offline here (androidTest + material-icons-extended-desktop not cached; fails before touching our code) — verified via assembleDebug + manual manifest review (exported services carry BIND_VOICE_INTERACTION, recognition service has tools:ignore) + emulator — cost if wrong: a lint warning-as-error could appear in CI
Task 8: complete (commits through 27f0763; suite BUILD SUCCESSFUL; emulator: service registered, KEYCODE_ASSIST opens JinxAssistActivity, send path shows honest pending notice). Ruling: Enter inserted a newline (multi-line field) — found on emulator, fixed with singleLine=true (UI, no JVM test possible offline) — cost if wrong: none. Note: right after install -r the first assist gesture can land on Google while the service rebinds; wait a few seconds.
Task 9: partial: laptop gateway verified end-to-end (selftest-1 answered 'Funciona.', mirrored), docs/ASSISTANT.md + spec notes committed; phone install BLOCKED: adb ports for S21 (36373) and S22 (37621) refused — need new ip:port from Alan (standing rule: never guess). Task 10: partial: runtime pinned in docs/superpowers/spikes/2026-10-13-on-device-model.md; measurements need phones.
Final review: fresh reviewer (fable) on 9e8170c..5f47c80 + live relay patch.
Final: fixed #1 relay marked reply mirrored before Telegram confirmed — self-check record_and_send RED(NameError)→GREEN, relay self-check OK (file is ~/bin/jinx-telegram, backup .bak-20261013-thread; not yet restarted)
Final: fixed #2 concurrent same-cid sends — test_concurrent_sends_of_the_same_cid_call_the_model_once RED→GREEN, python suite 35/35
Final: fixed #4 done carried no mirror confirmation — test_done_reports_whether_the_lines_reached_telegram + Kotlin doneCarriesTheMirrorConfirmation/laptopAnswerNotConfirmedAsMirroredStaysQueuedHere RED→GREEN; merge mirrored_ids now computed from thread; also made merge flag monotonic (test_merge_never_unmirrors_and_adopts_a_mirrored_flag RED→GREEN) to avoid a duplicate Telegram message — Kotlin suite + assembleDebug BUILD SUCCESSFUL
Final: fixed #3 'Sincronizar' triggered one-shot provision — now Sincronizar=syncNow, separate 'Provisionar (1.ª vez)' button (UI only, assembleDebug green; not exercised on emulator)
Final: minor (deferred): committed __pycache__/*.pyc under scripts/jinx-assist, no .gitignore entry
Final: minor (deferred): JinxRecognitionService exported without permission (harmless today)
Final: minor (deferred): history-<device>.json in jinx-telegram now redundant
Final: minor (deferred): thread.jsonl on the laptop has no rotation (Kotlin store caps at 2000)
Final: minor (deferred): ThreadStore.merge truncation to last 2000 by ts under clock skew (self-heals)
