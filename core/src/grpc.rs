//! gRPC Protocol Adapter for High-Throughput Event Streaming (Issue #583).
//!
//! Exposes `EventStreamService` — a server-side streaming gRPC service built
//! on [`tonic`] that pushes Soroban contract events over HTTP/2 with Protobuf
//! framing.  This eliminates the latency and serialization overhead of HTTP
//! JSON polling for enterprise indexing consumers.
//!
//! # Architecture
//!
//! ```text
//!  ┌─────────────────────┐       gRPC / HTTP-2        ┌──────────────────┐
//!  │  Enterprise Indexer  │ ◄────────────────────────  │ EventStreamService│
//!  │  (tonic client / any │       Protobuf frames       │                  │
//!  │   gRPC client)       │                             │  SimulationBus   │
//!  └─────────────────────┘                             │  (broadcast rx)  │
//!                                                      └──────────────────┘
//! ```
//!
//! The service subscribes to the existing [`crate::ws::SimulationBus`]
//! broadcast channel.  Inbound `Completed` events are translated into
//! [`ContractEvent`] Protobuf messages and pushed to connected subscribers.
//!
//! # Running the gRPC server
//!
//! The gRPC endpoint is bound on a separate port (default `50051`) so it does
//! not conflict with the existing HTTP/REST server:
//!
//! ```bash
//! GRPC_PORT=50051 RUST_LOG=info cargo run -p Sky Moon Scope-core
//! ```
//!
//! # Client example (grpcurl)
//!
//! ```bash
//! grpcurl -plaintext -d '{"contract_id":"CDLZFC3SYJYDZT7K67VZ75HPJVIEUVNIXF47ZG2FB2RMQQVU2HHGCYSC"}' \
//!   localhost:50051 Sky Moon Scope.events.v1.EventStreamService/StreamContractEvents
//! ```

use std::fs;
use std::io::{self, BufReader};
use std::path::Path;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rustls_pemfile::certs as read_cert_chain;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::Stream;
use tonic::transport::{Certificate, Identity, ServerTlsConfig};
use tonic::{Request, Response, Status};
use x509_parser::prelude::{FromDer, X509Certificate};

use crate::ws::{SimulationBus, SimulationEvent};

// Include the tonic-generated code from the compiled proto.
pub mod proto {
    tonic::include_proto!("soroscope.events.v1");
}

use proto::event_stream_service_server::EventStreamService;
pub use proto::event_stream_service_server::EventStreamServiceServer;
use proto::{ContractEvent, StreamContractEventsRequest};

// ── Service implementation ────────────────────────────────────────────────────

/// Default maximum number of concurrently connected gRPC streaming clients.
///
/// Each connection holds a broadcast receiver and a background Tokio task.
/// Without a limit a burst of connections exhausts file descriptors and task
/// memory (issue #579).  Operators can override this via
/// [`EventStreamServiceImpl::with_max_clients`].
pub const DEFAULT_MAX_GRPC_CLIENTS: usize = 1_000;

/// Maximum number of translated events buffered per gRPC connection before the
/// connection is treated as an overloaded (slow) consumer and disconnected.
///
/// Each connection decouples its shared broadcast subscription from the
/// outbound gRPC stream with a queue of this size.  Once a consumer lets the
/// queue fill, the producer stops pulling from the bus and the stream is closed
/// with an explicit overload status — a slow consumer can therefore never stall
/// the broadcast channel for other subscribers (Issue #066).
pub const GRPC_STREAM_QUEUE_CAPACITY: usize = 512;

/// gRPC service implementation.  Holds a reference to the shared
/// [`SimulationBus`] so it can fan out events to every connected gRPC client.
pub struct EventStreamServiceImpl {
    bus: Arc<SimulationBus>,
    /// Live count of connected streaming clients.
    connected_clients: Arc<AtomicUsize>,
    /// Maximum number of clients allowed to stream concurrently.
    max_clients: usize,
}

impl EventStreamServiceImpl {
    /// Create a new service instance backed by the given bus.
    pub fn new(bus: Arc<SimulationBus>) -> Self {
        Self {
            bus,
            connected_clients: Arc::new(AtomicUsize::new(0)),
            max_clients: DEFAULT_MAX_GRPC_CLIENTS,
        }
    }

    /// Override the maximum number of concurrently connected streaming clients.
    pub fn with_max_clients(mut self, max_clients: usize) -> Self {
        self.max_clients = max_clients.max(1);
        self
    }

    /// Current number of connected streaming clients.
    pub fn connected_clients(&self) -> usize {
        self.connected_clients.load(Ordering::Relaxed)
    }
}

/// Type alias for the streaming response used by the trait impl.
type EventStream = Pin<Box<dyn Stream<Item = Result<ContractEvent, Status>> + Send + 'static>>;

