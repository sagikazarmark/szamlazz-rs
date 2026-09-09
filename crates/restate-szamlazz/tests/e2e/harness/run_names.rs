//! The *step-name table*: the durable steps of every handler of both
//! services as the `ctx.run` names they journal ([`RUN_NAMES`]), read
//! through the harness crate's matcher (`restate_e2e_harness::run_names`: a
//! journaled name as its pattern, [`run_pattern`], and the prefix rule,
//! [`is_prefix_of_path`]). The check itself (over every invocation of the
//! run) is the last scenario (`pins`); the matcher's own tests are the
//! crate's.

use std::sync::LazyLock;

pub(crate) use restate_e2e_harness::is_prefix_of_path;
use restate_e2e_harness::{RunPath, RunPatterns};

/// The durable steps of every handler of both services, in order, as the
/// `ctx.run` names they journal. Deployments are immutable (ADR 0009), so an
/// in-flight invocation never replays against a later release's code by
/// itself; what does replay a journal on other code is Restate's *pause and
/// resume on a new deployment*, which needs the same run sequence, result
/// types that decode and unchanged inputs: this table is the sequence part,
/// and its diff between two releases is that part's answer. A
/// `{number}` / `{prefix}` segment is a parameter ([`run_pattern`]); a
/// handler with two rows has two paths. The check holds when every observed
/// sequence of a handler is a prefix of one of its paths (a handler that
/// answers early journals the first steps only; see [`is_prefix_of_path`])
/// and every path is observed in full at least once in the run. The
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
            "exclusivity-invoice",
            "exclusivity-prepayment",
            "exclusivity-final",
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
            "exclusivity-prepayment",
            "exclusivity-final",
            "proforma-link",
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
            "exclusivity-prepayment",
            "exclusivity-final",
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
            "exclusivity-invoice",
            "exclusivity-final",
            "proforma-link",
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
            "exclusivity-invoice",
            "exclusivity-final",
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
            "prepayment-for-final",
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
            "verify-base-{number}",
            "lookup-corrective",
            "create-corrective",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "storno_invoice",
        &[
            "namespace",
            "account",
            "verify-storno-{number}",
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
            "verify-storno-{number}",
            "hint-storno-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "delete_proforma",
        &[
            "namespace",
            "account",
            "proforma-for-delete",
            "delete-proforma-{number}",
        ],
    ),
    RunPath::new(
        "Szamlazz.Order",
        "get",
        &[
            "namespace",
            "account",
            "get-proforma",
            "get-invoice",
            "get-prepayment",
            "get-final",
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
        &["namespace", "account", "taxpayer-{prefix}"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "set_payments",
        &["namespace", "account", "set-payments-{number}"],
    ),
    RunPath::new(
        "Szamlazz.Agent",
        "storno",
        &[
            "namespace",
            "account",
            "verify-{number}",
            "lookup-storno-{number}",
            "storno-{number}",
        ],
    ),
];

/// The patterns of [`RUN_NAMES`], derived once from the table so the two
/// cannot disagree.
static PATTERNS: LazyLock<RunPatterns> = LazyLock::new(|| RunPatterns::of(RUN_NAMES));

/// The [`RUN_NAMES`] pattern of a journaled run name: a parametrized name by
/// its prefix, the longest first (`verify-storno-…` is
/// `verify-storno-{number}`, never `verify-{number}`), any other name as it
/// is.
pub(crate) fn run_pattern(name: &str) -> String {
    PATTERNS.pattern(name)
}
