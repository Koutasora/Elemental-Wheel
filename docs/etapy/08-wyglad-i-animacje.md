# Etap 08: wyglad-i-animacje

## Cel
Dopracowany wygląd koła, krótkie animacje (otwarcie, zamknięcie, podświetlenie, łuk podmenu) i usunięcie opóźnienia
pierwszego otwarcia po zmianie konfiguracji, bez zmiany działania.

## Kryteria zamknięcia
- [x] Kafelki czytelne na każdym tle (podkład pod kafelkami, rowki zamiast prześwitu); delikatny cień koła, podświetlenie w kolorze akcentu, motyw jasny i ciemny
- [x] Minimalna bezwzględna wielkość czcionki (8 px × DPI, niezależnie od rozmiaru koła)
- [x] Animacje: otwarcie (zanikanie + skala 0,92→1, 120 ms), zamknięcie (zanikanie, 80 ms), płynne podświetlenie (90 ms), wysunięcie łuku podmenu (110 ms); akcja startuje od razu, zamknięcie jej nie opóźnia
- [x] Animacje wyłączane opcją `animations` (konfiguracja i edytor) i zgodne z ustawieniem Windows „Efekty animacji”
- [x] Szkło (rozmyte tło jak Vista Aero / macOS) z opcją `glass_pct` 0–100 (0 = bez szkła) w konfiguracji i jako suwak w edytorze; bez szkła także przy wyłączonej przezroczystości Windows
- [x] Renderer i ikony przygotowane po wczytaniu konfiguracji (otwarcie nie buduje renderera); układy tekstu i geometrie nie tworzone przy każdym odrysowaniu
- [x] Mediana otwarcia do 16 ms (na żywo, fizyczna mysz, szkło: 6,3 ms, maks. 19,6 ms przy pierwszym otwarciu)
- [x] Pamięć prywatna w tle: ze szkłem do 25 MB, bez szkła do 15 MB (nazwy) i 17 MB (ikony) — `--run` 3 min, także po przeładowaniu konfiguracji: 21,6 / 24,6 MB ze szkłem, 13,5 / 16,2 MB bez
- [x] Opcja `replace_right_click` (konfiguracja i edytor, domyślnie wyłączona): koło od razu po wciśnięciu prawego przycisku; systemowy prawy przycisk nie dociera do programów (poza oknami administratora, wykluczeniami i pełnym ekranem); test fizyczną myszą
- [x] „Właściwości” z koła działa w Eksploratorze i na pulpicie (jeden plik, kilka plików, folder, ikona na pulpicie; test fizyczną myszą)
- [x] Akcja systemowa `context_menu` („Menu kontekstowe”) w konfiguracji i edytorze
- [x] Podgląd w edytorze zgodny z nowym wyglądem koła (szkło nad tłem próbnym, suwak działa od razu)
- [x] Wygląd zatwierdzony przez użytkownika na zrzutach (szkło runda 3: „wygląda bardzo dobrze”)
- [x] `cargo build --release`, `cargo test`, clippy `-D warnings`, `cargo fmt --check` czyste (68 + 130 + 30 testów); README i CHECKLISTA zaktualizowane