#[tonic::async_trait]
impl EventStreamService for EventStreamServiceImpl {
    type StreamContractEventsStream = EventStream;

    /// Open a server-side stream of [`ContractEvent`] messages.
    ///
    /// The server subscribes to the internal broadcast bus and translates each
    /// [`SimulationEvent::Completed`] into a [`ContractEvent`].  The stream
    /// remains open until the client disconnects or the server shuts down.
    ///
    /// Returns `RESOURCE_EXHAUSTED` when the number of connected clients
    /// already equals `max_clients` (default 1 000).  This prevents file
    /// descriptor and task exhaustion under a burst of connections (issue #579).
    ///
    /// Each connection buffers translated events in a bounded queue
    /// ([`GRPC_STREAM_QUEUE_CAPACITY`]).  If a slow consumer lets that queue
    /// overflow, the stream terminates with a `RESOURCE_EXHAUSTED` overload
    /// status instead of stalling the shared broadcast channel (Issue #066).
    async fn stream_contract_events(
        &self,
        request: Request<StreamContractEventsRequest>,
    ) -> Result<Response<Self::StreamContractEventsStream>, Status> {
        // Enforce the client limit before subscribing to the bus.
        // fetch_add is used rather than compare-and-swap so the counter
        // accurately reflects the peak even under races; the check below
        // corrects any over-admission and decrements back.
        let previous = self
            .connected_clients
            .fetch_add(1, Ordering::Relaxed);
        if previous >= self.max_clients {
            self.connected_clients.fetch_sub(1, Ordering::Relaxed);
            return Err(Status::resource_exhausted(format!(
                "maximum connected clients ({}) reached; try again later",
                self.max_clients,
            )));
        }

        let params = request.into_inner();
        let contract_id_filter = params.contract_id.clone();
        let event_types_filter: Vec<String> = params
            .event_types_filter
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect();

        tracing::info!(
            contract_id = %contract_id_filter,
            start_ledger = params.start_ledger,
            event_types = ?event_types_filter,
            connected_clients = previous + 1,
            max_clients = self.max_clients,
            "gRPC client connected to EventStreamService"
        );

        let mut receiver = self.bus.subscribe();

        // Decouple the shared broadcast subscription from the outbound gRPC
        // stream with a per-connection bounded queue (Issue #066).  A background
        // task drains the broadcast receiver into the queue; the response stream
        // reads from the queue.  When a slow consumer lets the queue fill, the
        // task flags the connection as overloaded and stops pulling from the bus
        // rather than stalling the broadcast channel; the stream then surfaces an
        // explicit overload status and closes.
        let (tx, rx) = mpsc::channel::<ContractEvent>(GRPC_STREAM_QUEUE_CAPACITY);
        let overloaded = Arc::new(AtomicBool::new(false));

        let contract_filter = contract_id_filter.clone();
        let overload_flag = Arc::clone(&overloaded);
        tokio::spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(traced_msg) => {
                        if let Some(event) =
                            translate_event(traced_msg.payload, &contract_filter, &event_types_filter)
                        {
                            match tx.try_send(event) {
                                Ok(()) => {}
                                Err(mpsc::error::TrySendError::Full(_)) => {
                                    // Slow consumer: signal overload and stop
                                    // feeding this connection so the broadcast
                                    // channel is never blocked.
                                    overload_flag.store(true, Ordering::SeqCst);
                                    break;
                                }
                                // Consumer already disconnected; nothing to do.
                                Err(mpsc::error::TrySendError::Closed(_)) => break,
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(skipped)) => {
                        tracing::warn!(
                            skipped = skipped,
                            "gRPC stream lagged; some events were dropped"
                        );
                        // Send explicit lagged notification to the client before closing
                        if let Some(lagged_event) = translate_lagged_event(skipped) {
                            if let Err(e) = tx.try_send(lagged_event) {
                                tracing::error!(
                                    error = ?e,
                                    "failed to send lagged event to gRPC stream"
                                );
                            }
                        }
                        // After notifying about the lag, stop consuming from the bus
                        // so the stream can close gracefully
                        break;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });

        // Clone the counter so the stream task can decrement it on drop.
        let client_counter = Arc::clone(&self.connected_clients);

        // Wrap the stream so the client counter is decremented when the stream
        // is dropped (i.e. the client disconnects or the server shuts down).
        let guarded = ClientCountGuard {
            inner: Box::pin(BoundedEventStream {
                rx,
                overloaded,
                finished: false,
            }),
            counter: client_counter,
        };

        Ok(Response::new(Box::pin(guarded)))
    }
}

// ── Client-count guard stream ─────────────────────────────────────────────────

/// Wraps an event stream and decrements the connected-client counter when the
/// stream is dropped.
struct ClientCountGuard {
    inner: Pin<Box<dyn Stream<Item = Result<ContractEvent, Status>> + Send + 'static>>,
    counter: Arc<AtomicUsize>,
}

impl Stream for ClientCountGuard {
    type Item = Result<ContractEvent, Status>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl Drop for ClientCountGuard {
    fn drop(&mut self) {
        // Saturating so a logic error cannot wrap the counter below zero.
        let prev = self.counter.fetch_sub(1, Ordering::Relaxed);
        if prev == 0 {
            // Guard against wrapping; restore to 0.
            self.counter.store(0, Ordering::Relaxed);
        }
    }
}

// ── Bounded per-connection queue stream ───────────────────────────────────────

/// Stream that drains the per-connection bounded event queue.
///
/// The matching producer task pulls from the shared [`SimulationBus`] broadcast
/// channel into this queue.  When the queue overflows because the consumer is
/// too slow, the producer flags the connection and stops consuming, and this
/// stream ends with an explicit overload [`Status`] instead of closing
/// silently.  This keeps a slow consumer from stalling the broadcast channel for
/// other subscribers (Issue #066).
struct BoundedEventStream {
    /// Receives translated events from the per-connection producer task.
    rx: mpsc::Receiver<ContractEvent>,
    /// Set by the producer task when the queue overflowed.
    overloaded: Arc<AtomicBool>,
    /// Whether the terminal item (error or clean end) has been emitted.
    finished: bool,
}

impl Stream for BoundedEventStream {
    type Item = Result<ContractEvent, Status>;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        // `BoundedEventStream` is `Unpin` (all fields are `Unpin`).
        let this = self.get_mut();
        if this.finished {
            return std::task::Poll::Ready(None);
        }
        match this.rx.poll_recv(cx) {
            std::task::Poll::Ready(Some(event)) => std::task::Poll::Ready(Some(Ok(event))),
            std::task::Poll::Ready(None) => {
                this.finished = true;
                if this.overloaded.load(Ordering::SeqCst) {
                    // Explicit overload signal for the slow consumer before the
                    // stream closes.
                    std::task::Poll::Ready(Some(Err(consumer_overloaded_status())))
                } else {
                    std::task::Poll::Ready(None)
                }
            }
            std::task::Poll::Pending => std::task::Poll::Pending,
        }
    }
}

/// Status returned to a slow consumer whose per-connection queue overflowed.
fn consumer_overloaded_status() -> Status {
    Status::resource_exhausted(format!(
        "consumer too slow: outbound event queue ({} events) overflowed; stream closed",
        GRPC_STREAM_QUEUE_CAPACITY
    ))
}

// ── Translation helpers ───────────────────────────────────────────────────────

/// Convert a [`SimulationEvent`] to a [`ContractEvent`] Protobuf message.
///
/// Returns `None` when the event does not match the caller's filters or is not
/// a contract-event-bearing variant.
fn translate_event(
    event: SimulationEvent,
    contract_id_filter: &str,
    event_types_filter: &[String],
) -> Option<ContractEvent> {
    // Only `Completed` events carry contract resource data that maps to an
    // indexable contract event.  Other bus events (progress, failover, etc.)
    // are internal telemetry and are not exposed over gRPC.
    let (job_id, data, timestamp) = match event {
        SimulationEvent::Completed {
            job_id,
            data,
            timestamp,
        } => (job_id, data, timestamp),
        _ => return None,
    };

    // Apply contract ID filter — empty filter means "all contracts".
    if !contract_id_filter.is_empty() && !job_id.contains(contract_id_filter) {
        return None;
    }

    let event_type = "simulation_completed".to_string();

    // Apply event-type filter — empty filter means "all types".
    if !event_types_filter.is_empty()
        && !event_types_filter
            .iter()
            .any(|f| event_type.contains(f.as_str()))
    {
        return None;
    }

    let topics_json = serde_json::json!([
        {"type": "symbol", "value": "simulation"},
        {"type": "symbol", "value": &event_type}
    ])
    .to_string();

    let value_json = serde_json::json!({
        "cpu_instructions": data.cpu_instructions,
        "ram_bytes": data.ram_bytes,
        "ledger_read_bytes": data.ledger_read_bytes,
        "ledger_write_bytes": data.ledger_write_bytes,
        "transaction_size_bytes": data.transaction_size_bytes,
        "cost_stroops": data.cost_stroops,
    })
    .to_string();

    Some(ContractEvent {
        ledger_sequence: 0, // populated by the indexer pipeline in future
        ledger_close_time: timestamp.timestamp(),
        contract_id: job_id,
        event_type,
        topics_json,
        value_json,
        tx_hash: String::new(),
    })
}

/// Create a lagged notification event for gRPC clients.
///
/// When the broadcast channel drops messages due to a slow subscriber,
/// we synthesize a special `ContractEvent` with a reserved event type
/// to notify the client about the dropped message count.
fn translate_lagged_event(dropped: u64) -> Option<ContractEvent> {
    Some(ContractEvent {
        ledger_sequence: 0,
        ledger_close_time: chrono::Utc::now().timestamp(),
        contract_id: String::new(), // no contract associated with this meta-event
        event_type: "lagged".to_string(),
        topics_json: "[]".to_string(),
        value_json: serde_json::json!({ "dropped": dropped }).to_string(),
        tx_hash: String::new(),
    })
}

// ── Server startup helper ─────────────────────────────────────────────────────

const MAX_GRPC_DECODING_MESSAGE_SIZE: usize = 5 * 1024 * 1024;

/// Bind and serve the gRPC endpoint on `addr` over plaintext (no TLS).
///
/// This is called from `main` and runs in a separate Tokio task alongside the
/// existing Axum HTTP server. For a TLS-enabled listener use [`serve_tls`].
pub async fn serve(addr: std::net::SocketAddr, bus: Arc<SimulationBus>) {
    let svc = EventStreamServiceServer::new(EventStreamServiceImpl::new(bus));

    tracing::info!(grpc_addr = %addr, tls = false, "gRPC EventStreamService listening");

    if let Err(e) = tonic::transport::Server::builder()
        .max_decoding_message_size(MAX_GRPC_DECODING_MESSAGE_SIZE)
        .add_service(svc)
        .serve(addr)
        .await
    {
        tracing::error!(error = %e, "gRPC server terminated with error");
    }
}

// ── TLS configuration & certificate chain loading (Issue #918) ────────────────

/// Server-side TLS configuration, loaded from the process environment.
///
/// * `GRPC_TLS_CERT` — path to the PEM **certificate chain**. The file must
///   contain the leaf certificate first, followed by any intermediate
///   certificates. Every `CERTIFICATE` block is loaded so the full chain is
///   presented to clients; presenting only the leaf (the previous bug) made
///   client-side chain validation fail.
/// * `GRPC_TLS_KEY` — path to the PEM private key for the leaf certificate.
/// * `GRPC_TLS_CA_CERT` — optional path to a custom root CA the server trusts
///   when validating client certificates (mutual TLS).
#[derive(Debug, Clone, Default)]
pub struct TlsConfig {
    /// PEM certificate chain (leaf + intermediates).
    pub cert_chain: String,
    /// PEM private key matching the leaf certificate.
    pub private_key: String,
    /// Optional custom root CA used to validate client certificates.
    pub ca_cert: Option<String>,
}

impl TlsConfig {
    /// Build a [`TlsConfig`] by reading the standardised gRPC environment
    /// variables. Returns `Ok(None)` when TLS is not configured.
    pub fn from_env() -> std::io::Result<Option<TlsConfig>> {
        let cert_chain = match std::env::var("GRPC_TLS_CERT") {
            Ok(c) => c,
            Err(_) => return Ok(None),
        };
        let private_key = std::env::var("GRPC_TLS_KEY").map_err(|_| {
            io::Error::new(
                io::ErrorKind::NotFound,
                "GRPC_TLS_CERT is set but GRPC_TLS_KEY is missing",
            )
        })?;
        let ca_cert = std::env::var("GRPC_TLS_CA_CERT").ok();
        Ok(Some(TlsConfig {
            cert_chain,
            private_key,
            ca_cert,
        }))
    }
}

/// Read every `CERTIFICATE` block out of the PEM file at `path` and re-encode
/// them as a single PEM chain.
///
/// # Why this fixes the validation failure
///
/// Tonic's [`Identity::from_pem`] expects the *complete* certificate chain in
/// one PEM document. The previous implementation only kept the first
/// certificate, so clients could not validate the presented chain up to their
/// trusted root and every TLS handshake failed. Parsing with `rustls-pemfile`
/// preserves every block (leaf + intermediates) in the correct order.
fn load_cert_chain(path: &str) -> io::Result<String> {
    let data = fs::read(path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("failed to read certificate chain file '{path}': {e}"),
        )
    })?;
    let mut reader = BufReader::new(&data[..]);
    let mut collected = read_cert_chain(&mut reader).collect::<io::Result<Vec<_>>>()?;
    if collected.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("no CERTIFICATE blocks found in '{path}'"),
        ));
    }
    // rustls-pemfile yields certs in file order (leaf first). Ensure that
    // order is preserved exactly as authored.
    collected.shrink_to_fit();

    let mut chain = String::new();
    for cert in collected {
        chain.push_str("-----BEGIN CERTIFICATE-----\n");
        for chunk in BASE64.encode(cert).as_bytes().chunks(64) {
            // SAFETY: base64 output is always ASCII.
            let line = str::from_utf8(chunk).unwrap_or_default();
            chain.push_str(line);
            chain.push('\n');
        }
        chain.push_str("-----END CERTIFICATE-----\n");
    }
    Ok(chain)
}

