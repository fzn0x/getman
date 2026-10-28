//! Measuring where a request's time goes.
//!
//! reqwest looks up host names and opens connections inside one shared
//! client, so the hooks it offers (a resolver and a connector layer) are
//! shared too. Each send runs in a task-local scope holding a [`Probe`];
//! the hooks find it there and record into it. A request that reuses an
//! open connection never reaches the hooks, which is how reuse shows.

use std::future::Future;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

tokio::task_local! {
    static PROBE: Arc<Probe>;
}

/// Times recorded while one request was sent.
#[derive(Debug, Default)]
pub(crate) struct Probe {
    dns_nanos: AtomicU64,
    connect_nanos: AtomicU64,
    connects: AtomicU32,
}

impl Probe {
    /// Time spent looking up host names.
    pub(crate) fn dns(&self) -> Duration {
        Duration::from_nanos(self.dns_nanos.load(Ordering::Relaxed))
    }

    /// Time spent opening connections, lookups included.
    pub(crate) fn connect(&self) -> Duration {
        Duration::from_nanos(self.connect_nanos.load(Ordering::Relaxed))
    }

    /// Connections opened over TCP.
    pub(crate) fn connects(&self) -> u32 {
        self.connects.load(Ordering::Relaxed)
    }

    fn add(counter: &AtomicU64, elapsed: Duration) {
        let nanos = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX);
        counter.fetch_add(nanos, Ordering::Relaxed);
    }
}

/// Runs `future` with `probe` receiving its timings.
pub(crate) async fn measure<F: Future>(probe: Arc<Probe>, future: F) -> F::Output {
    PROBE.scope(probe, future).await
}

fn current() -> Option<Arc<Probe>> {
    PROBE.try_with(Arc::clone).ok()
}

/// Looks up host names with the system resolver, as reqwest does by
/// default, and times it.
#[derive(Debug, Default)]
pub(crate) struct TimedResolver;

impl reqwest::dns::Resolve for TimedResolver {
    fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
        let probe = current();
        let host = name.as_str().to_owned();
        Box::pin(async move {
            let started = Instant::now();
            let found = tokio::net::lookup_host((host.as_str(), 0)).await;
            if let Some(probe) = probe {
                Probe::add(&probe.dns_nanos, started.elapsed());
            }
            let addresses: Vec<SocketAddr> = found?.collect();
            Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
        })
    }
}

/// Wraps reqwest's connector to time each new connection: the lookup,
/// TCP, any proxy tunnel and the TLS handshake.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TimedConnectLayer;

impl<S> tower_layer::Layer<S> for TimedConnectLayer {
    type Service = TimedConnect<S>;

    fn layer(&self, inner: S) -> TimedConnect<S> {
        TimedConnect(inner)
    }
}

#[derive(Debug, Clone)]
pub(crate) struct TimedConnect<S>(S);

impl<S, R> tower_service::Service<R> for TimedConnect<S>
where
    S: tower_service::Service<R>,
    S::Future: Send + 'static,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<S::Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.0.poll_ready(cx)
    }

    fn call(&mut self, request: R) -> Self::Future {
        let probe = current();
        let started = Instant::now();
        let connecting = self.0.call(request);
        Box::pin(async move {
            let result = connecting.await;
            if let (Some(probe), Ok(_)) = (probe, &result) {
                Probe::add(&probe.connect_nanos, started.elapsed());
                probe.connects.fetch_add(1, Ordering::Relaxed);
            }
            result
        })
    }
}
