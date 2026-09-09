//! The *step-name table*'s matching: a table of the `ctx.run` names every
//! handler journals, per path ([`RunPath`]), a journaled name read as its
//! pattern ([`RunPatterns::pattern`]) and the prefix rule
//! ([`is_prefix_of_path`]). The table itself is the consumer's: it names the
//! consumer's handlers and steps.
//!
//! A journal replays by name and position. Under in-place re-registration an
//! in-flight invocation replays the *previous* deployment's entries, so a
//! renamed, inserted or reordered step strands it; under immutable deployments
//! the same sequence is what a pause-and-resume onto new code needs. A
//! consumer that keeps its table and asserts, over every invocation a run
//! leaves on the server, that the observed run names are a prefix of one of
//! its handler's paths and that every path was walked in full, makes either a
//! failing test instead of a stranded invocation.

/// One path: a handler of a service and the ordered `ctx.run` names it
/// journals on that path. A `{…}` segment in a name (`verify-{number}`) is a
/// parameter, matched by the prefix before it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RunPath {
    /// The service (`target_service_name`).
    pub service: &'static str,
    /// The handler (`target_handler_name`).
    pub handler: &'static str,
    /// The `ctx.run` names, in journal order.
    pub path: &'static [&'static str],
}

impl RunPath {
    /// A path of `handler` on `service`.
    #[must_use]
    pub const fn new(
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

/// The parametrized run names of a table (every `{…}` pattern) by the prefix
/// that names the step, longest prefix first, so that a name is read as the
/// most specific pattern it starts with: `verify-storno-…` is
/// `verify-storno-{number}`, never `verify-{number}`. Derived from the table,
/// so the two cannot disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPatterns {
    parametrized: Vec<(&'static str, &'static str)>,
}

impl RunPatterns {
    /// The patterns of `paths`. Panics on a parametrized name with no fixed
    /// prefix (`{number}` alone): it would read every journaled name as
    /// itself, and the pin would explain anything. Panics likewise on two
    /// different patterns with one prefix (`step-{id}` beside
    /// `step-{number}`): a journaled `step-7` would read as whichever sorted
    /// first, and the other handler's path would go unexplained. And on a
    /// fixed name a parametrized prefix shadows (`step-special` beside
    /// `step-{id}`): [`Self::pattern`] would read the fixed name as the
    /// parameter, and its path could never be observed.
    #[must_use]
    pub fn of(paths: &[RunPath]) -> Self {
        let mut parametrized: Vec<(&str, &str)> = paths
            .iter()
            .flat_map(|row| row.path.iter())
            .filter_map(|pattern| {
                pattern
                    .split_once('{')
                    .map(|(prefix, _)| (prefix, *pattern))
            })
            .inspect(|(prefix, pattern)| {
                assert!(
                    !prefix.is_empty(),
                    "a parametrized run name needs a fixed prefix before its `{{`: {pattern:?}"
                );
            })
            .collect();
        parametrized.sort_by(|a, b| b.0.len().cmp(&a.0.len()).then_with(|| a.cmp(b)));
        parametrized.dedup();
        let fixed = paths
            .iter()
            .flat_map(|row| row.path.iter())
            .filter(|name| !name.contains('{'));
        for name in fixed {
            if let Some((prefix, pattern)) = parametrized
                .iter()
                .find(|(prefix, _)| name.starts_with(prefix) && name.len() > prefix.len())
            {
                panic!(
                    "the fixed run name {name:?} is shadowed by {pattern:?} (prefix {prefix:?}); \
                     a journaled {name:?} would read as the parameter and its path could never be \
                     observed"
                );
            }
        }
        for pair in parametrized.windows(2) {
            assert!(
                pair[0].0 != pair[1].0,
                "two parametrized run names share the prefix {:?}: {:?} and {:?}; a journaled name \
                 could read as either",
                pair[0].0,
                pair[0].1,
                pair[1].1
            );
        }
        Self { parametrized }
    }