/// Validate the loaded certificate chain (Issue #016).
///
/// Rejects the configuration when:
///
/// * the leaf certificate's `not_after` field is in the past (expired), or
/// * the issuer chain is incomplete — i.e. the leaf is not signed by the next
///   certificate up, and so on, until a self-signed root is reached.
///
/// `certs` must be in file order (leaf first, root last), matching what
/// [`load_cert_chain`] produces.
fn validate_cert_chain(certs: &[Vec<u8>]) -> io::Result<()> {
    if certs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "certificate chain is empty",
        ));
    }

    // Parse every certificate up front so we can inspect validity and issuers.
    let mut parsed: Vec<X509Certificate<'_>> = Vec::with_capacity(certs.len());
    for (idx, der) in certs.iter().enumerate() {
        let (_, cert) = X509Certificate::from_der(der).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("failed to parse certificate at chain index {idx}: {e}"),
            )
        })?;
        parsed.push(cert);
    }

    // 1. Expiry check on the leaf certificate.
    let leaf = &parsed[0];
    let now = chrono::Utc::now().timestamp();
    let not_after = leaf.validity().not_after.timestamp();
    if not_after < now {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "leaf certificate expired at {} (now {})",
                leaf.validity().not_after,
                chrono::Utc::now()
            ),
        ));
    }
    let not_before = leaf.validity().not_before.timestamp();
    if not_before > now {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "leaf certificate is not yet valid (not_before {})",
                leaf.validity().not_before
            ),
        ));
    }

    // 2. Chain completeness: each certificate must be issued by the next one
    //    up, ending at a self-signed root.
    for idx in 0..parsed.len() {
        let cert = &parsed[idx];
        let is_self_signed = cert.subject() == cert.issuer();
        if is_self_signed {
            // A self-signed certificate is only acceptable as the final root.
            if idx != parsed.len() - 1 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "certificate at chain index {idx} is self-signed but is not the last \
                         certificate in the chain; the issuer chain is incomplete"
                    ),
                ));
            }
            break;
        }

        // Non-root certificates must be followed by their issuer.
        let Some(issuer) = parsed.get(idx + 1) else {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "certificate at chain index {idx} (subject '{}') has no issuer in the \
                     chain; the issuer chain is incomplete",
                    cert.subject()
                ),
            ));
        };
        if cert.issuer() != issuer.subject() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "certificate at chain index {idx} (subject '{}') is not issued by the next \
                     certificate in the chain (issuer '{}', next subject '{}'); the issuer \
                     chain is incomplete",
                    cert.subject(),
                    cert.issuer(),
                    issuer.subject()
                ),
            ));
        }
    }

    Ok(())
}

