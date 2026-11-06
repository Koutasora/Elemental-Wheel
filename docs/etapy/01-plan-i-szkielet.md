# Etap 01: plan-i-szkielet

## Cel
Opisać koncept, zakres i technologię Elemental Wheel (koło akcji pod prawym przyciskiem myszy w układzie koła
kwintowego, Windows 10/11) i postawić szkielet projektu, który się buduje i ma jeden przechodzący test.

## Zakres
Program robi:
- otwiera koło pod kursorem po przytrzymaniu prawego przycisku myszy; krótkie kliknięcie i przeciąganie prawym
  działają w programach normalnie,
- 12 sektorów co 30° (pierwszy u góry), pierścień zewnętrzny (12 głównych elementów) i wewnętrzny (domyślnie
  komendy edycji: wytnij, kopiuj, wklej, usuń, cofnij, ponów i inne), środek = anuluj / tryb klikania,
- podmenu: po najechaniu wysuwa się łuk do 5 dodatkowych kafelków w tym samym kierunku,
- elementy edytowalne: nazwa, typ akcji (program, folder/plik, adres, skrót klawiszowy, komenda edycji, wklejenie
  tekstu, polecenie, akcja systemowa), cel, ikona, podmenu,
- widok kafelków: nazwy albo ikony; rozmiar koła 60–140% względem DPI monitora; motyw jasny i ciemny,
- edytor w osobnym oknie z podglądem, konfiguracja w pliku JSON,
- lista wykluczonych programów, wyłączenie w pełnym ekranie.

Program nie robi (wersja 1):
- macOS ani Linux,
- synchronizacji konfiguracji w chmurze ani kont,
- wtyczek ani skryptów innych niż polecenia powłoki,
- obsługi gestów dotykowych i pióra.

## Kryteria zamknięcia
- [ ] Cel i zakres opisane w pliku etapu: co robi, czego nie robi
- [ ] Technologie wybrane, powody w sekcji „Decyzje”
- [ ] Kolejne etapy dopisane do roadmapy jako PLANOWANY
- [ ] Szkielet: struktura folderów, `.gitignore`, `LICENSE` (MIT), `cargo build --release` działa, jeden test przechodzi
- [ ] Checklista po polsku w projekcie (`CHECKLISTA.md`)

## Plan
- [x] Koncept z makiety zatwierdzony (układ, wyzwalacz, architektura, widok nazw/ikon, rozmiar, komendy edycji)
- [x] Decyzje wpisane do sekcji „Decyzje”
- [ ] Instalacja narzędzi na LAB: `rustup` (winget Rustlang.Rustup) i Visual Studio 2022 Build Tools z narzędziami C++
      (linker MSVC, Windows SDK); wypisać zainstalowane wersje
- [ ] Szkielet Cargo workspace: `crates/wheel-core` (geometria, trafianie w sektory, model menu),
      `apps/wheel` (program w tle, na razie `--version`), `apps/wheel-editor` (zaczątek edytora);
      `.gitignore`, `LICENSE` (MIT), `README.md` (zwięźle), `CHECKLISTA.md`, makieta `docs/koncept.html`
- [ ] Test w `wheel-core`: punkt na kole trafia we właściwy sektor i pierścień (12 sektorów, środek, poza kołem)
- [x] Etapy 02–08 dopisane do roadmapy jako PLANOWANY
- Sprawdzenie: `cargo build --release`, `cargo test`, `wheel.exe --version` wypisuje wersję

## Decyzje
- Rdzeń w Rust (windows-rs) + Win32 + Direct2D/DirectComposition/DirectWrite: 5–15 MB RAM w tle, otwarcie w jednej
  klatce, brak runtime i pauz GC, bezpieczeństwo pamięci (C++ odrzucony z tego powodu, .NET/WPF i Tauri/Electron przez RAM i start).
- Edytor w Rust + egui jako osobny proces `wheel-editor.exe`: pamięć tylko gdy otwarty, awaria edytora nie dotyka koła.
- Konfiguracja w JSON (serde) w `%APPDATA%\Elemental-Wheel\config.json`: wersja schematu, walidacja, zapis atomowy + `.bak`.
- 12 sektorów jak koło kwintowe; wybór po kierunku (puszczenie poza kołem wybiera sektor w tym kierunku).
- Wyzwalacz: przytrzymanie prawego 180 ms bez ruchu (> 6 px = przeciąganie przekazane dalej); krótkie kliknięcie odtwarzane przez SendInput ze znacznikiem. Progi w konfiguracji.
- Pierścień wewnętrzny domyślnie: komendy edycji (Cofnij po lewej, Ponów po prawej, wytnij/kopiuj/wklej u góry), wysyłane do okna spod kursora; okno koła bez aktywacji (WS_EX_NOACTIVATE), żeby nie zabierać fokusu.
- Widok kafelków: przełącznik nazwy / ikony (prośba użytkownika); ikony programów z .exe, wbudowane dla komend, litera gdy brak ikony.
- Rozmiar koła regulowany w opcjach, 60–140% względem DPI monitora (prośba użytkownika).
- Wersja 1 tylko Windows 10/11.
- Licencja MIT (prośba użytkownika).
- Podglądy i makiety tylko lokalnie (plik HTML otwierany w przeglądarce), bez publikowania na claude.ai.

## Podsumowanie
(wypełnia /etap-zamknij)
