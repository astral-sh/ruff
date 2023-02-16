#[cfg(target_pointer_width = "64")]
#[macro_export]
macro_rules! static_assert {
    ($expr:expr) => {
        const _: i32 = 0 / $expr as i32;
    };
}
