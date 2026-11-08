# Etap 10: poprawki-po-wydaniu

## Cel
Poprawić znane błędy i braki z wydania 1.0.0 i wydać wersję 1.0.1 z nowym instalatorem.

## Kryteria zamknięcia
- [ ] Edytor ma przełącznik autostartu: zmienia wpis HKCU Run, zgodny z wyborem z instalatora
- [ ] Drugie uruchomienie edytora pokazuje działające okno zamiast drugiej kopii
- [ ] Długie nazwy w podglądzie edytora łamią się jak w kole i nie wychodzą poza kafelek
- [ ] Błąd po Escape: przyczyna znaleziona i poprawiona (48 par „otwórz, Escape, otwórz” bez błędu) albo opisana w Decyzjach jako znana, z wynikiem diagnozy
- [ ] Wyłączenie szkła w działającym kole zwalnia pamięć bez restartu (bez szkła do 15/17 MB) albo decyzja o pozostawieniu
- [ ] Usunięte wzmianki „claude.ai”, „scratchpad”, „dodany przez Claude” z plików etapów 01–08
- [ ] Wersja 1.0.1 w obu exe, instalator zbudowany, aktualizacja z 1.0.0 sprawdzona na LAB, szkic wydania v1.0.1
- [ ] Testy, clippy i fmt przechodzą; README i CHECKLISTA zgodne ze zmianami

## Plan
- [ ] Autostart w edytorze: `apps/wheel-editor/src/options.rs`, `main.rs` (odczyt i zapis HKCU Run, ta sama nazwa wpisu co w `installer/elemental-wheel.iss`)
- [ ] Jedna kopia edytora: `apps/wheel-editor/src/main.rs` (mutex, przeniesienie okna na wierzch)
- [ ] Łamanie nazw w podglądzie: `apps/wheel-editor` (podgląd), ewentualnie wspólna funkcja w `crates/wheel-core`
- [ ] Błąd po Escape: diagnoza `tools/soak.ps1` i ślad w `apps/wheel/src/hook.rs`, `window.rs`; jedna runda, potem decyzja
- [ ] Pamięć szkła: zwalnianie zasobów szkła po `glass_pct = 0` w `apps/wheel/src/window.rs` lub rendererze
- [ ] Usunięcie wzmianek z `docs/etapy/01..08`
- [ ] Wersja 1.0.1: `Cargo.toml`, `tools/build-installer.ps1`, test aktualizacji, README, CHECKLISTA, szkic wydania
- Sprawdzenie: `cargo test`, clippy, fmt; `soak.ps1` dla Escape (po zgodzie, przejmuje pulpit); pomiar pamięci `--stats`; zrzut podglądu z długimi nazwami do akceptacji; instalacja 1.0.1 na 1.0.0 przy działającym kole

## Decyzje
- (dopisywane na bieżąco: decyzja i powód)

## Podsumowanie
(wypełnia /etap-zamknij)
