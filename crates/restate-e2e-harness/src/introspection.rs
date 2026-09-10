//! Rows of the SQL introspection API: a `sys_journal` entry with its `raw`
//! hex-decoded to bytes ([`JournalEntry`]), the result of a named run
//! ([`run_result`]) and a `sys_invocation` row ([`Invocation`]); and a
//! handler as the admin API lists it ([`Handler`], `GET /services`). What a
//! watch saw of an invocation's attempts while it ran is
//! [`Retries`](crate::watch::Retries).

use serde_json::Value;

// Journal-v2 SQL display names in Restate 1.7.8's journal_v2::{CommandType,
// CompletionType, NotificationType}. Keep the supported representation explicit:
// an unfamiliar entry must not disappear from a named-run assertion.
const COMMANDS: &[&str] = &[
    "Input",
    "Output",
    "GetLazyState",
    "SetState",
    "ClearState",
    "ClearAllState",
    "GetLazyStateKeys",
    "GetEagerState",
    "GetEagerStateKeys",
    "GetPromise",
    "PeekPromise",
    "CompletePromise",
    "Sleep",
    "Call",
    "OneWayCall",
    "SendSignal",
    "Run",
    "AttachInvocation",
    "GetInvocationOutput",
    "CompleteAwakeable",
];
const COMPLETIONS: &[&str] = &[
    "GetLazyState",
    "GetLazyStateKeys",
    "GetPromise",
    "PeekPromise",
    "CompletePromise",
    "Sleep",
    "CallInvocationId",
    "Call",
    "Run",
    "AttachInvocation",
    "GetInvocationOutput",
];

fn assert_supported_type(entry_type: &str, index: u64) {
    let supported = entry_type
        .strip_prefix("Command: ")
        .is_some_and(|ty| COMMANDS.contains(&ty))
        || entry_type
            .strip_prefix("Notification: ")
            .is_some_and(|ty| ty == "Signal" || COMPLETIONS.contains(&ty));
    assert!(
        supported,
        "unsupported entry_type {entry_type:?} at journal entry {index}"
    );
}

/// Read only the enum tags, leaving unrelated command payloads opaque.
fn entry_classification(entry: &Value) -> Option<String> {
    fn variant(value: &Value) -> Option<(&str, &Value)> {
        let object = value.as_object()?;
        if object.len() != 1 {
            return None;
        }
        object
            .iter()
            .next()
            .map(|(name, value)| (name.as_str(), value))
    }
    match variant(entry)? {
        ("Command", command) => Some(format!("Command: {}", variant(command)?.0)),
        ("Notification", notification) => match variant(notification)? {
            ("Completion", completion) => Some(format!("Notification: {}", variant(completion)?.0)),
            ("Signal", _) => Some("Notification: Signal".to_owned()),
            _ => None,
        },
        _ => None,
    }
}

/// A handler of a registered service, as the admin API lists it
/// (`GET /services`, [`Admin::handlers`](crate::Admin::handlers)): what a
/// deployment offers, whether or not the run invoked it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Handler {
    /// The service (`target_service_name` of its invocations).
    pub service: String,
    /// The handler (`target_handler_name` of its invocations).
    pub name: String,
}

impl Handler {
    /// The handlers of one `GET /services` body: every `handlers[].name` of
    /// every `services[]`.
    #[must_use]
    pub fn from_services(body: &Value) -> Vec<Self> {
        let services = body["services"]
            .as_array()
            .unwrap_or_else(|| panic!("GET /services answers a `services` array: {body}"));
        services
            .iter()
            .flat_map(|service| {
                let name = service["name"]
                    .as_str()
                    .unwrap_or_else(|| panic!("a service has a name: {service}"));
                service["handlers"]
                    .as_array()
                    .unwrap_or_else(|| panic!("a service lists its handlers: {service}"))
                    .iter()
                    .map(move |handler| Self {
                        service: name.to_owned(),
                        name: handler["name"]
                            .as_str()
                            .unwrap_or_else(|| panic!("a handler has a name: {handler}"))
                            .to_owned(),
                    })
            })
            .collect()
    }
}

