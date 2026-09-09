//! Phase 2: the single → multi flag day and the isolation multi-account mode
//! leans on: the scope namespacing the Virtual Object key and the
//! `Idempotency-Key` (the same order key and the same key under two scopes
//! are two objects, two invocations), an account change and a credential
//! rotation between two executions of one step.

use restate_e2e_harness::run_result;
use rust_decimal::dec;
use wiremock::ResponseTemplate;

use restate_szamlazz::contract::TerminalCode;

use crate::harness::accounts::{AGENT_KEY, BANK_ACCOUNT, BANK_ACCOUNT_CHANGED, KEY_B, KEY_B_V2};
use crate::harness::szamlazz::{
    Doc, agent_key_tag, create_never_sent, create_with_bank_account, create_with_key, created,
    holds, not_found, order_query,
};
use crate::harness::{Harness, create_body};

/// The single → multi flag day. While the services are private the
/// ingress refuses a call without creating an invocation; after the drain
/// and the switch (same namespace, the same szamlazz.hu account now under
/// scope `acme`) the first scoped create for an order the single-account
/// phase invoiced finds it under the unchanged external id
/// (`already_issued`); an unscoped call on the multi-account deployment is
/// `unknown_account` (400) with `namespace` and `account` journaled and
/// nothing else, and zero szamlazz.hu requests.
pub(crate) async fn flag_day_keeps_the_documents_and_refuses_unscoped_calls(h: &mut Harness) {
    h.reset().await;

    // Private: the ingress refuses the call itself; nothing reaches the
    // handler or szamlazz.hu.
    h.set_public(false).await;
    let reply = h
        .call(
            "E2E-16",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-16-private",
        )
        .await;
    assert_eq!(reply.status, 400, "a private service: {}", reply.body);
    assert_eq!(reply.invocation_id, None, "no invocation was created");
    assert!(h.requests_of_order("E2E-16").await.is_empty());

    h.switch_to_multi_account().await;

    // The document issued unscoped in phase 1 (E2E-1 → SZ-1) is found by the
    // first scoped create under `acme`: the external id did not change.
    h.absent("E2E-1", &["prepayment", "final", "proforma"])
        .await;
    holds(
        &h.mock,
        &Doc {
            external_id: Some("acct:E2E-1:invoice"),
            ..Doc::of("SZ-1", "SZ", "E2E-1")
        },
    )
    .await;
    create_never_sent(&h.mock, "E2E-1").await;
    let reply = h
        .call_scoped(
            "acme",
            "E2E-1",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-1-scoped-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "already_issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-1");
    assert_eq!(reply.body["external_id"], "acct:E2E-1:invoice");
    let invocation = h.admin().invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope.as_deref(), Some("acme"), "{invocation:?}");
    let journal = h.admin().journal(reply.invocation_id()).await;
    let account = run_result(&journal, "account").expect("the account result");
    assert!(
        account.raw_contains("\"id\":\"acme\""),
        "{:?}",
        String::from_utf8_lossy(&account.raw)
    );

    // Unscoped on the multi-account deployment: refused before anything is
    // issued, the resolution journaled as data.
    h.reset().await;
    let reply = h
        .call(
            "E2E-16",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-16-unscoped",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::UnknownAccount, "{fault:?}");
    assert!(fault.message.contains("unscoped"), "{fault:?}");
    assert!(
        fault.message.contains("/restate/scope/"),
        "the fault tells the caller how to address an account: {fault:?}"
    );
    assert_eq!(
        h.admin().runs(reply.invocation_id()).await,
        ["namespace", "account"]
    );
    let invocation = h.admin().invocation(reply.invocation_id()).await;
    assert_eq!(invocation.scope, None, "{invocation:?}");
    assert!(
        h.requests_of_order("E2E-16").await.is_empty(),
        "nothing reached szamlazz.hu"
    );

    // A scope no account is reachable by is unknown the same way.
    let reply = h
        .call_scoped(
            "gamma",
            "E2E-16",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-16-gamma",
        )
        .await;
    assert_eq!(reply.status, 400, "{}", reply.body);
    let fault = reply.fault();
    assert_eq!(fault.code, TerminalCode::UnknownAccount, "{fault:?}");
    assert!(fault.message.contains("gamma"), "{fault:?}");
    assert!(h.requests_of_order("E2E-16").await.is_empty());
}

