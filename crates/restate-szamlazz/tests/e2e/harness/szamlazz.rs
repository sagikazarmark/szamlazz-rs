//! What szamlazz.hu holds, as wiremock stubs: the shared fixtures of
//! `tests/common` (the document renderer [`Doc`], the response templates,
//! the selector matchers; re-exported) and the document-centric mount helpers
//! this suite adds ([`holds`] and its siblings), which put one body on every
//! selector a document is reachable by, so the stubs cannot disagree. Every
//! stub a scenario mounts is discriminated by what it is about (an order key
//! on a create, a number on a storno or a credit entry, an external id on a
//! query), never by position in time: phase 1 mounts once and runs its
//! scenarios concurrently, and nothing is reset between them. The helpers'
//! own tests, against wiremock alone, close the file.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tokio::sync::watch;
use wiremock::matchers::body_string_contains;
use wiremock::{MockBuilder, MockServer, Request, ResponseTemplate};

pub(crate) use crate::common::{
    Doc, agent_key_tag, api_error, create_for, create_with_bank_account, create_with_key, created,
    credit_of, credited, delete_of, external_id_query, http_client, not_found, number_query,
    order_query, original_telj_tag, proforma_deleted, storno_of_number,
    storno_of_number_repeating_telj, szlahu_down, taxpayer_known, taxpayer_query_with_key,
    taxpayer_unknown,
};

/// The body of a `storno_invoice` / `Szamlazz.Agent.storno` call on `number`.
pub(crate) fn storno_of(number: &str) -> Value {
    json!({ "invoice_number": number })
}

/// A storno of `number` that must not reach szamlazz.hu: a handler that
/// stops before sending.
pub(crate) async fn storno_never_sent(mock: &MockServer, number: &str) {
    storno_of_number(number)
        .respond_with(created("SS-X", "-1000", "-1270"))
        .expect(0)
        .mount(mock)
        .await;
}

/// A create of `order` that must not reach szamlazz.hu.
pub(crate) async fn create_never_sent(mock: &MockServer, order: &str) {
    create_for(order)
        .respond_with(created("X-NEVER", "1000", "1270"))
        .expect(0)
        .mount(mock)
        .await;
}

/// The sentinel external id `check_account` probes under the run's namespace.
const PROBE_ID: &str = "acct:check-account";

/// The probe's query carrying `agent_key`: which account's key was checked.
pub(crate) fn probe_with_key(agent_key: &str) -> MockBuilder {
    external_id_query(PROBE_ID).and(body_string_contains(agent_key_tag(agent_key)))
}

// ----- what szamlazz.hu holds: one document, every selector ------------------

/// szamlazz.hu holds `doc`: one body on every selector the document is
/// reachable by (its number, its order number when it carries one, its
/// external id when the test states one), so the stubs cannot disagree.
pub(crate) async fn holds(mock: &MockServer, doc: &Doc<'_>) {
    let selectors = [
        Some(number_query(doc.number)),
        doc.order.map(order_query),
        doc.external_id.map(external_id_query),
    ];
    for selector in selectors.into_iter().flatten() {
        selector.respond_with(doc.response()).mount(mock).await;
    }
}

/// The next external-id query for `id` loses its reply (a 500 with no body)
/// once; whatever is mounted after this answers from the second query on:
/// wiremock takes the first active match in mount order, and an
/// `up_to_n_times(1)` mock is inactive after its one match.
pub(crate) async fn loses_reply_once(mock: &MockServer, id: &str) {
    external_id_query(id)
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .mount(mock)
        .await;
}

/// szamlazz.hu holds `doc` under its external id from the `misses + 1`th
/// query on: code 7 for `misses` queries, the document afterwards. The number
/// and order selectors are not mounted: the document is absent before the
/// misses and nothing reads it by number or order after. `doc` must state its
/// external id.
pub(crate) async fn holds_after_misses(mock: &MockServer, misses: u64, doc: &Doc<'_>) {
    let id = doc
        .external_id
        .expect("holds_after_misses needs the document's external id");
    external_id_query(id)
        .respond_with(not_found())
        .up_to_n_times(misses)
        .mount(mock)
        .await;
    external_id_query(id)
        .respond_with(doc.response())
        .mount(mock)
        .await;
}

