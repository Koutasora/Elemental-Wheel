# Etap 04: akcje-i-konfiguracja

## Cel
Wybór sektora wykonuje przypisaną akcję zamiast ją wypisywać. Elementy koła, opcje, progi wyzwalacza i wykluczenia
są w `%APPDATA%\Elemental-Wheel\config.json` z wersją schematu, zapisem atomowym i kopią `.bak`.

## Kryteria zamknięcia
- [x] Typy akcji działają: program, folder/plik, adres, skrót klawiszowy, komenda edycji, wklejenie tekstu, polecenie, akcja systemowa
- [x] Pierścień wewnętrzny wysyła komendy edycji (wytnij, kopiuj, wklej, wklej bez formatu, usuń, cofnij, ponów, zaznacz wszystko, zapisz, znajdź, zmień nazwę, właściwości) do okna, w którym pracuje użytkownik; fokus bez zmian
- [x] Każdy element (także komendy edycji) ma nazwę w konfiguracji, wyświetlaną zamiast domyślnej
- [x] Opcje w konfiguracji: liczba sektorów, motyw, rozmiar koła, widok (nazwy/ikony; ikony w 05), próg czasu i ruchu
- [x] `config.json`: wersja schematu, walidacja, zapis atomowy (plik tymczasowy + zamiana) i `.bak`; przy pierwszym uruchomieniu tworzony z wartościami domyślnymi
- [x] Błędna konfiguracja nie zatrzymuje programu: komunikat, potem `.bak`, potem wartości domyślne; plik użytkownika nie jest nadpisywany
- [x] Zmiana pliku działa bez restartu (obserwacja folderu konfiguracji, zmiana od następnego wciśnięcia)
- [x] Lista wykluczonych programów (nazwa .exe, bez rozróżniania wielkości liter): prawy przycisk przechodzi bez zmian
- [x] Wyłączenie w pełnym ekranie (opcja, domyślnie włączona): okno pełnoekranowe pod kursorem dostaje prawy przycisk bez zmian
- [x] `--config <ścieżka>` (testy na osobnym pliku); argumenty nadpisują wartości z pliku
- [x] Testy w `wheel-core`: wczytanie, walidacja, wartości domyślne, zapis i ponowne wczytanie, `.bak`, rozbiór skrótów klawiszowych, dopasowanie wykluczeń
- [x] `cargo build --release`, `cargo test`, clippy `-D warnings` i `cargo fmt --check` czyste
- [x] Pełny test automatyczny na LAB (wejście wstrzykiwane, `--accept-injected`): komendy edycji w Notatniku i Eksploratorze, uruchomienie programu, wykluczenie, pełny ekran w prawdziwej przeglądarce, polskie nazwy na kole, stan wyciszenia
- [x] `CHECKLISTA.md` i `README.md` zaktualizowane

