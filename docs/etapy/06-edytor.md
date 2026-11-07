# Etap 06: edytor

## Cel
`wheel-editor.exe` to osobne okno egui z podglądem koła. Edytuje kafelki obu pierścieni i podmenu oraz opcje programu,
a potem zapisuje `config.json` atomowo; działające koło samo przeładowuje konfigurację po zapisie.
Poprawiony `save()`: do `.bak` trafia tylko poprawny plik.

## Kryteria zamknięcia
- [ ] Edytor wczytuje konfigurację z `%APPDATA%\Elemental-Wheel\config.json` albo z `--config <ścieżka>` i pokazuje błędy i ostrzeżenia wczytania
- [ ] Podgląd koła: oba pierścienie, liczba sektorów z opcji, nazwy, motyw jasny albo ciemny, łuk podmenu zaznaczonego kafelka; kliknięcie kafelka w podglądzie go zaznacza
- [ ] Edycja kafelka: nazwa, typ akcji z polami właściwymi dla typu (program: ścieżka, argumenty, folder roboczy; plik/folder; adres; skrót; komenda edycji z listy; wklejenie tekstu; polecenie; akcja systemowa z listy), ikona (ścieżka i wybór pliku), opróżnienie slotu
- [ ] Podmenu tylko w pierścieniu zewnętrznym: dodawanie, usuwanie, zmiana kolejności, najwyżej 5 kafelków
- [ ] Nazwę da się zmienić w każdym elemencie, także w komendach edycji w pierścieniu wewnętrznym
- [ ] Opcje: motyw, rozmiar 60–140%, liczba sektorów 4–12, widok, `hold_ms`, `move_px`, wyłączenie w pełnym ekranie, lista wykluczonych programów
- [ ] Przeciągnięcie pliku lub programu na slot podglądu tworzy element „program” albo „otwórz” z nazwą z pliku
- [ ] Zapis: walidacja przed zapisem (przy błędzie plik zostaje nietknięty), znacznik niezapisanych zmian, pytanie przy zamykaniu z niezapisanymi zmianami; `save()` kopiuje do `.bak` tylko poprawny istniejący plik
- [ ] Testy: `save()` i `.bak` w `wheel-core`; logika modelu edytora bez okna (zmiana typu akcji, podmenu, zmiana liczby sektorów)
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings` i `cargo fmt --check` czyste
- [ ] Test na LAB z konfiguracją w scratchpadzie (`%APPDATA%` nietknięty): zrzuty edytora, zapis, przeładowanie w działającym kole
- [ ] Układ okna zatwierdzony przez użytkownika na zrzucie przed dopracowaniem
- [ ] `README.md` i `CHECKLISTA.md` zaktualizowane

## Plan
- [ ] `crates/wheel-core/src/config.rs`: `save()` kopiuje do `.bak` tylko plik, który się wczytuje bez błędu; testy
- [ ] `apps/wheel-editor/Cargo.toml`: `eframe`/`egui`, `rfd` (okno wyboru pliku)
- [ ] `apps/wheel-editor/src/model.rs`: stan edytora (konfiguracja, zaznaczenie, znacznik zmian, operacje na kafelkach i podmenu) jako czysta logika; testy
- [ ] `apps/wheel-editor/src/preview.rs`: rysowanie koła w egui z geometrii `wheel-core` (`layout`, `geometry`), trafianie kliknięciem i przeciąganiem pliku
- [ ] `apps/wheel-editor/src/panel.rs`, `options.rs`: panel kafelka i panel opcji
- [ ] `apps/wheel-editor/src/main.rs`: argumenty (`--version`, `--config`), okno, wczytanie, zapis, pytanie przy zamykaniu
- [ ] Zrzut pierwszej wersji układu do akceptacji użytkownika
- [ ] `README.md`, `CHECKLISTA.md`
- Sprawdzenie: tester: build, testy, clippy, fmt; skrypt PowerShell: edytor z `--config` w scratchpadzie, zrzuty, sprawdzenie zapisu, `.bak` i przeładowania w `wheel.exe` (`--accept-injected`); na końcu krótki test ręczny użytkownika

## Decyzje
- Opcje programu i przeciąganie plików na sloty w tym etapie (są w makiecie; decyzja użytkownika).
- Poza zakresem: uruchamianie edytora z zasobnika i z drugiej kopii programu (etap 08), pomiary pamięci (etap 07).

## Podsumowanie
(wypełnia /etap-zamknij)