/// The scope namespaces both identities Restate keys an invocation by. The
/// same order key under scopes `acme` and `beta`, concurrently, is two
/// Virtual Objects: two `issued`, each account's own agent key on the create
/// wire exactly once (the prologue opens each execution's gateway on its own
/// account; the lookup queries carry the key as well, the create bodies are
/// what identify *which account issued*), the same external id on two
/// szamlazz.hu accounts. And the **same** `Idempotency-Key` under two scopes
/// is two invocations (two `x-restate-id`s, two documents), because Restate
/// hashes the scope into the idempotency identity; the key replayed under
/// either scope returns that scope's own stored completion without a call.
#[allow(
    clippy::too_many_lines,
    reason = "one scenario: the two identities the scope namespaces"
)]
pub(crate) async fn the_scope_namespaces_the_order_key_and_the_idempotency_key(h: &Harness) {
    // The same order key, two scopes, at once.
    h.reset().await;
    h.absent("E2E-17", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-17")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(AGENT_KEY)
        .respond_with(created("SZ-A17", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B)
        .respond_with(created("SZ-B17", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let body = create_body(dec!(1000), false);
    let (acme, beta) = tokio::join!(
        h.call_scoped("acme", "E2E-17", "create_invoice", &body, "e2e-17-acme"),
        h.call_scoped("beta", "E2E-17", "create_invoice", &body, "e2e-17-beta"),
    );
    assert_eq!(acme.status, 200, "{}", acme.body);
    assert_eq!(beta.status, 200, "{}", beta.body);
    assert_eq!(acme.body["outcome"], "issued", "{}", acme.body);
    assert_eq!(beta.body["outcome"], "issued", "{}", beta.body);
    assert_eq!(
        acme.body["invoice_number"], "SZ-A17",
        "acme's key issued acme's document"
    );
    assert_eq!(
        beta.body["invoice_number"], "SZ-B17",
        "beta's key issued beta's document"
    );
    assert_eq!(acme.body["external_id"], "acct:E2E-17:invoice");
    assert_eq!(
        beta.body["external_id"], "acct:E2E-17:invoice",
        "the same namespace and order: the same external id on two szamlazz.hu accounts"
    );
    let creates = h.create_bodies_of("E2E-17").await;
    assert_eq!(creates.len(), 2, "one create per account");
    for key in [AGENT_KEY, KEY_B] {
        assert_eq!(
            creates
                .iter()
                .filter(|body| body.contains(&agent_key_tag(key)))
                .count(),
            1,
            "{key} on the create wire exactly once"
        );
    }
    for (reply, scope, id) in [(&acme, "acme", "acme"), (&beta, "beta", "beta")] {
        let invocation = h.admin().invocation(reply.invocation_id()).await;
        assert_eq!(invocation.scope.as_deref(), Some(scope), "{invocation:?}");
        assert_eq!(invocation.status, "completed");
        let journal = h.admin().journal(reply.invocation_id()).await;
        let account = run_result(&journal, "account").expect("the account result");
        assert!(
            account.raw_contains(&format!("\"id\":\"{id}\"")),
            "{scope}: {:?}",
            String::from_utf8_lossy(&account.raw)
        );
    }

    // The same `Idempotency-Key`, two scopes.
    h.reset().await;
    h.absent("E2E-17B", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-17B")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(AGENT_KEY)
        .respond_with(created("SZ-A17B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B)
        .respond_with(created("SZ-B17B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let acme = h
        .call_scoped("acme", "E2E-17B", "create_invoice", &body, "e2e-17b-shared")
        .await;
    let beta = h
        .call_scoped("beta", "E2E-17B", "create_invoice", &body, "e2e-17b-shared")
        .await;
    assert_eq!(acme.status, 200, "{}", acme.body);
    assert_eq!(beta.status, 200, "{}", beta.body);
    assert_eq!(acme.body["outcome"], "issued", "{}", acme.body);
    assert_eq!(beta.body["outcome"], "issued", "{}", beta.body);
    assert_eq!(acme.body["invoice_number"], "SZ-A17B");
    assert_eq!(beta.body["invoice_number"], "SZ-B17B");
    assert_ne!(
        acme.invocation_id(),
        beta.invocation_id(),
        "the same Idempotency-Key under two scopes is two invocations"
    );
    assert_eq!(
        h.create_bodies_of("E2E-17B").await.len(),
        2,
        "two documents"
    );

    // The key again under each scope replays that scope's own completion.
    let before = h.requests_of_order("E2E-17B").await.len();
    for (scope, original) in [("acme", &acme), ("beta", &beta)] {
        let replay = h
            .call_scoped(scope, "E2E-17B", "create_invoice", &body, "e2e-17b-shared")
            .await;
        assert_eq!(replay.status, 200, "{}", replay.body);
        assert_eq!(
            replay.body["invoice_number"], original.body["invoice_number"],
            "{scope}"
        );
        assert_eq!(replay.invocation_id(), original.invocation_id(), "{scope}");
    }
    assert_eq!(
        h.requests_of_order("E2E-17B").await.len(),
        before,
        "replays, not calls"
    );
}

/// `acme`'s seller bank account changes between two executions of a
/// create step (the first loses its reply): the second execution's create
/// carries the **journaled** account's bank account (the invocation
/// finishes on the account it started on), and only new invocations see the
/// change. The change is made while the second execution is held at its
/// fetch (its `account` step already replayed; a hold on the store, not a
/// race against the run retry delay), so it is certainly in place before the second
/// execution opens its gateway and sends.
pub(crate) async fn account_change_between_executions_does_not_reach_the_invocation(h: &Harness) {
    h.reset().await;
    h.absent("E2E-19", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-19")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    // The first create with the journaled bank account loses its reply; the
    // second, still with it, lands. The changed one never reaches the wire.
    create_with_bank_account(BANK_ACCOUNT)
        .respond_with(ResponseTemplate::new(500))
        .up_to_n_times(1)
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_bank_account(BANK_ACCOUNT)
        .respond_with(created("SZ-19", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_bank_account(BANK_ACCOUNT_CHANGED)
        .respond_with(created("SZ-X", "1000", "1270"))
        .expect(0)
        .mount(&h.mock)
        .await;

    let body = create_body(dec!(1000), false);
    // The second execution's fetch (the first execution's is the first).
    let hold = h.multi().hold_fetch("acme", 2);
    let call = h.call_scoped("acme", "E2E-19", "create_invoice", &body, "e2e-19-k1");
    let change = async {
        hold.reached().await;
        assert_eq!(
            h.create_bodies_of("E2E-19").await.len(),
            1,
            "the first execution sent before the second reached its fetch"
        );
        h.multi().update("acme", |account| {
            account.seller.bank_account = Some(BANK_ACCOUNT_CHANGED.to_owned());
        });
        hold.release();
    };
    let (reply, ()) = tokio::join!(call, change);
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-19");
    let creates = h.create_bodies_of("E2E-19").await;
    assert_eq!(creates.len(), 2, "two executions of the create step");
    assert!(
        creates
            .iter()
            .all(|body| body.contains(&format!("<bankszamlaszam>{BANK_ACCOUNT}</bankszamlaszam>"))),
        "both executions carry the journaled seller"
    );
    let journal = h.admin().journal(reply.invocation_id()).await;
    let account = run_result(&journal, "account").expect("the account result");
    assert!(
        account.raw_contains(BANK_ACCOUNT) && !account.raw_contains(BANK_ACCOUNT_CHANGED),
        "{:?}",
        String::from_utf8_lossy(&account.raw)
    );
    assert_eq!(
        journal
            .iter()
            .filter(|entry| entry.is_run() && entry.name.as_deref() == Some("account"))
            .count(),
        1,
        "the re-execution replayed the account, it did not resolve again"
    );

    // A new invocation resolves the changed account.
    h.reset().await;
    h.absent("E2E-19B", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-19B")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_bank_account(BANK_ACCOUNT_CHANGED)
        .respond_with(created("SZ-19B", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;
    let reply = h
        .call_scoped(
            "acme",
            "E2E-19B",
            "create_invoice",
            &create_body(dec!(1000), false),
            "e2e-19b-k1",
        )
        .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
}

/// `beta`'s agent key is rotated between two executions of a create
/// step (the first loses its reply): the second execution fetches the
/// credentials again and carries the new key, while the journaled `account`
/// entry is byte-identical before and after; credentials are never in it.
/// The rotation is made while the second execution is held at its fetch (a
/// hold on the store, not a race against the run retry delay), so the fetch that
/// answers it is the one the second execution's gateway opens with.
pub(crate) async fn credential_rotation_between_executions_is_picked_up(h: &Harness) {
    h.reset().await;
    h.absent("E2E-20", &["prepayment", "final", "proforma", "invoice"])
        .await;
    order_query("E2E-20")
        .respond_with(not_found())
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B)
        .respond_with(ResponseTemplate::new(500))
        .expect(1)
        .mount(&h.mock)
        .await;
    create_with_key(KEY_B_V2)
        .respond_with(created("SZ-20", "1000", "1270"))
        .expect(1)
        .mount(&h.mock)
        .await;

    let body = create_body(dec!(1000), false);
    // The second execution's fetch (the first execution's is the first).
    let hold = h.multi().hold_fetch("beta", 2);
    let call = h.call_scoped("beta", "E2E-20", "create_invoice", &body, "e2e-20-k1");
    let rotate = async {
        // The second execution is parked at its fetch: the first execution's
        // create is on the wire and answered, the `account` entry is
        // journaled and replayed. Read the entry as journaled *before* the
        // rotation, then rotate, then let the fetch answer. (Journal entries
        // are immutable, so the comparison below proves the rotation left
        // the second execution's account as journaled: the credentials are
        // not part of it.)
        hold.reached().await;
        assert_eq!(
            h.create_bodies_of("E2E-20").await.len(),
            1,
            "the first execution sent before the second reached its fetch"
        );
        let invocations = h.admin().all_invocations().await;
        let (id, _) = invocations
            .iter()
            .find(|(_, invocation)| {
                invocation.scope.as_deref() == Some("beta")
                    && invocation.handler == "create_invoice"
                    && invocation.status != "completed"
            })
            .expect("the in-flight invocation under beta");
        let before = run_result(&h.admin().journal(id).await, "account")
            .expect("the account result while in flight")
            .raw
            .clone();
        h.multi().rotate("beta", KEY_B_V2);
        hold.release();
        (id.clone(), before)
    };
    let (reply, (id, before)) = tokio::join!(call, rotate);
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert_eq!(reply.body["outcome"], "issued", "{}", reply.body);
    assert_eq!(reply.body["invoice_number"], "SZ-20");
    assert_eq!(reply.invocation_id(), id);

    let creates = h.create_bodies_of("E2E-20").await;
    assert_eq!(creates.len(), 2, "two executions of the create step");
    assert!(
        creates[0].contains(&agent_key_tag(KEY_B)),
        "execution one carried the old key"
    );
    assert!(
        creates[1].contains(&agent_key_tag(KEY_B_V2)),
        "execution two carried the rotated key"
    );
    let journal = h.admin().journal(reply.invocation_id()).await;
    let account = run_result(&journal, "account").expect("the account result");
    assert_eq!(
        account.raw, before,
        "the journaled account is byte-identical"
    );
    assert!(account.raw_contains("\"id\":\"beta\""));
    assert!(
        !account.raw_contains(KEY_B) && !account.raw_contains(KEY_B_V2),
        "neither key is in the account entry"
    );
    assert_eq!(
        journal
            .iter()
            .filter(|entry| entry.is_run() && entry.name.as_deref() == Some("account"))
            .count(),
        1,
        "the re-execution replayed the account, it did not resolve again"
    );
}
