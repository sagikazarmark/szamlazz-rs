//! The *step-name table*: a table of the `ctx.run` names every handler
//! journals, per path ([`RunPath`]), held as a [`Table`] that reads a
//! journaled name as its pattern ([`Table::pattern`]) and checks a whole run
//! against the table ([`Table::check`]). The table's rows are the consumer's:
//! they name the consumer's handlers and steps; the patterns
//! ([`RunPatterns`]) and the prefix rule ([`is_prefix_of_path`]) are the
//! table's parts, reachable for a consumer that composes its own check.
//!
//! The check establishes named-run sequence conformance and observed path
//! coverage against the **current rows**. A renamed, inserted or reordered
//! step can fail against an unchanged table; changing the implementation and
//! table together can pass. It does not compare historical deployments, the
//! full journal command sequence, result serialization/decoding, inputs or
//! historical branch decisions.
//!
//! **Immutable deployments are the normal execution model:** register each
//! release separately and keep the original code available for invocations
//! pinned to it. Exceptional cross-deployment resume or restart from a retained
//! journal prefix requires reviewing the actual retained prefix against the
//! candidate code: exact command sequence (including names and parameters),
//! result serialization and decoding, operation inputs, and branch behavior.
//! The table and its diff are supporting evidence, not proof of replay
//! compatibility or a general cross-release journal-compatibility contract.
//! Allowed paths do not establish that an old result takes the same branch.
//! For restart, also reconcile external effects of operations outside the
//! copied prefix before allowing them to execute again.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use crate::introspection::{Handler, Invocation, JournalEntry};

/// One path: a handler of a service and the ordered `ctx.run` names it
/// journals on that path. A `{…}` segment in a name (`lookup-{sku}`) is a
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
/// most specific pattern it starts with: `release-hold-…` is
/// `release-hold-{sku}`, never `release-{sku}`. Derived from the table, so
/// the two cannot disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunPatterns {
    parametrized: Vec<(&'static str, &'static str)>,
}

impl RunPatterns {
    /// The patterns of `paths`. Panics on a parametrized name with no fixed
    /// prefix (`{sku}` alone): it would read every journaled name as itself,
    /// and the table would explain anything. Panics likewise on two different
    /// patterns with one prefix (`step-{id}` beside `step-{sku}`): a
    /// journaled `step-7` would read as whichever sorted first, and the other
    /// handler's path would go unexplained. And on a fixed name a parametrized
    /// prefix shadows (`step-special` beside `step-{id}`): [`Self::pattern`]
    /// would read the fixed name as the parameter, and its path could never
    /// be observed.
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

    /// The table's pattern of a journaled run name: the longest matching
    /// prefix before the first `{`, requiring a non-empty remainder; any
    /// unmatched name is returned as it is. The remainder is not validated
    /// against the pattern, and parameter values and operation inputs are not
    /// compared. For example, `release-1-A` under `release-{sku}` and
    /// `release-1-{x}` reads as the longer pattern, `release-1-{x}`.
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

/// The step-name table: the consumer's rows ([`RunPath`]) with their
/// patterns derived once ([`RunPatterns::of`], so the two cannot disagree),
/// and the check of a whole run against them ([`Table::check`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    rows: &'static [RunPath],
    patterns: RunPatterns,
}

impl Table {
    /// The table of `rows`. Panics as [`RunPatterns::of`] does on rows whose
    /// patterns would read a journaled name ambiguously.
    #[must_use]
    pub fn new(rows: &'static [RunPath]) -> Self {
        Self {
            rows,
            patterns: RunPatterns::of(rows),
        }
    }

    /// The table's pattern of a journaled run name ([`RunPatterns::pattern`]).
    #[must_use]
    pub fn pattern(&self, name: &str) -> String {
        self.patterns.pattern(name)
    }

    /// The `service.handler` names the rows cover.
    fn tabled(&self) -> BTreeSet<String> {
        self.rows
            .iter()
            .map(|row| target(row.service, row.handler))
            .collect()
    }

