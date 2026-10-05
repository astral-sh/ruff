#[cfg(feature = "accumulator")]
#[macro_export]
macro_rules! gate_accumulated {
    ($($body:tt)*) => {
        $($body)*
    };
}

#[cfg(not(feature = "accumulator"))]
#[macro_export]
macro_rules! gate_accumulated {
    ($($body:tt)*) => {};
}
