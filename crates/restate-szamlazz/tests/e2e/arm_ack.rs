//! Interrupt the actual protocol-v7 acknowledgement, between Restate and the SDK.
//! A byte-transparent loopback HTTP/2 relay observes DATA payloads, identifies the
//! arm run's completion id, and withholds the DATA frame completing its ack. No
//! application checkpoint can substitute for this transport boundary.
use std::collections::HashMap;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use restate_e2e_harness::{Call, ReusePolicy, ServerSpec, launcher_or_skip};
use restate_sdk::prelude::*;
use rust_decimal::dec;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{Notify, watch};
use tokio::task::JoinSet;
use wiremock::MockServer;

use crate::common::{create_for, created, external_id_query, not_found, order_query};
use crate::harness::{MAIN_SERVER, create_body};

#[derive(Default)]
struct Stream {
    incoming: Vec<u8>,
    outgoing: Vec<u8>,
    arm: Option<u64>,
}

// Protocol v7: eight-byte big-endian header (type:16, flags:16, length:32).
// RunCommand 0x0411: completion id field 11, name field 12.
// ProposeRunCompletionAck 0x0007: completion id field 1.
// Source: service-protocol v7 / SDK shared core 7.0.3 generated messages.
fn varint(bytes: &mut &[u8]) -> Option<u64> {
    let mut result = 0;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = bytes.split_first()?;
        *bytes = rest;
        result |= u64::from(byte & 127) << shift;
        if byte & 128 == 0 {
            return Some(result);
        }
    }
    None
}

fn fields(mut bytes: &[u8]) -> Option<(u64, Option<&[u8]>)> {
    let mut id = 0;
    let mut name = None;
    while !bytes.is_empty() {
        let tag = varint(&mut bytes)?;
        match tag & 7 {
            0 => {
                let value = varint(&mut bytes)?;
                if tag >> 3 == 1 || tag >> 3 == 11 {
                    id = value;
                }
            }
            2 => {
                let len = usize::try_from(varint(&mut bytes)?).ok()?;
                let (value, rest) = bytes.split_at_checked(len)?;
                if tag >> 3 == 12 {
                    name = Some(value);
                }
                bytes = rest;
            }
            1 => bytes = bytes.get(8..)?,
            5 => bytes = bytes.get(4..)?,
            _ => return None,
        }
    }
    Some((id, name))
}

impl Stream {
    fn data(&mut self, incoming: bool, data: &[u8]) -> bool {
        let buffer = if incoming {
            &mut self.incoming
        } else {
            &mut self.outgoing
        };
        buffer.extend_from_slice(data);
        while buffer.len() >= 8 {
            let kind = u16::from_be_bytes([buffer[0], buffer[1]]);
            let len = u32::from_be_bytes(buffer[4..8].try_into().expect("length")) as usize;
            // Discovery bodies are JSON, not invocation protocol.
            if len > 8 * 1024 * 1024 {
                buffer.clear();
                return false;
            }
            if buffer.len() < 8 + len {
                return false;
            }
            let parsed = fields(&buffer[8..8 + len]);
            let blocked = if incoming && kind == 7 {
                parsed.is_some_and(|(id, _)| self.arm == Some(id))
            } else {
                if !incoming
                    && kind == 0x0411
                    && let Some((id, Some(b"arm-write"))) = parsed
                {
                    self.arm = Some(id);
                }
                false
            };
            buffer.drain(..8 + len);
            if blocked {
                return true;
            }
        }
        false
    }
}

async fn relay(
    mut read: impl AsyncRead + Unpin,
    mut write: impl AsyncWrite + Unpin,
    incoming: bool,
    streams: Arc<Mutex<HashMap<u32, Stream>>>,
    blocked: Arc<Notify>,
    once: Arc<AtomicBool>,
) -> std::io::Result<()> {
    loop {
        let mut header = [0; 9];
        read.read_exact(&mut header).await?;
        let len =
            usize::from(header[0]) << 16 | usize::from(header[1]) << 8 | usize::from(header[2]);
        let mut payload = vec![0; len];
        read.read_exact(&mut payload).await?;
        let stream = u32::from_be_bytes(header[5..9].try_into().expect("stream id")) & 0x7fff_ffff;
        if header[3] == 0 {
            let data = if header[4] & 8 == 0 {
                &payload[..]
            } else {
                let padding = usize::from(payload[0]);
                &payload[1..payload.len() - padding]
            };
            let is_ack = streams
                .lock()
                .expect("streams")
                .entry(stream)
                .or_default()
                .data(incoming, data);
            if is_ack && !once.swap(true, Ordering::SeqCst) {
                blocked.notify_one();
                // Withhold the bytes until the test aborts this connection.
                std::future::pending::<()>().await;
            }
        }
        write.write_all(&header).await?;
        write.write_all(&payload).await?;
    }
}

