//! The `Szamlazz.Order` lock and the in-flight `Idempotency-Key`, same key,
//! same scope: two `create_invoice` calls racing on one order with distinct
//! keys get one document (the second is queued behind the Virtual Object's
//! per-key lock while the first is mid-send, or while the first's create step
//! waits out the issue policy's `initial_delay`), and a retry with the
//! **same** key while the first invocation is still in flight attaches to it
//! instead of queueing a second invocation (#125). The cross-scope twins (the
//! same key under two scopes is two objects, two invocations) are in
//! [`crate::multi_account`].

use std::future::Future;
use std::time::{Duration, Instant};

use rust_decimal::dec;
use wiremock::ResponseTemplate;

use crate::harness::ingress::Reply;
use crate::harness::szamlazz::{Doc, not_found, order_query};
use crate::harness::{Harness, create_body};

/// A reply and the instant it arrived: what the order of two answers is read
/// from.
struct Timed {
    reply: Reply,
    done: Instant,
}

/// Awaits `call` and stamps its reply with the instant it arrived.
async fn timed(call: impl Future<Output = Reply>) -> Timed {
    let reply = call.await;
    Timed {
        reply,
        done: Instant::now(),
    }
}

/// The order-key lock's proof, shared by the two races below: the second
/// call, started while the first is inside its create step, is answered
/// **after** the first (queued, not run beside it), from its lookup step
/// (`already_issued`, no `create-invoice` run) and without a send of its own:
/// every create on the wire carries `order`, and how many there are is the
/// scenario's to say.
async fn assert_second_call_queued_behind_the_first(
    h: &Harness,
    order: &str,
    number: &str,
    first: Timed,
    second: Timed,
) {
    assert_eq!(first.reply.status, 200, "{}", first.reply.body);
    assert_eq!(second.reply.status, 200, "{}", second.reply.body);
    assert_eq!(
        first.reply.body["outcome"], "issued",
        "{}",
        first.reply.body
    );
    assert_eq!(first.reply.body["invoice_number"], number);
    assert_eq!(
        second.reply.body["outcome"], "already_issued",
        "the second call found the first's document: {}",
        second.reply.body
    );
    assert_eq!(second.reply.body["invoice_number"], number);
    assert_ne!(
        first.reply.invocation_id(),
        second.reply.invocation_id(),
        "two keys, two invocations"
    );
    assert!(
        second.done >= first.done,
        "the second call was answered after the first: the lock serialised them \
         (first {:?}, second {:?})",
        first.done,
        second.done
    );

    let creates = h.create_bodies().await;
    assert!(
        creates
            .iter()
            .all(|body| body.contains(&format!("<rendelesSzam>{order}</rendelesSzam>"))),
        "every send is this order's: {creates:?}"
    );

    for (which, reply) in [("first", &first.reply), ("second", &second.reply)] {
        let invocation = h.invocation(reply.invocation_id()).await;
        assert_eq!(invocation.status, "completed", "{which}: {invocation:?}");
        assert_eq!(
            invocation.completion_failure, None,
            "{which}: {invocation:?}"
        );
    }
    let first_runs = h.runs(first.reply.invocation_id()).await;
    assert_eq!(
        first_runs.last().map(String::as_str),
        Some("create-invoice"),
        "the first call sent: {first_runs:?}"
    );
    assert_eq!(
        h.runs(second.reply.invocation_id()).await,
        [
            "namespace",
            "account",
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
            "lookup-invoice",
        ],
        "the second call's runs end at the lookup step: no create step"
    );
}