    /// Check the supplied observations against the current table:
    ///
    /// - Each supplied invocation's named `ctx.run` entries, in supplied
    ///   journal order and read through [`Self::pattern`], form a prefix of
    ///   at least one path for its service and handler. Fixed names match
    ///   exactly; parameter patterns use [`RunPatterns::pattern`]'s longest
    ///   prefix rule.
    /// - Every handler `deployed` offers or an invocation names has a row,
    ///   and every row's handler is deployed.
    /// - Every row is walked in full: at least one invocation's observed
    ///   pattern sequence equals the entire row. Invocation completion is
    ///   not checked; coverage is of declared named-run paths, not all
    ///   possible branches.
    ///
    /// Supply journals in journal order. Entries other than named runs are
    /// ignored. A missing journal (for example, retention ended between the
    /// two reads) is an empty observed sequence: a prefix of every path,
    /// walking only an empty row, if declared.
    /// Supplied entries must be journal v2; missing or unsupported versions
    /// panic rather than being interpreted as empty sequences.
    ///
    /// This checks current observations against current rows, not historical
    /// replay compatibility: changing the implementation and table together
    /// can pass. See the [module guidance](self) for the review required for
    /// exceptional cross-deployment resume or retained-prefix restart.
    ///
    /// # Errors
    ///
    /// Every violation found, by kind ([`Violations`]); its `Display` is the
    /// report a suite fails with.
    pub fn check(
        &self,
        deployed: &[Handler],
        invocations: &[(String, Invocation)],
        journals: &BTreeMap<String, Vec<JournalEntry>>,
    ) -> Result<Walked, Violations> {
        let tabled = self.tabled();
        let deployed: BTreeSet<String> = deployed
            .iter()
            .map(|handler| target(&handler.service, &handler.name))
            .collect();
        let mut violations = Violations {
            untabled: deployed.difference(&tabled).cloned().collect(),
            undeployed: tabled.difference(&deployed).cloned().collect(),
            unexplained: Vec::new(),
            unwalked: Vec::new(),
        };
        let mut walked = BTreeSet::new();
        for (id, invocation) in invocations {
            let observed: Vec<String> = journals
                .get(id)
                .map(|journal| {
                    journal
                        .iter()
                        .filter(|entry| entry.is_run())
                        .filter_map(|entry| entry.name.as_deref())
                        .map(|name| self.pattern(name))
                        .collect()
                })
                .unwrap_or_default();
            let target = target(&invocation.service, &invocation.handler);
            let paths: Vec<(usize, &[&str])> = self
                .rows
                .iter()
                .enumerate()
                .filter(|(_, row)| {
                    row.service == invocation.service && row.handler == invocation.handler
                })
                .map(|(index, row)| (index, row.path))
                .collect();
            if paths.is_empty() {
                violations.untabled.insert(target);
                continue;
            }
            if !paths
                .iter()
                .any(|(_, path)| is_prefix_of_path(&observed, path))
            {
                violations
                    .unexplained
                    .push(format!("{id} {target}: {observed:?}"));
            }
            for (row, path) in paths {
                if observed.len() == path.len() && is_prefix_of_path(&observed, path) {
                    walked.insert(row);
                }
            }
        }
        violations.unwalked = self
            .rows
            .iter()
            .enumerate()
            .filter(|(row, _)| !walked.contains(row))
            .map(|(_, row)| format!("{}: {:?}", target(row.service, row.handler), row.path))
            .collect();
        if violations.is_empty() {
            Ok(Walked {
                invocations: invocations.len(),
                handlers: tabled.len(),
                paths: self.rows.len(),
            })
        } else {
            Err(violations)
        }
    }
}

/// `service.handler`, as the check names a handler.
fn target(service: &str, handler: &str) -> String {
    format!("{service}.{handler}")
}

/// What a passed [`Table::check`] covered: the numbers behind "every
/// invocation's run sequence is a prefix of one of its handler's paths, every
/// path walked in full".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Walked {
    /// The invocations checked.
    pub invocations: usize,
    /// The handlers the table covers.
    pub handlers: usize,
    /// The paths, every one walked in full.
    pub paths: usize,
}

