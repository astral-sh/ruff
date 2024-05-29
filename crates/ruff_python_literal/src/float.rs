use std::f64;

fn is_integer(v: f64) -> bool {
    (v - v.round()).abs() < f64::EPSILON
}

// TODO: rewrite using format_general
pub fn to_string(value: f64) -> String {
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
