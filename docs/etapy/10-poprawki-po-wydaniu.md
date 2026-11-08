# Etap 10: poprawki-po-wydaniu

## Cel
Poprawić znane błędy i braki z wydania 1.0.0 i wydać wersję 1.0.1 z nowym instalatorem.

## Kryteria zamknięcia
- [x] Edytor ma przełącznik autostartu: zmienia wpis HKCU Run, zgodny z wyborem z instalatora
- [ ] Drugie uruchomienie edytora pokazuje działające okno zamiast drugiej kopii
- [x] Długie nazwy w podglądzie edytora łamią się jak w kole i nie wychodzą poza kafelek
- [x] Błąd po Escape: przyczyna znaleziona i poprawiona (48 par „otwórz, Escape, otwórz” bez błędu) albo opisana w Decyzjach jako znana, z wynikiem diagnozy
- [x] Wyłączenie szkła w działającym kole zwalnia pamięć bez restartu (bez szkła do 15/17 MB) albo decyzja o pozostawieniu
- [x] Usunięte wzmianki „claude.ai”, „scratchpad”, „dodany przez Claude” z plików etapów 01–08
- [x] Wersja 1.0.1 w obu exe, instalator zbudowany, aktualizacja z 1.0.0 sprawdzona na LAB, szkic wydania v1.0.1
- [ ] Podpis „O – opcje” w środku koła przy każdym otwarciu, znika po 10 s; klawisz O przy otwartym kole zamyka koło i otwiera edytor na zakładce Opcje, litera nie trafia do programu pod kołem; wygląd zaakceptowany na zrzucie
- [x] Testy, clippy i fmt przechodzą; README i CHECKLISTA zgodne ze zmianami

## Plan
- [x] Autostart w edytorze: `apps/wheel-editor/src/options.rs`, `main.rs` (odczyt i zapis HKCU Run, ta sama nazwa wpisu co w `installer/elemental-wheel.iss`)
- [x] Jedna kopia edytora: `apps/wheel-editor/src/main.rs` (mutex, przeniesienie okna na wierzch)
- [x] Łamanie nazw w podglądzie: `apps/wheel-editor` (podgląd), ewentualnie wspólna funkcja w `crates/wheel-core`
- [x] Błąd po Escape: diagnoza `tools/soak.ps1` i ślad w `apps/wheel/src/hook.rs`, `window.rs`; jedna runda, potem decyzja
- [x] Pamięć szkła: zwalnianie zasobów szkła po `glass_pct = 0` w `apps/wheel/src/window.rs` lub rendererze
- [x] Usunięcie wzmianek z `docs/etapy/01..08`
- [ ] Podpis i klawisz O: `crates/wheel-core/src/trigger.rs` (efekt, testy), `apps/wheel/src/hook.rs` (klawisz O jak Escape), `render.rs`/`anim.rs` (podpis, zanikanie), podgląd w edytorze, README, CHECKLISTA; zrzut do akceptacji, test na LAB wejściem wstrzykiwanym (edytor na Opcjach, litera nie trafia do Notatnika); potem ponownie instalator i szkic 1.0.1
- [x] Wersja 1.0.1: `Cargo.toml`, `tools/build-installer.ps1`, test aktualizacji, README, CHECKLISTA, szkic wydania
- Sprawdzenie: `cargo test`, clippy, fmt; `soak.ps1` dla Escape (po zgodzie, przejmuje pulpit); pomiar pamięci `--stats`; zrzut podglądu z długimi nazwami do akceptacji; instalacja 1.0.1 na 1.0.0 przy działającym kole

