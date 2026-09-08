//! Rows of the SQL introspection API: a `sys_journal` entry with its `raw`
//! hex-decoded to bytes ([`JournalEntry`]), the result of a named run
//! ([`run_result`]), a `sys_invocation` row ([`Invocation`]) and what a watch
//! saw of an invocation's attempts while it ran ([`Retries`]).

use serde_json::Value;

/// A `sys_journal` row with `raw` decoded from hex to bytes: run results are
/// stored as bytes and render as integer arrays in `entry_json`, so a text
/// match on `entry_json` is vacuous.
///
/// Under protocol v7 (journal v2) a run is two rows: `Command: Run`, which
/// carries the name, and the `Notification: Run` that follows it, which
/// carries the result bytes (verified against 1.7.8). A leak check must scan
/// every row, not the named ones.
#[derive(Debug)]
pub(crate) struct JournalEntry {
    pub(crate) index: u64,
    pub(crate) entry_type: String,
    pub(crate) name: Option<String>,
    pub(crate) raw: Vec<u8>,
}

impl JournalEntry {
    /// One `sys_journal` row (`index`, `entry_type`, `name`, `raw`).
    pub(crate) fn from_row(row: &Value) -> Self {
        Self {
            index: row["index"].as_u64().expect("index"),
            entry_type: row["entry_type"].as_str().unwrap_or_default().to_owned(),
            name: row["name"].as_str().map(str::to_owned),
            raw: row["raw"]
                .as_str()
                .map(|hex| decode_hex(hex).unwrap_or_else(|| panic!("hex raw: {hex}")))
                .unwrap_or_default(),
        }
    }

    /// Whether the entry is a `ctx.run` command (named).
    pub(crate) fn is_run(&self) -> bool {
        self.entry_type == "Command: Run"
    }

    /// Whether the entry's bytes contain `needle`.
    pub(crate) fn raw_contains(&self, needle: &str) -> bool {
        self.raw
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }
}

/// The result of the run named `name`: the `Notification: Run` row that
/// follows its command before any other command (the handlers await every
/// run, so its notification is the next journal event after the command).
pub(crate) fn run_result<'a>(journal: &'a [JournalEntry], name: &str) -> Option<&'a JournalEntry> {
    let command = journal
        .iter()
        .position(|entry| entry.is_run() && entry.name.as_deref() == Some(name))?;
    journal[command + 1..]
        .iter()
        .take_while(|entry| !entry.entry_type.starts_with("Command:"))
        .find(|entry| entry.entry_type == "Notification: Run")
}

/// What [`Harness::watch`](crate::harness::Harness::watch) saw of an
/// invocation's attempts while it ran.
#[derive(Debug, Default)]
pub(crate) struct Retries {
    pub(crate) max_retry_count: u64,
    pub(crate) failures: Vec<String>,
    pub(crate) failing_commands: Vec<String>,
}

/// A `sys_invocation` row of a completed invocation. `retry_count` and the
/// last failure are attempt state, gone once the invocation completed; see
/// [`Harness::watch`](crate::harness::Harness::watch) for them.
#[derive(Debug)]
pub(crate) struct Invocation {
    pub(crate) status: String,
    pub(crate) completion_failure: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) service: String,
    pub(crate) handler: String,
}

impl Invocation {
    /// The columns every `sys_invocation` query of the harness selects.
    pub(crate) const COLUMNS: &str =
        "status, completion_failure, scope, target_service_name, target_handler_name";

    /// One `sys_invocation` row with [`Self::COLUMNS`].
    pub(crate) fn from_row(row: &Value) -> Self {
        Self {
            status: row["status"].as_str().unwrap_or_default().to_owned(),
            completion_failure: row["completion_failure"].as_str().map(str::to_owned),
            scope: row["scope"].as_str().map(str::to_owned),
            service: row["target_service_name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
            handler: row["target_handler_name"]
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        }
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(hex.get(i..i + 2)?, 16).ok())
        .collect()
}
