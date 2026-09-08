//! Rows of the SQL introspection API: a `sys_journal` entry with its `raw`
//! hex-decoded to bytes ([`JournalEntry`]), the result of a named run
//! ([`run_result`]) and a `sys_invocation` row ([`Invocation`]). What a watch
//! saw of an invocation's attempts while it ran is
//! [`Retries`](crate::admin::Retries).

use serde_json::Value;

/// A `sys_journal` row with `raw` decoded from hex to bytes: run results are
/// stored as bytes and render as integer arrays in `entry_json`, so a text
/// match on `entry_json` is vacuous.
///
/// Under protocol v7 (journal v2) a run is two rows: `Command: Run`, which
/// carries the name, and the `Notification: Run` that follows it, which
/// carries the result bytes (verified against 1.7.8). A leak check must scan
/// every row, not the named ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// The entry's position in the journal.
    pub index: u64,
    /// `entry_type` as the server names it (`Command: Run`, `Notification:
    /// Run`, …).
    pub entry_type: String,
    /// The name of a named entry (a `ctx.run`'s).
    pub name: Option<String>,
    /// `raw`, hex-decoded.
    pub raw: Vec<u8>,
}

impl JournalEntry {
    /// One `sys_journal` row (`index`, `entry_type`, `name`, `raw`).
    pub fn from_row(row: &Value) -> Self {
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
    #[must_use]
    pub fn is_run(&self) -> bool {
        self.entry_type == "Command: Run"
    }

    /// Whether the entry's bytes contain `needle`.
    #[must_use]
    pub fn raw_contains(&self, needle: &str) -> bool {
        self.raw
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }
}

/// The result of the run named `name`: the `Notification: Run` row that
/// follows its command before any other command (a handler that awaits every
/// run has its notification as the next journal event after the command).
#[must_use]
pub fn run_result<'a>(journal: &'a [JournalEntry], name: &str) -> Option<&'a JournalEntry> {
    let command = journal
        .iter()
        .position(|entry| entry.is_run() && entry.name.as_deref() == Some(name))?;
    journal[command + 1..]
        .iter()
        .take_while(|entry| !entry.entry_type.starts_with("Command:"))
        .find(|entry| entry.entry_type == "Notification: Run")
}

/// A `sys_invocation` row. `retry_count` and the last failure are attempt
/// state, gone once the invocation completed; see
/// [`Watch`](crate::admin::Watch) for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// `status` (`completed`, `running`, `backing-off`, …).
    pub status: String,
    /// `completion_failure`: the terminal error's text of a failed
    /// invocation.
    pub completion_failure: Option<String>,
    /// `scope`: the partition key the server keyed the invocation by, under
    /// scoped Virtual Objects.
    pub scope: Option<String>,
    /// `target_service_name`.
    pub service: String,
    /// `target_handler_name`.
    pub handler: String,
}

impl Invocation {
    /// The columns every `sys_invocation` query of the harness selects.
    pub const COLUMNS: &str =
        "status, completion_failure, scope, target_service_name, target_handler_name";

    /// One `sys_invocation` row with [`Self::COLUMNS`].
    pub fn from_row(row: &Value) -> Self {
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// `raw` is hex on the wire and bytes in the entry; a run's result is the
    /// notification after its command.
    #[test]
    fn a_journal_row_decodes_its_raw_and_a_run_finds_its_result() {
        let journal: Vec<JournalEntry> = [
            json!({ "index": 0, "entry_type": "Command: Input", "name": null, "raw": "" }),
            json!({ "index": 1, "entry_type": "Command: Run", "name": "step", "raw": "00" }),
            json!({ "index": 2, "entry_type": "Notification: Run", "name": null, "raw": "7b7d" }),
            json!({ "index": 3, "entry_type": "Command: Output", "name": null, "raw": null }),
        ]
        .iter()
        .map(JournalEntry::from_row)
        .collect();
        assert!(journal[1].is_run());
        assert!(!journal[2].is_run());
        let result = run_result(&journal, "step").expect("the run's result");
        assert_eq!(result.index, 2);
        assert_eq!(result.raw, b"{}");
        assert!(result.raw_contains("{}"));
        assert!(run_result(&journal, "other").is_none());
        assert_eq!(decode_hex("abc"), None, "an odd length is not hex");
        assert_eq!(decode_hex("zz"), None);
    }
}