## Decyzje
- Autostart w edytorze zmienia rejestr od razu, niezależnie od „Zapisz”: nie jest częścią `config.json`; ten sam wpis `Elemental Wheel` co instalator, ścieżka `wheel.exe` obok edytora.
- Etykieta autostartu tylko po polsku: edytor nie ma mechanizmu tłumaczeń.
- Jedna kopia edytora tylko bez `--config` (testy z własną konfiguracją nie dotykają edytora użytkownika); drugie uruchomienie nie przekazuje `--tab`.
- Logika dopasowania nazw (`fit`) przeniesiona z `apps/wheel` do `crates/wheel-core`, wspólna dla koła i podglądu, żeby się nie rozjeżdżały; podgląd używa Segoe UI zwykłej wagi (koło: Medium), miejsce przycięcia może się różnić o znak.
- Błąd po Escape nie występuje w obecnym kodzie: na LAB (wejście wstrzykiwane, release) 48/48 par „otwórz, Escape, puść, otwórz” z odstępem 30–500 ms (połowa z ruchem poza środek) i `soak.ps1` 5 min (119 cykli, ok. 24 z Escape) bez błędu; w etapie 07 ok. 2 na 48. Przyczyna nieustalona, prawdopodobnie usunięta zmianami okna z etapów 08–09 (`WM_WHEEL_PRESS`, `finish_closing`); kod bez zmian, wpis „Known issues” usunięty z README.
- `soak.ps1` daje werdykt FAIL tylko przez limit pamięci 15 MB sprzed szkła (cel ze szkłem 25 MB, etap 08); skryptu nie zmieniam w tym etapie.
- Pamięć szkła: przy przełączeniu szkła z włączonego na wyłączone w działającym kole wspólne urządzenia D3D i D2D są tworzone od nowa (efekty rozmycia zostawały w nich); zwykłe przeładowanie zatrzymuje urządzenia jak dotąd (nowe urządzenie przy każdym przeładowaniu zostawiało pamięć w sterowniku). Pomiar `--run --stats`, 10+10 otwarć, po ok. 70 s: nazwy 18,6 → 13,2 MB, ikony 18,1 → 12,7 MB (uruchomione bez szkła: 12,5 / 11,7 MB); ponowne włączenie szkła działa. Konfiguracja pomiaru: 8+8 sektorów bez podmenu (inna niż w etapie 08, 12+12 z podmenu), więc liczby nie są porównywalne z tabelą w README.
- Pojedyncze nieotwarcie koła zaraz po starcie ze szkłem (2 przytrzymania w jednym z 9 przebiegów pomiaru pamięci) nie powtórzyło się: 10 uruchomień × 4 przytrzymania, 40/40; przyczyna nieznana, zapisane jako ryzyko.
- Wydanie 1.0.1: instalator 4,14 MB, aktualizacja z 1.0.0 przy działającym kole na LAB (cicha instalacja ok. 4 s, obie exe 1.0.1, wpis Run i `config.json` bez zmian, koło uruchomione ponownie, jedna kopia); szkic wydania v1.0.1 z celem `main` i SHA-256, publikuje użytkownik po scaleniu PR; szkic v1.0.0 zostaje do decyzji użytkownika.
- Łamanie nazw w podglądzie zaakceptowane przez użytkownika; obcięte nazwy można skrócić w edytorze.
- Dostęp do opcji niezależny od liczby sektorów (decyzja użytkownika): kafelek „Opcje koła” znika przy mniejszej liczbie sektorów, więc podpis „O – opcje” w środku koła przy każdym otwarciu i klawisz O; kafelek i zasobnik zostają; bez opcji wyłączenia podpisu.
- Podpis „O – opcje” widoczny 10 s po otwarciu zamiast ok. 2 s (decyzja użytkownika po zrzutach).
- Podpis rysowany pod „Anuluj” w środku (środek przy kursorze na środku pokazuje „Anuluj”), tylko gdy kursor jest na środku; kolor przygaszony motywu, 10 px; w podglądzie edytora statycznie. Klawisz O ignorowany przy Ctrl/Alt/Win (skróty programów działają). Test na LAB wejściem wstrzykiwanym: O przy przytrzymaniu i w trybie klikania otwiera edytor `--tab opcje --config <test>`, litera nie trafia do Notatnika; O przy zamkniętym kole i Ctrl+O przechodzą.
- Wzmianki w etapach 01–08: „claude.ai” → „w sieci”, „scratchpad” → „folder tymczasowy”, bez „przez Claude”; treść decyzji bez zmian.

## Podsumowanie
(wypełnia /etap-zamknij)
