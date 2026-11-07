//! Polish versions of the English messages from `wheel-core` (validation, loading, shortcuts).
//! Known messages are translated; anything else is shown as it came.

/// Translates `msg`; an unknown message is returned unchanged.
pub fn pl(msg: &str) -> String {
    translate(msg).unwrap_or_else(|| msg.to_string())
}

fn translate(msg: &str) -> Option<String> {
    if let Some(t) = known(msg) {
        return Some(t);
    }
    // "<path>: <message>" as produced when loading a file.
    let (head, tail) = msg.split_once(": ")?;
    Some(format!("{head}: {}", translate(tail)?))
}

/// `outer[3]` becomes "pierścień zewnętrzny, sektor 4"; `outer[3].submenu[1]` adds the tile.
fn place(text: &str) -> Option<String> {
    let (ring, rest) = if let Some(r) = text.strip_prefix("outer[") {
        ("pierścień zewnętrzny", r)
    } else {
        ("pierścień wewnętrzny", text.strip_prefix("inner[")?)
    };
    let (index, rest) = rest.split_once(']')?;
    let sector: usize = index.parse().ok()?;
    let mut out = format!("{ring}, sektor {}", sector + 1);
    if let Some(sub) = rest.strip_prefix(".submenu[") {
        let (k, tail) = sub.split_once(']')?;
        if !tail.is_empty() {
            return None;
        }
        out.push_str(&format!(
            ", kafelek podmenu {}",
            k.parse::<usize>().ok()? + 1
        ));
    } else if !rest.is_empty() {
        return None;
    }
    Some(out)
}

fn option_name(name: &str) -> &str {
    match name {
        "scale_pct" => "rozmiar koła",
        "inner_sectors" => "liczba sektorów wewnętrznych",
        "outer_sectors" => "liczba sektorów zewnętrznych",
        "hold_ms" => "czas przytrzymania",
        "move_px" => "ruch",
        other => other,
    }
}

