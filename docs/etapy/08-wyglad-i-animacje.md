# Etap 08: wyglad-i-animacje

## Cel
Dopracowany wygląd koła, krótkie animacje (otwarcie, zamknięcie, podświetlenie, łuk podmenu) i usunięcie opóźnienia
pierwszego otwarcia po zmianie konfiguracji, bez zmiany działania.

## Kryteria zamknięcia
- [ ] Kafelki nieprzezroczyste i czytelne na każdym tle (dziś przez pierścień wewnętrzny prześwituje tło); delikatny cień koła, podświetlenie w kolorze akcentu, motyw jasny i ciemny
- [ ] Minimalna bezwzględna wielkość czcionki (przy 60–75% nie schodzi do ok. 5 px)
- [ ] Animacje: otwarcie (zanikanie + lekkie skalowanie, ok. 120 ms), zamknięcie (zanikanie, ok. 80 ms), płynne podświetlenie, wysunięcie łuku podmenu; akcja startuje od razu, zamknięcie jej nie opóźnia
- [ ] Animacje wyłączane opcją `animations` (konfiguracja i edytor) i zgodne z ustawieniem Windows „Pokazuj animacje”
- [ ] Renderer i ikony przygotowane po wczytaniu konfiguracji: otwarcie po zmianie konfiguracji poniżej 16 ms (dziś 89 ms); układy tekstu nie tworzone przy każdym odrysowaniu
- [ ] Pamięć prywatna w widoku ikon do 15 MB (dziś 17 MB); mediana otwarcia nadal do 16 ms; wyniki w pliku etapu
- [ ] Podgląd w edytorze zgodny z nowym wyglądem koła
- [ ] Wygląd zatwierdzony przez użytkownika na zrzutach (jasny/ciemny, nazwy/ikony, łuk podmenu)
- [ ] `cargo build --release`, `cargo test`, clippy `-D warnings`, `cargo fmt --check` czyste; README i CHECKLISTA zaktualizowane

## Plan
- [ ] `apps/wheel/src/render.rs`, `theme.rs`: nieprzezroczyste wypełnienia, cień, akcent, minimalna czcionka; zrzuty do akceptacji przed animacjami
- [ ] `apps/wheel/src/window.rs`, `render.rs`: animacje otwarcia i zamknięcia przez DirectComposition (przezroczystość i skala, bez ponownego rysowania klatek), podświetlenie i łuk przez interpolację na istniejącym zegarze 16 ms
- [ ] `crates/wheel-core/src/config.rs`, `apps/wheel-editor/src/options.rs`: opcja `animations` (domyślnie włączona), testy
- [ ] `apps/wheel/src/window.rs`, `render.rs`, `icons.rs`: renderer i ikony po wczytaniu konfiguracji, pamięć podręczna układów tekstu, mniejsze bitmapy ikon
- [ ] `apps/wheel-editor/src/preview.rs`: ten sam wygląd
- [ ] Pomiary `tools/soak.ps1` (wejście wstrzykiwane: najpierw uprzedzenie i czekanie na zgodę użytkownika)
- [ ] README, CHECKLISTA
- Sprawdzenie: tester: build, testy, clippy, fmt; zrzuty z `--demo` z konfiguracją w scratchpadzie (`%APPDATA%` nietknięty); pomiary `soak.ps1`

## Decyzje
- Kolejność etapów zamieniona (decyzja użytkownika): 08 wyglad-i-animacje, 09 instalator-i-wydanie, żeby jedno wydanie 1.0.0 na końcu miało gotowy wygląd.
- Wydanie: Inno Setup (instalacja dla jednego użytkownika) i wersja 1.0.0 (decyzje użytkownika, do etapu 09).
- Poza zakresem: znany błąd po Escape (decyzja z etapu 07), zasobnik i tryb bez konsoli (etap 09).

## Podsumowanie
(wypełnia /etap-zamknij)