/// Load the Tonic server [`Identity`] (full certificate chain + private key).
fn load_identity(tls: &TlsConfig) -> io::Result<Identity> {
    let cert_chain = load_cert_chain(&tls.cert_chain)?;

    // Re-parse the normalised chain so we can validate it before handing it to
    // rustls (Issue #016). `load_cert_chain` already guarantees at least one
    // well-formed CERTIFICATE block.
    let mut reader = BufReader::new(cert_chain.as_bytes());
    let certs = read_cert_chain(&mut reader).collect::<io::Result<Vec<_>>>()?;
    validate_cert_chain(&certs)?;

    let key = fs::read_to_string(&tls.private_key).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("failed to read private key file '{}': {e}", tls.private_key),
        )
    })?;
    // `Identity::from_pem` carries the entire PEM document through to rustls,
    // so the full chain (leaf + intermediates) is what gets presented to
    // clients — this is what makes chain validation succeed (Issue #918).
    Ok(Identity::from_pem(cert_chain, key))
}

/// Load the optional custom root CA certificate used to validate client certs.
fn load_client_ca(ca_path: &Path) -> io::Result<Option<Certificate>> {
    if !ca_path.as_os_str().is_empty() {
        let pem = fs::read(ca_path).map_err(|e| {
            io::Error::new(
                e.kind(),
                format!(
                    "failed to read CA certificate file '{}': {e}",
                    ca_path.display()
                ),
            )
        })?;
        return Ok(Some(Certificate::from_pem(pem)));
    }
    Ok(None)
}

