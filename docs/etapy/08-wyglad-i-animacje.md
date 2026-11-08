# Etap 08: wyglad-i-animacje

## Cel
Dopracowany wygląd koła, krótkie animacje (otwarcie, zamknięcie, podświetlenie, łuk podmenu) i usunięcie opóźnienia
pierwszego otwarcia po zmianie konfiguracji, bez zmiany działania.

## Kryteria zamknięcia
- [ ] Kafelki nieprzezroczyste i czytelne na każdym tle (dziś przez pierścień wewnętrzny prześwituje tło); delikatny cień koła, podświetlenie w kolorze akcentu, motyw jasny i ciemny
- [ ] Minimalna bezwzględna wielkość czcionki (przy 60–75% nie schodzi do ok. 5 px)
- [ ] Animacje: otwarcie (zanikanie + lekkie skalowanie, ok. 120 ms), zamknięcie (zanikanie, ok. 80 ms), płynne podświetlenie, wysunięcie łuku podmenu; akcja startuje od razu, zamknięcie jej nie opóźnia
- [ ] Animacje wyłączane opcją `animations` (konfiguracja i edytor) i zgodne z ustawieniem Windows „Pokazuj animacje”
- [ ] Szkło (rozmyte tło jak Vista Aero / macOS) z opcją `glass_pct` 0–100 (przezroczystość szkła; 0 = bez szkła, wygląd nieprzezroczysty) w konfiguracji i jako suwak w edytorze; bez szkła także przy wyłączonej przezroczystości Windows
- [ ] Renderer i ikony przygotowane po wczytaniu konfiguracji: otwarcie po zmianie konfiguracji poniżej 16 ms (dziś 89 ms); układy tekstu nie tworzone przy każdym odrysowaniu
- [ ] Pamięć prywatna w tle: ze szkłem do 25 MB, bez szkła do 15 MB (nazwy) i 17 MB (ikony); mediana otwarcia nadal do 16 ms; wyniki w pliku etapu
- [ ] Opcja `replace_right_click` (konfiguracja i edytor, domyślnie wyłączona): koło otwiera się od razu po wciśnięciu prawego przycisku, bez przytrzymania; systemowy prawy przycisk nie dociera do programów (poza oknami administratora, wykluczeniami i pełnym ekranem)
- [ ] „Właściwości” z koła działa w Eksploratorze i na pulpicie (test fizyczną myszą)
- [ ] Podgląd w edytorze zgodny z nowym wyglądem koła
- [ ] Wygląd zatwierdzony przez użytkownika na zrzutach (jasny/ciemny, nazwy/ikony, łuk podmenu)
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings`, `cargo fmt --check` czyste; README i CHECKLISTA zaktualizowane

## Plan
- [x] `apps/wheel/src/render.rs`, `theme.rs`: nieprzezroczyste wypełnienia, cień, akcent, minimalna czcionka; zrzuty do akceptacji przed animacjami
- [ ] `apps/wheel/src/window.rs`, `render.rs`: animacje otwarcia i zamknięcia przez DirectComposition (przezroczystość i skala, bez ponownego rysowania klatek), podświetlenie i łuk przez interpolację na istniejącym zegarze 16 ms
- [ ] `crates/wheel-core/src/config.rs`, `apps/wheel-editor/src/options.rs`: opcje `animations` (domyślnie włączona) i `glass_pct` (suwak w edytorze), testy
- [ ] `apps/wheel/src/window.rs`, `render.rs`, `icons.rs`: renderer i ikony po wczytaniu konfiguracji, pamięć podręczna układów tekstu, mniejsze bitmapy ikon
- [ ] `apps/wheel-editor/src/preview.rs`: ten sam wygląd
- [ ] Pomiary `tools/soak.ps1` (wejście wstrzykiwane: najpierw uprzedzenie i czekanie na zgodę użytkownika)
- [ ] README, CHECKLISTA
- Sprawdzenie: tester: build, testy, clippy, fmt; zrzuty z `--demo` z konfiguracją w scratchpadzie (`%APPDATA%` nietknięty); pomiary `soak.ps1`

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
- Do etapu 09: `--run` nie startuje, gdy GetCursorPos odmawia dostępu (np. zminimalizowane RDP) — przyjąć główny monitor.
- Podglądy wyglądu tylko lokalnie (plik HTML na LAB), nie na claude.ai (zasada użytkownika).
- Poza zakresem: znany błąd po Escape (decyzja z etapu 07), zasobnik i tryb bez konsoli (etap 09).

## Podsumowanie
(wypełnia /etap-zamknij)
