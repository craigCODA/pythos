//! Read-only monotonic clock facade derived from the PIT timer tick source.

use crate::architecture::x86_64::timer;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MonotonicTicks(u64);

impl MonotonicTicks {
    pub const fn raw(self) -> u64 {
        self.0
    }
}

pub fn initialize() -> Result<(), ()> {
    let first = monotonic_ticks();
    if first.raw() == 0 {
        return Err(());
    }
    let second = monotonic_ticks();
    if second < first {
        return Err(());
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn monotonic_ticks() -> MonotonicTicks {
    MonotonicTicks(timer::ticks())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monotonic_ticks_are_ordered_by_raw_tick_value() {
        assert!(MonotonicTicks(2) > MonotonicTicks(1));
        assert_eq!(MonotonicTicks(7).raw(), 7);
    }

    #[test]
    fn initialized_clock_requires_observed_timer_tick() {
        let _guard = timer::test_ticks_guard();
        timer::reset_ticks_for_test();

        assert_eq!(initialize(), Err(()));
        timer::handle_timer_interrupt();
        assert_eq!(initialize(), Ok(()));
    }
}