/// The create of the document's order lands on szamlazz.hu but its reply is
/// lost: the create answers 500, `expect(1)`, and `doc` is the holder of its
/// external id from the moment the create request is received (code 7
/// before, the document after). The transition is the create stub being
/// matched (one flag, flipped by the create's responder and read by the
/// external id's), so how many queries precede the send is not the test's to
/// know. `doc` must state its external id and its order; the number and
/// order selectors are not mounted.
pub(crate) async fn create_lands_but_reply_lost(mock: &MockServer, doc: &Doc<'_>) {
    let _sends = create_lands_when(mock, doc, vec![(ResponseTemplate::new(500), true)]).await;
}

/// The create lands and is answered, but its reply takes `delay` to arrive:
/// `doc` is the holder of its external id from the moment the create request
/// is **received** (code 7 before), while its sender is still waiting for
/// szamlazz.hu's answer. wiremock records the request and runs the responder
/// at receipt and sleeps the delay outside its lock, so a query in that window
/// finds the document at once. The window a second caller on the same order
/// or a cancellation arrives in, and the returned [`Sends`] is how a scenario
/// knows the window is open. `expect(1)`; `doc` must state its external id and
/// its order; the number and order selectors are not mounted.
pub(crate) async fn create_lands_slowly(
    mock: &MockServer,
    doc: &Doc<'_>,
    delay: Duration,
) -> Sends {
    create_lands_when(
        mock,
        doc,
        vec![(created(doc.number, "1000", "1270").set_delay(delay), true)],
    )
    .await
}

/// The first create request is answered `first` **without landing** (a reply
/// to which szamlazz.hu did not act: `szlahu_down`, a 500) and the document
/// stays absent; the second lands, is answered `created` at once, and `doc` is
/// the holder of its external id from that request's receipt. What a create
/// step that re-executes after an *Unconfirmed* first send meets. `expect(2)`;
/// `doc` must state its external id and its order; the number and order
/// selectors are not mounted.
pub(crate) async fn create_lands_on_the_second_send(
    mock: &MockServer,
    doc: &Doc<'_>,
    first: ResponseTemplate,
) -> Sends {
    create_lands_when(
        mock,
        doc,
        vec![(first, false), (created(doc.number, "1000", "1270"), true)],
    )
    .await
}

/// How many create requests the stub has **received**, as a signal a scenario
/// awaits: the moment szamlazz.hu has the send and its reply is still on its
/// way (a delayed stub), or the moment the first of two sends is answered. The
/// count moves at the responder, which wiremock runs at receipt, before any
/// delay: a transport-side fact, not a guess from the client's clock. The
/// document's landing is published **with** the count (both under the one
/// write that notifies the receiver), so a query made after `received`
/// resolves finds what the counted request landed.
#[derive(Clone)]
pub(crate) struct Sends(watch::Receiver<u64>);

impl Sends {
    /// How long [`received`](Self::received) waits before failing the
    /// scenario.
    const DEADLINE: Duration = Duration::from_secs(30);

    /// Resolves once the stub has received at least `count` create requests;
    /// fails the scenario when it has not within [`Self::DEADLINE`].
    pub(crate) async fn received(&mut self, count: u64) {
        let received = self.0.wait_for(|seen| *seen >= count);
        tokio::time::timeout(Self::DEADLINE, received)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "szamlazz.hu did not receive {count} create request(s) within {:?}",
                    Self::DEADLINE
                )
            })
            .expect("the create stub outlives the scenario");
    }
}