fn known(msg: &str) -> Option<String> {
    if let Some((p, rest)) = msg.split_once(": ")
        && let Some(p) = place(p)
    {
        let what = match rest {
            "empty path" => "pusta ścieżka".to_string(),
            "empty url" => "pusty adres".to_string(),
            "empty command line" => "puste polecenie".to_string(),
            "a submenu item cannot have its own submenu" => {
                "kafelek podmenu nie może mieć własnego podmenu".to_string()
            }
            _ if rest.starts_with("invalid shortcut: ") => {
                format!(
                    "niepoprawny skrót: {}",
                    pl(&rest["invalid shortcut: ".len()..])
                )
            }
            _ if rest.starts_with("submenu has ") => {
                let n = rest["submenu has ".len()..].split(' ').next()?;
                let max = rest.rsplit(' ').next()?;
                format!("podmenu ma {n} kafelków, maksimum to {max}")
            }
            _ => return None,
        };
        return Some(format!("{p}: {what}"));
    }
    if let Some(r) = msg.strip_prefix("unsupported config version ") {
        let (v, expected) = r.split_once(" (expected ")?;
        return Some(format!(
            "nieobsługiwana wersja pliku {v} (oczekiwana {})",
            expected.trim_end_matches(')')
        ));
    }
    for (ring, name) in [("outer", "zewnętrzny"), ("inner", "wewnętrzny")] {
        if let Some(r) = msg.strip_prefix(&format!("{ring} has "))
            && let Some((n, max)) = r.split_once(" items, the maximum is ")
        {
            return Some(format!(
                "pierścień {name} ma {n} elementów, maksimum to {max}"
            ));
        }
    }
    if let Some(r) = msg.strip_prefix("options.")
        && let Some((name, r)) = r.split_once(" = ")
        && let Some((value, r)) = r.split_once(" is outside ")
        && let Some((range, used)) = r.split_once(", using ")
    {
        return Some(format!(
            "opcja „{}” = {value} jest poza zakresem {}, użyto {used}",
            option_name(name),
            range.replace("..=", "–")
        ));
    }
    match msg {
        "the top level must be a JSON object" => {
            return Some("najwyższy poziom pliku musi być obiektem JSON".to_string());
        }
        "config file is not valid UTF-16" => {
            return Some("plik nie jest poprawnym UTF-16".to_string());
        }
        "config file must be UTF-8" => return Some("plik musi być zapisany w UTF-8".to_string()),
        "empty shortcut" => return Some("pusty skrót".to_string()),
        _ => {}
    }
    if let Some(r) = msg.strip_prefix("expected value at line ")
        && let Some((line, col)) = r.split_once(" column ")
    {
        return Some(format!(
            "oczekiwano wartości w wierszu {line}, kolumnie {col}"
        ));
    }
    // Shortcut errors quote the text: `<what> "<a>" in "<b>"`.
    let quoted = |prefix: &str| msg.strip_prefix(prefix);
    if let Some(r) = quoted("empty element in shortcut ") {
        return Some(format!("pusty element w skrócie {r}"));
    }
    if let Some(r) = quoted("more than one key in ") {
        return Some(format!("więcej niż jeden klawisz w {r}"));
    }
    if let Some(r) = quoted("unknown key ") {
        return Some(format!("nieznany klawisz {}", r.replacen(" in ", " w ", 1)));
    }
    if let Some(r) = quoted("repeated modifier ") {
        return Some(format!(
            "powtórzony modyfikator {}",
            r.replacen(" in ", " w ", 1)
        ));
    }
    if let Some(r) = quoted("no key in ") {
        return Some(format!(
            "brak klawisza w {}",
            r.replace("(only modifiers)", "(same modyfikatory)")
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translates_validation_messages() {
        assert_eq!(
            pl("outer[3]: empty path"),
            "pierścień zewnętrzny, sektor 4: pusta ścieżka"
        );
        assert_eq!(
            pl("outer[9].submenu[1]: empty url"),
            "pierścień zewnętrzny, sektor 10, kafelek podmenu 2: pusty adres"
        );
        assert_eq!(
            pl("inner[0]: invalid shortcut: unknown key \"x\" in \"ctrl+x\""),
            "pierścień wewnętrzny, sektor 1: niepoprawny skrót: nieznany klawisz \"x\" w \"ctrl+x\""
        );
        assert_eq!(
            pl("outer[2]: submenu has 7 items, the maximum is 5"),
            "pierścień zewnętrzny, sektor 3: podmenu ma 7 kafelków, maksimum to 5"
        );
        assert_eq!(
            pl("unsupported config version 2 (expected 1)"),
            "nieobsługiwana wersja pliku 2 (oczekiwana 1)"
        );
        assert_eq!(
            pl("options.hold_ms = 5000 is outside 50..=2000, using 2000"),
            "opcja „czas przytrzymania” = 5000 jest poza zakresem 50–2000, użyto 2000"
        );
        assert_eq!(
            pl("options.inner_sectors = 20 is outside 4..=12, using 12"),
            "opcja „liczba sektorów wewnętrznych” = 20 jest poza zakresem 4–12, użyto 12"
        );
        assert_eq!(
            pl("options.outer_sectors = 1 is outside 4..=12, using 4"),
            "opcja „liczba sektorów zewnętrznych” = 1 jest poza zakresem 4–12, użyto 4"
        );
    }

    #[test]
    fn translates_load_errors_with_a_path_prefix() {
        assert_eq!(
            pl("C:\\a\\config.json: outer[0]: empty command line"),
            "C:\\a\\config.json: pierścień zewnętrzny, sektor 1: puste polecenie"
        );
        assert_eq!(
            pl("C:\\a\\config.json: expected value at line 1 column 2"),
            "C:\\a\\config.json: oczekiwano wartości w wierszu 1, kolumnie 2"
        );
    }

    #[test]
    fn unknown_messages_stay_as_they_are() {
        assert_eq!(pl("something odd"), "something odd");
        assert_eq!(pl("C:\\a: Access denied"), "C:\\a: Access denied");
        assert_eq!(pl("outer[x]: empty path"), "outer[x]: empty path");
    }
}
