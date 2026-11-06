# Etap 03: wyzwalacz-ppm

## Cel
Program w tle otwiera koło pod kursorem po przytrzymaniu prawego przycisku (180 ms bez ruchu); krótkie kliknięcie
i przeciąganie prawym działają w programach normalnie. Puszczenie wybiera sektor po kierunku, puszczenie w środku
zostawia koło otwarte do wyboru lewym, Esc albo klik poza kołem zamyka. Akcje w etapie 04: tu wybór jest tylko wypisywany.

## Kryteria zamknięcia
- [ ] `wheel.exe --run` zakłada globalny hak myszy (`WH_MOUSE_LL`); działa jedna kopia, `wheel.exe --quit` zamyka działającą
- [ ] Przytrzymanie prawego 180 ms bez ruchu otwiera koło ze środkiem pod kursorem; puszczenie wybiera sektor po kierunku i wypisuje go (pierścień, numer, nazwa); menu kontekstowe okna pod spodem się nie pokazuje
- [ ] Krótkie kliknięcie prawym odtwarzane przez SendInput ze znacznikiem działa normalnie (menu kontekstowe w Notatniku i Eksploratorze)
- [ ] Ruch > 6 px przed 180 ms przekazuje przeciąganie dalej (przeciąganie prawym w Eksploratorze daje menu „Kopiuj/Przenieś tutaj”)
- [ ] Puszczenie w środku zostawia koło otwarte; lewy przycisk wybiera, Esc albo klik poza kołem zamyka
- [ ] Okno koła przepuszcza kliknięcia (`WS_EX_TRANSPARENT`, mysz obsługuje tylko hak): zamyka otwarte sprawy z etapu 02 (narożniki okna, puszczenie trafiające do okna pod spodem)
- [ ] Okno administratora pod kursorem (UIPI, program bez uprawnień administratora): prawy przycisk przechodzi bez zmian
- [ ] Strażnik haka: porównuje ostatnie wejście systemu (`GetLastInputInfo`) z ostatnim zdarzeniem haka i zakłada hak ponownie
- [ ] Progi 180 ms / 6 px w strukturze ustawień (`wheel-core`), na razie z argumentów `--hold-ms`, `--move-px` (zapis w konfiguracji: etap 04)
- [ ] Testy automatu stanów w `wheel-core`: kliknięcie, przytrzymanie, przeciąganie, ruch na samym progu, puszczenie w środku i poza kołem, Esc, zdarzenia własne i obce wstrzyknięte
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings` i `cargo fmt --check` czyste
- [ ] Sprawdzenie ręczne na LAB przez użytkownika: kliknięcie, przeciąganie, przytrzymanie, środek, Esc
- [ ] `CHECKLISTA.md` i `README.md` zaktualizowane

## Plan
- [x] `crates/wheel-core/src/trigger.rs`: automat stanów bez Win32 (wejście: zdarzenie myszy + czas; wyjście: przepuść / połknij / odtwórz kliknięcie / otwórz koło / wybierz / anuluj), `TriggerSettings` (próg czasu i ruchu), testy
- [ ] `apps/wheel/src/hook.rs`: hak `WH_MOUSE_LL` + klawiatury (Esc tylko przy otwartym kole); w haku tylko krok automatu i komunikat do wątku okna; własne zdarzenia po `dwExtraInfo`, obce wstrzyknięte przepuszczane; odtwarzanie przez SendInput; wykrycie okna administratora pod kursorem
- [ ] `apps/wheel/src/window.rs`: `WS_EX_TRANSPARENT`, otwieranie i zamykanie na komunikat z haka (bez limitu 10 s w `--run`), tryb klikania po puszczeniu w środku
- [ ] `apps/wheel/src/watchdog.rs`: strażnik haka (timer co kilka sekund)
- [ ] `apps/wheel/src/args.rs`, `main.rs`: `--run`, `--quit`, `--hold-ms`, `--move-px`, jedna kopia (mutex); `--demo` bez zmian
- [ ] `CHECKLISTA.md` (punkty 03, ikona w zasobniku w 08), `README.md` (status, uruchomienie w tle)
- Sprawdzenie: tester: build, testy, clippy, fmt; skrypt PowerShell z ukrytą flagą testową (wstrzyknięte zdarzenia traktowane jak fizyczne) sprawdza kliknięcie (menu kontekstowe Notatnika), przytrzymanie (koło otwarte, wybór wypisany, brak menu) i przeciąganie, zrzuty w scratchpadzie; potem użytkownik fizyczną myszą

## Decyzje
- Lista wykluczonych programów i wyłączenie w pełnym ekranie w etapie 04 (wymagają pliku konfiguracji); w 03 automatycznie przepuszczane tylko okna administratora, bo bez tego prawy przycisk przestałby w nich działać.
- Układ koła podawany automatowi przez `set_layout` przy otwarciu (automat bez Win32, DPI liczy program); zdarzenia wstrzyknięte (własne i obce) zawsze przepuszczane bez zmiany stanu.
- Hak w osobnym wątku, okno i rysowanie w głównym: rysowanie nie może opóźniać haka (limit czasu Windows); efekty (SendInput, otwarcie koła) wykonywane poza wywołaniem haka.
- Odtworzenie przeciągania: ruch bezwzględny do punktu wciśnięcia, wciśnięcie, powrót do bieżącej pozycji, żeby przeciąganie zaczynało się od elementu pod punktem wciśnięcia.
- Ikona w zasobniku systemowym dopisana do etapu 08 (decyzja użytkownika); do tego czasu wyjście przez `wheel.exe --quit`.

## Podsumowanie
(wypełnia /etap-zamknij)