/// Build a Tonic [`ServerTlsConfig`] from the supplied [`TlsConfig`], loading
/// the full certificate chain (see [`load_cert_chain`]) and, when configured,
/// the custom root CA used to validate client certificates.
fn build_server_tls(tls: &TlsConfig) -> io::Result<ServerTlsConfig> {
    let mut server_tls = ServerTlsConfig::new();
    server_tls = server_tls.identity(load_identity(tls)?);

    if let Some(ca_path) = tls.ca_cert.as_deref() {
        if let Some(ca) = load_client_ca(Path::new(ca_path))? {
            server_tls = server_tls.client_ca_root(ca);
        }
    }
    Ok(server_tls)
}

/// Bind and serve the gRPC endpoint on `addr` over TLS using the supplied
/// [`TlsConfig`].
///
/// The Tonic server builder is configured with an [`Identity`] that carries the
/// full certificate chain (see [`load_cert_chain`]). If a custom CA root is
/// configured it is registered so client certificates are validated against it.
pub async fn serve_tls(addr: std::net::SocketAddr, bus: Arc<SimulationBus>, tls: TlsConfig) {
    let server_tls = match build_server_tls(&tls) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "failed to configure gRPC TLS");
            return;
        }
    };
    let svc = EventStreamServiceServer::new(EventStreamServiceImpl::new(bus));

    tracing::info!(
        grpc_addr = %addr,
        tls = true,
        "gRPC EventStreamService listening with TLS"
    );

    let builder = match tonic::transport::Server::builder()
        .max_decoding_message_size(MAX_GRPC_DECODING_MESSAGE_SIZE)
        .tls_config(server_tls)
    {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(error = %e, "invalid gRPC TLS server configuration");
            return;
        }
    };
    if let Err(e) = builder.add_service(svc).serve(addr).await {
        tracing::error!(error = %e, "gRPC TLS server terminated with error");
    }
}

