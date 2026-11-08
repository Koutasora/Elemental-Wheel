# Etap 09: instalator-i-wydanie

## Cel
Koło gotowe do codziennego używania: działa w tle bez okna konsoli, ma ikonę w zasobniku, uruchamia się przy logowaniu,
instaluje się instalatorem Inno Setup dla jednego użytkownika; wydanie 1.0.0.

## Kryteria zamknięcia
- [ ] `wheel.exe --run` bez okna konsoli; `--version`, `--demo`, `--quit` nadal wypisują wynik z konsoli
- [ ] Ikona w zasobniku z menu: Edytor, Wyłącz/Włącz, Zamknij; wyłączone koło przepuszcza prawy przycisk bez zmian, stan widoczny w podpowiedzi ikony
- [x] Jeden wariant ikony programu zaakceptowany przez użytkownika na lokalnym zrzucie; ikona w `wheel.exe`, `wheel-editor.exe`, zasobniku i instalatorze
- [x] Wersja 1.0.0 w `--version` obu programów, we właściwościach .exe i w instalatorze (tytuł i ostatnia strona)
- [x] Instalator Inno Setup dla jednego użytkownika (bez administratora, `%LOCALAPPDATA%\Programs\Elemental-Wheel`): skróty w menu Start (koło, edytor), opcja „Uruchamiaj przy logowaniu” (`HKCU\...\Run`, domyślnie zaznaczona), uruchomienie koła na koniec
- [x] Deinstalator zamyka działające koło, usuwa pliki, skróty i autostart; konfiguracja w `%APPDATA%` zostaje
- [ ] Jeden test instalacji i odinstalowania na LAB oraz krótki test fizyczną myszą zainstalowanej wersji (zasobnik, Wyłącz/Włącz, Edytor)
- [ ] Szkic wydania v1.0.0 na GitHubie z instalatorem i opisem zgodnym z README (publikuje użytkownik po scaleniu PR)
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings`, `cargo fmt --check` czyste; README i CHECKLISTA zaktualizowane

## Plan
- [x] `apps/wheel/src/main.rs`: `windows_subsystem = "windows"`, `AttachConsole(ATTACH_PARENT_PROCESS)` dla wypisywania z konsoli
- [x] `apps/wheel/src/tray.rs` (nowy): Shell_NotifyIcon, menu; Wyłącz/Włącz przez flagę w `hook.rs`, Edytor uruchamia `wheel-editor.exe` z folderu koła, Zamknij jak `--quit`
- [x] Ikona: jeden wariant jako lokalny HTML ze zrzutem do akceptacji, potem `assets/icon.svg`, `assets/elemental-wheel.ico`; `build.rs` w `apps/wheel` i `apps/wheel-editor` (`winresource`) z ikoną i danymi wersji
- [x] Wersja 1.0.0 w `Cargo.toml` (workspace)
- [x] `installer/elemental-wheel.iss`, `tools/build-installer.ps1` (build release, ISCC, wypisuje wersję i ścieżkę instalatora)
- [ ] Test na LAB: kopia `%APPDATA%\Elemental-Wheel` przed testem i przywrócenie po; instalacja, pliki, skróty, klucz Run, działające koło; odinstalowanie i sprawdzenie usunięcia; test fizyczną myszą przez użytkownika
- [x] Akcja systemowa „Opcje koła” (`wheel_options`, edytor otwarty na zakładce Opcje, `--tab opcje`), 5. kafelek łuku „Ustawienia” w domyślnej konfiguracji, pozycja „Opcje” w menu zasobnika (`crates/wheel-core/src/config.rs`, `apps/wheel/src/actions.rs`, `tray.rs`, `apps/wheel-editor`)
- [ ] README (instalacja, zasobnik, odinstalowanie, status 1.0.0), CHECKLISTA, szkic wydania (`gh release create --draft`)
- Sprawdzenie: tester: build, testy, clippy, fmt, `--version` obu programów, kompilacja instalatora; test instalacji i odinstalowania na LAB; test fizyczną myszą; manager: przegląd diffu

## Decyzje
- Zakres skrócony (decyzja użytkownika z etapu 08, ok. 1–1,5 h): poza zakresem przełącznik autostartu w edytorze, edytor z drugiej kopii programu, znany błąd po Escape, łamanie długich nazw w podglądzie (lista na później).
- Konfiguracja w `%APPDATA%` zostaje po odinstalowaniu (plan zatwierdzony przez użytkownika).
- Wydanie jako szkic na GitHubie; publikuje użytkownik po scaleniu PR.
- Ikona programu (wariant 1, zaakceptowany przez użytkownika: „ikona idealna”): koło w kolorze akcentu #2851d0, 12 sektorów, górny sektor biały, biały środek; generowana skryptem `tools/make-icon.py` (SVG i ICO z tej samej geometrii, bez zależności).
- Zasobnik: ikona ładowana `LoadImageW` (zasób nr 1 exe, rozmiar z DPI), bo `LoadIconMetric` wymaga comctl32 v6 i manifestu (exe nie startował); hak przepuszcza zdarzenia nad paskiem zadań i obszarem powiadomień (klasy Shell_TrayWnd, Shell_SecondaryTrayWnd, NotifyIconOverflowWindow, TopLevelWindowForOverflowXamlIsland) i nad oknami własnego procesu (menu zasobnika), żeby prawy klik na ikonie nie otwierał koła.
- Test instalatora (tester): `--quit` kończy się po zniknięciu okna, a proces trzyma `wheel.exe` jeszcze ok. 0,5 s; aktualizacja przy działającym kole przerywana przez Restart Manager, deinstalator zostawiał `wheel.exe`. Poprawka: instalator i deinstalator po `--quit` czekają na zwolnienie mutexu `Local\ElementalWheel` (do 5 s), inaczej czytelny komunikat.
- Aktualizacja zachowuje wybór zadania „Uruchamiaj przy logowaniu” z poprzedniej instalacji (standardowe zachowanie Inno Setup, zostaje); pozostałość `is-*-uninstall.tmp` w `%TEMP%` to zwykłe zachowanie Inno Setup.
- „Opcje koła” w kole i w zasobniku (prośba użytkownika: z koła nie dało się dojść do opcji, łuk „Ustawienia” to ustawienia Windows); w domyślnej konfiguracji jako 5. kafelek łuku „Ustawienia”, istniejące konfiguracje bez zmian.

## Pomiary i testy
- Instalator 4,13 MB (Inno Setup 6.7.3), wersja 1.0.0.
- Test na LAB (tester, instalacja cicha): instalacja, aktualizacja przy działającym kole bez autostartu i z autostartem, odinstalowanie przy działającym kole: PASS po poprawce czekania na mutex (przed poprawką aktualizacja przerywana, `wheel.exe` zostawał). Stan LAB przywrócony z kopii.

## Podsumowanie
(wypełnia /etap-zamknij)
