# Etap 02: kolo-render

## Cel
Narysować koło pod kursorem: pierścień zewnętrzny (4–12 sektorów, domyślnie 12), wewnętrzny (12 sektorów) i środek
anulowania, z nazwami kafelków, w przezroczystym oknie, które nie zabiera fokusu. Motyw jasny i ciemny, rozmiar
60–140% względem DPI monitora. Bez wyzwalacza prawym przyciskiem (etap 03): koło otwiera polecenie testowe.

## Kryteria zamknięcia
- [x] `wheel.exe --demo` otwiera koło ze środkiem pod kursorem; zamyka się kliknięciem albo po 10 s
- [x] Rysowane: pierścień zewnętrzny, wewnętrzny (12 sektorów, pierwszy u góry) i środek, z nazwami kafelków; podświetlenie sektora pod kursorem
- [x] Liczba sektorów pierścienia zewnętrznego: `--sectors 4..12`, domyślnie 12; pierwszy sektor u góry, wartości spoza zakresu przycinane
- [x] Okno bez aktywacji: okno na pierwszym planie (np. Notatnik) to samo przed i po otwarciu koła
- [x] Motyw `--theme system|light|dark`, domyślnie `system` (odczyt z ustawień Windows)
- [x] Rozmiar `--scale 60..140`; promienie liczone w `wheel-core` z DPI monitora pod kursorem, wartości spoza zakresu przycinane
- [x] Testy w `wheel-core`: skalowanie promieni (DPI 96/144, 60/100/140%), przycinanie zakresów, trafianie w sektory dla 4, 6, 8 i 12 sektorów, granice `hit_test` na promieniach i kątach
- [x] `cargo build --release`, `cargo test`, clippy `-D warnings` i `cargo fmt --check` czyste
- [x] Zrzuty koła (jasny/ciemny, 60/100/140%, 12 i mniej sektorów) pokazane i zaakceptowane
- [x] `CHECKLISTA.md` i `README.md` zaktualizowane

## Plan
- [x] `crates/wheel-core`: liczba sektorów jako parametr (`sector_at`, `hit_test`; osobno dla pierścienia zewnętrznego i wewnętrznego), moduł `layout.rs` (promienie z DPI i skali, kąty łuków sektorów), model nazw kafelków demo (wewnętrzny: komendy edycji jak w makiecie), testy
- [x] `apps/wheel`: zależność `windows` (windows-rs); okno `WS_EX_NOACTIVATE | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP`, DPI Per-Monitor V2
- [x] `apps/wheel`: rysowanie Direct2D + DirectComposition (przezroczystość alfa), tekst DirectWrite; kolory motywów z `docs/koncept.html`, jeden akcent
- [x] `apps/wheel`: argumenty `--demo`, `--theme`, `--scale`, `--sectors`; motyw systemu z rejestru (`AppsUseLightTheme`); podświetlenie sektora pod kursorem
- [x] `CHECKLISTA.md` (punkty 02 + liczba sektorów), `README.md` (status, argumenty)
- Sprawdzenie: tester: build, testy, clippy, fmt; skrypt PowerShell porównuje okno na pierwszym planie przed i po `--demo`; zrzuty ekranu w scratchpadzie pokazane przed zamknięciem

## Decyzje
- Widok ikon przeniesiony do etapu 05 (zgodnie z checklistą); w 02 tylko nazwy, bo ikony z .exe wymagają modelu akcji z etapu 04.
- Środek koła zawsze pod kursorem, także przy krawędzi ekranu (część koła może wyjść poza monitor); wybór i tak działa po kierunku.
- Ustawienia na razie w argumentach wiersza poleceń; plik konfiguracji w etapie 04 (tam też opcje rozmiaru, motywu i liczby sektorów).
- Liczba sektorów pierścienia zewnętrznego regulowana (prośba użytkownika): domyślnie 12, zakres 4–12. Minimum 4: mniej daje sektory po 120°+ i koło traci sens wobec zwykłego menu; maksimum 12: przy więcej nazwy przestają się mieścić, a wybór po kierunku robi się niepewny (sektory < 30°).
- Pierścień wewnętrzny zostaje na 12 sektorach (komendy edycji, stałe kierunki Cofnij lewo / Ponów prawo).
- Wymiary bazowe (100%, 96 DPI) z makiety `docs/koncept.html`: środek 60 px, granica pierścieni 124 px (środek przerwy 122–126), promień zewnętrzny 210 px (okno 420 px).
- Nazwy kafelków demo wzięte z makiety (polski tekst UI); `sector_at` traktuje 0 sektorów jak 1, żeby nie dzielić przez zero.
- windows-rs 0.62.2 (przypięty) + windows-numerics 0.3.1 (typy wektorów wymagane przez Direct2D w tej wersji, nie są reeksportowane).
- Podświetlenie przez odpytywanie kursora co 16 ms (`GetCursorPos`): wybór po kierunku działa też, gdy kursor jest poza oknem koła.
- Brak GPU: zapasowo rasteryzer programowy WARP.
- Kafelki i środek nieprzezroczyste (decyzja użytkownika po zrzutach): półprzezroczyste z makiety mieszały tekst okna pod spodem z nazwami; przezroczyste zostaje tylko tło poza kołem i odstępy.
- Dopasowanie nazw (decyzja użytkownika): jedna linia, potem dwie przy spacji, potem czcionka zmniejszana do 80%, wielokropek dopiero na końcu; przy 100% nazwy pierścienia wewnętrznego były ucinane.
- Etykieta w sektorze: ramka może przesunąć się promieniowo (sektor szerszy przy zewnętrznej krawędzi), ale zostaje pośrodku, gdy tam się mieści; odstęp 1 px, czcionka minimum 75%.
- Domyślna nazwa „Właściwości” skrócona do „Właśc.” (decyzja użytkownika): jedno długie słowo nie mieści się w sektorze wewnętrznym nawet przy 75%, a niższa czcionka jest nieczytelna.
- Każdy element ma edytowalną nazwę, także komendy edycji w pierścieniu wewnętrznym (prośba użytkownika): nazwy w konfiguracji (etap 04) i w edytorze (etap 06); nazwy demo w `wheel-core` to tylko wartości domyślne, a dopasowanie nazw z tego etapu działa dla dowolnego tekstu.
- Wygląd zaakceptowany przez użytkownika jako użytkowy; dopracowanie wyglądu, efekty i animacje później, w nowym etapie 09 wyglad-i-animacje (PLANOWANY).
- Sprawdzenie na ekranie: 7 wariantów (jasny/ciemny/system, 60/100/140%, 12/6/4 sektory) na własnym oknie testowym z tłem z tekstem; fokus bez zmian, koło zamyka się samo z kodem 0.

## Podsumowanie
(wypełnia /etap-zamknij)