/// Run the gRPC server, automatically enabling TLS when `GRPC_TLS_CERT` and
/// `GRPC_TLS_KEY` are present in the environment, otherwise serving plaintext.
///
/// Convenience entry point used by `main` so a single call always "just works".
pub async fn serve_with_tls_from_env(addr: std::net::SocketAddr, bus: Arc<SimulationBus>) {
    match TlsConfig::from_env() {
        Ok(Some(tls)) => serve_tls(addr, bus, tls).await,
        Ok(None) => {
            tracing::warn!(
                grpc_addr = %addr,
                "GRPC_TLS_CERT not set; gRPC server running without TLS"
            );
            serve(addr, bus).await;
        }
        Err(e) => {
            tracing::error!(error = %e, "invalid gRPC TLS configuration");
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ws::{CompletedPayload, SimulationEvent};
    use chrono::Utc;

    fn make_completed_event(job_id: &str) -> SimulationEvent {
        SimulationEvent::Completed {
            job_id: job_id.to_string(),
            data: CompletedPayload {
                cpu_instructions: 1_000_000,
                ram_bytes: 512_000,
                ledger_read_bytes: 1024,
                ledger_write_bytes: 256,
                transaction_size_bytes: 400,
                cost_stroops: 500,
            },
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn translate_completed_event_no_filter() {
        let event = make_completed_event("contract-abc");
        let result = translate_event(event, "", &[]);
        assert!(result.is_some());
        let ce = result.unwrap();
        assert_eq!(ce.contract_id, "contract-abc");
        assert_eq!(ce.event_type, "simulation_completed");
        assert!(ce.topics_json.contains("simulation"));
        assert!(ce.value_json.contains("cpu_instructions"));
    }

    #[test]
    fn translate_completed_event_matching_filter() {
        let event = make_completed_event("contract-xyz");
        let result = translate_event(event, "contract-xyz", &[]);
        assert!(result.is_some());
    }

    #[test]
    fn translate_completed_event_non_matching_filter() {
        let event = make_completed_event("contract-abc");
        let result = translate_event(event, "contract-xyz", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn translate_progress_event_is_filtered_out() {
        use crate::ws::ProgressPayload;
        let event = SimulationEvent::Progress {
            job_id: "job-1".to_string(),
            data: ProgressPayload {
                percent: 50,
                message: "halfway".to_string(),
            },
            timestamp: Utc::now(),
        };
        let result = translate_event(event, "", &[]);
        assert!(result.is_none(), "Progress events should not be streamed");
    }

    #[test]
    fn translate_event_type_filter_matches() {
        let event = make_completed_event("contract-abc");
        let filter = vec!["simulation_completed".to_string()];
        let result = translate_event(event, "", &filter);
        assert!(result.is_some());
    }

    #[test]
    fn translate_event_type_filter_no_match() {
        let event = make_completed_event("contract-abc");
        let filter = vec!["transfer".to_string()];
        let result = translate_event(event, "", &filter);
        assert!(result.is_none());
    }

    // ── TLS certificate chain loading & secure connection (Issue #918) ────

    use rcgen::{
        BasicConstraints, Certificate as RcCert, CertificateParams, DistinguishedName, DnType,
        IsCa, KeyPair,
    };
    use rustls::RootCertStore;
    use rustls_pki_types::{CertificateDer, ServerName};
    use std::net::SocketAddr;
    use tokio::net::TcpStream;
    use tokio_rustls::TlsConnector;

    /// Generate a throwaway root CA used as the trust anchor for the test.
    fn make_test_ca() -> (RcCert, KeyPair) {
        let mut params =
            CertificateParams::new(vec!["Sky Moon Scope.test".to_string()]).expect("ca params");
        params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "Sky Moon Scope test root CA".to_string());
        params.distinguished_name = dn;
        let key = KeyPair::generate().expect("generate ca key");
        let cert = params.self_signed(&key).expect("self-sign ca");
        (cert, key)
    }

    /// Issue a leaf certificate (CN/SAN `localhost`) signed by `ca`.
    fn make_test_leaf(ca: &RcCert, ca_key: &KeyPair) -> (RcCert, KeyPair) {
        let mut params =
            CertificateParams::new(vec!["localhost".to_string()]).expect("leaf params");
        params.is_ca = IsCa::NoCa;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "localhost".to_string());
        params.distinguished_name = dn;
        let key = KeyPair::generate().expect("generate leaf key");
        let cert = params
            .signed_by(&key, ca, ca_key)
            .expect("sign leaf with ca");
        (cert, key)
    }

    /// Open a TCP connection to the TLS server, retrying briefly to allow the
    /// async server to finish binding.
    async fn connect_with_retry(addr: SocketAddr) -> TcpStream {
        let mut attempt = 0;
        loop {
            match TcpStream::connect(addr).await {
                Ok(s) => return s,
                Err(_) if attempt < 50 => {
                    attempt += 1;
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                Err(e) => panic!("failed to connect to gRPC TLS server: {e}"),
            }
        }
    }

    /// Starts a Tonic gRPC server terminated with TLS using the supplied leaf
    /// signed by a shared CA. Both the server chain and the trust anchor come
    /// from the same CA so the handshake can only succeed if the full chain is
    /// loaded (the regression this test guards).
    #[tokio::test]
    async fn tls_server_presents_chain_and_accepts_secure_connection() {
        let (ca, ca_key) = make_test_ca();
        let (leaf, leaf_key) = make_test_leaf(&ca, &ca_key);

        // Server presents the full chain: leaf + issuing CA.
        let chain_pem = format!("{}\n{}", leaf.pem(), ca.pem());
        let key_pem = leaf_key.serialize_pem();
        let ca_der: CertificateDer<'static> = ca.der().clone();

        let dir = tempfile::tempdir().expect("temp dir");
        let cert_path = dir.path().join("chain.pem");
        let key_path = dir.path().join("key.pem");
        std::fs::write(&cert_path, chain_pem).expect("write chain.pem");
        std::fs::write(&key_path, key_pem).expect("write key.pem");

        // Pick a free port for the server.
        let probe = std::net::TcpListener::bind("127.0.0.1:0").expect("free port probe");
        let port = probe.local_addr().expect("probe addr").port();
        drop(probe);
        let addr: SocketAddr = format!("127.0.0.1:{port}").parse().expect("addr");

        let tls = TlsConfig {
            cert_chain: cert_path.display().to_string(),
            private_key: key_path.display().to_string(),
            ca_cert: None,
        };
        let bus = crate::ws::SimulationBus::new();
        let server = tokio::spawn(serve_tls(addr, bus, tls));

        // The client trusts only the shared test root CA. A successful
        // handshake proves the server presented the full, valid chain.
        let mut roots = RootCertStore::empty();
        roots.add(ca_der).expect("add root ca");
        let client_cfg = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(client_cfg));

        let stream = connect_with_retry(addr).await;
        let server_name = ServerName::try_from("localhost").expect("dns name");
        let tls_stream = connector
            .connect(server_name, stream)
            .await
            .expect("TLS handshake must succeed when the full certificate chain is loaded");

        // Sanity: the server authenticated with our leaf certificate.
        let peer = tls_stream
            .get_ref()
            .1
            .peer_certificates()
            .expect("peer certificate present");
        assert!(
            !peer.is_empty(),
            "server presented at least one certificate"
        );

        server.abort();
    }

    #[test]
    fn load_cert_chain_reads_every_certificate_block() {
        let (ca, ca_key) = make_test_ca();
        let (leaf, _leaf_key) = make_test_leaf(&ca, &ca_key);
        let chain_pem = format!("{}\n{}", leaf.pem(), ca.pem());

        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("chain.pem");
        std::fs::write(&path, chain_pem).expect("write chain");

        let loaded = load_cert_chain(path.to_str().unwrap()).expect("load chain");
        // Both the leaf and the CA block survive; the chain is the whole file.
        assert_eq!(
            loaded.matches("BEGIN CERTIFICATE").count(),
            2,
            "expected both certificate blocks to be preserved"
        );
        assert_eq!(
            loaded.matches("END CERTIFICATE").count(),
            2,
            "expected both certificate terminators to be preserved"
        );
    }

    // ── Certificate chain validation (Issue #016) ─────────────────────────

    /// Issue a leaf certificate signed by `ca` that expired in the past.
    fn make_expired_leaf(ca: &RcCert, ca_key: &KeyPair) -> (RcCert, KeyPair) {
        let mut params =
            CertificateParams::new(vec!["localhost".to_string()]).expect("leaf params");
        params.is_ca = IsCa::NoCa;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "localhost".to_string());
        params.distinguished_name = dn;
        // Valid from 2020-01-01 to 2020-02-01 — firmly in the past.
        params.not_before = time::OffsetDateTime::from_unix_timestamp(1_577_836_800)
            .expect("not_before");
        params.not_after = time::OffsetDateTime::from_unix_timestamp(1_580_515_200)
            .expect("not_after");
        let key = KeyPair::generate().expect("generate leaf key");
        let cert = params
            .signed_by(&key, ca, ca_key)
            .expect("sign expired leaf with ca");
        (cert, key)
    }

    fn der_of(cert: &RcCert) -> Vec<u8> {
        cert.der().as_ref().to_vec()
    }

    #[test]
    fn validate_cert_chain_accepts_valid_chain() {
        let (ca, ca_key) = make_test_ca();
        let (leaf, _leaf_key) = make_test_leaf(&ca, &ca_key);
        let chain = vec![der_of(&leaf), der_of(&ca)];
        validate_cert_chain(&chain).expect("valid chain must be accepted");
    }

    #[test]
    fn validate_cert_chain_rejects_expired_leaf() {
        let (ca, ca_key) = make_test_ca();
        let (leaf, _leaf_key) = make_expired_leaf(&ca, &ca_key);
        let chain = vec![der_of(&leaf), der_of(&ca)];
        let err = validate_cert_chain(&chain).expect_err("expired leaf must be rejected");
        assert!(
            err.to_string().contains("expired"),
            "error should mention expiry, got: {err}"
        );
    }

    #[test]
    fn validate_cert_chain_rejects_self_signed_leaf() {
        // A self-signed leaf (not a CA) presented as the only certificate.
        let mut params =
            CertificateParams::new(vec!["localhost".to_string()]).expect("leaf params");
        params.is_ca = IsCa::NoCa;
        let mut dn = DistinguishedName::new();
        dn.push(DnType::CommonName, "localhost".to_string());
        params.distinguished_name = dn;
        let key = KeyPair::generate().expect("generate key");
        let leaf = params.self_signed(&key).expect("self-sign leaf");
        let chain = vec![der_of(&leaf)];
        let err =
            validate_cert_chain(&chain).expect_err("self-signed leaf must be rejected");
        assert!(
            err.to_string().contains("incomplete"),
            "error should mention incomplete chain, got: {err}"
        );
    }

    #[test]
    fn validate_cert_chain_rejects_incomplete_chain() {
        // Leaf only, issued by a CA that is not present in the chain.
        let (ca, ca_key) = make_test_ca();
        let (leaf, _leaf_key) = make_test_leaf(&ca, &ca_key);
        let chain = vec![der_of(&leaf)];
        let err = validate_cert_chain(&chain)
            .expect_err("leaf without its issuer must be rejected");
        assert!(
            err.to_string().contains("incomplete"),
            "error should mention incomplete chain, got: {err}"
        );
    }

    #[test]
    fn validate_cert_chain_rejects_mismatched_issuer() {
        // Leaf signed by CA A, but chain presents unrelated CA B.
        let (ca_a, ca_a_key) = make_test_ca();
        let (leaf, _leaf_key) = make_test_leaf(&ca_a, &ca_a_key);
        let (ca_b, _ca_b_key) = make_test_ca();
        let chain = vec![der_of(&leaf), der_of(&ca_b)];
        let err = validate_cert_chain(&chain)
            .expect_err("mismatched issuer must be rejected");
        assert!(
            err.to_string().contains("incomplete"),
            "error should mention incomplete chain, got: {err}"
        );
    }

    #[test]
    fn build_server_tls_rejects_expired_chain() {
        let (ca, ca_key) = make_test_ca();
        let (leaf, leaf_key) = make_expired_leaf(&ca, &ca_key);
        let chain_pem = format!("{}\n{}", leaf.pem(), ca.pem());

        let dir = tempfile::tempdir().expect("temp dir");
        let cert_path = dir.path().join("chain.pem");
        let key_path = dir.path().join("key.pem");
        std::fs::write(&cert_path, chain_pem).expect("write chain");
        std::fs::write(&key_path, leaf_key.serialize_pem()).expect("write key");

        let tls = TlsConfig {
            cert_chain: cert_path.display().to_string(),
            private_key: key_path.display().to_string(),
            ca_cert: None,
        };
        let err = build_server_tls(&tls).expect_err("expired chain must be rejected");
        assert!(
            err.to_string().contains("expired"),
            "error should mention expiry, got: {err}"
        );
    }
}
