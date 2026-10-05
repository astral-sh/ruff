#[macro_export]
macro_rules! macro_if {
    (true => $($t:tt)*) => {
        $($t)*
    };

    (false => $($t:tt)*) => {
    };

    (if true { $($t:tt)* } else { $($f:tt)*}) => {
        $($t)*
    };

    (if false { $($t:tt)* } else { $($f:tt)*}) => {
        $($f)*
    };

    (if0 0 { $($t:tt)* } else { $($f:tt)*}) => {
        $($t)*
    };

    (if0 $n:literal { $($t:tt)* } else { $($f:tt)*}) => {
        $($f)*
    };

    (iftt () { $($t:tt)* } else { $($f:tt)*}) => {
        $($f)*
    };

    (iftt ($($tt:tt)+) { $($t:tt)* } else { $($f:tt)*}) => {
        $($t)*
    };
}
