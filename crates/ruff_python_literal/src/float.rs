use std::f64;

fn is_integer(v: f64) -> bool {
    (v - v.round()).abs() < f64::EPSILON
}

/// Format a float value to match Python's `repr()` output.
///
/// Python uses the following rules for float repr:
/// - Special values: `inf`, `-inf`, `nan`
/// - If the exponent is in range `-5 <= exp < 16`, use fixed-point notation
///   - Integers get `.0` suffix (e.g., `1.0`, `100.0`)
///   - Non-integers use shortest representation (e.g., `0.1`, `1.5`)
/// - Otherwise, use scientific notation with `e+XX` / `e-XX` format
pub fn to_string(value: f64) -> String {
    if value.is_nan() {
        return "nan".to_string();
    }
    if value.is_infinite() {
        return if value.is_sign_positive() {
            "inf".to_string()
        } else {
            "-inf".to_string()
        };
    }

    let lit = format!("{value:e}");
    if let Some(position) = lit.find('e') {
        let significand = &lit[..position];
        let exponent = &lit[position + 1..];
        let exponent = exponent.parse::<i32>().unwrap();
        if exponent < 16 && exponent > -5 {
            if is_integer(value) {
                format!("{value:.1?}")
            } else {
                value.to_string()
            }
        } else {
            format!("{significand}e{exponent:+#03}")
        }
    } else {
        let mut s = value.to_string();
        s.make_ascii_lowercase();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero() {
        assert_eq!(to_string(0.0), "0.0");
    }

    #[test]
    fn test_negative_zero() {
        assert_eq!(to_string(-0.0), "-0.0");
    }

    #[test]
    fn test_positive_integers() {
        assert_eq!(to_string(1.0), "1.0");
        assert_eq!(to_string(100.0), "100.0");
        assert_eq!(to_string(1000.0), "1000.0");
    }

    #[test]
    fn test_negative_integers() {
        assert_eq!(to_string(-1.0), "-1.0");
        assert_eq!(to_string(-100.0), "-100.0");
    }

    #[test]
    fn test_fractional_values() {
        assert_eq!(to_string(0.1), "0.1");
        assert_eq!(to_string(1.5), "1.5");
        assert_eq!(to_string(0.001), "0.001");
        assert_eq!(to_string(0.0001), "0.0001");
    }

    #[test]
    fn test_scientific_notation_large() {
        // Exponent >= 16 should use scientific notation
        assert_eq!(to_string(1e16), "1e+16");
        assert_eq!(to_string(1e20), "1e+20");
        assert_eq!(to_string(1e308), "1e+308");
    }

    #[test]
    fn test_scientific_notation_small() {
        // Exponent <= -5 should use scientific notation
        assert_eq!(to_string(1e-5), "1e-05");
        assert_eq!(to_string(1e-10), "1e-10");
    }

    #[test]
    fn test_boundary_exponents() {
        // exponent == 15 (< 16) => fixed point
        assert_eq!(to_string(1e15), "1000000000000000.0");
        // exponent == -4 (> -5) => fixed point
        assert_eq!(to_string(1e-4), "0.0001");
    }

    #[test]
    fn test_special_values() {
        assert_eq!(to_string(f64::INFINITY), "inf");
        assert_eq!(to_string(f64::NEG_INFINITY), "-inf");
        assert_eq!(to_string(f64::NAN), "nan");
    }

    #[test]
    fn test_is_integer() {
        assert!(is_integer(1.0));
        assert!(is_integer(0.0));
        assert!(is_integer(-5.0));
        assert!(!is_integer(1.5));
        assert!(!is_integer(0.1));
    }

    #[test]
    fn test_smallest_positive() {
        // Smallest positive f64
        let result = to_string(5e-324);
        assert_eq!(result, "5e-324");
    }

    #[test]
    fn test_negative_scientific() {
        assert_eq!(to_string(-1e20), "-1e+20");
        assert_eq!(to_string(-1e-10), "-1e-10");
    }
}
