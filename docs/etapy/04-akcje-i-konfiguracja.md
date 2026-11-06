# Etap 04: akcje-i-konfiguracja

## Cel
Wybór sektora wykonuje przypisaną akcję zamiast ją wypisywać. Elementy koła, opcje, progi wyzwalacza i wykluczenia
są w `%APPDATA%\Elemental-Wheel\config.json` z wersją schematu, zapisem atomowym i kopią `.bak`.

## Kryteria zamknięcia
- [ ] Typy akcji działają: program, folder/plik, adres, skrót klawiszowy, komenda edycji, wklejenie tekstu, polecenie, akcja systemowa
- [ ] Pierścień wewnętrzny wysyła komendy edycji (wytnij, kopiuj, wklej, wklej bez formatu, usuń, cofnij, ponów, zaznacz wszystko, zapisz, znajdź, zmień nazwę, właściwości) do okna, w którym pracuje użytkownik; fokus bez zmian
- [ ] Każdy element (także komendy edycji) ma nazwę w konfiguracji, wyświetlaną zamiast domyślnej
- [ ] Opcje w konfiguracji: liczba sektorów, motyw, rozmiar koła, widok (nazwy/ikony; ikony w 05), próg czasu i ruchu
- [ ] `config.json`: wersja schematu, walidacja, zapis atomowy (plik tymczasowy + zamiana) i `.bak`; przy pierwszym uruchomieniu tworzony z wartościami domyślnymi
- [ ] Błędna konfiguracja nie zatrzymuje programu: komunikat, potem `.bak`, potem wartości domyślne; plik użytkownika nie jest nadpisywany
- [ ] Zmiana pliku działa bez restartu (sprawdzenie daty modyfikacji przy otwarciu koła)
- [ ] Lista wykluczonych programów (nazwa .exe, bez rozróżniania wielkości liter): prawy przycisk przechodzi bez zmian
- [ ] Wyłączenie w pełnym ekranie (opcja, domyślnie włączona): okno pełnoekranowe pod kursorem dostaje prawy przycisk bez zmian
- [ ] `--config <ścieżka>` (testy na osobnym pliku); argumenty nadpisują wartości z pliku
- [ ] Testy w `wheel-core`: wczytanie, walidacja, wartości domyślne, zapis i ponowne wczytanie, `.bak`, rozbiór skrótów klawiszowych, dopasowanie wykluczeń
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings` i `cargo fmt --check` czyste
- [ ] Sprawdzenie ręczne na LAB przez użytkownika: komendy edycji w Notatniku i Eksploratorze, uruchomienie programu, wykluczenie, pełny ekran
- [ ] `CHECKLISTA.md` i `README.md` zaktualizowane

## Plan
- [x] `crates/wheel-core/src/config.rs`: model (`Config`, `Options`, `Item { name, action, icon, submenu }`, `Action`) w `serde`/`serde_json`, wersja schematu, walidacja, wartości domyślne, wczytanie i zapis atomowy z `.bak` (tylko std); testy na plikach w `target\test-tmp\` z pełnymi ścieżkami
- [x] `crates/wheel-core/src/keys.rs`: rozbiór skrótu (`"Ctrl+Shift+V"`) na kody klawiszy, testy
- [x] `crates/wheel-core/src/demo.rs`: zastąpiony konfiguracją domyślną; etykiety z `Config`
- [ ] `apps/wheel/src/actions.rs`: wykonanie akcji (ShellExecuteW: program, plik/folder, adres; CreateProcess `cmd /c` z `CREATE_NO_WINDOW`: polecenie; SendInput: skrót i komenda edycji; schowek + Ctrl+V: wklejenie tekstu; akcje systemowe: zablokuj, zamknij okno, minimalizuj, pokaż pulpit, wycisz, głośniej, ciszej, zrzut ekranu)
- [ ] `apps/wheel/src/hook.rs`: przepuszczanie wykluczonych .exe i okien pełnoekranowych (rozszerzenie decyzji o oknie administratora przy wciśnięciu, pamięć na proces)
- [ ] `apps/wheel/src/window.rs`, `args.rs`, `main.rs`: wczytanie konfiguracji, odświeżenie przy otwarciu, `--config`, akcja po wyborze; wypisywany wybór numerowany od 1 (otwarta sprawa z 03)
- [ ] Domyślny pierścień zewnętrzny tylko z elementami Windows: Notatnik, Kalkulator, Ustawienia, Menedżer zadań zamiast Blendera, Abletona, Godota i VS Code
- [ ] `CHECKLISTA.md`, `README.md` (konfiguracja, typy akcji, `--config`)
- Sprawdzenie: tester: build, testy, clippy, fmt; skrypt PowerShell z `--accept-injected` i testowym `config.json` w scratchpadzie: kopiuj/wklej i wklejenie tekstu w testowym Notatniku, uruchomienie programu, wykluczenie (`notepad.exe` daje zwykłe menu mimo przytrzymania), błędny plik (program działa na `.bak`); pełny ekran i Eksplorator: użytkownik fizyczną myszą

## Decyzje
- Domyślny pierścień zewnętrzny tylko z elementami Windows (Notatnik, Kalkulator, Ustawienia, Menedżer zadań zamiast Blendera, Abletona, Godota, VS Code): domyślna konfiguracja ma działać na każdym komputerze; użytkownik dopasuje ją sam (decyzja użytkownika).
- Pole `submenu` (do 5 elementów, walidowane) w modelu już teraz, nieużywane do etapu 05: etap 05 nie zmieni schematu.
- Testy konfiguracji tylko na plikach w `target\test-tmp\` i przez `--config`: nigdy nie dotykają `%APPDATA%` użytkownika.

- Wczytanie przycina liczby spoza zakresu (z ostrzeżeniem) zamiast odrzucać plik, także te poza zakresem typu pola (np. 300 sektorów); najwyższy poziom pliku musi być obiektem JSON (serde czytałby też tablicę); komunikaty i ostrzeżenia po angielsku, jak reszta programu.
- Krótsze pierścienie są dopełniane pustymi elementami (`none`) przy wczytaniu, więc zawsze mają 12 pozycji; zależności `serde` i `serde_json` przypięte `=`.

## Podsumowanie
(wypełnia /etap-zamknij)
