pub(crate) use blocking_http_call::{blocking_http_call, BlockingHttpCallInAsyncFunction};
pub(crate) use blocking_os_call::{blocking_os_call, BlockingOsCallInAsyncFunction};
pub(crate) use open_sleep_or_subprocess_call::{
    open_sleep_or_subprocess_call, OpenSleepOrSubprocessInAsyncFunction,
};

mod blocking_http_call;
mod blocking_os_call;
mod open_sleep_or_subprocess_call;