/// A `sys_journal` row with `raw` decoded from hex to bytes: run results are
/// stored as bytes and render as integer arrays in `entry_json`, so a text
/// match on `entry_json` is vacuous.
///
/// Under journal v2 a run is two rows: `Command: Run`, which carries the name
/// and completion id, and `Notification: Run` with the same completion id,
/// which carries the result bytes (verified against server 1.7.8, protocol v7).
/// The notification need not be adjacent. A leak check must scan every row,
/// not the named ones.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalEntry {
    /// The entry's position in the journal.
    pub index: u64,
    /// `version` from `sys_journal`; absent when not selected or not supplied.
    pub version: Option<u64>,
    /// `entry_type` as the server names it (`Command: Run`, `Notification:
    /// Run`, …).
    pub entry_type: String,
    /// The name of a named entry (a `ctx.run`'s). Run commands require a
    /// string; an unnamed run carries `Some(String::new())`, not `None`.
    pub name: Option<String>,
    /// The run command's or run notification's `completion_id`, read from
    /// journal-v2 `entry_json`. Not the row index; `None` for other entry types
    /// or when the run identity is missing or cannot be decoded.
    pub run_completion_id: Option<u32>,
    /// `raw`, hex-decoded. An empty vector means explicitly supplied empty
    /// bytes, never missing evidence.
    pub raw: Vec<u8>,
}

impl JournalEntry {
    /// One `sys_journal` row (`index`, `version`, `entry_type`, `name`,
    /// `entry_json`, `raw`). `entry_json` is the server's JSON-encoded string;
    /// only the run identity is retained from it. Missing or malformed identity
    /// metadata stays `None` so [`run_result`] can explicitly reject it.
    /// A missing, empty or non-string `entry_type`, or missing/non-string/invalid
    /// hex `raw`, panics: unavailable evidence cannot establish absence. Select
    /// these columns even when only inspecting content, without [`run_result`].
    /// For v2, unknown classifications, a missing/non-string run name, and
    /// contradictions with a decoded `entry_json` classification or run name
    /// also panic. Unnamed runs carry an empty string.
    pub fn from_row(row: &Value) -> Self {
        let index = row["index"].as_u64().expect("index");
        let entry_type = row["entry_type"]
            .as_str()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| panic!("missing or malformed entry_type at journal entry {index}"));
        let raw = row["raw"]
            .as_str()
            .and_then(decode_hex)
            .unwrap_or_else(|| panic!("missing or malformed hex raw at journal entry {index}"));
        let entry = row["entry_json"]
            .as_str()
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        let version = row["version"].as_u64();
        let name = row["name"].as_str().map(str::to_owned);
        if version == Some(2) {
            assert_supported_type(entry_type, index);
            if let Some(entry) = &entry {
                assert_eq!(
                    entry_classification(entry).as_deref(),
                    Some(entry_type),
                    "entry_json classification disagrees with entry_type at journal entry {index}"
                );
            }
            if entry_type == "Command: Run" {
                assert!(
                    name.is_some(),
                    "missing or malformed run name at journal entry {index}"
                );
                if let Some(json_name) = entry
                    .as_ref()
                    .and_then(|entry| entry["Command"]["Run"].get("name"))
                {
                    assert_eq!(
                        json_name.as_str(),
                        name.as_deref(),
                        "entry_json run name disagrees with SQL name at journal entry {index}"
                    );
                }
            }
        }
        let run_completion_id = entry.as_ref().and_then(|entry| {
            let id = match entry_type {
                "Command: Run" => &entry["Command"]["Run"]["completion_id"],
                "Notification: Run" => &entry["Notification"]["Completion"]["Run"]["completion_id"],
                _ => return None,
            };
            id.as_u64().and_then(|id| u32::try_from(id).ok())
        });
        Self {
            index,
            version,
            entry_type: entry_type.to_owned(),
            name,
            run_completion_id,
            raw,
        }
    }

    /// Whether the entry is a `ctx.run` command, named or unnamed.
    /// Panics on a missing or unsupported journal version or classification,
    /// or a run without name evidence: only the journal-v2 representation in
    /// Restate 1.7.8 is understood; another spelling cannot establish absence.
    #[must_use]
    pub fn is_run(&self) -> bool {
        self.assert_supported_version();
        assert_supported_type(&self.entry_type, self.index);
        let is_run = self.entry_type == "Command: Run";
        assert!(
            !is_run || self.name.is_some(),
            "missing run name at journal entry {}",
            self.index
        );
        is_run
    }

    /// A named run's non-empty name, after validating the entry's evidence.
    pub(crate) fn named_run_name(&self) -> Option<&str> {
        if self.is_run() {
            self.name.as_deref().filter(|name| !name.is_empty())
        } else {
            None
        }
    }

    fn assert_supported_version(&self) {
        assert_eq!(
            self.version,
            Some(2),
            "unsupported journal version at entry {}",
            self.index
        );
    }

    /// Whether the entry's bytes contain `needle`; the empty needle is
    /// contained in everything (`windows(0)` would panic).
    #[must_use]
    pub fn raw_contains(&self, needle: &str) -> bool {
        needle.is_empty()
            || self
                .raw
                .windows(needle.len())
                .any(|window| window == needle.as_bytes())
    }
}