## Plan
- [x] `apps/wheel/src/render.rs`, `theme.rs`: nieprzezroczyste wypełnienia, cień, akcent, minimalna czcionka; zrzuty do akceptacji przed animacjami
- [x] `render.rs`, `theme.rs`, `backdrop.rs`: zaokrąglone kafelki, szkło z migawki tła (rozmycie D2D, połysk, poświata tekstu)
- [x] `apps/wheel/src/window.rs`, `render.rs`, `anim.rs`: animacje otwarcia i zamknięcia przez DirectComposition, podświetlenie i łuk przez interpolację na zegarze 16 ms
- [x] `crates/wheel-core/src/config.rs`, `look.rs`, `apps/wheel-editor/src/options.rs`: opcje `animations`, `glass_pct`, `replace_right_click`, testy
- [x] `window.rs`, `render.rs`, `hook.rs`: migawka przy wciśnięciu, renderer po wczytaniu konfiguracji, wspólne urządzenie D3D/D2D, trim po zamknięciu
- [x] `apps/wheel-editor/src/preview.rs`: ten sam wygląd
- [x] `apps/wheel/src/properties.rs`, `actions.rs`: „Właściwości” przez powłokę, kody skanowania w skrótach
- [x] `actions.rs`, `hook.rs`, `config.rs`: akcja `context_menu`
- [x] ~~Pomiary `tools/soak.ps1`~~ test na żywo fizyczną myszą z `--stats` (decyzja użytkownika)
- [x] README, CHECKLISTA
- Sprawdzenie: tester: build, testy, clippy, fmt; zrzuty offscreen i `--demo` z konfiguracją w scratchpadzie (`%APPDATA%` nietknięty); test na żywo `--run --stats` z konfiguracją w scratchpadzie

## Decyzje
- Kolejność etapów zamieniona (decyzja użytkownika): 08 wyglad-i-animacje, 09 instalator-i-wydanie, żeby jedno wydanie 1.0.0 na końcu miało gotowy wygląd.
- Wydanie: Inno Setup (instalacja dla jednego użytkownika) i wersja 1.0.0 (decyzje użytkownika, do etapu 09).
- Wygląd, runda 1: podkład pod kafelkami (rowki zamiast prześwitu), cień, rodzic łuku zabarwiony akcentem 22%, znacznik podmenu nie zasłania nazwy, glify wyrównane do 72% pola ikony.
- Wygląd, runda 2 (użytkownik): bardziej zaokrąglone rogi kafelków i próba efektu szkła (prawdziwe rozmycie tła); minimum czcionki 8 px zamiast 9 (przy 60% 9 px obcinało nazwy), cień w jasnym motywie o połowę słabszy.
- Szkło systemowe (HostBackdropBrush) odrzucone: rozmycie za mocne i nieregulowane, wymaga nieudokumentowanego API; zamiast niego migawka ekranu pod kołem przy otwarciu, rozmycie i nasycenie w D2D, połysk i poświata tekstu (cel jakości: Vista Aero / macOS, decyzja użytkownika).
- Szkło z migawki zaakceptowane przez użytkownika (runda 3: rozmycie σ 9 px, nasycenie 1.4, tint ciemny 0.45 / jasny 0.42 przy mocy 50, połysk u góry, poświata tekstu). Do poprawy: migawka BitBlt ok. 31 ms na LAB (RDP) → robiona w tle przy wciśnięciu prawego przycisku (180 ms progu wystarcza); pamięć 31 MB → rozmycie na migawce zmniejszonej 4×, zwalnianie bitmap po zamknięciu.
- Pamięć ze szkłem (decyzja użytkownika, wariant 1): efekty rozmycia zwalniane po zamknięciu i tworzone przy wciśnięciu (w czasie progu 180 ms); cel ze szkłem najwyżej 20 MB, bez szkła 15 MB (pomiar demo po zmniejszeniu migawki: szkło 25 MB, bez szkła 14,8 MB; efekty GPU ok. 7 MB).
- Moc przezroczystości szkła jako suwak w opcjach (`glass_pct`, decyzja użytkownika).
- Test na żywo fizyczną myszą zamiast soak.ps1 (decyzja użytkownika: LAB w użyciu). Wynik: otwarcie 5–11 ms; pamięć w tle ze szkłem (nazwy) ok. 23,5 MB, ponad cel 20 MB (demo 18,4) — do zbadania.
- Naprawa „Właściwości” w tym etapie (decyzja użytkownika): Alt+Enter z koła nie działa w Eksploratorze (błąd od etapu 04); w Eksploratorze okno właściwości zaznaczonych elementów przez powłokę, gdzie indziej Alt+Enter.
- Cel pamięci zmieniony (decyzja użytkownika): ze szkłem do 25 MB (zmierzone: nazwy 21,6, ikony 24,3 MB), bez szkła 15 MB nazwy / 17 MB ikony (zmierzone 13,8 / 17,0); liczy się szybkość otwarcia.
- Nowa opcja `replace_right_click` (prośba użytkownika): koło zamiast systemowego prawego przycisku, otwarcie od razu po wciśnięciu.
- `--run` nie startował, gdy GetCursorPos odmawia dostępu (np. zminimalizowane RDP): w tym etapie start na głównym monitorze; sprawdzone testem jednostkowym czystej funkcji, bez próby na żywo.
- Akcja `context_menu` (prośba użytkownika): wstrzyknięte prawe kliknięcie z MARKER w punkcie wciśnięcia, żeby w trybie `replace_right_click` dało się otworzyć zwykłe menu.
- Pamięć w tle zbadana jednak w tym etapie (prośba użytkownika): 30,9 MB to stan po przeładowaniu konfiguracji — rozgrzewka renderera kolejkowała rysunki bez czekania na GPU, sterownik AMD zostawiał po każdej przebudowie 8–13 MB; czekanie na GPU po każdym rysunku rozgrzewki usuwa wzrost. Po akcji „Właściwości” pamięć rośnie o ok. 4 MB (arkusz właściwości w procesie koła).
- Ryzyko: przeładowanie ze szkła na brak szkła zostawia potoki efektów szkła (ok. 19 zamiast 13 MB; niesprawdzone po poprawce).
- Znane ograniczenie: w trybie `replace_right_click` migawka tła (BitBlt, 20–50 ms przez RDP) jest robiona przed otwarciem, więc otwarcie ze szkłem trwa o tyle dłużej; bez pomiaru na żywo.
- Niestabilny test edytora (wspólny katalog tymczasowy przy testach równoległych) poprawiony: osobny katalog na każde wywołanie.
- Etap 09 w skróconym zakresie (decyzja użytkownika, ok. 1–1,5 h): instalator Inno Setup, autostart, zasobnik tylko z pozycjami Edytor / Wyłącz-Włącz / Zamknij, jeden wariant ikony programu do akceptacji, jeden test instalacji i odinstalowania na LAB plus krótki test fizyczną myszą, wydanie 1.0.0; nowe pomysły tylko na listę na później.
- Podglądy wyglądu tylko lokalnie (plik HTML na LAB), nie na claude.ai (zasada użytkownika).
- Poza zakresem: znany błąd po Escape (decyzja z etapu 07), zasobnik i tryb bez konsoli (etap 09).

