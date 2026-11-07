# Etap 05: podmenu

## Cel
Najechanie na kafelek zewnętrzny z podmenu wysuwa łuk do 5 kafelków w tym samym kierunku, a wybór kafelka łuku
wykonuje jego akcję. Widok ikon działa (ikona z .exe, glify wbudowane dla komend i akcji, litera, gdy brak ikony).
Sprawdzenie fizyczną myszą przeniesione z etapu 04.

## Kryteria zamknięcia
- [x] Najechanie na kafelek z podmenu wysuwa łuk (pasmo 218–292 jak w makiecie, 26° na kafelek, wyśrodkowany na kierunku rodzica); zjechanie na inny kafelek go zamyka
- [x] Puszczenie lub kliknięcie na kafelku łuku wykonuje jego akcję; puszczenie dalej w kierunku łuku też go wybiera; poza kątem łuku działa dotychczasowy wybór sektora po kierunku; rodzic dalej wykonuje własną akcję
- [x] Kafelek z podmenu ma kreskę przy zewnętrznej krawędzi (jak w makiecie)
- [x] Tryb klikania (puszczenie w środku) obsługuje podmenu; klik poza kołem i łukiem zamyka
- [x] Okno mieści pasmo podmenu; koło przy krawędzi ekranu działa (okno częściowo poza ekranem, łuk poza ekranem wybierany kierunkiem)
- [x] Widok ikon (`view: icons`): `icon` w konfiguracji (ścieżka .ico/.exe/.dll) ma pierwszeństwo; bez niego program i plik dostają ikonę systemową celu, komendy edycji i akcje systemowe glif z Segoe Fluent Icons (Windows 10: Segoe MDL2 Assets), reszta pierwszą literę nazwy; ikony w pamięci podręcznej odświeżanej po zmianie konfiguracji; środek pokazuje nazwę wskazanego kafelka
- [x] Testy w `wheel-core`: trafianie w łuk (granice, zawijanie przez 0°, 1–5 kafelków, 4–12 sektorów), otwieranie i zamykanie łuku, rozmiar okna z pasmem podmenu
- [x] `cargo build --release`, `cargo test`, clippy `-D warnings` i `cargo fmt --check` czyste
- [x] Test automatyczny na LAB (`--accept-injected`, konfiguracja w scratchpadzie): akcje z łuku, zrzuty łuku i widoku ikon, koło przy krawędzi ekranu, regresje z 03 i 04
- [x] Sprawdzenie fizyczną myszą przez użytkownika według listy: kliknięcie, przeciąganie, przytrzymanie, akcje, podmenu
- [x] Puszczenie prawego na środku („Anuluj”) po przejściu kursorem po kafelkach zamyka koło; puszczenie na środku bez wyjeżdżania z niego zostawia tryb klikania lewym
- [x] `CHECKLISTA.md` i `README.md` zaktualizowane (`submenu` i `icon` nie są już zarezerwowane)

## Plan
- [x] `crates/wheel-core/src/geometry.rs`, `layout.rs`: promienie pasma podmenu, kąty kafelków łuku, `Hit::Sub(rodzic, k)` w `hit_test` przy otwartym łuku, rozmiar okna z pasmem; testy
- [x] `crates/wheel-core`: czysta logika stanu łuku (który otwarty po danym trafieniu) i mapowanie trafienia na `&Item`; testy
- [x] `apps/wheel/src/icons.rs` (nowy): ikony z plików (`SHDefExtractIconW` / `SHGetFileInfoW`, HICON na bitmapę D2D przez WIC), mapa glifów dla 12 komend edycji i 8 akcji systemowych, wybór czcionki glifów wg systemu, pamięć podręczna
- [x] `apps/wheel/src/render.rs`: kafelki łuku, kreska podmenu, podświetlenie, widok ikon, nazwa w środku
- [x] `apps/wheel/src/window.rs`, `hook.rs`, `crates/wheel-core/src/trigger.rs`: stan łuku przy ruchu, przekazanie łuku hakowi, wykonanie akcji z łuku w obu trybach
- [x] Domyślna konfiguracja: kafelek „Ustawienia” z podmenu Ekran, Dźwięk, Bluetooth, Aktualizacje (`ms-settings:`)
- [x] `README.md`, `CHECKLISTA.md`
- [x] `crates/wheel-core/src/trigger.rs`: w stanie otwartym znacznik wyjścia kursora ze środka (zdarzenia ruchu w haku); puszczenie na środku po wyjściu = anuluj; testy
- Sprawdzenie: tester: build, testy, clippy, fmt; skrypt PowerShell z `--accept-injected` i testowym `config.json` w scratchpadzie (`%APPDATA%` nietknięty) ze zrzutami; na końcu test fizyczną myszą przez użytkownika według listy kroków