## Plan
- [x] `crates/wheel-core/src/config.rs`: model (`Config`, `Options`, `Item { name, action, icon, submenu }`, `Action`) w `serde`/`serde_json`, wersja schematu, walidacja, wartości domyślne, wczytanie i zapis atomowy z `.bak` (tylko std); testy na plikach w `target\test-tmp\` z pełnymi ścieżkami
- [x] `crates/wheel-core/src/keys.rs`: rozbiór skrótu (`"Ctrl+Shift+V"`) na kody klawiszy, testy
- [x] `crates/wheel-core/src/demo.rs`: zastąpiony konfiguracją domyślną; etykiety z `Config`
- [x] `apps/wheel/src/actions.rs`: wykonanie akcji (ShellExecuteW: program, plik/folder, adres; CreateProcess `cmd /c` z `CREATE_NO_WINDOW`: polecenie; SendInput: skrót i komenda edycji; schowek + Ctrl+V: wklejenie tekstu; akcje systemowe: zablokuj, zamknij okno, minimalizuj, pokaż pulpit, wycisz, głośniej, ciszej, zrzut ekranu)
- [x] `apps/wheel/src/hook.rs`: przepuszczanie wykluczonych .exe i okien pełnoekranowych (rozszerzenie decyzji o oknie administratora przy wciśnięciu, pamięć na proces)
- [x] `apps/wheel/src/window.rs`, `args.rs`, `main.rs`: wczytanie konfiguracji, odświeżenie przy otwarciu, `--config`, akcja po wyborze; wypisywany wybór numerowany od 1 (otwarta sprawa z 03)
- [x] Domyślny pierścień zewnętrzny tylko z elementami Windows: Notatnik, Kalkulator, Ustawienia, Menedżer zadań zamiast Blendera, Abletona, Godota i VS Code
- [x] `CHECKLISTA.md`, `README.md` (konfiguracja, typy akcji, `--config`)
- Sprawdzenie: tester: build, testy, clippy, fmt; skrypt PowerShell z `--accept-injected` i testowym `config.json` w scratchpadzie (wszystkie typy akcji poza `lock`, `show_desktop`, `volume_up`/`volume_down`; komendy edycji w Notatniku i Eksploratorze; wykluczenia; błędny plik i `.bak`; UTF-16; polskie nazwy na zrzucie; Edge w F11; stan wyciszenia przez Core Audio); fizyczna mysz w etapie 05 (decyzja użytkownika)

## Decyzje
- Domyślny pierścień zewnętrzny tylko z elementami Windows (Notatnik, Kalkulator, Ustawienia, Menedżer zadań zamiast Blendera, Abletona, Godota, VS Code): domyślna konfiguracja ma działać na każdym komputerze; użytkownik dopasuje ją sam (decyzja użytkownika).
- Pole `submenu` (do 5 elementów, walidowane) w modelu już teraz, nieużywane do etapu 05: etap 05 nie zmieni schematu.
- Testy konfiguracji tylko na plikach w `target\test-tmp\` i przez `--config`: nigdy nie dotykają `%APPDATA%` użytkownika.
- Wczytanie przycina liczby spoza zakresu (z ostrzeżeniem) zamiast odrzucać plik, także te poza zakresem typu pola (np. 300 sektorów); najwyższy poziom pliku musi być obiektem JSON (serde czytałby też tablicę); komunikaty i ostrzeżenia po angielsku, jak reszta programu.
- Krótsze pierścienie są dopełniane pustymi elementami (`none`) przy wczytaniu, więc zawsze mają 12 pozycji; zależności `serde` i `serde_json` przypięte `=`.
- Okno docelowe akcji to okno główne spod kursora w chwili wciśnięcia prawego przycisku (zapamiętane w haku); akcja rusza w osobnym wątku, gdy okno koła jest już ukryte (czekanie do 0,5 s na `IsWindowVisible`).
- `paste_text`: poprzedni tekst schowka wraca po 500 ms tylko wtedy, gdy schowek nadal zawiera wklejony tekst (nie nadpisujemy tego, co użytkownik skopiował w międzyczasie); zawartość inna niż tekst nie jest przywracana.
- Plik konfiguracji jest obserwowany (wątek z `FindFirstChangeNotificationW` na folderze, bez odpytywania; zdarzenie zatrzymania kończy go przy wyjściu): zmiana trafia do okna UI jako `WM_CONFIG_CHANGED`, po 100 ms ciszy (zbicie serii zapisów) następuje `refresh()` i publikacja hakowi, więc nowa konfiguracja (wykluczenia, akcje, nazwy, motyw) działa od następnego wciśnięcia, a rozmiar i liczba sektorów od następnego otwarcia; reload czeka, aż koło zniknie. Sprawdzenie daty przy otwarciu koła zostaje jako zabezpieczenie; gdy folder nie istnieje, obserwator się nie uruchamia (komunikat na stderr).
- Akcje edycji, skrótu i wklejania wyciągają okno spod kursora na pierwszy plan (`SetForegroundWindow`), bo bez tego wejście trafiłoby do innego okna; „fokus bez zmian” dotyczy samego koła, które nie zabiera fokusu.
- Konfiguracja współdzielona z hakiem przez `Mutex<Option<Arc<Config>>>` trzymany tylko na czas klonowania `Arc`; hak nie dotyka plików.
- Argumenty wiersza poleceń są nakładane na wczytaną konfigurację (`Option` w `Overrides`), także po odświeżeniu pliku.
- Pełny ekran = okno główne pokrywa cały `rcMonitor` i nie jest pulpitem ani paskiem zadań (`Progman`/`WorkerW`/`Shell_TrayWnd`/`Shell_SecondaryTrayWnd`); zmaksymalizowane okno z paskiem tytułu (`IsZoomed` + `WS_CAPTION`) nie jest pełnym ekranem, także przy ukrywanym pasku zadań, a F11 w przeglądarce i gry (bez ramki) tak.
- `.bak` powstaje przy każdym udanym wczytaniu pliku (kopia ostatniej dobrej wersji, przez plik tymczasowy i zamianę); plik, który nie przejdzie walidacji, zostawia `.bak` bez zmian, a plik użytkownika nigdy nie jest przepisywany.
- Odświeżenie nie tworzy pliku (`load_existing`), a plik domyślny przy starcie powstaje przez `File::create_new`, więc nie zastąpi pliku, który pojawił się w międzyczasie.
- Plik konfiguracji: UTF-8 (BOM dozwolony) albo UTF-16 LE z BOM (tak zapisuje PowerShell 5.1); inne kodowanie to błąd `config file must be UTF-8`.
- `close_window` i `minimize_window` nic nie robią na pulpicie i pasku zadań.
- Sektor „Głośność” z makiety to akcja `volume_mute` o nazwie „Wycisz”; sektor „Terminal” uruchamia `powershell.exe` (jest na każdym Windows 10 i 11, w Windows 11 otwiera się w Terminalu) zamiast `wt.exe`.
- Schowek przy `paste_text`: okno koła jest właścicielem (`OpenClipboard`), tekst jest przygotowany przed `EmptyClipboard`, odczyt ograniczony do `GlobalSize`; `cmd.exe` uruchamiany z `%ComSpec%` (lub `System32`); ślad `pass: ...` wypisuje pętla efektów, nie wywołanie haka.
- Sprawdzenie ręczne fizyczną myszą przeniesione do etapu 05; w 04 zastępuje je pełny test automatyczny na LAB (decyzja użytkownika): logika wyzwalacza dla wejścia fizycznego i wstrzykiwanego z `--accept-injected` jest ta sama, fizyczną mysz sprawdzono w etapie 03.

## Podsumowanie
(wypełnia /etap-zamknij)
