use std::future::Future;
use std::pin::Pin;
use std::task::{self, Poll};
use std::time::{Duration, Instant};
use std::fmt;

/// Waits until `deadline` is reached.
///
/// No work is performed while awaiting on the delay to complete. The delay
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// # Cancellation
///
/// Canceling a delay is done by dropping the returned future. No additional
/// cleanup work is required.
pub fn sleep_until(deadline: Instant) -> Delay {
    Delay::new_timeout(deadline)
}

/// Waits until `duration` has elapsed.
///
/// Equivalent to `sleep_until(Instant::now() + duration)`. An asynchronous
/// analog to `std::thread::sleep`.
///
/// No work is performed while awaiting on the delay to complete. The delay
/// operates at millisecond granularity and should not be used for tasks that
/// require high-resolution timers.
///
/// To run something regularly on a schedule, see [`interval`].
///
/// # Cancellation
///
/// Canceling a delay is done by dropping the returned future. No additional
/// cleanup work is required.
///
/// # Examples
///
/// Wait 100ms and print "100 ms have elapsed".
///
/// ```
/// use tokio::time::{sleep, Duration};
///
/// #[tokio::main]
/// async fn main() {
///     sleep(Duration::from_millis(100)).await;
///     println!("100 ms have elapsed");
/// }
/// ```
///
/// [`interval`]: crate::time::interval()
pub fn sleep(duration: Duration) -> Delay {
    sleep_until(Instant::now() + duration)
}

/// Future returned by [`sleep`](sleep) and
/// [`sleep_until`](sleep_until).
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Delay {
    /// The link between the `Delay` instance and the timer that drives it.
    ///
    /// This also stores the `deadline` value.
    fut: Pin<Box<dyn Future<Output=()> + Send>>,

    deadline: Instant
}

impl Delay {
    pub(crate) fn new_timeout(deadline: Instant) -> Delay {
        let now = Instant::now();
        match deadline.checked_duration_since(now) {
            Some(dur) => Delay {
                fut: Box::pin(kube::abi::register_delay(dur)),
                deadline
            },
            None => Delay {
                fut: Box::pin(futures::future::ready(())),
                deadline: now
            }
        }
    }

    /// Returns the instant at which the future will complete.
    pub fn deadline(&self) -> Instant {
        self.deadline.clone()
    }

    /// Returns `true` if the `Delay` has elapsed
    ///
    /// A `Delay` is elapsed when the requested duration has elapsed.
    pub fn is_elapsed(&self) -> bool {
        self.deadline.checked_duration_since(Instant::now()).is_none()
    }

    pub fn reset(&mut self, deadline: Instant) {
        *self = Delay::new_timeout(deadline);
    }
}

impl Future for Delay {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Self::Output> {
        self.fut.as_mut().poll(cx)
    }
}

impl fmt::Debug for Delay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Delay with deadline {:?}", self.deadline)
    }
}