/// (xxiii) same key, same scope, two `create_invoice` with distinct
/// `Idempotency-Key`s, the first's send **delayed** by szamlazz.hu: the second
/// call arrives while the first is mid-send and is queued behind the Virtual
/// Object's lock; it runs once the first has completed, its lookup finds the
/// document that landed, and it answers `already_issued` without a
/// `create-invoice` run. One create on the wire; both invocations completed;
/// the second answered after the first. The race is real (the second call is
/// started the moment szamlazz.hu has received the first's create, three
/// seconds before it answers), and the order provable (the reply times, the
/// runs), not a lucky interleaving: without the lock the second call's lookup
/// would find the document that szamlazz.hu already holds and answer inside
/// the three seconds, before the first.
pub(crate) async fn same_key_same_scope_concurrent_creates_issue_once(h: &Harness) {
    h.reset().await;
    h.absent("E2E-L1", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-L1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.create_lands_slowly(
        &Doc {
            external_id: Some("acct:E2E-L1:invoice"),
            ..Doc::new("SZ-L1", "SZ", "E2E-L1")
        },
        Duration::from_secs(3),
    )
    .await;

    let body = create_body(dec!(1000), false);
    let started = Instant::now();
    let (first, second) = tokio::join!(
        timed(h.call("E2E-L1", "create_invoice", &body, "e2e-l1-k1")),
        async {
            // The moment szamlazz.hu has the first call's create request and
            // its reply is still three seconds away.
            h.wait_for_creates(1).await;
            timed(h.call("E2E-L1", "create_invoice", &body, "e2e-l1-k2")).await
        },
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_secs(3) && elapsed < Duration::from_secs(60),
        "the first call waited for szamlazz.hu's delayed reply and nothing was retried by the handler: {elapsed:?}"
    );
    assert_second_call_queued_behind_the_first(h, "E2E-L1", "SZ-L1", first, second).await;
    assert_eq!(h.create_bodies().await.len(), 1, "one create on the wire");
    eprintln!(
        "(xxiii) same key, same scope, two concurrent creates with the first's send delayed → issued + already_issued, one create, the second's runs end at the lookup: pass"
    );
}

/// (xxiii-b) the delay variant: the first call's send is answered
/// `szlahu_down` (nothing landed: the immediate re-query finds nothing, the
/// create step is *Unconfirmed* and waits out the issue policy's 1 s
/// `initial_delay` before re-executing), and the second call arrives in that
/// delay. It is queued behind the lock while the first re-executes its create
/// step and sends again; the second send lands; the second call then finds
/// the document from its lookup. Two creates on the wire, both the first
/// call's; the same assertions as (xxiii) otherwise. That the second call
/// arrived *during* the delay, not during the second send, is asserted, not
/// assumed, from the server's side: the second call's row is on
/// `sys_invocation` (accepted, queued behind the lock, not completed) before
/// szamlazz.hu receives the second create; the ingress reports the id only
/// with the answer, so the row is what places the call in time.
pub(crate) async fn same_key_same_scope_second_call_in_the_first_calls_delay(h: &Harness) {
    h.reset().await;
    h.absent("E2E-L2", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-L2")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.create_lands_on_the_second_send(
        &Doc {
            external_id: Some("acct:E2E-L2:invoice"),
            ..Doc::new("SZ-L2", "SZ", "E2E-L2")
        },
        ResponseTemplate::new(503).insert_header("szlahu_down", "maintenance"),
    )
    .await;

    let body = create_body(dec!(1000), false);
    let watch = h.watch("E2E-L2");
    let started = Instant::now();
    let (first, second, (second_queued, second_create_received)) = tokio::join!(
        timed(h.call("E2E-L2", "create_invoice", &body, "e2e-l2-k1")),
        async {
            // The first send was received (and answered szlahu_down); the
            // create step's second execution is a second away.
            h.wait_for_creates(1).await;
            timed(h.call("E2E-L2", "create_invoice", &body, "e2e-l2-k2")).await
        },
        async {
            // The server holds two invocations on the key (the first, and the
            // second queued behind it) before szamlazz.hu sees a second create.
            h.wait_for_creates(1).await;
            let in_flight = h.await_in_flight_on("E2E-L2", 2).await;
            let queued = Instant::now();
            assert_eq!(
                h.create_bodies().await.len(),
                1,
                "the second call was queued on the server while the first's create step waits out its delay, \
                 before the second send: {in_flight:?}"
            );
            h.wait_for_creates(2).await;
            (queued, Instant::now())
        },
    );
    let elapsed = started.elapsed();
    let retries = watch.await.expect("watch");
    assert!(
        second_queued < second_create_received,
        "the second call was on the server before szamlazz.hu received the second create \
         (queued {second_queued:?}, second create {second_create_received:?})"
    );
    assert!(
        elapsed >= Duration::from_secs(1) && elapsed < Duration::from_secs(60),
        "the run policy's 1 s delay was waited out, not the handler's: {elapsed:?}"
    );
    assert_eq!(
        retries.failing_commands,
        ["create-invoice"],
        "the first call's create step is what re-executed: {retries:?}"
    );
    assert_second_call_queued_behind_the_first(h, "E2E-L2", "SZ-L2", first, second).await;
    assert_eq!(
        h.create_bodies().await.len(),
        2,
        "two sends, both the first call's: the one szamlazz.hu did not act on and the one that landed"
    );
    eprintln!(
        "(xxiii-b) same key, same scope, the second call in the first's create-step delay → issued + already_issued, two sends of the first call, the second's runs end at the lookup: pass"
    );
}

/// (xxiv) the same `Idempotency-Key` sent while the first invocation is still
/// in flight (the first's send delayed, as in (xxiii)) **attaches** to it:
/// one invocation id on both replies, the same body (`issued`), one create on
/// the wire, one invocation on the order. What the glossary's *Idempotency-Key*
/// entry asks the caller to do after **no answer**: keep the key, since a new
/// one would queue a second invocation behind the lock, as (xxiii) shows.
/// The completed case (the stored completion replayed) is (ii); that this is
/// not it is asserted: the retry was sent before the first call was answered,
/// and it **waited** for the in-flight invocation's answer (a replayed
/// completion answers in milliseconds; the retry, sent inside the first
/// second of szamlazz.hu's 3 s delay, waits out the rest). Never the order of
/// the two answers: one completion releases both, so which client task reads
/// its reply first is not an event.
pub(crate) async fn same_idempotency_key_in_flight_attaches_to_the_invocation(h: &Harness) {
    h.reset().await;
    h.absent("E2E-L3", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-L3")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    h.create_lands_slowly(
        &Doc {
            external_id: Some("acct:E2E-L3:invoice"),
            ..Doc::new("SZ-L3", "SZ", "E2E-L3")
        },
        Duration::from_secs(3),
    )
    .await;

    let body = create_body(dec!(1000), false);
    let (first, (in_flight, retry_sent, retry)) = tokio::join!(
        timed(h.call("E2E-L3", "create_invoice", &body, "e2e-l3-shared")),
        async {
            h.wait_for_creates(1).await;
            let in_flight = h.in_flight_on("E2E-L3").await;
            let sent = Instant::now();
            (
                in_flight,
                sent,
                timed(h.call("E2E-L3", "create_invoice", &body, "e2e-l3-shared")).await,
            )
        },
    );
    assert!(
        retry_sent < first.done,
        "the retry was sent while the first invocation was in flight (sent {retry_sent:?}, first answered {:?})",
        first.done
    );
    let retry_waited = retry.done.duration_since(retry_sent);
    assert!(
        retry_waited >= Duration::from_secs(1),
        "the retry waited for the in-flight invocation's answer, it did not replay a completion \
         (waited {retry_waited:?})"
    );
    assert_eq!(first.reply.status, 200, "{}", first.reply.body);
    assert_eq!(retry.reply.status, 200, "{}", retry.reply.body);
    assert_eq!(
        first.reply.body["outcome"], "issued",
        "{}",
        first.reply.body
    );
    assert_eq!(first.reply.body["invoice_number"], "SZ-L3");
    assert_eq!(
        retry.reply.body, first.reply.body,
        "the retry received the in-flight invocation's outcome"
    );
    assert_eq!(
        retry.reply.invocation_id(),
        first.reply.invocation_id(),
        "one invocation: the retry attached to it"
    );
    assert_eq!(
        in_flight,
        first.reply.invocation_id(),
        "the invocation the retry found in flight is the one it attached to"
    );
    assert_eq!(h.create_bodies().await.len(), 1, "one create on the wire");
    let on_the_order = h
        .sql("SELECT id, target_handler_name, idempotency_key FROM sys_invocation WHERE target_service_key = 'E2E-L3'")
        .await;
    assert_eq!(
        on_the_order.len(),
        1,
        "one invocation on the order, not a second one queued behind the lock: {on_the_order:?}"
    );
    let invocation = h.invocation(first.reply.invocation_id()).await;
    assert_eq!(invocation.status, "completed", "{invocation:?}");
    assert_eq!(
        h.runs(first.reply.invocation_id())
            .await
            .last()
            .map(String::as_str),
        Some("create-invoice")
    );
    eprintln!(
        "(xxiv) the same Idempotency-Key while the first invocation is in flight → attached: one invocation id, one create: pass"
    );
}
