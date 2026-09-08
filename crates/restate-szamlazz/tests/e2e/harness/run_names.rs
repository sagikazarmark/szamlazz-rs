//! The *run-name pin*'s table and matching: the durable steps of every
//! handler of both services as the `ctx.run` names they journal
//! ([`RUN_NAMES`]), a journaled name read as its pattern ([`run_pattern`])
//! and the prefix rule ([`is_prefix_of_path`]). The pin itself (over every
//! invocation of the run) is the last scenario (`pins`); the matching's own
//! tests close this file.

use std::sync::LazyLock;

/// One pinned path of [`RUN_NAMES`]: a handler of a service and the ordered
/// `ctx.run` names it journals on that path.
pub(crate) struct RunPath {
    pub(crate) service: &'static str,
    pub(crate) handler: &'static str,
    pub(crate) path: &'static [&'static str],
}

impl RunPath {
    const fn new(
        service: &'static str,
        handler: &'static str,
        path: &'static [&'static str],
    ) -> Self {
        Self {
            service,
            handler,
            path,
        }
    }
}

/// The durable steps of every handler of both services, in order, as the
/// `ctx.run` names they journal: the part of the journal contract the type
/// fixtures (`tests/journal/`) do not cover. An in-flight invocation replays
/// the *previous* deployment's entries by name and position, so a
/// renamed, inserted or reordered step strands it; this table makes that a
/// failing test instead of a killed invocation. A `{number}` / `{prefix}`
/// segment is a parameter ([`run_pattern`]); a handler with two rows has two
/// paths. The pin holds when every observed sequence of a handler is a prefix
/// of one of its paths (a handler that answers early journals the first steps
/// only; see [`is_prefix_of_path`]) and every path is observed in full at
/// least once in the run. The parameter of a parametrized name is pinned by
/// its prefix only: a number that itself began with a pinned stem (`storno-1`)
/// would read as the longer pattern; none of the suite's do.
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

/// The parametrized run names of [`RUN_NAMES`] (every `{…}` pattern) by
/// the prefix that names the step, longest prefix first, so that a name is
/// read as the most specific pattern it starts with: `verify-storno-…` is
/// `verify-storno-{number}`, never `verify-{number}`. Derived from the table,
/// so the two cannot disagree.
static PARAMETRIZED_RUNS: LazyLock<Vec<(&'static str, &'static str)>> = LazyLock::new(|| {
    let mut patterns: Vec<(&str, &str)> = RUN_NAMES
        .iter()
        .flat_map(|row| row.path.iter())
        .filter_map(|pattern| {
            pattern
                .split_once('{')
                .map(|(prefix, _)| (prefix, *pattern))
        })
        .collect();
    patterns.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
    patterns.dedup();
    patterns
});

/// The [`RUN_NAMES`] pattern of a journaled run name: a parametrized name by
/// its prefix ([`PARAMETRIZED_RUNS`]), any other name as it is.
pub(crate) fn run_pattern(name: &str) -> String {
    PARAMETRIZED_RUNS
        .iter()
        .find(|(prefix, _)| name.starts_with(prefix) && name.len() > prefix.len())
        .map_or_else(|| name.to_owned(), |(_, pattern)| (*pattern).to_owned())
}

/// Whether `observed` (patterns, in journal order) is a prefix of `path`.
pub(crate) fn is_prefix_of_path(observed: &[String], path: &[&str]) -> bool {
    observed.len() <= path.len()
        && observed
            .iter()
            .zip(path)
            .all(|(seen, expected)| seen == expected)
}

// ----- the run-name pin's matching, without a server ----------------------------

/// A parametrized run name is read as its pattern by its prefix, the longest
/// prefix first: `verify-storno-SZ-1` is `verify-storno-{number}`, never
/// `verify-{number}`; a fixed name is itself.
#[test]
fn run_patterns_read_a_parametrized_name_by_its_longest_prefix() {
    for (name, pattern) in [
        ("namespace", "namespace"),
        ("lookup-invoice", "lookup-invoice"),
        ("lookup-storno-SZ-1", "lookup-storno-{number}"),
        ("storno-SZ-1", "storno-{number}"),
        ("verify-SZ-1", "verify-{number}"),
        ("verify-storno-E-TST-2026-1", "verify-storno-{number}"),
        ("verify-proforma-D-1", "verify-proforma-{number}"),
        ("verify-base-SZ-1", "verify-base-{number}"),
        ("hint-storno-SZ-1", "hint-storno-{number}"),
        ("delete-proforma-D-1", "delete-proforma-{number}"),
        ("set-payments-SZ-30", "set-payments-{number}"),
        ("taxpayer-12345678", "taxpayer-{prefix}"),
        ("proforma-for-delete", "proforma-for-delete"),
    ] {
        assert_eq!(run_pattern(name), pattern, "{name}");
    }
}

/// An observed sequence is explained by a path when it is a prefix of it: a
/// handler that answers early journals the first steps only; a renamed step,
/// an inserted one or one out of order is explained by none.
#[test]
fn an_observed_run_sequence_is_a_prefix_of_one_of_its_handlers_paths_or_unexplained() {
    let paths: &[&[&str]] = &[
        &[
            "namespace",
            "account",
            "verify-storno-{number}",
            "lookup-storno-{number}",
            "storno-{number}",
        ],
        &[
            "namespace",
            "account",
            "verify-storno-{number}",
            "hint-storno-{number}",
        ],
    ];
    let explained = |observed: &[&str]| {
        paths.iter().any(|path| {
            is_prefix_of_path(
                &observed
                    .iter()
                    .map(|name| run_pattern(name))
                    .collect::<Vec<_>>(),
                path,
            )
        })
    };
    assert!(
        explained(&[]),
        "nothing journaled (refused before the prologue)"
    );
    assert!(explained(&["namespace", "account"]), "unknown_account");
    assert!(
        explained(&["namespace", "account", "verify-storno-SZ-1"]),
        "not_managed"
    );
    assert!(explained(&[
        "namespace",
        "account",
        "verify-storno-SZ-1",
        "hint-storno-SZ-1"
    ]));
    assert!(explained(&[
        "namespace",
        "account",
        "verify-storno-SZ-1",
        "lookup-storno-SZ-1",
        "storno-SZ-1"
    ]));
    assert!(
        !explained(&["namespace", "account", "check-storno-SZ-1"]),
        "a renamed step"
    );
    assert!(
        !explained(&[
            "namespace",
            "account",
            "verify-storno-SZ-1",
            "lookup-storno-SZ-1",
            "confirm-SZ-1",
            "storno-SZ-1"
        ]),
        "an inserted step"
    );
    assert!(
        !explained(&["namespace", "account", "verify-storno-SZ-1", "storno-SZ-1"]),
        "a removed step"
    );
    assert!(!explained(&["account", "namespace"]), "out of order");
    assert!(
        !explained(&[
            "namespace",
            "account",
            "verify-storno-SZ-1",
            "lookup-storno-SZ-1",
            "storno-SZ-1",
            "storno-SZ-1"
        ]),
        "a step past the path's end"
    );
}