impl fmt::Display for Walked {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "every run sequence of {} invocations is a prefix of one of its handler's paths; all {} \
             paths of {} handlers walked in full",
            self.invocations, self.paths, self.handlers
        )
    }
}

/// What a failed [`Table::check`] found, by kind; empty fields are kinds
/// with nothing to report. Its `Display` is the report, each kind with what
/// to do about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violations {
    /// Handlers with no row: `service.handler` of every handler a deployment
    /// offers or an invocation names that the table does not cover. Add
    /// their steps to the table.
    pub untabled: BTreeSet<String>,
    /// Rows for a handler no deployment offers: renamed or removed in the
    /// code, kept in the table.
    pub undeployed: BTreeSet<String>,
    /// Run sequences no path of their handler explains, one per invocation
    /// (`{id} {service.handler}: {patterns}`): a renamed, inserted,
    /// reordered or repeated step.
    pub unexplained: Vec<String>,
    /// Paths no invocation walked in full (`{service.handler}: {path}`): a
    /// scenario missing, or the path's last step dropped from the handler.
    pub unwalked: Vec<String>,
}

impl Violations {
    fn is_empty(&self) -> bool {
        self.untabled.is_empty()
            && self.undeployed.is_empty()
            && self.unexplained.is_empty()
            && self.unwalked.is_empty()
    }
}

