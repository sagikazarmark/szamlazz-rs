//! Wiremock tests of the `gateway` module: the lookup and create steps,
//! storno validation, deletion, credit entries, credential rejections and
//! failed exchanges (`Unanswered` on the reads, `Unconfirmed` on the writes)
//! against synthetic szamlazz.hu responses, the shared fixtures of
//! `tests/common` (the document renderer, the response templates, the
//! selector matchers).
//!
//! What a test here proves is the **wire**: which requests a step sends and
//! in what order (`expect(n)`, the recorded bodies), what it puts in them, and
//! that each answer szamlazz.hu can give, in headers or in the body alone, is
//! read into the outcome the gateway's classifiers name. The classifiers
//! themselves (`settle_create`, `settle_storno`, `classify_failure`,
//! `QueryError::answered`, `is_foreign`, the credential codes) are pure and
//! table-tested in the module's unit tests; a decision they make is pinned
//! here once per step, as a row of a table with one gateway per row, not once
//! per code.
//!
//! One file per family, mirroring the e2e suite's, so that
//! `cargo test --test gateway storno::` runs one; the fixtures the families
//! share are in [`harness`].

#[path = "../common/mod.rs"]
mod common;
mod harness;

mod agent_writes;
mod create;
mod credentials;
mod duplicate;
mod lookup;
mod open;
mod privacy;
mod probe;
mod reads;
mod storno;
mod taxpayer;
