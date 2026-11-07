# Etap 07: stabilnosc-i-pomiary

## Cel
Zmierzyć pamięć i czas otwarcia koła i zapisać wyniki; uodpornić koło na błędy (ostatnia dobra konfiguracja w pamięci,
log awarii w `%LOCALAPPDATA%`), a edytor na zmianę pliku z zewnątrz. Test wielogodzinny na LAB potwierdza brak wycieków
i żywy hak.

## Kryteria zamknięcia
- [ ] Log `%LOCALAPPDATA%\Elemental-Wheel\wheel.log` (i `editor.log`): komunikaty dziś wypisywane na konsolę, panic z miejscem wystąpienia, limit rozmiaru z jednym starszym plikiem; w testach jawna ścieżka
- [ ] Panic w haku myszy i w wątkach akcji nie kończy procesu bez śladu: zdarzenie przepuszczone, wpis w logu
- [ ] Zepsuty `config.json` w trakcie pracy: koło zostaje na ostatniej dobrej konfiguracji w pamięci, błąd w logu, `.bak` nietknięty; testy
- [ ] Awaria edytora (zabity proces, także w trakcie zapisu) nie psuje `config.json`; plik tymczasowy po przerwanym zapisie nie przeszkadza kolejnemu zapisowi; test
- [ ] Edytor wykrywa zmianę pliku z zewnątrz: bez niezapisanych zmian wczytuje go ponownie i informuje w pasku stanu; z niezapisanymi zmianami pyta (wczytaj plik / zostaw moje); zapis nadpisujący nowszy plik pyta przed nadpisaniem; testy logiki bez okna
- [ ] Pomiar pamięci w tle (prywatna, working set, uchwyty, GDI/USER) w widoku nazw i ikon, wyniki w pliku etapu; cel do 15 MB prywatnej
- [ ] Pomiar czasu otwarcia (od progu przytrzymania do pierwszej klatki): pierwsze otwarcie, kolejne, po zmianie konfiguracji; mediana i maksimum w pliku etapu; cel mediana do 16 ms
- [ ] Renderer i ikony przygotowane od razu po wczytaniu konfiguracji, nie przy pierwszym otwarciu; układ tekstu glifów nie tworzony przy każdym odrysowaniu; pomiar przed i po
- [ ] Test wielogodzinny na LAB (co najmniej 4 h, wejście wstrzykiwane, konfiguracja w scratchpadzie): bez awarii, hak żywy, wzrost pamięci prywatnej do 1 MB, liczba uchwytów stała
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings`, `cargo fmt --check` czyste
- [ ] `README.md` (log, wyniki pomiarów) i `CHECKLISTA.md` zaktualizowane

## Plan
- [ ] `apps/wheel/src/log.rs` (nowy): plik logu z limitem rozmiaru, panic hook, zapis zamiast samego `eprintln!` (konsola zostaje); to samo w edytorze
- [ ] `apps/wheel/src/hook.rs`, `actions.rs`: `catch_unwind` w procedurze haka i wątkach akcji, przy błędzie zdarzenie przepuszczone
- [ ] `crates/wheel-core/src/config.rs`, `apps/wheel/src/settings.rs`: przeładowanie zepsutego pliku zostawia bieżącą konfigurację (bez `.bak` i domyślnych); start bez zmian (plik, `.bak`, domyślne); testy
- [ ] `config::save()`: test przerwanego zapisu (pozostały plik tymczasowy)
- [ ] `apps/wheel-editor/src/model.rs`, `main.rs`: czas modyfikacji pliku przy wczytaniu i zapisie, sprawdzanie przy odzyskaniu fokusu i co kilka sekund, pytanie przy konflikcie, pytanie przed nadpisaniem nowszego pliku; testy modelu
- [ ] Ukryta flaga `--stats`: czas otwarcia każdego otwarcia do logu, co minutę pamięć i uchwyty (`GetProcessMemoryInfo`, `GetGuiResources`)
- [ ] `apps/wheel/src/window.rs`, `render.rs`, `icons.rs`: przygotowanie renderera i ikon po wczytaniu konfiguracji, pamięć podręczna układu tekstu glifów
- [ ] `tools/soak.ps1`: `wheel.exe --run --accept-injected --stats --config <scratchpad>`, cykliczne otwieranie i zamykanie (z łukiem podmenu), zmiany pliku konfiguracji (także na zepsuty), zbieranie CSV
- [ ] Wyniki pomiarów do pliku etapu i `README.md`; `CHECKLISTA.md`
- Sprawdzenie: tester: build, testy, clippy, fmt; krótki przebieg `soak.ps1` (10 min), potem pełny (co najmniej 4 h) z CSV; pomiary przed i po zmianach renderera; `%APPDATA%` nietknięty

## Decyzje
- Wykrywanie ręcznej zmiany pliku przy otwartym edytorze w tym etapie (decyzja użytkownika).
- Poza zakresem: WM_DPICHANGED i wiele monitorów.

## Podsumowanie
(wypełnia /etap-zamknij)
