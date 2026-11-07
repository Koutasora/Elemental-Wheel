# Etap 07: stabilnosc-i-pomiary

## Cel
Zmierzyć pamięć i czas otwarcia koła i zapisać wyniki; uodpornić koło na błędy (ostatnia dobra konfiguracja w pamięci,
log awarii w `%LOCALAPPDATA%`), a edytor na zmianę pliku z zewnątrz. `tools/soak.ps1` zostaje jako narzędzie testu i pomiarów
(wejście wstrzykiwane).

## Kryteria zamknięcia
- [x] Log `%LOCALAPPDATA%\Elemental-Wheel\wheel.log` (i `editor.log`): komunikaty dziś wypisywane na konsolę, panic z miejscem wystąpienia, limit rozmiaru z jednym starszym plikiem; w testach jawna ścieżka
- [x] Panic w haku myszy i w wątkach akcji nie kończy procesu bez śladu: zdarzenie przepuszczone, wpis w logu
- [x] Zepsuty `config.json` w trakcie pracy: koło zostaje na ostatniej dobrej konfiguracji w pamięci, błąd w logu, `.bak` nietknięty; testy
- [x] Awaria edytora (zabity proces, także w trakcie zapisu) nie psuje `config.json`; plik tymczasowy po przerwanym zapisie nie przeszkadza kolejnemu zapisowi; test
- [x] Edytor wykrywa zmianę pliku z zewnątrz: bez niezapisanych zmian wczytuje go ponownie i informuje w pasku stanu; z niezapisanymi zmianami pyta (wczytaj plik / zostaw moje); zapis nadpisujący nowszy plik pyta przed nadpisaniem; testy logiki bez okna
- [x] Pomiar pamięci w tle (prywatna, working set, uchwyty, GDI/USER) w widoku nazw i ikon, wyniki w pliku etapu; cel do 15 MB prywatnej (zmierzone: nazwy 13,9 MB, ikony 17 MB; przekroczenie w widoku ikon przyjęte, patrz Decyzje)
- [x] Pomiar czasu otwarcia (od progu przytrzymania do pierwszej klatki): pierwsze otwarcie, kolejne, po zmianie konfiguracji; mediana i maksimum w pliku etapu; cel mediana do 16 ms
- ~~Renderer i ikony przygotowane od razu po wczytaniu konfiguracji, nie przy pierwszym otwarciu; układ tekstu glifów nie tworzony przy każdym odrysowaniu; pomiar przed i po~~ (przeniesione do etapu 09)
- ~~Przebieg `soak.ps1` 10 min na LAB (wejście wstrzykiwane, konfiguracja w scratchpadzie, zmiany konfiguracji): bez awarii i błędów cykli, hak żywy~~ (rezygnacja, patrz Decyzje)
- [x] `cargo build --release`, `cargo test`, clippy `-D warnings`, `cargo fmt --check` czyste
- [x] `README.md` (log, wyniki pomiarów) i `CHECKLISTA.md` zaktualizowane

## Plan
- [x] `crates/wheel-core/src/log.rs` (nowy, zamiast `apps/wheel/src/log.rs`): plik logu z limitem rozmiaru, panic hook, zapis zamiast samego `eprintln!` (konsola zostaje); to samo w edytorze
- [x] `apps/wheel/src/hook.rs`, `actions.rs`, `window.rs`: `catch_unwind` w procedurze haka i wątkach akcji, przy błędzie zdarzenie przepuszczone
- [x] `crates/wheel-core/src/config.rs`, `apps/wheel/src/settings.rs`: przeładowanie zepsutego pliku zostawia bieżącą konfigurację (bez `.bak` i domyślnych); start bez zmian (plik, `.bak`, domyślne); testy
- [x] `config::save()`: test przerwanego zapisu (pozostały plik tymczasowy)
- [x] `apps/wheel-editor/src/model.rs`, `main.rs`: czas modyfikacji pliku przy wczytaniu i zapisie, sprawdzanie przy odzyskaniu fokusu i co kilka sekund, pytanie przy konflikcie, pytanie przed nadpisaniem nowszego pliku; testy modelu
- [x] Ukryta flaga `--stats`: czas otwarcia każdego otwarcia do logu, co minutę pamięć i uchwyty (`GetProcessMemoryInfo`, `GetGuiResources`)
- ~~`apps/wheel/src/window.rs`, `render.rs`, `icons.rs`: przygotowanie renderera i ikon po wczytaniu konfiguracji, pamięć podręczna układu tekstu glifów~~ (przeniesione do etapu 09)
- [x] `tools/soak.ps1`: `wheel.exe --run --accept-injected --stats --config <scratchpad>`, cykliczne otwieranie i zamykanie (z łukiem podmenu), zmiany pliku konfiguracji (także na zepsuty), zbieranie CSV
- [x] Wyniki pomiarów do pliku etapu i `README.md`; `CHECKLISTA.md`
- ~~Poprawka: przytrzymanie tuż po anulowaniu Escape nie otwiera koła~~ (znany błąd, patrz Decyzje)
- Sprawdzenie: tester: build, testy, clippy, fmt; `%APPDATA%` nietknięty

