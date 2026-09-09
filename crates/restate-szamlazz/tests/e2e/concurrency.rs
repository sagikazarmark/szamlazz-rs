//! The `Szamlazz.Order` lock and the in-flight `Idempotency-Key`, same key,
//! same scope: two `create_invoice` calls racing on one order with distinct
//! keys get one document (the second is queued behind the Virtual Object's
//! per-key lock while the first is mid-send, or while the first's create step
//! is between its two executions), and a retry with the **same** key while the
//! first invocation is still in flight attaches to it instead of queueing a
//! second invocation (#125). Phase 2, under the `acme` scope: the first
//! invocation is **held** where the scenario needs it by a parked credential
//! fetch (`MutableAccounts::hold_fetch`), so what "in flight" and "between
//! two executions" mean is decided by the scenario, never by a clock, and the
//! second call's place in time is read off the server (`sys_invocation`),
//! never off the client. The cross-scope twins (the same key under two scopes
//! is two objects, two invocations) are in [`crate::multi_account`].

use std::future::Future;
use std::pin::pin;
use std::time::{Duration, Instant};

use rust_decimal::dec;

use crate::harness::ingress::Reply;
use crate::harness::szamlazz::{Doc, create_for, created, not_found, order_query, szlahu_down};
use crate::harness::{Harness, create_body};

