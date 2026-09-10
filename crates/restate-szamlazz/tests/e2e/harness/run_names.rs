//! The *step-name table*: the durable steps of every handler of both
//! services as the `ctx.run` names they journal ([`RUN_NAMES`]), held as
//! the harness crate's [`Table`] ([`TABLE`]: a journaled name read as its
//! pattern, the check of a whole run). The check itself (over every
//! invocation the server holds and every handler its deployments offer) is
//! the last scenario (`invariants`); the check's own tests, on scripted
//! rows, are the crate's.

use std::sync::LazyLock;

use restate_e2e_harness::{RunPath, Table};

/// The durable steps of every handler of both services, in order, as the
/// `ctx.run` names they journal. Deployments are immutable (ADR 0009), so an
/// in-flight invocation never replays against a later release's code by
/// itself; what does replay a journal on other code is Restate's *pause and
/// resume on a new deployment*, which needs the same run sequence, result
/// types that decode and unchanged inputs: this table is the sequence part,
/// and its diff between two releases is that part's answer. A
/// `{number}` / `{prefix}` segment is a parameter ([`Table::pattern`]); a
/// handler with two rows has two paths. The check ([`Table::check`]) holds
/// when every observed sequence of a handler is a prefix of one of its paths
/// (a handler that answers early journals the first steps only), every
/// handler the deployments offer has a row (so a handler added with neither a
/// row nor a scenario is not invisible) and every path is observed in full at
/// least once in the run. The
/// parameter of a parametrized name is matched by its prefix only: a number
/// that itself began with a fixed stem (`storno-1`) would read as the longer
/// pattern; none of the suite's do. The walk requirement sizes the suite:
/// every row is walked by at least one phase-1 or phase-2 scenario, and a row
/// whose only walker were removed fails the check (#134).
pub(crate) const RUN_NAMES: &[RunPath] = &[
    RunPath::new(
        "Szamlazz.Order",
        "create_proforma",
        &[
            "namespace",
            "account",
            "lookup-proforma",
            "lookup-invoice",
            "lookup-prepayment",
            "lookup-final",
            "lookup-proforma",
            "create-proforma",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_invoice",
        &[
            "namespace",
            "account",
            "lookup-invoice",
            "lookup-prepayment",
            "lookup-final",
            "lookup-proforma",
            "lookup-invoice",
            "create-invoice",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_invoice",
        &[
            "namespace",
            "account",
            "lookup-invoice",
            "lookup-prepayment",
            "lookup-final",
            "verify-proforma-{number}",
            "lookup-invoice",
            "create-invoice",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_prepayment",
        &[
            "namespace",
            "account",
            "lookup-prepayment",
            "lookup-invoice",
            "lookup-final",
            "lookup-proforma",
            "lookup-prepayment",
            "create-prepayment",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_prepayment",
        &[
            "namespace",
            "account",
            "lookup-prepayment",
            "lookup-invoice",
            "lookup-final",
            "verify-proforma-{number}",
            "lookup-prepayment",
            "create-prepayment",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_final",
        &[
            "namespace",
            "account",
            "lookup-final",
            "lookup-prepayment",
            "lookup-final",
            "create-final",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "correct_invoice",
        &[
            "namespace",
            "account",
            "lookup-corrective",
            "verify-base-{number}",
            "lookup-corrective",
            "create-corrective",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_proforma",
        &[
            "namespace",
            "account",
            "lookup-proforma",
            "hint-storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_invoice",
        &[
            "namespace",
            "account",
            "lookup-invoice",
            "hint-storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_prepayment",
        &[
            "namespace",
            "account",
            "lookup-prepayment",
            "hint-storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "create_final",
        &[
            "namespace",
            "account",
            "lookup-final",
            "hint-storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "storno_invoice",
        &[
            "namespace",
            "account",
            "verify-original-{number}",
            "lookup-storno-{number}",
            "storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "storno_invoice",
        &[
            "namespace",
            "account",
            "verify-original-{number}",
            "hint-storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "delete_proforma",
        &[
            "namespace",
            "account",
            "lookup-proforma",
            "delete-proforma-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "get",
        &[
            "namespace",
            "account",
            "lookup-proforma",
            "lookup-invoice",
            "lookup-prepayment",
            "lookup-final",
        ],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "check_account",
        &["namespace", "account", "probe"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "query",
        &["namespace", "account", "query"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "query_taxpayer",
        &["namespace", "account", "lookup-taxpayer-{prefix}"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "set_credit_entries",
        &["namespace", "account", "set-credit-entries-{number}"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "storno",
        &[
            "namespace",
            "account",
            "verify-original-{number}",
            "lookup-storno-{number}",
            "storno-{number}",
        ],
    ),
];

/// [`RUN_NAMES`] as the crate's [`Table`], built once: its patterns are
/// derived from the rows, so the two cannot disagree.
pub(crate) static TABLE: LazyLock<Table> = LazyLock::new(|| Table::new(RUN_NAMES));