/// szamlazz.hu holds `doc` under its external id from the moment a create of
/// its order **lands** (code 7 before). `answers` are szamlazz.hu's replies
/// to the create requests in the order they are received, each with whether
/// that request lands: the flag flips at its receipt, whatever the reply or
/// its delay, since a document exists on szamlazz.hu once the send reaches
/// it. The create stub expects exactly `answers.len()` requests of the order;
/// one beyond the list is answered 500 without landing and fails the `expect`
/// at the next `verify`. Failure-injection sequencing, not a model of
/// szamlazz.hu: one flag for one document. The three helpers above are its
/// callers; the [`Sends`] counts the requests as they arrive.
async fn create_lands_when(
    mock: &MockServer,
    doc: &Doc<'_>,
    answers: Vec<(ResponseTemplate, bool)>,
) -> Sends {
    let id = doc
        .external_id
        .expect("a landing create needs the document's external id");
    let order = doc
        .order
        .expect("a landing create needs the document's order");
    let landed = Arc::new(AtomicBool::new(false));
    let flip = Arc::clone(&landed);
    let expected = u64::try_from(answers.len()).expect("a few answers");
    let (sends, received) = watch::channel(0u64);
    create_for(order)
        .respond_with(move |_: &Request| {
            // The answer is chosen and the landing recorded under the same
            // write that publishes the count: a receiver woken by the count
            // sees the document landed, never the count ahead of it.
            let mut reply = None;
            sends.send_modify(|count| {
                let index = usize::try_from(*count).expect("a few sends");
                reply = Some(match answers.get(index) {
                    Some((template, lands)) => {
                        if *lands {
                            flip.store(true, Ordering::SeqCst);
                        }
                        template.clone()
                    }
                    None => ResponseTemplate::new(500),
                });
                *count += 1;
            });
            reply.expect("chosen under the write")
        })
        .expect(expected)
        .mount(mock)
        .await;
    let document = doc.response();
    external_id_query(id)
        .respond_with(move |_: &Request| {
            if landed.load(Ordering::SeqCst) {
                document.clone()
            } else {
                not_found()
            }
        })
        .mount(mock)
        .await;
    Sends(received)
}

// ----- the harness's stub helpers, against wiremock alone -----------------------

/// A query as the Számla Agent client puts it on the wire, reduced to what
/// the selector matchers read: the operation's field name and the one
/// selector element.
async fn query_by(mock: &MockServer, selector: &str) -> (u16, String) {
    let response = http_client()
        .post(mock.uri())
        .body(format!("name=\"action-szamla_agent_xml\"\n{selector}"))
        .send()
        .await
        .expect("query");
    let status = response.status().as_u16();
    (status, response.text().await.expect("body"))
}

/// A create as the Számla Agent client puts it on the wire, reduced to what
/// the matchers read: the operation's field name and the order number.
fn create_reply_body(order: &str) -> String {
    format!("name=\"action-xmlagentxmlfile\"\n<rendelesSzam>{order}</rendelesSzam>")
}

/// `holds` mounts one body on every selector the document is reachable by and
/// nothing else: with an order and an external id three stubs, without an
/// order two, without either one; an unmounted selector is wiremock's 404.
#[tokio::test]
async fn holds_answers_every_selector_the_document_is_reachable_by_with_one_body() {
    let mock = MockServer::start().await;
    holds(
        &mock,
        &Doc {
            external_id: Some("acct:ORD-1:invoice"),
            ..Doc::of("SZ-1", "SZ", "ORD-1")
        },
    )
    .await;
    holds(&mock, &Doc::of("SZ-2", "SZ", "ORD-2")).await;
    holds(&mock, &Doc::unmanaged("SZ-3", "SZ")).await;

    let mut bodies = Vec::new();
    for selector in [
        "<szamlaszam>SZ-1</szamlaszam>",
        "<rendelesSzam>ORD-1</rendelesSzam>",
        "<szamlaKulsoAzon>acct:ORD-1:invoice</szamlaKulsoAzon>",
    ] {
        let (status, body) = query_by(&mock, selector).await;
        assert_eq!(status, 200, "{selector}");
        assert!(
            body.contains("<szamlaszam>SZ-1</szamlaszam>"),
            "{selector}: {body}"
        );
        bodies.push(body);
    }
    assert!(
        bodies.iter().all(|body| body == &bodies[0]),
        "the three selectors answer one body"
    );

    for selector in [
        "<szamlaszam>SZ-2</szamlaszam>",
        "<rendelesSzam>ORD-2</rendelesSzam>",
    ] {
        let (status, body) = query_by(&mock, selector).await;
        assert_eq!(status, 200, "{selector}");
        assert!(
            body.contains("<szamlaszam>SZ-2</szamlaszam>"),
            "{selector}: {body}"
        );
    }
    let (status, _) = query_by(
        &mock,
        "<szamlaKulsoAzon>acct:ORD-2:invoice</szamlaKulsoAzon>",
    )
    .await;
    assert_eq!(status, 404, "no external id was stated: no stub");

    let (status, body) = query_by(&mock, "<szamlaszam>SZ-3</szamlaszam>").await;
    assert_eq!(status, 200);
    assert!(body.contains("<szamlaszam>SZ-3</szamlaszam>"), "{body}");
    let (status, _) = query_by(&mock, "<rendelesSzam>ORD-3</rendelesSzam>").await;
    assert_eq!(
        status, 404,
        "an unmanaged document is reachable by number only"
    );
}