impl fmt::Display for Violations {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "the step-name table check failed:")?;
        if !self.untabled.is_empty() {
            writeln!(
                f,
                "\nhandlers not in the table (deployed or invoked): {:?}\n  Add their steps to the \
                 table.",
                self.untabled
            )?;
        }
        if !self.undeployed.is_empty() {
            writeln!(
                f,
                "\nrows for handlers no deployment offers: {:?}\n  The handler was renamed or \
                 removed; bring the table to match the code.",
                self.undeployed
            )?;
        }
        if !self.unexplained.is_empty() {
            writeln!(
                f,
                "\nrun sequences no path of their handler explains:\n  {}\n  The table is the \
                 record of which steps a handler journals and in what order. Bring the table to \
                 match the code. Its diff is a regression signal for exceptional resume or prefix \
                 restart, not proof of replay compatibility: review the actual invocation prefix, \
                 branch logic, exact commands, serialization and inputs.",
                self.unexplained.join("\n  ")
            )?;
        }
        if !self.unwalked.is_empty() {
            writeln!(
                f,
                "\npaths no invocation of the run walked in full:\n  {}\n  Either a scenario must \
                 exercise the path or its last step was dropped from the handler.",
                self.unwalked.join("\n  ")
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for Violations {}

// ----- the step-name table's matching, without a server ----------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    /// A table of an endpoint with two services: a Virtual Object whose
    /// `reserve` has two paths (a hold placed, or answered from an existing
    /// one), whose `release` has one and whose `release_hold` shares a stem
    /// with it, and a service with one probe. The one table of this module's
    /// tests.
    const TABLE: &[RunPath] = &[
        RunPath::new(
            "Inv.Stock",
            "reserve",
            &["settings", "lookup-{sku}", "place-hold-{sku}"],
        ),
        RunPath::new(
            "Inv.Stock",
            "reserve",
            &["settings", "lookup-{sku}", "existing-hold-{sku}"],
        ),
        RunPath::new("Inv.Stock", "release", &["settings", "release-{sku}"]),
        RunPath::new(
            "Inv.Stock",
            "release_hold",
            &["settings", "release-hold-{sku}"],
        ),
        RunPath::new("Inv.Api", "probe", &["settings", "probe"]),
    ];

    /// A parametrized run name is read as its pattern by its prefix, the
    /// longest prefix first: `release-hold-SKU-1` is `release-hold-{sku}`,
    /// never `release-{sku}`; a fixed name is itself.
    #[test]
    fn run_patterns_read_a_parametrized_name_by_its_longest_prefix() {
        let patterns = RunPatterns::of(TABLE);
        for (name, pattern) in [
            ("settings", "settings"),
            ("probe", "probe"),
            ("lookup-SKU-1", "lookup-{sku}"),
            ("place-hold-SKU-1", "place-hold-{sku}"),
            ("existing-hold-SKU-1", "existing-hold-{sku}"),
            ("release-SKU-1", "release-{sku}"),
            ("release-hold-A-2026-1", "release-hold-{sku}"),
            ("release-", "release-"),
        ] {
            assert_eq!(patterns.pattern(name), pattern, "{name}");
        }
    }

    /// A parameter with nothing before it would match every name; the table
    /// is refused when built, naming the pattern.
    #[test]
    fn a_parametrized_name_without_a_prefix_is_refused() {
        let bare = [RunPath::new("Svc", "h", &["settings", "{sku}"])];
        let outcome = std::panic::catch_unwind(|| RunPatterns::of(&bare));
        let message = outcome
            .expect_err("refused")
            .downcast::<String>()
            .expect("a message");
        assert!(message.contains("{sku}"), "{message}");
    }

    /// Two patterns with one prefix would make a journaled name ambiguous;
    /// the table is refused when built, naming both. The same pattern on two
    /// rows is one pattern.
    #[test]
    fn two_patterns_with_one_prefix_are_refused() {
        let same = [
            RunPath::new("A", "h", &["step-{sku}"]),
            RunPath::new("B", "h", &["step-{sku}"]),
        ];
        assert_eq!(RunPatterns::of(&same).pattern("step-7"), "step-{sku}");
        let clashing = [
            RunPath::new("A", "h", &["step-{id}"]),
            RunPath::new("B", "h", &["step-{sku}"]),
        ];
        let outcome = std::panic::catch_unwind(|| RunPatterns::of(&clashing));
        let message = outcome
            .expect_err("refused")
            .downcast::<String>()
            .expect("a message");
        assert!(
            message.contains("step-{id}") && message.contains("step-{sku}"),
            "{message}"
        );
    }

    /// A fixed name under a parametrized prefix would never be read as
    /// itself; the table is refused when built, naming both. A fixed name
    /// that merely shares letters with a prefix (`lookup-settings` beside
    /// `lookup-hold-{sku}`) is fine.
    #[test]
    fn a_fixed_name_shadowed_by_a_parametrized_prefix_is_refused() {
        let fine = [RunPath::new(
            "A",
            "h",
            &["lookup-settings", "lookup-hold-{sku}", "hold-{sku}"],
        )];
        assert_eq!(
            RunPatterns::of(&fine).pattern("lookup-settings"),
            "lookup-settings"
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
        let patterns = RunPatterns::of(TABLE);
        let paths: Vec<&[&str]> = TABLE
            .iter()
            .filter(|row| row.handler == "reserve")
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
        assert!(explained(&["settings"]), "answered early");
        assert!(
            explained(&["settings", "lookup-SKU-1"]),
            "answered after the lookup"
        );
        assert!(explained(&[
            "settings",
            "lookup-SKU-1",
            "existing-hold-SKU-1"
        ]));
        assert!(explained(&["settings", "lookup-SKU-1", "place-hold-SKU-1"]));
        assert!(!explained(&["settings", "check-SKU-1"]), "a renamed step");
        assert!(
            !explained(&[
                "settings",
                "lookup-SKU-1",
                "confirm-SKU-1",
                "place-hold-SKU-1"
            ]),
            "an inserted step"
        );
        assert!(
            !explained(&["settings", "place-hold-SKU-1"]),
            "a removed step"
        );
        assert!(!explained(&["lookup-SKU-1", "settings"]), "out of order");
        assert!(
            !explained(&[
                "settings",
                "lookup-SKU-1",
                "place-hold-SKU-1",
                "place-hold-SKU-1"
            ]),
            "a step past the path's end"
        );
    }

    // ----- the check over a run, on scripted rows --------------------------------

    fn table() -> Table {
        Table::new(TABLE)
    }

    fn handler(service: &str, name: &str) -> Handler {
        Handler {
            service: service.to_owned(),
            name: name.to_owned(),
        }
    }

    /// Every handler the table names, deployed.
    fn deployed() -> Vec<Handler> {
        vec![
            handler("Inv.Stock", "reserve"),
            handler("Inv.Stock", "release"),
            handler("Inv.Stock", "release_hold"),
            handler("Inv.Api", "probe"),
        ]
    }

    /// One completed invocation of `service.handler` with `runs` as its
    /// journaled `ctx.run` names, in order, plus the rows a journal has around
    /// them (an input, a notification per run, an output).
    fn invocation(
        id: &str,
        service: &str,
        handler: &str,
        runs: &[&str],
    ) -> ((String, Invocation), (String, Vec<JournalEntry>)) {
        let mut journal = vec![JournalEntry {
            index: 0,
            version: Some(2),
            run_completion_id: None,
            entry_type: "Command: Input".to_owned(),
            name: None,
            raw: Vec::new(),
        }];
        for run in runs {
            let index = u64::try_from(journal.len()).expect("index");
            journal.push(JournalEntry {
                index,
                version: Some(2),
                run_completion_id: Some(u32::try_from(index).expect("completion id")),
                entry_type: "Command: Run".to_owned(),
                name: Some((*run).to_owned()),
                raw: Vec::new(),
            });
            journal.push(JournalEntry {
                index: index + 1,
                version: Some(2),
                run_completion_id: Some(u32::try_from(index).expect("completion id")),
                entry_type: "Notification: Run".to_owned(),
                name: None,
                raw: b"{}".to_vec(),
            });
        }
        let row = Invocation {
            status: "completed".to_owned(),
            completion_failure: None,
            scope: None,
            service: service.to_owned(),
            handler: handler.to_owned(),
        };
        ((id.to_owned(), row), (id.to_owned(), journal))
    }

    /// A run as the check reads it: the `sys_invocation` rows and the
    /// journals by id.
    type Run = (
        Vec<(String, Invocation)>,
        BTreeMap<String, Vec<JournalEntry>>,
    );

    /// The invocations of a run that walks every path once, with an early
    /// answer among them.
    fn full_walk() -> Run {
        [
            invocation(
                "inv_1",
                "Inv.Stock",
                "reserve",
                &["settings", "lookup-SKU-1", "place-hold-SKU-1"],
            ),
            invocation(
                "inv_2",
                "Inv.Stock",
                "reserve",
                &["settings", "lookup-SKU-1", "existing-hold-SKU-1"],
            ),
            invocation("inv_3", "Inv.Stock", "reserve", &["settings"]),
            invocation(
                "inv_4",
                "Inv.Stock",
                "release",
                &["settings", "release-SKU-1"],
            ),
            invocation("inv_5", "Inv.Api", "probe", &["settings", "probe"]),
            invocation(
                "inv_6",
                "Inv.Stock",
                "release_hold",
                &["settings", "release-hold-SKU-1"],
            ),
        ]
        .into_iter()
        .unzip()
    }

    /// A run in which every invocation's run sequence is a prefix of one of
    /// its handler's paths, every deployed handler is tabled and every path
    /// was walked in full passes, reporting what it covered.
    #[test]
    fn a_run_walking_every_path_passes_the_check() {
        let (invocations, journals) = full_walk();
        let walked = table()
            .check(&deployed(), &invocations, &journals)
            .expect("the check holds");
        assert_eq!(
            walked,
            Walked {
                invocations: 6,
                handlers: 4,
                paths: 5,
            }
        );
        let report = walked.to_string();
        assert!(report.contains("6 invocations"), "{report}");
        assert!(report.contains("5 paths"), "{report}");
    }

    /// A handler with no row is reported whether an invocation of it exists
    /// or only a deployment offers it: the second is what an invocation-only
    /// check cannot see (a handler added with neither a row nor a scenario).
    #[test]
    fn an_untabled_handler_is_reported_deployed_or_invoked() {
        let (mut invocations, mut journals) = full_walk();
        let mut deployed = deployed();
        deployed.push(handler("Inv.Api", "audit"));
        let violations = table()
            .check(&deployed, &invocations, &journals)
            .expect_err("a deployed handler without a row");
        assert_eq!(
            violations.untabled,
            BTreeSet::from(["Inv.Api.audit".to_owned()])
        );
        assert!(violations.undeployed.is_empty());
        assert!(violations.unexplained.is_empty());
        assert!(violations.unwalked.is_empty());
        assert!(violations.to_string().contains("Inv.Api.audit"));

        let (row, journal) = invocation("inv_7", "Inv.Stock", "restock", &["settings"]);
        invocations.push(row);
        journals.insert(journal.0, journal.1);
        let violations = table()
            .check(&deployed, &invocations, &journals)
            .expect_err("an invoked handler without a row");
        assert_eq!(
            violations.untabled,
            BTreeSet::from(["Inv.Api.audit".to_owned(), "Inv.Stock.restock".to_owned()])
        );
    }

    /// A row for a handler no deployment offers is stale: renamed or removed
    /// in the code, kept in the table.
    #[test]
    fn a_tabled_handler_no_deployment_offers_is_reported() {
        let (invocations, journals) = full_walk();
        let deployed: Vec<Handler> = deployed()
            .into_iter()
            .filter(|handler| handler.name != "probe")
            .collect();
        let violations = table()
            .check(&deployed, &invocations, &journals)
            .expect_err("a row without a deployed handler");
        assert_eq!(
            violations.undeployed,
            BTreeSet::from(["Inv.Api.probe".to_owned()])
        );
        assert!(violations.untabled.is_empty());
    }

    /// A run sequence no path of its handler explains (a renamed, inserted,
    /// reordered or repeated step) is reported with its invocation id and the
    /// sequence read as patterns; a sequence short of every path is explained
    /// (an early answer) and walks nothing.
    #[test]
    fn an_unexplained_run_sequence_is_reported_by_invocation() {
        let (mut invocations, mut journals) = full_walk();
        let (row, journal) = invocation(
            "inv_7",
            "Inv.Stock",
            "reserve",
            &["settings", "check-SKU-2", "place-hold-SKU-2"],
        );
        invocations.push(row);
        journals.insert(journal.0, journal.1);
        let violations = table()
            .check(&deployed(), &invocations, &journals)
            .expect_err("a renamed step");
        assert_eq!(
            violations.unexplained,
            [
                "inv_7 Inv.Stock.reserve: [\"settings\", \"check-SKU-2\", \"place-hold-{sku}\"]"
                    .to_owned()
            ]
        );
        assert!(
            violations.unwalked.is_empty(),
            "every path was still walked"
        );
        let message = violations.to_string();
        assert!(message.contains("inv_7"), "{message}");
    }

    /// A path no invocation walked to its end is reported: a scenario dropped,
    /// or the path's last step dropped from the handler.
    #[test]
    fn a_path_no_invocation_walked_in_full_is_reported() {
        let (invocations, journals) = full_walk();
        let (invocations, journals): Run = invocations
            .into_iter()
            .filter(|(id, _)| id != "inv_2")
            .map(|(id, row)| {
                let journal = journals[&id].clone();
                ((id.clone(), row), (id, journal))
            })
            .unzip();
        let violations = table()
            .check(&deployed(), &invocations, &journals)
            .expect_err("a path not walked");
        assert_eq!(
            violations.unwalked,
            [
                "Inv.Stock.reserve: [\"settings\", \"lookup-{sku}\", \"existing-hold-{sku}\"]"
                    .to_owned()
            ]
        );
        assert!(violations.unexplained.is_empty());
    }

    /// An invocation the server holds a row for but no journal of (retention
    /// ended, or purged mid-query) journaled nothing observable: explained by
    /// every path, walking none.
    #[test]
    fn an_invocation_without_a_journal_is_an_empty_sequence() {
        let (mut invocations, journals) = full_walk();
        let (row, _) = invocation("inv_8", "Inv.Api", "probe", &["settings", "probe"]);
        invocations.push(row);
        let walked = table()
            .check(&deployed(), &invocations, &journals)
            .expect("an empty sequence is a prefix of every path");
        assert_eq!(walked.invocations, 7);
    }
}