/// The result of the uniquely named run: its `Notification: Run`, matched by
/// completion id, never proximity. `None` means the name or its notification
/// is absent. Repeated names require [`run_result_at`].
///
/// Supports journal v2's `entry_json` shape as exposed by server 1.7.8 (tested
/// with Rust SDK 0.12.0, protocol v7). Supply one invocation's unfiltered journal
/// or prefix in strictly increasing index order, as [`Admin::journal`](crate::Admin::journal)
/// does. Notifications must follow their commands but may complete in either
/// order, with unrelated entries between them. Rust SDK 0.12 requires immediately
/// awaiting each run; interleaved runs are supported here for journal inspection,
/// not as an endorsement of interleaving SDK context operations.
///
/// An unnamed run can be selected with `name = ""`; repeated unnamed runs
/// require [`run_result_at`] just like repeated non-empty names.
///
/// # Panics
///
/// A repeated name is ambiguous. Also rejects unsupported versions (including
/// missing versions and v1), unordered/duplicate indices, missing run identities,
/// duplicate command/completion identities and orphan run notifications. Validates
/// the whole supplied journal even when the requested name is absent. There is
/// no adjacency fallback when metadata is unavailable.
#[must_use]
pub fn run_result<'a>(journal: &'a [JournalEntry], name: &str) -> Option<&'a JournalEntry> {
    validate_run_journal(journal);
    let mut commands = journal
        .iter()
        .filter(|entry| entry.is_run() && entry.name.as_deref() == Some(name));
    let command = commands.next()?;
    assert!(
        commands.next().is_none(),
        "ambiguous run name {name:?}; use run_result_at"
    );
    result_for_command(journal, command)
}

/// The result of the zero-based `occurrence` of a run named `name`, in journal
/// index order. Unlike [`run_result`], permits repeated names. Returns `None`
/// when that occurrence or its notification is absent.
///
/// # Panics
///
/// Like [`run_result`], rejects unsupported or ambiguous journal evidence.
#[must_use]
pub fn run_result_at<'a>(
    journal: &'a [JournalEntry],
    name: &str,
    occurrence: usize,
) -> Option<&'a JournalEntry> {
    validate_run_journal(journal);
    let command = journal
        .iter()
        .filter(|entry| entry.is_run() && entry.name.as_deref() == Some(name))
        .nth(occurrence)?;
    result_for_command(journal, command)
}

fn validate_run_journal(journal: &[JournalEntry]) {
    let mut commands = std::collections::BTreeSet::new();
    let mut notifications = std::collections::BTreeSet::new();
    let mut previous = None;
    for entry in journal {
        entry.assert_supported_version();
        assert!(
            previous.is_none_or(|index| index < entry.index),
            "unsupported journal order at entry {}; expected strictly increasing indices",
            entry.index
        );
        previous = Some(entry.index);
        if entry.is_run() || entry.entry_type == "Notification: Run" {
            let id = entry.run_completion_id.unwrap_or_else(|| {
                panic!(
                    "unsupported run identity at entry {}; select journal-v2 entry_json",
                    entry.index
                )
            });
            if entry.is_run() {
                assert!(
                    commands.insert(id),
                    "ambiguous run completion identity {id}"
                );
            } else {
                assert!(
                    commands.contains(&id),
                    "unsupported orphan run notification at entry {} (completion {id})",
                    entry.index
                );
                assert!(
                    notifications.insert(id),
                    "ambiguous run notifications for completion {id}"
                );
            }
        }
    }
}