async fn connection(
    mut incoming: TcpStream,
    port: u16,
    blocked: Arc<Notify>,
    once: Arc<AtomicBool>,
) {
    let mut outgoing = TcpStream::connect(("127.0.0.1", port))
        .await
        .expect("SDK endpoint");
    let mut preface = [0; 24];
    if incoming.read_exact(&mut preface).await.is_err() {
        return;
    }
    outgoing.write_all(&preface).await.expect("preface");
    if &preface != b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n" {
        let _ = tokio::io::copy_bidirectional(&mut incoming, &mut outgoing).await;
        return;
    }
    let streams = Arc::new(Mutex::new(HashMap::new()));
    let (from_server, to_server) = incoming.split();
    let (from_sdk, to_sdk) = outgoing.split();
    let _ = tokio::try_join!(
        relay(
            from_server,
            to_sdk,
            true,
            streams.clone(),
            blocked.clone(),
            once.clone()
        ),
        relay(from_sdk, to_server, false, streams, blocked, once),
    );
}

#[tokio::test]
#[ignore = "needs RESTATE_SERVER_BIN; loss of actual arm completion acknowledgement"]
#[allow(
    clippy::too_many_lines,
    reason = "one complete transport interruption and replay scenario"
)]
async fn e2e_unresolved_arm_ack_loss_never_grants_replayed_send_permission() {
    let Some(launcher) = launcher_or_skip(ReusePolicy::Never) else {
        return;
    };
    let restate = launcher
        .launch(&ServerSpec {
            name: "arm-ack-loss",
            ..MAIN_SERVER
        })
        .await;
    let mock = MockServer::start().await;
    let (order, _) = crate::harness::accounts::services(&mock.uri());
    let options = restate_sdk::endpoint::ServiceOptions::default().handler(
        "create_invoice",
        restate_sdk::endpoint::HandlerOptions::default()
            .retry_policy_max_attempts(1)
            .retry_policy_pause_on_max_attempts(),
    );
    let endpoint = restate
        .deploy(
            Endpoint::builder()
                .bind(order.into_service_definition().options(options))
                .build(),
        )
        .await;
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("relay listener");
    let port = listener.local_addr().expect("relay address").port();
    let blocked = Arc::new(Notify::new());
    let observed = blocked.clone();
    let once = Arc::new(AtomicBool::new(false));
    let (control, mut commands) = watch::channel(0_u8);
    let proxy = tokio::spawn(async move {
        let mut tasks = JoinSet::new();
        loop {
            tokio::select! {
                accepted = listener.accept() => {
                    let (socket, _) = accepted.expect("accept");
                    tasks.spawn(connection(socket, endpoint.port, blocked.clone(), once.clone()));
                }
                changed = commands.changed() => {
                    tasks.abort_all();
                    while let Some(result) = tasks.join_next().await { if let Err(error) = result { assert!(error.is_cancelled(), "{error}"); } }
                    if changed.is_err() || *commands.borrow_and_update() == 2 { break; }
                }
                Some(result) = tasks.join_next(), if !tasks.is_empty() => { result.expect("relay task"); }
            }
        }
    });
    restate
        .admin()
        .register(&format!("http://127.0.0.1:{port}"))
        .await;
    let key = "ACK-LOSS";
    for kind in ["invoice", "prepayment", "final", "proforma"] {
        external_id_query(&format!("acct:{key}:{kind}"))
            .respond_with(not_found())
            .mount(&mock)
            .await;
    }
    order_query(key)
        .respond_with(not_found())
        .mount(&mock)
        .await;
    let sends = Arc::new(AtomicUsize::new(0));
    let count = sends.clone();
    create_for(key)
        .respond_with(move |_: &wiremock::Request| {
            count.fetch_add(1, Ordering::SeqCst);
            created("UNEXPECTED", "1000", "1270")
        })
        .mount(&mock)
        .await;
    let call = Call::object("Szamlazz.Order", key, "create_invoice");
    let owner = restate
        .invoke(&call.send(), Some(&create_body(dec!(1000))), Some(key))
        .await;
    tokio::time::timeout(Duration::from_secs(30), observed.notified())
        .await
        .expect("actual arm acknowledgement intercepted");
    let journal = restate.admin().journal(owner.invocation_id()).await;
    crate::write_commands::check(&journal);
    assert!(
        restate_e2e_harness::run_result(&journal, "arm-write").is_some(),
        "server recorded arming"
    );
    assert_eq!(
        sends.load(Ordering::SeqCst),
        0,
        "SDK cannot send before receiving the ack"
    );
    control.send(1).expect("interrupt protocol connection");
    restate
        .admin()
        .await_status(owner.invocation_id(), &["paused"])
        .await;
    restate.admin().resume(owner.invocation_id()).await;
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            if restate
                .admin()
                .runs(owner.invocation_id())
                .await
                .iter()
                .any(|name| name == "reconcile-write")
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("replay reaches read-only reconciliation");
    restate
        .admin()
        .await_status(owner.invocation_id(), &["paused"])
        .await;
    assert_eq!(
        sends.load(Ordering::SeqCst),
        0,
        "recorded arming cannot grant permission on replay"
    );
    restate.admin().kill(owner.invocation_id()).await;
    control.send(2).expect("shutdown relay");
    proxy.await.expect("relay joined");
    restate.finish().await;
}
