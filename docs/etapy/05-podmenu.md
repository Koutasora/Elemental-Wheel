# Etap 05: podmenu

## Cel
Najechanie na kafelek zewnętrzny z podmenu wysuwa łuk do 5 kafelków w tym samym kierunku, a wybór kafelka łuku
wykonuje jego akcję. Widok ikon działa (ikona z .exe, glify wbudowane dla komend i akcji, litera, gdy brak ikony).
Sprawdzenie fizyczną myszą przeniesione z etapu 04.

## Kryteria zamknięcia
- [ ] Najechanie na kafelek z podmenu wysuwa łuk (pasmo 218–292 jak w makiecie, 26° na kafelek, wyśrodkowany na kierunku rodzica); zjechanie na inny kafelek go zamyka
- [ ] Puszczenie lub kliknięcie na kafelku łuku wykonuje jego akcję; puszczenie dalej w kierunku łuku też go wybiera; poza kątem łuku działa dotychczasowy wybór sektora po kierunku; rodzic dalej wykonuje własną akcję
- [ ] Kafelek z podmenu ma kreskę przy zewnętrznej krawędzi (jak w makiecie)
- [ ] Tryb klikania (puszczenie w środku) obsługuje podmenu; klik poza kołem i łukiem zamyka
- [ ] Okno i dopasowanie do krawędzi ekranu (`fit.rs`) uwzględniają pasmo podmenu
- [ ] Widok ikon (`view: icons`): `icon` w konfiguracji (ścieżka .ico/.exe/.dll) ma pierwszeństwo; bez niego program i plik dostają ikonę systemową celu, komendy edycji i akcje systemowe glif z Segoe Fluent Icons (Windows 10: Segoe MDL2 Assets), reszta pierwszą literę nazwy; ikony w pamięci podręcznej odświeżanej po zmianie konfiguracji; środek pokazuje nazwę wskazanego kafelka
- [ ] Testy w `wheel-core`: trafianie w łuk (granice, zawijanie przez 0°, 1–5 kafelków, 4–12 sektorów), otwieranie i zamykanie łuku, rozmiar okna z pasmem podmenu
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings` i `cargo fmt --check` czyste
- [ ] Test automatyczny na LAB (`--accept-injected`, konfiguracja w scratchpadzie): akcje z łuku, zrzuty łuku i widoku ikon, koło przy krawędzi ekranu, regresje z 03 i 04
- [ ] Sprawdzenie fizyczną myszą przez użytkownika według listy: kliknięcie, przeciąganie, przytrzymanie, akcje, podmenu
- [ ] `CHECKLISTA.md` i `README.md` zaktualizowane (`submenu` i `icon` nie są już zarezerwowane)

## Plan
- [ ] `crates/wheel-core/src/geometry.rs`, `layout.rs`: promienie pasma podmenu, kąty kafelków łuku, `Hit::Sub(rodzic, k)` w `hit_test` przy otwartym łuku, rozmiar okna z pasmem; testy
- [ ] `crates/wheel-core`: czysta logika stanu łuku (który otwarty po danym trafieniu) i mapowanie trafienia na `&Item`; testy
- [ ] `apps/wheel/src/icons.rs` (nowy): ikony z plików (`SHDefExtractIconW` / `SHGetFileInfoW`, HICON na bitmapę D2D przez WIC), mapa glifów dla 12 komend edycji i 8 akcji systemowych, wybór czcionki glifów wg systemu, pamięć podręczna
- [ ] `apps/wheel/src/render.rs`: kafelki łuku, kreska podmenu, podświetlenie, widok ikon, nazwa w środku
- [ ] `apps/wheel/src/window.rs`, `fit.rs`: większe okno, stan łuku przy ruchu, wykonanie akcji z łuku w obu trybach
- [ ] Domyślna konfiguracja: kafelek „Ustawienia” z podmenu Ekran, Dźwięk, Bluetooth, Aktualizacje (`ms-settings:`)
- [ ] `README.md`, `CHECKLISTA.md`
- Sprawdzenie: tester: build, testy, clippy, fmt; skrypt PowerShell z `--accept-injected` i testowym `config.json` w scratchpadzie (`%APPDATA%` nietknięty) ze zrzutami; na końcu test fizyczną myszą przez użytkownika według listy kroków

## Decyzje
- Domyślna konfiguracja dostaje przykładowe podmenu „Ustawienia” (Ekran, Dźwięk, Bluetooth, Aktualizacje): funkcja widoczna od pierwszego uruchomienia (decyzja użytkownika).

## Podsumowanie
(wypełnia /etap-zamknij)
