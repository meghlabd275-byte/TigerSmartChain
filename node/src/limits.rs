//! Per-address admission limits shared by the node's listeners.
//!
//! The RPC, the WebSocket endpoint and the peer listener are all reachable by
//! anyone who can open a socket, and each of them needs the same two bounds:
//! how fast one address may start work, and how much work one address may hold
//! at once. Keeping one implementation means the three listeners cannot drift
//! apart, and that the table bounding and the poisoned-lock recovery are
//! written once.
//!
//! Both limits are decided under the same call so a refused request never
//! leaves a count behind: the rate is consulted first, then the held count, and
//! only a request that passes both is recorded.

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Mutex;
use std::time::Instant;

/// The largest per-address table before expired entries are dropped.
///
/// A caller with a large address range could otherwise grow the table without
/// limit, which is its own denial of service. The table is only swept once it
/// is this large, so the common case pays nothing.
const MAX_TRACKED_ADDRESSES: usize = 100_000;

/// One address's token bucket.
///
/// `last` is in seconds on the limiter's own clock rather than an `Instant`, so
/// the refill arithmetic is a plain subtraction and the whole thing can be
/// driven by a test clock.
struct Bucket {
    tokens: f64,
    last: f64,
}

/// Where the limiter reads the time from.
///
/// Refill is inherently time-dependent, and a test that sleeps is a test that
/// fails on a loaded machine: at a thousand tokens a second, a one-millisecond
/// pause during the drain loop refills a token and the "bucket is empty"
/// assertion stops holding. Injecting the clock makes refill exactly checkable.
enum Clock {
    /// Seconds since the limiter was created.
    Monotonic(Instant),
    /// Milliseconds since the test began, advanced explicitly by the test.
    #[cfg(test)]
    Manual(std::sync::Arc<std::sync::atomic::AtomicU64>),
}

impl Clock {
    fn now_secs(&self) -> f64 {
        match self {
            Clock::Monotonic(origin) => origin.elapsed().as_secs_f64(),
            #[cfg(test)]
            Clock::Manual(millis) => {
                millis.load(std::sync::atomic::Ordering::SeqCst) as f64 / 1000.0
            }
        }
    }
}

/// A token bucket and a concurrent-work cap, both keyed by client address.
pub struct PerIpLimiter {
    /// Tokens added per second, per address.
    refill_per_second: f64,
    /// The most tokens a bucket may hold, which bounds a burst.
    burst: f64,
    /// The most requests one address may hold at once.
    max_held: usize,
    /// In-flight requests, per address.
    held: Mutex<HashMap<IpAddr, usize>>,
    /// Accept-rate token buckets, per address.
    buckets: Mutex<HashMap<IpAddr, Bucket>>,
    /// The source of time for refill.
    clock: Clock,
}

impl PerIpLimiter {
    /// A limiter that refills `refill_per_second` tokens per second, holds at
    /// most `burst` of them, and lets one address hold `max_held` requests at
    /// once.
    ///
    /// A `refill_per_second` of zero disables both the rate limit and the held
    /// count, which is what an operator who has put the node behind their own
    /// proxy asks for.
    pub fn new(refill_per_second: f64, burst: f64, max_held: usize) -> Self {
        Self {
            refill_per_second,
            burst,
            max_held,
            held: Mutex::new(HashMap::new()),
            buckets: Mutex::new(HashMap::new()),
            clock: Clock::Monotonic(Instant::now()),
        }
    }

    /// A limiter whose clock the test controls, plus a handle to advance it.
    ///
    /// Test-only: production always reads the monotonic clock.
    #[cfg(test)]
    pub fn with_manual_clock(
        refill_per_second: f64,
        burst: f64,
        max_held: usize,
    ) -> (Self, std::sync::Arc<std::sync::atomic::AtomicU64>) {
        let millis = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        (
            Self {
                refill_per_second,
                burst,
                max_held,
                held: Mutex::new(HashMap::new()),
                buckets: Mutex::new(HashMap::new()),
                clock: Clock::Manual(std::sync::Arc::clone(&millis)),
            },
            millis,
        )
    }