## Decyzje
- Domyślna konfiguracja dostaje przykładowe podmenu „Ustawienia” (Ekran, Dźwięk, Bluetooth, Aktualizacje): funkcja widoczna od pierwszego uruchomienia (decyzja użytkownika).
- Trafianie z łukiem w osobnej funkcji `hit_test_arc(..., Option<SubArc>)`, a `hit_test` to nakładka z `None`: wyzwalacz i okno działają bez zmian, dopóki krok 5 nie poda łuku.
- Granica pierścienia zewnętrznego `Radii.outer` = 214 (środek szczeliny 210..218 z makiety); za nią, w kącie łuku, trafienie to kafelek łuku przy dowolnej odległości; łuk z rodzicem >= liczby sektorów albo liczbą kafelków spoza 1..=5 jest pomijany.
- Stan łuku (`arc_for_hit`) i mapowanie trafienia na element (`item_for_hit`) w nowym `submenu.rs`: to logika trafienia, nie dane konfiguracji.
- Okno ma bok `2 * (292 + 4)` (592 px przy 100% i 96 DPI zamiast 420): pasmo podmenu plus margines 4 px na obrys i wygładzanie (bez niego zrzuty miały obcięte krawędzie łuku); rozmiar kafelków bez zmian.
- Ikony ładowane tylko przy `view: icons`, razem z Rendererem (odbudowa po zmianie konfiguracji odświeża pamięć podręczną); w widoku nazw zbędne, oszczędność pamięci i czasu otwarcia.
- `icon` w konfiguracji: ścieżka z opcjonalnym `,N` (konwencja Windows, ujemne N = identyfikator zasobu), zmienne środowiskowe rozwijane, sama nazwa programu szukana przez `SearchPathW`; ładowanie `SHDefExtractIconW` w rozmiarze docelowym, a dla folderów, dokumentów i innych plików (także `icon` spoza .ico/.exe/.dll) ikona systemowa typu z `SHGetFileInfoW`; pierwsza litera, gdy i to zawiedzie (np. brak pliku).
- Glify także dla adresu (globus), skrótu (klawiatura), wklejenia tekstu i polecenia (wiersz poleceń); `wklej bez formatu` = E8E9. Czcionka: Segoe Fluent Icons, potem Segoe MDL2 Assets, sprawdzane przez `HasCharacter`; brak glifu daje literę.
- Rozmiary ikon z makiety: 24 px (wewnętrzny), 36 px (zewnętrzny), 30 px (podmenu) przy 100%, skalowane z kołem.
- Rysowanie wydzielone do `Painter` (kontekst D2D, pędzle, czcionki, ikony), `Renderer` dokłada swap chain i DirectComposition: test `render_cases` (`#[ignore]`, `WHEEL_RENDER_OUT=<folder>`) rysuje na bitmapie bez okna i zapisuje PNG do porównania z makietą.
- Kreska podmenu: zwykle kolor przygaszony, na wskazanym kafelku kolor tekstu podświetlenia, przy otwartym łuku (wskazany kafelek łuku) kolor akcentu, bo biała kreska na białym rodzicu byłaby niewidoczna.
- Glify rysowane czcionką (wypełnione), nie obrysem jak SVG w makiecie: ta sama czcionka co systemowe ikony Windows.
- COM (STA) inicjowany raz na wątku UI przy budowie ikon (wymaga go WIC); wcześniej miał go tylko wątek akcji.
- Koło przy krawędzi ekranu zostaje wyśrodkowane pod kursorem, bez dosuwania: przesunięcie zepsułoby wybór po kierunku, a łuk poza ekranem wybiera się kierunkiem. `fit.rs` dopasowuje tylko etykiety, więc plan i kryterium poprawione (wcześniej błędnie wskazywały `fit.rs`).
- Wybierane jest to, co widać: stan łuku prowadzi okno (odpytywanie kursora), a hak przy puszczeniu i kliknięciu bierze łuk opublikowany przez okno, zamiast liczyć własny z innych próbek ruchu.
- Kanał okno -> hak: `AtomicU16` (`rodzic << 8 | liczba`, 0 = brak łuku), zapis tylko w wątku UI, odczyt w haku tylko przy puszczeniu prawego i kliknięciu lewego; bez Mutexa na ścieżce zdarzeń myszy.
- Tryb klikania: klik w kafelek łuku wybiera do `sub_click_limit` = 292 + 4 (makieta: `R.s2 + 4`), dalej zamyka; pozostałe kafelki wybierają do granicy pierścienia zewnętrznego 214 (makieta: `R.o2 + 4`; wcześniej 210, więc klik w szczelinę 210..214 zamykał); okno nie zna trybu klikania, więc podświetlenie poza oknem (dalej niż 296) może zostać, choć klik zamknie, jak dotąd dla sektorów zewnętrznych.
- Test na żywo (`--accept-injected`, skrypt w scratchpadzie, `%APPDATA%` nietknięty): 13 scenariuszy, 25 sprawdzeń PASS (łuk, puszczenie daleko, poza kątem, rodzic, zawijanie przez 0°, zamykanie, tryb klikania z kafelkiem, za pasmem i w szczelinie, lewa krawędź i róg ekranu, widok ikon w ciemnym motywie po zmianie pliku, Escape, krótki klik, przeciąganie); akcje potwierdzone plikiem znacznika.
- Ograniczenie: łuk sektora skierowanego poza ekran się nie otworzy, bo kursor nie dojdzie do tego sektora; sam sektor nadal wybiera się kierunkiem.
- Puszczenie prawego na środku po wyjeździe kursora ze środka anuluje, bez wyjazdu wchodzi w tryb klikania (prośba użytkownika po teście myszą: tryb klikania zostaje „w razie co”). Wyjście ze środka liczy wyzwalacz ze zdarzeń ruchu w haku, bo tam zapada decyzja przy puszczeniu.
- Test fizyczną myszą przez użytkownika: wszystkie 10 punktów listy działa (2026-11-07).

## Podsumowanie
(wypełnia /etap-zamknij)