## Decyzje
- Wykrywanie ręcznej zmiany pliku przy otwartym edytorze w tym etapie (decyzja użytkownika).
- Poza zakresem: WM_DPICHANGED i wiele monitorów.
- Log we wspólnym `wheel_core::log` (tylko std, bez zależności), bo używają go koło i edytor; czas w UTC (bez biblioteki dat), limit 1 MB, jeden starszy plik `*.old.log`.
- Do logu trafia to, co dziś idzie przez `eprintln!` (błędy, ostrzeżenia, raport konfiguracji, strażnik); ślady wyboru na stdout zostają tylko na stdout (szum przy każdym otwarciu).
- Przełącznik `--log <ścieżka>` w kole i edytorze, żeby testy i `soak.ps1` nie pisały do `%LOCALAPPDATA%`.
- Przechwytywanie paniki także w procedurach okna koła (nie tylko hak i akcje): panika w `extern "system"` kończy proces; po panice koło się chowa, stan wyzwalacza wraca do spoczynku, zdarzenie idzie dalej.
- Ukryta flaga `--test-panic hook|window|action` (jak `--test-stall-hook`), żeby sprawdzić odzyskiwanie na żywo wejściem wstrzykiwanym.
- Edytor przy otwarciu i ponownym wczytaniu zostaje przy dotychczasowym zachowaniu (plik, `.bak`, domyślne, z komunikatem): to jawne otwarcie przez użytkownika, nie podmiana działającej konfiguracji.
- Plik tymczasowy zapisu ma stałą nazwę `config.json.tmp`; pozostałość po przerwanym zapisie jest nadpisywana przy następnym zapisie i ignorowana przy wczytaniu (test, bez zmian w kodzie).
- Awaria edytora sprawdzona przez zabicie procesu po zmianie pliku z zewnątrz (plik nietknięty, brak `.tmp`); zabicie w samym trakcie zapisu pokrywa zapis atomowy i test pozostałego `.tmp` (trafienie w tę chwilę na żywo niepraktyczne).
- Zmiana pliku rozpoznawana po czasie modyfikacji i długości; zmiana o tej samej długości w obrębie rozdzielczości czasu pliku tuż po własnym zapisie edytora nie zostanie zauważona (akceptowalne).
- `--stats` tylko z `--run`. Czas otwarcia liczony od efektu otwarcia w wątku haka (próg spełniony) do powrotu `Commit` i `SetWindowPos` w oknie; w logu także rozbicie: kolejka (do obsługi `WM_WHEEL_OPEN`) i praca okna oraz rodzaj otwarcia (pierwsze, kolejne, po zmianie konfiguracji, inny DPI). Faktyczne pojawienie się klatki na ekranie (DWM) nie jest mierzone: to zależy od kompozytora, nie od koła.
- Pamięć i uchwyty w logu przy starcie i co 60 s, wiersze `stats ...` w formacie `klucz=wartość`, żeby `soak.ps1` mógł je zamienić na CSV.
- Kolejność kroków: `soak.ps1` przed zmianą renderera, bo służy też do pomiaru „przed”.
- `soak.ps1` mierzy osobno widok `names` i `icons` (w konfiguracji nie ma widoku z oboma naraz); pomiar czasu otwarcia bez zmian konfiguracji (`-NoConfigChanges`), czas po zmianie konfiguracji w osobnym przebiegu.
- Błąd znaleziony w `soak.ps1`: przytrzymanie tuż po anulowaniu Escape czasem nie otwiera koła (5 z 6 błędów cykli); diagnoza i poprawka przed zmianą renderera, bo przebieg `soak.ps1` wymaga zera błędów cykli.
- Bez testu wielogodzinnego i bez przebiegu `soak.ps1` przy zamknięciu (decyzja użytkownika): komputer jest regularnie restartowany, a koło można w każdej chwili uruchomić ponownie; odłączony hak naprawia strażnik (etap 03); w pomiarach 6 min pamięć prywatna w granicach 0,4 MB, liczba uchwytów stała. `soak.ps1` zostaje jako narzędzie.
- Skrócony zakres (decyzja użytkownika): przygotowanie renderera po wczytaniu konfiguracji (otwarcie po zmianie konfiguracji 89 ms zamiast ok. 3 ms) i pamięć widoku ikon (17 MB przy celu 15 MB) przeniesione do etapu 09; poprawka błędu po Escape zostaje w etapie 07.
- Błąd po Escape zostaje jako znany (decyzja użytkownika: koło używane rzadko): opisany w README i CHECKLISTA; reprodukcja: ok. 2 błędy na 48 par „otwórz, Escape, otwórz”, ślad diagnostyczny cofnięty bez ustalenia przyczyny.

