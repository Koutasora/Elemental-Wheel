# Etap 09: instalator-i-wydanie

## Cel
Koło gotowe do codziennego używania: działa w tle bez okna konsoli, ma ikonę w zasobniku, uruchamia się przy logowaniu,
instaluje się instalatorem Inno Setup dla jednego użytkownika; wydanie 1.0.0.

## Kryteria zamknięcia
- [ ] `wheel.exe --run` bez okna konsoli; `--version`, `--demo`, `--quit` nadal wypisują wynik z konsoli
- [ ] Ikona w zasobniku z menu: Edytor, Wyłącz/Włącz, Zamknij; wyłączone koło przepuszcza prawy przycisk bez zmian, stan widoczny w podpowiedzi ikony
- [ ] Jeden wariant ikony programu zaakceptowany przez użytkownika na lokalnym zrzucie; ikona w `wheel.exe`, `wheel-editor.exe`, zasobniku i instalatorze
- [ ] Wersja 1.0.0 w `--version` obu programów, we właściwościach .exe i w instalatorze (tytuł i ostatnia strona)
- [ ] Instalator Inno Setup dla jednego użytkownika (bez administratora, `%LOCALAPPDATA%\Programs\Elemental-Wheel`): skróty w menu Start (koło, edytor), opcja „Uruchamiaj przy logowaniu” (`HKCU\...\Run`, domyślnie zaznaczona), uruchomienie koła na koniec
- [ ] Deinstalator zamyka działające koło, usuwa pliki, skróty i autostart; konfiguracja w `%APPDATA%` zostaje
- [ ] Jeden test instalacji i odinstalowania na LAB oraz krótki test fizyczną myszą zainstalowanej wersji (zasobnik, Wyłącz/Włącz, Edytor)
- [ ] Szkic wydania v1.0.0 na GitHubie z instalatorem i opisem zgodnym z README (publikuje użytkownik po scaleniu PR)
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings`, `cargo fmt --check` czyste; README i CHECKLISTA zaktualizowane

## Plan
- [ ] `apps/wheel/src/main.rs`: `windows_subsystem = "windows"`, `AttachConsole(ATTACH_PARENT_PROCESS)` dla wypisywania z konsoli
- [ ] `apps/wheel/src/tray.rs` (nowy): Shell_NotifyIcon, menu; Wyłącz/Włącz przez flagę w `hook.rs`, Edytor uruchamia `wheel-editor.exe` z folderu koła, Zamknij jak `--quit`
- [ ] Ikona: jeden wariant jako lokalny HTML ze zrzutem do akceptacji, potem `assets/icon.svg`, `assets/elemental-wheel.ico`; `build.rs` w `apps/wheel` i `apps/wheel-editor` (`winresource`) z ikoną i danymi wersji
- [ ] Wersja 1.0.0 w `Cargo.toml` (workspace)
- [ ] `installer/elemental-wheel.iss`, `tools/build-installer.ps1` (build release, ISCC, wypisuje wersję i ścieżkę instalatora)
- [ ] Test na LAB: kopia `%APPDATA%\Elemental-Wheel` przed testem i przywrócenie po; instalacja, pliki, skróty, klucz Run, działające koło; odinstalowanie i sprawdzenie usunięcia; test fizyczną myszą przez użytkownika
- [ ] README (instalacja, zasobnik, odinstalowanie, status 1.0.0), CHECKLISTA, szkic wydania (`gh release create --draft`)
- Sprawdzenie: tester: build, testy, clippy, fmt, `--version` obu programów, kompilacja instalatora; test instalacji i odinstalowania na LAB; test fizyczną myszą; manager: przegląd diffu

## Decyzje
- Zakres skrócony (decyzja użytkownika z etapu 08, ok. 1–1,5 h): poza zakresem przełącznik autostartu w edytorze, edytor z drugiej kopii programu, znany błąd po Escape, łamanie długich nazw w podglądzie (lista na później).
- Konfiguracja w `%APPDATA%` zostaje po odinstalowaniu (plan zatwierdzony przez użytkownika).
- Wydanie jako szkic na GitHubie; publikuje użytkownik po scaleniu PR.
- Ikona programu (wariant 1, zaakceptowany przez użytkownika: „ikona idealna”): koło w kolorze akcentu #2851d0, 12 sektorów, górny sektor biały, biały środek; generowana skryptem `tools/make-icon.py` (SVG i ICO z tej samej geometrii, bez zależności).

## Podsumowanie
(wypełnia /etap-zamknij)