/// `holds_after_misses(n, doc)` answers the document's external id with code
/// 7 exactly `n` times and the document from then on; the number and order
/// selectors are not mounted: the document is absent before the misses.
#[tokio::test]
async fn holds_after_misses_answers_code_7_n_times_then_the_document() {
    let mock = MockServer::start().await;
    holds_after_misses(
        &mock,
        2,
        &Doc {
            external_id: Some("acct:ORD-4:invoice"),
            reversed: true,
            ..Doc::of("SZ-4", "SZ", "ORD-4")
        },
    )
    .await;

    let by_id = "<szamlaKulsoAzon>acct:ORD-4:invoice</szamlaKulsoAzon>";
    for miss in 1..=2 {
        let (status, body) = query_by(&mock, by_id).await;
        assert_eq!(status, 200, "miss {miss}");
        assert!(
            body.contains("<hibakod><![CDATA[7]]></hibakod>"),
            "miss {miss}: {body}"
        );
    }
    for hit in 1..=2 {
        let (status, body) = query_by(&mock, by_id).await;
        assert_eq!(status, 200, "hit {hit}");
        assert!(
            body.contains("<szamlaszam>SZ-4</szamlaszam>"),
            "hit {hit}: {body}"
        );
        assert!(
            body.contains("<sztornozott>true</sztornozott>"),
            "hit {hit}: {body}"
        );
    }
    let (status, _) = query_by(&mock, "<szamlaszam>SZ-4</szamlaszam>").await;
    assert_eq!(status, 404, "the number selector is not mounted");
    let (status, _) = query_by(&mock, "<rendelesSzam>ORD-4</rendelesSzam>").await;
    assert_eq!(status, 404, "the order selector is not mounted");
}

/// `create_lands_but_reply_lost(doc)` answers the document's external id with
/// code 7 until a create **of its order** is received (however many queries
/// precede it, and whatever another order's creates do), and with the
/// document from that moment on; the create itself is a 500. The transition
/// is the create stub being matched, not a query count.
#[tokio::test]
async fn create_lands_but_reply_lost_makes_the_document_the_holder_on_the_create_hit() {
    let mock = MockServer::start().await;
    create_lands_but_reply_lost(
        &mock,
        &Doc {
            external_id: Some("acct:ORD-5:invoice"),
            ..Doc::of("SZ-5", "SZ", "ORD-5")
        },
    )
    .await;

    let by_id = "<szamlaKulsoAzon>acct:ORD-5:invoice</szamlaKulsoAzon>";
    for query in 1..=5 {
        let (status, body) = query_by(&mock, by_id).await;
        assert_eq!(status, 200, "query {query} before the create");
        assert!(
            body.contains("<hibakod><![CDATA[7]]></hibakod>"),
            "query {query} before the create: {body}"
        );
    }
    // Another order's create matches nothing here and lands nothing.
    let other = http_client()
        .post(mock.uri())
        .body(create_reply_body("ORD-6"))
        .send()
        .await
        .expect("another order's create");
    assert_eq!(other.status().as_u16(), 404, "not this order's stub");
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<hibakod><![CDATA[7]]></hibakod>"),
        "another order's create landed nothing here: {body}"
    );

    let response = http_client()
        .post(mock.uri())
        .body(create_reply_body("ORD-5"))
        .send()
        .await
        .expect("create");
    assert_eq!(response.status().as_u16(), 500, "the reply is lost");
    for query in 1..=2 {
        let (status, body) = query_by(&mock, by_id).await;
        assert_eq!(status, 200, "query {query} after the create");
        assert!(
            body.contains("<szamlaszam>SZ-5</szamlaszam>"),
            "query {query} after the create: {body}"
        );
    }
}

