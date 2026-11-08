# Checklista

## 01 plan-i-szkielet
- [x] Szkielet Cargo workspace (wheel-core, wheel, wheel-editor)
- [x] Geometria koła: 12 sektorów, środek, pierścienie, wybór po kierunku
- [x] Test geometrii w wheel-core
- [x] `wheel.exe --version`
- [x] `.gitignore`, `LICENSE` (MIT), `README.md`
- [x] Makieta koncepcji `docs/koncept.html`

## 02 kolo-render
- [x] Koło z 12 sektorami (pierścień zewnętrzny i wewnętrzny) rysowane pod kursorem
- [x] Liczba sektorów pierścienia zewnętrznego 4–12, domyślnie 12
- [x] Okno koła bez aktywacji (nie zabiera fokusu)
- [x] Motyw jasny i ciemny (domyślnie jak w Windows)
- [x] Rozmiar koła 60–140% względem DPI monitora
- [x] Nazwy kafelków dopasowane do sektora (dwie linie, mniejsza czcionka)

## 03 wyzwalacz-ppm
- [x] Koło otwiera się po przytrzymaniu prawego przycisku
- [x] Krótkie kliknięcie prawym działa w programach normalnie
- [x] Przeciąganie prawym działa w programach normalnie
- [x] Progi 180 ms / 6 px ustawiane argumentami (`--hold-ms`, `--move-px`); w pliku konfiguracji w etapie 04
- [x] Środek koła = anuluj / tryb klikania
- [x] Esc zamyka koło; klik poza kołem w trybie klikania zamyka
- [x] Okna administratora dostają prawy przycisk bez zmian
- [x] Strażnik haka zakłada hak ponownie, gdy Windows go odłączy
- [x] Jedna kopia programu, `wheel.exe --quit` zamyka działającą
- [x] Sprawdzenie fizyczną myszą przez użytkownika

## 04 akcje-i-konfiguracja
- [x] Typy akcji: program, folder/plik, adres, skrót klawiszowy, komenda edycji, wklejenie tekstu, polecenie, akcja systemowa
- [x] Pierścień wewnętrzny z komendami edycji (wytnij, kopiuj, wklej, usuń, cofnij, ponów)
- [x] Nazwa każdego elementu (także komend edycji) zapisana w konfiguracji i wyświetlana zamiast domyślnej
- [x] Liczba sektorów, motyw, rozmiar koła i progi wyzwalacza w opcjach
- [x] Konfiguracja w `%APPDATA%\Elemental-Wheel\config.json` z wersją schematu, zapis atomowy i kopia `.bak`
- [x] Lista wykluczonych programów i wyłączenie w pełnym ekranie

## 05 podmenu
- [x] Łuk z maksymalnie 5 dodatkowymi kafelkami po najechaniu
- [x] Wybór kafelka łuku puszczeniem, kliknięciem i kierunkiem; kreska przy kafelku z podmenu
- [x] Widok kafelków: nazwy albo ikony (ikona z .exe, glify dla komend i akcji, litera gdy brak)
- [x] Przykładowe podmenu „Ustawienia” w domyślnej konfiguracji
- [x] Test automatyczny na LAB (wejście wstrzykiwane): łuk, tryb klikania, krawędź ekranu, widok ikon
- [x] Puszczenie prawego na środku po przejściu po kafelkach zamyka koło; puszczenie od razu na środku zostawia tryb klikania
- [x] Sprawdzenie fizyczną myszą

## 06 edytor
- [x] Edytor jako osobny proces z podglądem
- [x] Edycja elementu: nazwa, typ akcji, cel, ikona, podmenu
- [x] Edytowalna nazwa każdego elementu, także komend edycji w pierścieniu wewnętrznym
- [x] Zapis konfiguracji do JSON (walidacja przed zapisem, `.bak` tylko z poprawnego pliku)
- [x] Opcje koła w edytorze (motyw, rozmiar, sektory, widok, progi, pełny ekran, wykluczenia)
- [x] Przeciągnięcie pliku na kafelek podglądu
- [x] Polskie komunikaty błędów i ostrzeżeń w edytorze
- [x] Układ okna zatwierdzony
- [x] Test na LAB: zapis, `.bak`, przeładowanie w działającym kole
- [x] Podgląd: rozmiar koła z opcji, widok ikon, zaokrąglone kafelki, podświetlenie pod kursorem
- [x] Liczba sektorów pierścienia wewnętrznego 4–12

## 07 stabilnosc-i-pomiary
- [x] Pomiar zużycia pamięci w tle (nazwy 13,9 MB, ikony 17 MB)
- [x] Pomiar czasu otwarcia koła (mediana ok. 3 ms; po zmianie konfiguracji 89 ms)
- [x] Odporność na awarie edytora i błędną konfigurację (zepsuty plik w trakcie pracy, zabity edytor, przerwany zapis)
- [x] Edytor wykrywa zmianę pliku konfiguracji z zewnątrz
- [x] Log w `%LOCALAPPDATA%\Elemental-Wheel` (koło i edytor), limit 1 MB, `--log`
- [x] Panika w haku, oknie koła i akcji przechwycona i zapisana w logu; koło działa dalej
- [x] Narzędzie `tools/soak.ps1` (test i pomiary); test wielogodzinny pominięty (decyzja)
- [ ] Znany błąd: przytrzymanie tuż po zamknięciu koła Escape czasem nie otwiera koła

## 08 wyglad-i-animacje
- [x] Podkład pod kafelkami, cień, zaokrąglone kafelki, znacznik podmenu nie zasłania nazwy
- [x] Szkło: rozmyte tło pod kołem, połysk, poświata tekstu; moc szkła 0–100% w opcjach
- [x] Animacje otwarcia, zamknięcia, podświetlenia i łuku; opcja wyłączenia, zgodność z ustawieniem Windows
- [x] Renderer gotowy przed pierwszym otwarciem (także po zmianie konfiguracji), migawka tła przy wciśnięciu
- [x] Otwarcie na żywo: mediana 6,3 ms
- [x] Opcja „Koło zamiast prawego przycisku”
- [x] „Właściwości” z koła w Eksploratorze i na pulpicie
- [x] Start w tle bez dostępu do kursora (główny monitor; test jednostkowy)
- [x] Akcja „Menu kontekstowe”
- [x] Podgląd w edytorze z nowym wyglądem
- [x] Pamięć w tle: szkło 22,9 MB (nazwy) / 25,0 MB (ikony), bez szkła 13,4 / 16,7 MB (po wyłączeniu szkła w działającym kole do restartu 19,3 / 22,3 MB)

## 09 instalator-i-wydanie
- [x] Instalator (Inno Setup, jeden użytkownik), który wypisuje zainstalowaną wersję
- [x] Autostart przy logowaniu
- [x] Ikona w zasobniku systemowym (włącz/wyłącz, edytor, zamknij)
- [x] Opcje koła z koła (akcja systemowa) i z zasobnika
- [x] Tryb bez konsoli
- [x] Ikona programu (exe, zasobnik, instalator)
- [x] Przywróć domyślne (cała konfiguracja) w edytorze
- [x] Koło zaznacza plik pod kursorem w Eksploratorze i na pulpicie
- [x] Test instalacji, aktualizacji i odinstalowania na LAB
- [x] Test fizyczną myszą zainstalowanej wersji
- [x] Wydanie 1.0.0 z opisem zgodnym z README (szkic na GitHubie, publikacja po scaleniu PR)
