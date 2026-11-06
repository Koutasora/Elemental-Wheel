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
- [ ] Łuk z maksymalnie 5 dodatkowymi kafelkami po najechaniu
- [ ] Widok kafelków: nazwy albo ikony (ikona z .exe, litera gdy brak)

## 06 edytor
- [ ] Edytor jako osobny proces z podglądem
- [ ] Edycja elementu: nazwa, typ akcji, cel, ikona, podmenu
- [ ] Edytowalna nazwa każdego elementu, także komend edycji w pierścieniu wewnętrznym
- [ ] Zapis konfiguracji do JSON

## 07 stabilnosc-i-pomiary
- [ ] Pomiar zużycia pamięci w tle
- [ ] Pomiar czasu otwarcia koła
- [ ] Odporność na awarie edytora i błędną konfigurację
- [ ] Log awarii i test wielogodzinny

## 08 instalator-i-wydanie
- [ ] Instalator, który wypisuje zainstalowaną wersję
- [ ] Autostart przy logowaniu
- [ ] Ikona w zasobniku systemowym (włącz/wyłącz, edytor, zamknij)
- [ ] Wydanie z opisem zgodnym z README

## 09 wyglad-i-animacje
- [ ] Dopracowany wygląd koła i efekty
- [ ] Animacje otwierania, zamykania i podświetlenia
