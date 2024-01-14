pub(crate) use async_function_with_timeout::*;
pub(crate) use sync_call::*;
pub(crate) use timeout_without_await::*;
pub(crate) use unneeded_sleep::*;
pub(crate) use zero_sleep_call::*;

mod async_function_with_timeout;
mod sync_call;
mod timeout_without_await;
mod unneeded_sleep;
mod zero_sleep_call;
