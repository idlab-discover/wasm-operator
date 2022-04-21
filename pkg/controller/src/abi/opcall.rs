use futures::future::maybe_done;
use futures::future::FusedFuture;
use futures::future::MaybeDone;
use futures::ready;
use futures::task::noop_waker;
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;

/// Wrapper around a Future, which causes that Future to be polled immediately.
/// (Background: ops are stored in a `FuturesUnordered` structure which polls
/// them, but without the `OpCall` wrapper this doesn't happen until the next
/// turn of the event loop, which is too late for certain ops.)
pub struct OpCall<T>(MaybeDone<Pin<Box<dyn Future<Output = T> + Send>>>);

impl<T> OpCall<T> {
    /// Wraps a future, and polls the inner future immediately.
    /// This should be the default choice for ops.
    pub fn eager(fut: impl Future<Output = T> + 'static + Send) -> Self {
        let boxed = Box::pin(fut) as Pin<Box<dyn Future<Output = T> + Send>>;
        let mut inner = maybe_done(boxed);
        let waker = noop_waker();
        let mut cx = Context::from_waker(&waker);
        let mut pinned = Pin::new(&mut inner);
        let _ = pinned.as_mut().poll(&mut cx);
        Self(inner)
    }

    #[allow(dead_code)]
    /// Wraps a future; the inner future is polled the usual way (lazily).
    pub fn lazy(fut: impl Future<Output = T> + 'static + Send) -> Self {
        let boxed = Box::pin(fut) as Pin<Box<dyn Future<Output = T> + Send>>;
        let inner = maybe_done(boxed);
        Self(inner)
    }

    #[allow(dead_code)]
    /// Create a future by specifying its output. This is basically the same as
    /// `async { value }` or `futures::future::ready(value)`.
    pub fn ready(value: T) -> Self {
        Self(MaybeDone::Done(value))
    }

    #[allow(dead_code)]
    pub fn gone() -> Self {
        Self(MaybeDone::Gone)
    }
}

impl<T> Future for OpCall<T> {
    type Output = T;

    fn poll(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        let inner = unsafe { &mut self.get_unchecked_mut().0 };
        let mut pinned = Pin::new(inner);
        ready!(pinned.as_mut().poll(cx));
        Poll::Ready(pinned.as_mut().take_output().unwrap())
    }
}

impl<F> FusedFuture for OpCall<F>
where
    F: Future,
{
    fn is_terminated(&self) -> bool {
        self.0.is_terminated()
    }
}