/// `create_lands_slowly(doc, delay)` makes the document the holder of its
/// external id the moment the create request is *received*, while the create's
/// own reply is still `delay` away: the [`Sends`] resolves at that receipt
/// with the send's reply still outstanding, a query then finds the document,
/// and the reply, when it comes, is the created document, no sooner than the
/// delay. The order is read off the send task (not finished when the signal
/// resolves) and the delay off a lower bound; no upper bound on the clock,
/// which a loaded host would break without the stub being wrong.
#[tokio::test]
async fn create_lands_slowly_makes_the_document_the_holder_while_the_reply_is_in_flight() {
    const DELAY: Duration = Duration::from_secs(2);
    let mock = MockServer::start().await;
    let mut sends = create_lands_slowly(
        &mock,
        &Doc {
            external_id: Some("acct:ORD-6:invoice"),
            ..Doc::of("SZ-6", "SZ", "ORD-6")
        },
        DELAY,
    )
    .await;

    let by_id = "<szamlaKulsoAzon>acct:ORD-6:invoice</szamlaKulsoAzon>";
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<hibakod><![CDATA[7]]></hibakod>"),
        "absent before the create: {body}"
    );

    let started = Instant::now();
    let send = tokio::spawn({
        let uri = mock.uri();
        async move {
            http_client()
                .post(uri)
                .body(create_reply_body("ORD-6"))
                .send()
                .await
                .expect("create")
        }
    });
    // The signal resolves at receipt: the request is recorded and the send
    // has no reply yet. The landing was published with the count, so the
    // query finds the document whenever it is made.
    sends.received(1).await;
    assert!(
        !send.is_finished(),
        "the receipt was signalled while the send's reply was still outstanding"
    );
    assert_eq!(mock.received_requests().await.expect("requests").len(), 2);
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<szamlaszam>SZ-6</szamlaszam>"),
        "the holder from the receipt on: {body}"
    );

    let response = send.await.expect("join");
    assert!(
        started.elapsed() >= DELAY,
        "the reply waited the delay: {:?}",
        started.elapsed()
    );
    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(
        response
            .headers()
            .get("szlahu_szamlaszam")
            .and_then(|value| value.to_str().ok()),
        Some("SZ-6"),
        "the created document"
    );
}

/// `create_lands_on_the_second_send(doc, first)` answers the first create
/// request `first` without landing (the document stays absent), and the
/// second `created`, the document being the holder of its external id from
/// the second request's receipt; the [`Sends`] counts both.
#[tokio::test]
async fn create_lands_on_the_second_send_keeps_the_document_absent_until_the_second_create() {
    let mock = MockServer::start().await;
    let mut sends = create_lands_on_the_second_send(
        &mock,
        &Doc {
            external_id: Some("acct:ORD-7:invoice"),
            ..Doc::of("SZ-7", "SZ", "ORD-7")
        },
        szlahu_down(),
    )
    .await;

    let by_id = "<szamlaKulsoAzon>acct:ORD-7:invoice</szamlaKulsoAzon>";
    let create = || {
        http_client()
            .post(mock.uri())
            .body(create_reply_body("ORD-7"))
            .send()
    };
    let first = create().await.expect("first create");
    assert_eq!(first.status().as_u16(), 503, "the first answer");
    assert!(first.headers().contains_key("szlahu_down"));
    sends.received(1).await;
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<hibakod><![CDATA[7]]></hibakod>"),
        "still absent after a send that did not land: {body}"
    );

    let second = create().await.expect("second create");
    sends.received(2).await;
    assert_eq!(second.status().as_u16(), 200, "the second lands");
    assert_eq!(
        second
            .headers()
            .get("szlahu_szamlaszam")
            .and_then(|value| value.to_str().ok()),
        Some("SZ-7")
    );
    let (_, body) = query_by(&mock, by_id).await;
    assert!(
        body.contains("<szamlaszam>SZ-7</szamlaszam>"),
        "the holder from the second send on: {body}"
    );
}