/// The scope every call of this family goes under.
const SCOPE: &str = "acme";

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
/// call, accepted by the server while the first was in flight, is answered
/// **after** the first (queued, not run beside it), from its lookup step
/// (`already_issued`, no `create-invoice` run) and without a send of its own;
/// how many creates the order saw is the scenario's to say.
async fn assert_second_call_queued_behind_the_first(
    h: &Harness,
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

    for (which, reply) in [("first", &first.reply), ("second", &second.reply)] {
        let invocation = h.invocation(reply.invocation_id()).await;
        assert_eq!(invocation.status, "completed", "{which}: {invocation:?}");
        assert_eq!(
            invocation.completion_failure, None,
            "{which}: {invocation:?}"
        );
        assert_eq!(
            invocation.scope.as_deref(),
            Some(SCOPE),
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

/// Same key, same scope, two `create_invoice` with distinct
/// `Idempotency-Key`s, the first's send **delayed** by szamlazz.hu: the second
/// call is queued behind the Virtual Object's lock while the first is
/// mid-send; it runs once the first has completed, its lookup finds the
/// document that landed, and it answers `already_issued` without a
/// `create-invoice` run. One create on the wire; both invocations completed;
/// the second answered after the first.
///
/// The race is real and its order is the server's, not a clock's: the first
/// invocation is held at its credential fetch (before its lookup, before its
/// send) until the second call is **on the server** (`sys_invocation` holds
/// both, the first `running`, the second queued behind it, neither
/// completed); only then is the first released to look up, send and wait out
/// szamlazz.hu's three-second reply with the second still queued. A host that
/// stalls anywhere delays the release, never the order. Without the lock the
/// second call's lookup would find the document szamlazz.hu already holds and
/// answer inside the three seconds, before the first.
pub(crate) async fn same_key_same_scope_concurrent_creates_issue_once(h: &Harness) {
    h.reset().await;
    h.absent("E2E-L1", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-L1")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    let mut sends = h
        .create_lands_slowly(
            &Doc {
                external_id: Some("acct:E2E-L1:invoice"),
                ..Doc::of("SZ-L1", "SZ", "E2E-L1")
            },
            Duration::from_secs(3),
        )
        .await;

    let body = create_body(dec!(1000), false);
    // The first invocation's one and only fetch: held until the second call is
    // queued behind it.
    let hold = h.multi().hold_fetch(SCOPE, 1);
    let started = Instant::now();
    let (first, second, released) = tokio::join!(
        timed(h.call_scoped(SCOPE, "E2E-L1", "create_invoice", &body, "e2e-l1-k1")),
        async {
            hold.reached().await;
            timed(h.call_scoped(SCOPE, "E2E-L1", "create_invoice", &body, "e2e-l1-k2")).await
        },
        async {
            hold.reached().await;
            let in_flight = h.await_in_flight_on("E2E-L1", 2).await;
            assert!(
                h.create_bodies_of("E2E-L1").await.is_empty(),
                "nothing sent while the first is held: {in_flight:?}"
            );
            let released = Instant::now();
            hold.release();
            // The first's send reaches szamlazz.hu; its reply is three seconds
            // away, and the second is still queued.
            sends.received(1).await;
            assert_eq!(
                h.await_in_flight_on("E2E-L1", 2).await.len(),
                2,
                "the second is still queued while the first's send is in flight"
            );
            (in_flight, released)
        },
    );
    let elapsed = started.elapsed();
    let (in_flight, released) = released;
    assert!(
        in_flight.contains(&first.reply.invocation_id().to_owned()),
        "the first invocation was one of the two in flight: {in_flight:?}"
    );
    assert!(
        released < first.done,
        "both were on the server before the first was released, let alone answered \
         (released {released:?}, first answered {:?})",
        first.done
    );
    assert!(
        first.done.duration_since(released) >= Duration::from_secs(3),
        "the first call waited for szamlazz.hu's delayed reply after the release: {:?}",
        first.done.duration_since(released)
    );
    assert!(
        elapsed < Duration::from_secs(60),
        "nothing was retried by the handler: {elapsed:?}"
    );
    assert_second_call_queued_behind_the_first(h, "SZ-L1", first, second).await;
    assert_eq!(
        h.create_bodies_of("E2E-L1").await.len(),
        1,
        "one create on the wire"
    );
}

/// The re-execution variant: the first call's send is answered
/// `szlahu_down` (nothing landed: the immediate re-query finds nothing, the
/// create step is *Unconfirmed*, and the issue policy re-executes it after its
/// `initial_delay`), and the second call arrives **between the two
/// executions**: the first invocation's second execution is held at its
/// credential fetch (after it replayed its journal, before it opens its
/// gateway and sends again), the second call is accepted and queued behind
/// the lock while it is held, and only then is it released to send. The
/// second send lands; the second call then finds the document from its
/// lookup. Two creates on the wire, both the first call's; the same
/// assertions as the race above otherwise.
///
/// #125 asks for the second call to arrive *while the create step waits out
/// its delay*. This scenario places it a step later, at the re-execution's
/// fetch, on purpose: the delay is a server-side timer of one second that
/// cannot be widened (under vqueues a run retry delay at or above 2 s leaves
/// the invoker for the scheduler and takes every in-flight column with it:
/// `worker_config`'s rustdoc, verified in #123), the worker runs nothing
/// during it, and a call raced into it is accepted before or after the timer
/// as the host's load decides, which is the class of scenario #123 removed
/// from this suite. The hold is the deterministic form of the same property:
/// the second call is on the server after the first send that did not land
/// and before the second that does, the lock queues it across the whole of
/// the first invocation's retry (the key is held through `backing-off` and
/// the re-execution alike), and it is answered from its lookup with nothing
/// of its own sent.
pub(crate) async fn same_key_same_scope_second_call_between_the_first_calls_executions(
    h: &Harness,
) {
    h.reset().await;
    h.absent("E2E-L2", &["prepayment", "final", "proforma"])
        .await;
    order_query("E2E-L2")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    let mut sends = h
        .create_lands_on_the_second_send(
            &Doc {
                external_id: Some("acct:E2E-L2:invoice"),
                ..Doc::of("SZ-L2", "SZ", "E2E-L2")
            },
            szlahu_down(),
        )
        .await;

    let body = create_body(dec!(1000), false);
    // The second execution's fetch (the first execution's is the first).
    let hold = h.multi().hold_fetch(SCOPE, 2);
    let watch = h.watch("E2E-L2");
    let (first, second, (in_flight, released, second_send)) = tokio::join!(
        timed(h.call_scoped(SCOPE, "E2E-L2", "create_invoice", &body, "e2e-l2-k1")),
        async {
            // The first execution sent (and was answered szlahu_down); the
            // second execution is parked at its fetch.
            hold.reached().await;
            timed(h.call_scoped(SCOPE, "E2E-L2", "create_invoice", &body, "e2e-l2-k2")).await
        },
        async {
            hold.reached().await;
            assert_eq!(
                h.create_bodies_of("E2E-L2").await.len(),
                1,
                "the first execution sent before the second reached its fetch"
            );
            // The server holds both: the first parked in its second execution,
            // the second queued behind it.
            let in_flight = h.await_in_flight_on("E2E-L2", 2).await;
            assert_eq!(
                h.create_bodies_of("E2E-L2").await.len(),
                1,
                "the second call was queued before the second send: {in_flight:?}"
            );
            let released = Instant::now();
            hold.release();
            sends.received(2).await;
            (in_flight, released, Instant::now())
        },
    );
    let retries = watch.finish().await;
    assert!(
        in_flight.contains(&first.reply.invocation_id().to_owned()),
        "the first invocation was one of the two in flight: {in_flight:?}"
    );
    assert!(
        released < second_send,
        "the second call was on the server before szamlazz.hu received the second send \
         (released {released:?}, second send {second_send:?})"
    );
    assert_eq!(
        retries.failing_commands,
        ["create-invoice"],
        "the first call's create step is what re-executed: {retries:?}"
    );
    assert_second_call_queued_behind_the_first(h, "SZ-L2", first, second).await;
    assert_eq!(
        h.create_bodies_of("E2E-L2").await.len(),
        2,
        "two sends, both the first call's: the one szamlazz.hu did not act on and the one that landed"
    );
}

/// The same `Idempotency-Key` sent while the first invocation is in
/// flight **attaches** to it: one invocation id on both replies, the same body
/// (`issued`), one create on the wire, one invocation on the order. What the
/// glossary's *Idempotency-Key* entry asks the caller to do after **no
/// answer**: keep the key, since a new one would queue a second invocation
/// behind the lock, as the first race shows. The completed case (the stored
/// completion replayed) is (ii); that this is not it is the hold's doing: the
/// first invocation is parked at its credential fetch while the retry is
/// sent, the retry stays **unanswered** for as long as the hold is held (a
/// replayed completion would answer at once) and adds **no row** to
/// `sys_invocation` (a second invocation would), and only the release lets
/// both be answered, together, with one id.
pub(crate) async fn same_idempotency_key_in_flight_attaches_to_the_invocation(h: &Harness) {
    h.reset().await;
    h.absent("E2E-L3", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-L3")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_for("E2E-L3")
        .respond_with(created("SZ-L3", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let body = create_body(dec!(1000), false);
    let hold = h.multi().hold_fetch(SCOPE, 1);
    let (first, (in_flight, retry)) = tokio::join!(
        timed(h.call_scoped(SCOPE, "E2E-L3", "create_invoice", &body, "e2e-l3-shared")),
        async {
            hold.reached().await;
            let in_flight = h.in_flight_on("E2E-L3").await;
            let mut retry =
                pin!(h.call_scoped(SCOPE, "E2E-L3", "create_invoice", &body, "e2e-l3-shared"));
            // While the first is held nothing can answer the retry: it waits
            // on the in-flight invocation, and is not a second one.
            assert!(
                tokio::time::timeout(Duration::from_secs(1), &mut retry)
                    .await
                    .is_err(),
                "the retry has no answer while the invocation it attached to is held"
            );
            assert_eq!(
                h.await_in_flight_on("E2E-L3", 1).await,
                std::slice::from_ref(&in_flight),
                "one invocation on the order: the retry attached, it did not queue a second one"
            );
            hold.release();
            (in_flight, timed(retry).await)
        },
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
    assert_eq!(
        h.create_bodies_of("E2E-L3").await.len(),
        1,
        "one create on the wire"
    );
    let on_the_order = h
        .sql("SELECT id, target_handler_name, idempotency_key FROM sys_invocation WHERE target_service_key = 'E2E-L3'")
        .await;
    assert_eq!(
        on_the_order.len(),
        1,
        "one invocation on the order after both were answered: {on_the_order:?}"
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
}