    /// Whether a request from `ip` may proceed, recording it if so.
    ///
    /// A caller that is admitted must call [`release`](Self::release) once the
    /// work is done, or its held count will never come down.
    pub fn admit(&self, ip: IpAddr) -> bool {
        if self.refill_per_second <= 0.0 {
            return true;
        }
        let now = self.clock.now_secs();

        let mut buckets = self.buckets.lock().unwrap_or_else(|e| e.into_inner());
        if buckets.len() > MAX_TRACKED_ADDRESSES {
            buckets.retain(|_, b| now - b.last < 60.0);
        }
        let bucket = buckets.entry(ip).or_insert(Bucket {
            tokens: self.burst,
            last: now,
        });
        let elapsed = now - bucket.last;
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_second).min(self.burst);
        bucket.last = now;
        if bucket.tokens < 1.0 {
            return false;
        }

        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        let count = held.entry(ip).or_insert(0);
        if *count >= self.max_held {
            return false;
        }
        *count += 1;
        // Spend the token only once the request is certain to be counted.
        bucket.tokens -= 1.0;
        true
    }

    /// Release a request slot for `ip`.
    pub fn release(&self, ip: IpAddr) {
        if self.refill_per_second <= 0.0 {
            return;
        }
        let mut held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(count) = held.get_mut(&ip) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                held.remove(&ip);
            }
        }
    }

    /// How many requests `ip` currently holds. Test-only.
    #[cfg(test)]
    fn held_by(&self, ip: IpAddr) -> usize {
        let held = self.held.lock().unwrap_or_else(|e| e.into_inner());
        held.get(&ip).copied().unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn a_burst_is_allowed_and_the_next_request_is_refused() {
        let (limiter, _clock) = PerIpLimiter::with_manual_clock(5.0, 5.0, 100);
        let addr = ip("203.0.113.7");
        for i in 0..5 {
            assert!(limiter.admit(addr), "request {i} within the burst must pass");
        }
        assert!(!limiter.admit(addr), "the request past the burst must be refused");
    }

    #[test]
    fn a_zero_rate_disables_the_limit() {
        let (limiter, _clock) = PerIpLimiter::with_manual_clock(0.0, 0.0, 1);
        let addr = ip("203.0.113.8");
        for _ in 0..1000 {
            assert!(limiter.admit(addr));
        }
    }

    #[test]
    fn buckets_refill_over_time() {
        let (limiter, clock) = PerIpLimiter::with_manual_clock(50.0, 10.0, 100_000);
        let addr = ip("203.0.113.9");
        for i in 0..10 {
            assert!(limiter.admit(addr), "request {i} within the bucket must pass");
        }
        assert!(!limiter.admit(addr), "the bucket must be empty");

        // Twenty milliseconds at fifty tokens a second is exactly one token.
        clock.fetch_add(20, Ordering::SeqCst);
        assert!(limiter.admit(addr), "the bucket must refill with time");
        assert!(
            !limiter.admit(addr),
            "one token buys exactly one request, so the bucket is empty again"
        );

        // The refill is bounded by the burst: a long idle period must not bank
        // more than the burst allows.
        clock.fetch_add(1_000_000, Ordering::SeqCst);
        for i in 0..10 {
            assert!(limiter.admit(addr), "burst request {i} must pass");
        }
        assert!(!limiter.admit(addr), "the burst must not exceed ten tokens");
    }

    #[test]
    fn addresses_do_not_share_a_budget() {
        let (limiter, _clock) = PerIpLimiter::with_manual_clock(1.0, 1.0, 100);
        let attacker = ip("192.0.2.10");
        let honest = ip("192.0.2.11");
        assert!(limiter.admit(attacker));
        assert!(!limiter.admit(attacker));
        assert!(
            limiter.admit(honest),
            "one address exhausting its budget must not deny another"
        );
    }

    #[test]
    fn the_held_count_caps_concurrent_work_and_release_frees_it() {
        // A generous rate, so only the held count can refuse.
        let (limiter, _clock) = PerIpLimiter::with_manual_clock(1_000_000.0, 1_000_000.0, 3);
        let addr = ip("198.51.100.5");
        for i in 0..3 {
            assert!(limiter.admit(addr), "slot {i} must be available");
        }
        assert!(!limiter.admit(addr), "the slot past the cap must be refused");
        assert_eq!(limiter.held_by(addr), 3);

        limiter.release(addr);
        assert!(limiter.admit(addr), "a released slot must be reusable");
    }

    #[test]
    fn a_refused_request_does_not_leave_a_held_count_behind() {
        let (limiter, _clock) = PerIpLimiter::with_manual_clock(1.0, 1.0, 10);
        let addr = ip("198.51.100.6");
        assert!(limiter.admit(addr));
        assert!(!limiter.admit(addr));
        // Only the admitted request is held, so the count is one rather than
        // two; otherwise refusals would accumulate and lock the address out.
        assert_eq!(limiter.held_by(addr), 1);
    }
}
