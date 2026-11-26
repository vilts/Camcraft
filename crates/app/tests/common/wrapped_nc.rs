//! Compare these wrapped-A fixtures across platforms without weakening XYZ,
//! modal, or structural checks. Geometry can land on opposite sides of an A
//! rounding tie: allow one 0.001-degree step, and only the resulting change
//! to an A-rate-limited G93 F word. All other text remains exact.

fn millis(word: &str) -> Option<i64> {
    let value = word.get(1..)?;
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits = fraction.len() as u32;
    let negative = whole.starts_with('-');
    let whole: i64 = whole.parse().ok()?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse().ok()?
    };
    Some(whole * 1000 + if negative { -fraction } else { fraction } * 10_i64.pow(3 - digits))
}

pub fn compare(actual: &str, expected: &str) -> Result<(), String> {
    let actual: Vec<_> = actual.lines().collect();
    let expected: Vec<_> = expected.lines().collect();
    if actual.len() != expected.len() {
        return Err(format!(
            "NC line count: {} != {}",
            actual.len(),
            expected.len()
        ));
    }
    let (mut actual_a, mut expected_a) = (None::<i64>, None::<i64>);
    let mut inverse_time = false;
    for (index, (actual, expected)) in actual.iter().zip(&expected).enumerate() {
        let actual_words: Vec<_> = actual.split(' ').collect();
        let expected_words: Vec<_> = expected.split(' ').collect();
        let mismatch = || format!("NC line {}: {actual:?} != {expected:?}", index + 1);
        if actual_words.len() != expected_words.len() {
            return Err(mismatch());
        }
        // Modes themselves must compare exactly below.
        if expected_words.contains(&"G93") {
            inverse_time = true;
        }
        if expected_words.contains(&"G94") {
            inverse_time = false;
        }
        let next_actual_a = actual_words
            .iter()
            .find(|w| w.starts_with('A'))
            .and_then(|w| millis(w));
        let next_expected_a = expected_words
            .iter()
            .find(|w| w.starts_with('A'))
            .and_then(|w| millis(w));
        let actual_turn = actual_a.zip(next_actual_a).map(|(a, b)| (a - b).abs());
        let expected_turn = expected_a.zip(next_expected_a).map(|(a, b)| (a - b).abs());
        for (a, e) in actual_words.iter().zip(&expected_words) {
            if a == e {
                continue;
            }
            match (
                a.as_bytes().first(),
                e.as_bytes().first(),
                millis(a),
                millis(e),
            ) {
                (Some(b'A'), Some(b'A'), Some(a), Some(e)) if (a - e).abs() <= 1 => {}
                (Some(b'F'), Some(b'F'), Some(a), Some(e))
                    if inverse_time
                        && expected_words.contains(&"G1")
                        && actual_turn != expected_turn
                        && actual_turn.is_some_and(|d| d > 0 && a == 360_000_000 / d)
                        && expected_turn.is_some_and(|d| d > 0 && e == 360_000_000 / d) => {}
                _ => return Err(mismatch()),
            }
        }
        actual_a = next_actual_a.or(actual_a);
        expected_a = next_expected_a.or(expected_a);
    }
    Ok(())
}

#[test]
fn permits_one_a_rounding_step_and_its_exact_rate_limited_feed() {
    let expected =
        "G0 A48.097\nG93 G1 X0.0625 A46.548 F232.408\nG1 X0.0625 A45 F232.558\nG94\nM30\n";
    let actual = "G0 A48.097\nG93 G1 X0.0625 A46.549 F232.558\nG1 X0.0625 A45 F232.408\nG94\nM30\n";
    assert_eq!(compare(actual, expected), Ok(()));
    assert_eq!(compare("G0 A-43.171\n", "G0 A-43.172\n"), Ok(()));
}

#[test]
fn rejects_larger_a_changes_and_unrelated_nc_changes() {
    let expected = "G0 A48.097\nG93 G1 X0.0625 A46.548 F232.408\nG94\nM30\n";
    for actual in [
        expected.replace("A46.548", "A46.550"),
        expected.replace("X0.0625", "X0.0626"),
        expected.replace("G93", "G94"),
        expected.replace("G1 ", "G0 "),
        expected.replace("F232.408", "F232.409"),
        expected.replace("A46.548 F232.408", "A46.549 F232.559"),
        expected.replace(" A46.548", ""),
        expected.replace("M30\n", ""),
        expected.replace("M30", "M3"),
    ] {
        assert!(compare(&actual, expected).is_err(), "accepted {actual:?}");
    }
}