## Pomiary
LAB (RDP), 1920x1080, 96 DPI, release, konfiguracja 12+12 sektorów, dwa podmenu, szkło 50.

| | wynik |
|---|---|
| otwarcie na żywo (`--run --stats`, fizyczna mysz, 8 otwarć) | mediana 6,3 ms, maks. 19,6 ms (pierwsze) |
| wcześniejsza seria na żywo (48 otwarć, poprzednia wersja) | 5–11 ms |
| migawka tła przy wciśnięciu (BitBlt przez RDP) | 20–50 ms, poza ścieżką otwarcia |
| upload + rozmycie migawki | 1–3 ms |
| pamięć prywatna demo po zamknięciu (nazwy / ikony, szkło) | 21,6 / 24,3 MB |
| pamięć prywatna demo po zamknięciu (nazwy / ikony, bez szkła) | 13,8 / 17,0 MB |
| pamięć prywatna `--run` (nazwy, szkło) przed poprawką: po przeładowaniu konfiguracji / po teście z „Właściwości” | 30,9 / 34,8 MB |
| pamięć prywatna `--run`, 3 min z przeładowaniem, po poprawce: nazwy / ikony, szkło | 21,6 / 24,6 MB |
| pamięć prywatna `--run`, 3 min z przeładowaniem, po poprawce: nazwy / ikony, bez szkła | 13,5 / 16,2 MB |

## Podsumowanie
(wypełnia /etap-zamknij)
