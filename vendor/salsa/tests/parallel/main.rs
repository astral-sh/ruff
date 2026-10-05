#![cfg(feature = "inventory")]

mod setup;
mod signal;

mod cancellation_in_fixpoint;
mod cancellation_token_cycle_nested;
mod cancellation_token_multi_blocked;
mod cancellation_token_recomputes;
mod cycle_a_t1_b_t2;
mod cycle_a_t1_b_t2_fallback;
mod cycle_ab_peeping_c;
mod cycle_iteration_mismatch;
mod cycle_merge_after_panic;
mod cycle_nested_deep;
mod cycle_nested_deep_conditional;
mod cycle_nested_deep_conditional_changed;
mod cycle_nested_deep_panic;
mod cycle_nested_three_threads;
mod cycle_nested_three_threads_changed;
mod cycle_panic;
mod cycle_postcommit_event_panic;
mod cycle_provisional_depending_on_itself;
mod lru_eviction_cancels_cycle;
mod memo_table_first_insert;

#[cfg(not(feature = "shuttle"))]
pub(crate) mod sync {
    pub use std::sync::*;
    pub use std::thread;

    pub fn check(f: impl Fn() + Send + Sync + 'static) {
        f();
    }
}

#[cfg(feature = "shuttle")]
pub(crate) mod sync {
    pub use shuttle::sync::*;
    pub use shuttle::thread;

    pub fn check(f: impl Fn() + Send + Sync + 'static) {
        let mut config = shuttle::Config::default();
        config.stack_size = 1024 * 1024;
        let scheduler = shuttle::scheduler::PctScheduler::new(50, 2500);
        shuttle::Runner::new(scheduler, config).run(f);
    }
}

pub(crate) use setup::*;