    /// The table's pattern of a journaled run name: a parametrized name by
    /// its prefix, any other name as it is. The parameter is matched by its
    /// prefix only: a name that itself began with a pinned stem (`storno-1`
    /// under a `storno-{number}` and a `storno-1-{x}`) would read as the
    /// longer pattern.
    #[must_use]
    pub fn pattern(&self, name: &str) -> String {
        self.parametrized
            .iter()
            .find(|(prefix, _)| name.starts_with(prefix) && name.len() > prefix.len())
            .map_or_else(|| name.to_owned(), |(_, pattern)| (*pattern).to_owned())
    }
}

/// Whether `observed` (patterns, in journal order) is a prefix of `path`: a
/// handler that answers early journals the first steps only.
#[must_use]
pub fn is_prefix_of_path(observed: &[String], path: &[&str]) -> bool {
    observed.len() <= path.len()
        && observed
            .iter()
            .zip(path)
            .all(|(seen, expected)| seen == expected)
}

// ----- the step-name table's matching, without a server ----------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PATHS: &[RunPath] = &[
        RunPath::new(
            "Svc.Object",
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
            "Svc.Object",
            "storno_invoice",
            &[
                "namespace",
                "account",
                "verify-storno-{number}",
                "hint-storno-{number}",
            ],
        ),
        RunPath::new(
            "Svc.Object",
            "delete_proforma",
            &[
                "namespace",
                "account",
                "proforma-for-delete",
                "delete-proforma-{number}",
            ],
        ),
        RunPath::new(
            "Svc.Agent",
            "storno",
            &[
                "namespace",
                "account",
                "verify-{number}",
                "lookup-storno-{number}",
                "storno-{number}",
            ],
        ),
        RunPath::new(
            "Svc.Agent",
            "query_taxpayer",
            &["namespace", "account", "taxpayer-{prefix}"],
        ),
    ];

    /// A parametrized run name is read as its pattern by its prefix, the
    /// longest prefix first: `verify-storno-SZ-1` is `verify-storno-{number}`,
    /// never `verify-{number}`; a fixed name is itself.
    #[test]
    fn run_patterns_read_a_parametrized_name_by_its_longest_prefix() {
        let patterns = RunPatterns::of(PATHS);
        for (name, pattern) in [
            ("namespace", "namespace"),
            ("lookup-invoice", "lookup-invoice"),
            ("lookup-storno-SZ-1", "lookup-storno-{number}"),
            ("storno-SZ-1", "storno-{number}"),
            ("verify-SZ-1", "verify-{number}"),
            ("verify-storno-E-TST-2026-1", "verify-storno-{number}"),
            ("hint-storno-SZ-1", "hint-storno-{number}"),
            ("delete-proforma-D-1", "delete-proforma-{number}"),
            ("taxpayer-12345678", "taxpayer-{prefix}"),
            ("proforma-for-delete", "proforma-for-delete"),
            ("verify-", "verify-"),
        ] {
            assert_eq!(patterns.pattern(name), pattern, "{name}");
        }
    }

    /// A parameter with nothing before it would match every name; the table
    /// is refused when built, naming the pattern.
    #[test]
    fn a_parametrized_name_without_a_prefix_is_refused() {
        let bare = [RunPath::new("Svc", "h", &["namespace", "{number}"])];
        let outcome = std::panic::catch_unwind(|| RunPatterns::of(&bare));
        let message = outcome
            .expect_err("refused")
            .downcast::<String>()
            .expect("a message");
        assert!(message.contains("{number}"), "{message}");
    }

    /// Two patterns with one prefix would make a journaled name ambiguous;
    /// the table is refused when built, naming both. The same pattern on two
    /// rows is one pattern.
    #[test]
    fn two_patterns_with_one_prefix_are_refused() {
        let same = [
            RunPath::new("A", "h", &["step-{number}"]),
            RunPath::new("B", "h", &["step-{number}"]),
        ];
        assert_eq!(RunPatterns::of(&same).pattern("step-7"), "step-{number}");
        let clashing = [
            RunPath::new("A", "h", &["step-{id}"]),
            RunPath::new("B", "h", &["step-{number}"]),
        ];
        let outcome = std::panic::catch_unwind(|| RunPatterns::of(&clashing));
        let message = outcome
            .expect_err("refused")
            .downcast::<String>()
            .expect("a message");
        assert!(
            message.contains("step-{id}") && message.contains("step-{number}"),
            "{message}"
        );
    }

    /// A fixed name under a parametrized prefix would never be read as
    /// itself; the table is refused when built, naming both. A fixed name
    /// that merely shares letters with a prefix (`lookup-proforma` beside
    /// `lookup-storno-{number}`) is fine.
    #[test]
    fn a_fixed_name_shadowed_by_a_parametrized_prefix_is_refused() {
        let fine = [RunPath::new(
            "A",
            "h",
            &[
                "lookup-proforma",
                "lookup-storno-{number}",
                "storno-{number}",
            ],
        )];
        assert_eq!(
            RunPatterns::of(&fine).pattern("lookup-proforma"),
            "lookup-proforma"
        );
        let shadowed = [
            RunPath::new("A", "h", &["step-special"]),
            RunPath::new("B", "h", &["step-{id}"]),
        ];
        let outcome = std::panic::catch_unwind(|| RunPatterns::of(&shadowed));
        let message = outcome
            .expect_err("refused")
            .downcast::<String>()
            .expect("a message");
        assert!(
            message.contains("step-special") && message.contains("step-{id}"),
            "{message}"
        );
    }

    /// An observed sequence is explained by a path when it is a prefix of it:
    /// a handler that answers early journals the first steps only; a renamed
    /// step, an inserted one or one out of order is explained by none.
    #[test]
    fn an_observed_run_sequence_is_a_prefix_of_one_of_its_handlers_paths_or_unexplained() {
        let patterns = RunPatterns::of(PATHS);
        let paths: Vec<&[&str]> = PATHS
            .iter()
            .filter(|row| row.handler == "storno_invoice")
            .map(|row| row.path)
            .collect();
        let explained = |observed: &[&str]| {
            paths.iter().any(|path| {
                is_prefix_of_path(
                    &observed
                        .iter()
                        .map(|name| patterns.pattern(name))
                        .collect::<Vec<_>>(),
                    path,
                )
            })
        };
        assert!(
            explained(&[]),
            "nothing journaled (refused before the first step)"
        );
        assert!(explained(&["namespace", "account"]), "answered early");
        assert!(
            explained(&["namespace", "account", "verify-storno-SZ-1"]),
            "answered after the verify"
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
}