## Pomiary
LAB: AMD Radeon Vega 11, 1920x1080, 96 DPI, wersja release, wejście wstrzykiwane, 6 min na przebieg, konfiguracja 8 sektorów zewnętrznych (jeden z podmenu 3 pozycji) i 8 wewnętrznych.

Przed zmianą renderera (2026-11-07):

| | ikony | nazwy | ikony, zmiana konfiguracji co 1 min |
|---|---|---|---|
| prywatna na starcie | 15584 KB | 12496 KB | 15800 KB |
| prywatna po rozgrzewce: pierwsza / ostatnia / max | 17004 / 17044 / 17044 KB | 13544 / 13908 / 13908 KB | 17436 / 17084 / 17644 KB |
| working set max | 35884 KB | 31632 KB | 39380 KB |
| uchwyty / GDI / USER (ostatnie) | 305 / 8 / 15 | 275 / 0 / 13 | 294 / 8 / 14 |
| pierwsze otwarcie | 3,96 ms | 3,68 ms | 6,07 ms |
| kolejne: mediana / max | 3,18 / 4,86 ms (139) | 3,26 / 5,16 ms (137) | 3,68 / 6,07 ms (129) |
| po zmianie konfiguracji: mediana / max | – | – | 89,2 / 96,6 ms (5) |

## Podsumowanie

### Decyzje
- Wspólny log `wheel_core::log` (tylko std, UTC, 1 MB + `*.old.log`), `--log` dla testów; do logu idzie to, co szło przez `eprintln!`.
- Panika przechwytywana w haku, procedurach okna i wątkach akcji; po panice koło się chowa, wyzwalacz wraca do spoczynku, zdarzenie idzie dalej.
- Zepsuty plik w trakcie pracy zostawia bieżącą konfigurację; start bez zmian (plik, `.bak`, domyślne).
- Edytor rozpoznaje zmianę pliku po czasie modyfikacji i długości; pyta przy konflikcie i przed nadpisaniem nowszego pliku.
- Ukryta flaga `--stats` (tylko `--run`): czas otwarcia od progu w haku do `Commit`/`SetWindowPos`, pamięć i uchwyty co 60 s.
- Skrócony zakres (decyzja użytkownika): bez testu wielogodzinnego i przebiegu `soak.ps1`; renderer po wczytaniu konfiguracji i pamięć widoku ikon do etapu 09; błąd po Escape jako znany.

### Zrobione
- Log koła i edytora w `%LOCALAPPDATA%\Elemental-Wheel`, panic hook z miejscem wystąpienia.
- `catch_unwind` w haku, oknie i akcjach; ukryta flaga `--test-panic hook|window|action`; sprawdzone na żywo.
- Odporność na zepsuty `config.json` w trakcie pracy i na zabity edytor; test pozostałego `config.json.tmp`.
- Wykrywanie zmiany pliku z zewnątrz w edytorze (fokus i co kilka sekund), testy modelu.
- `--stats` i `tools/soak.ps1` (wejście wstrzykiwane, łuk podmenu, Escape, zmiany i zepsucie konfiguracji, CSV, podsumowanie z werdyktem, blokada ścieżek z danymi użytkownika także w postaci 8.3).
- Pomiary (LAB, 96 DPI): otwarcie mediana ok. 3,2 ms (max 6 ms), po zmianie konfiguracji 89 ms; pamięć prywatna: nazwy 13,9 MB, ikony 17 MB; w 6 min pamięć w granicach 0,4 MB, uchwyty stałe.
- README (log, pomiary, znany błąd), CHECKLISTA; build, 192 testy, clippy, fmt czyste.

### Otwarte sprawy
- Etap 09: przygotowanie renderera i ikon po wczytaniu konfiguracji (89 ms → ok. 3 ms), pamięć podręczna układu tekstu glifów, pamięć widoku ikon do 15 MB.
- Znany błąd: przytrzymanie tuż po zamknięciu koła Escape czasem nie otwiera koła (ok. 2 na 48 prób wejściem wstrzykiwanym); przyczyna nieustalona.
- `--stats` nazywa `dpi` otwarcie, przy którym zmienia się rozmiar z konfiguracji wykrytej dopiero przy poprzednim otwarciu.

### Ryzyka
- Błąd po Escape widziany tylko przy wejściu wstrzykiwanym; nie wiadomo, czy dotyczy fizycznej myszy.
- Brak testu wielogodzinnego: wolny wyciek nie jest wykluczony (w krótkich pomiarach niewidoczny).
- `soak.ps1` z domyślnym widokiem ikon kończy się FAIL, dopóki pamięć przekracza 15 MB.
- Pomiar czasu kończy się przed faktycznym pokazaniem klatki przez DWM.