fn result_for_command<'a>(
    journal: &'a [JournalEntry],
    command: &JournalEntry,
) -> Option<&'a JournalEntry> {
    let completion_id = command
        .run_completion_id
        .expect("run command has a completion identity");
    journal.iter().find(|entry| {
        entry.entry_type == "Notification: Run" && entry.run_completion_id == Some(completion_id)
    })
}

/// A `sys_invocation` row. `retry_count` and the last failure are attempt
/// state, gone once the invocation completed; see
/// [`Watch`](crate::watch::Watch) for them.
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

    /// One `sys_invocation` row queried with [`Self::COLUMNS`]. Required strings
    /// must be present; nullable columns accept a string, null or omission
    /// (Restate 1.7.8's JSON writer omits SQL nulls). Other types panic.
    /// A row alone cannot distinguish an omitted SQL null from an unselected
    /// nullable column: custom queries must select all columns themselves.
    /// Unknown status strings are preserved.
    #[must_use]
    pub fn from_row(row: &Value) -> Self {
        let string = |column: &str| {
            row.get(column)
                .and_then(Value::as_str)
                .unwrap_or_else(|| panic!("missing or malformed sys_invocation column {column}"))
                .to_owned()
        };
        let nullable_string = |column: &str| match row.get(column) {
            None | Some(Value::Null) => None,
            Some(Value::String(value)) => Some(value.clone()),
            _ => panic!("malformed nullable sys_invocation column {column}"),
        };
        Self {
            status: string("status"),
            completion_failure: nullable_string("completion_failure"),
            scope: nullable_string("scope"),
            service: string("target_service_name"),
            handler: string("target_handler_name"),
        }
    }
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || !hex.bytes().all(|byte| byte.is_ascii_hexdigit()) {
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
    /// notification sharing its completion id.
    #[test]
    fn a_journal_row_decodes_its_raw_and_a_run_finds_its_result() {
        let journal: Vec<JournalEntry> = [
            json!({ "index": 0, "version": 2, "entry_type": "Command: Input", "name": null, "raw": "" }),
            json!({ "index": 1, "version": 2, "entry_type": "Command: Run", "name": "step", "raw": "00",
                "entry_json": r#"{"Command":{"Run":{"completion_id":0,"name":"step"}}}"# }),
            json!({ "index": 2, "version": 2, "entry_type": "Notification: Run", "name": null, "raw": "7b7d",
                "entry_json": r#"{"Notification":{"Completion":{"Run":{"completion_id":0,"result":{"Success":[]}}}}}"# }),
            json!({ "index": 3, "version": 2, "entry_type": "Command: Output", "name": null, "raw": "" }),
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
        assert!(result.raw_contains(""), "the empty needle is in everything");
        assert!(!result.raw_contains("{}}"), "longer than the bytes");
        assert!(run_result(&journal, "other").is_none());
        assert_eq!(decode_hex("abc"), None, "an odd length is not hex");
        assert_eq!(decode_hex("zz"), None);
    }

    /// `GET /services` lists every registered service with its handlers; the
    /// flattened pairs are what a table of run names is compared with.
    #[test]
    fn the_handlers_of_a_services_listing_are_flattened() {
        let body = json!({
            "services": [
                {
                    "name": "Inv.Stock",
                    "ty": "VirtualObject",
                    "handlers": [
                        { "name": "reserve", "ty": "Exclusive" },
                        { "name": "release", "ty": "Exclusive" },
                    ],
                },
                { "name": "Inv.Api", "ty": "Service", "handlers": [{ "name": "probe" }] },
                { "name": "Inv.Idle", "ty": "Service", "handlers": [] },
            ]
        });
        let handlers = Handler::from_services(&body);
        let pairs: Vec<(&str, &str)> = handlers
            .iter()
            .map(|handler| (handler.service.as_str(), handler.name.as_str()))
            .collect();
        assert_eq!(
            pairs,
            [
                ("Inv.Stock", "reserve"),
                ("Inv.Stock", "release"),
                ("Inv.Api", "probe"),
            ]
        );
        assert_eq!(Handler::from_services(&json!({ "services": [] })), []);
    }
}
