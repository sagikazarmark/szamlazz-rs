//! Rows of the SQL introspection API: a `sys_journal` entry with its `raw`
//! hex-decoded to bytes ([`JournalEntry`]), the result of a named run
//! ([`run_result`]) and a `sys_invocation` row ([`Invocation`]). What a watch
//! saw of an invocation's attempts while it ran is
//! [`admin::Retries`](crate::harness::admin::Retries).

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

// ----- run-result samples ----------------------------------------------------------

/// The wiremock's URI on the run that wrote the committed samples: what the
/// `account` sample's endpoint carries, substituted for this run's by
/// [`sample_with`]. Re-dump the samples and this together (a pre-seam run of
/// scenario (i) on `main`, `a7dbc2a`).
pub(crate) const SAMPLE_MOCK_URI: &str = "http://127.0.0.1:34653";

/// A committed run-result sample (`tests/e2e/samples/<name>.raw`: the raw
/// `Notification: Run` bytes a pre-seam deployment wrote, #133).
pub(crate) fn sample(name: &str) -> Vec<u8> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/e2e/samples")
        .join(format!("{name}.raw"));
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

/// The sample re-enveloped for this run: `from` replaced by `to` in its JSON
/// payload (the account's endpoint carries the wiremock's port, different on
/// every run) and the two protobuf length prefixes recomputed. The layout is
/// the server's `RunCompletionNotificationMessage` under journal v2
/// (verified against 1.7.8): `08 <completion id> 2a <len> 0a <len> <payload>`;
/// a sample of another shape is a panic naming it.
pub(crate) fn sample_with(sample: &[u8], from: &str, to: &str) -> Vec<u8> {
    fn varint(bytes: &[u8], at: &mut usize) -> u64 {
        let mut value = 0u64;
        let mut shift = 0;
        loop {
            let byte = *bytes
                .get(*at)
                .unwrap_or_else(|| panic!("the sample ends inside a varint at {at}: {bytes:?}"));
            *at += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte < 0x80 {
                return value;
            }
            shift += 7;
        }
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "each pushed byte is masked to seven bits first"
    )]
    fn encode_varint(mut value: usize, into: &mut Vec<u8>) {
        while value >= 0x80 {
            into.push((value as u8 & 0x7f) | 0x80);
            value >>= 7;
        }
        into.push(value as u8);
    }
    fn expect_tag(sample: &[u8], at: usize, tag: u8, field: &str) {
        assert_eq!(
            sample.get(at),
            Some(&tag),
            "{field} expected at byte {at} of the sample: {sample:?}"
        );
    }

    let mut at = 0;
    expect_tag(sample, at, 0x08, "field 1 (completion id)");
    at += 1;
    let completion_id = varint(sample, &mut at);
    expect_tag(sample, at, 0x2a, "field 5 (result)");
    at += 1;
    let _outer = varint(sample, &mut at);
    expect_tag(sample, at, 0x0a, "field 1 of the result (value)");
    at += 1;
    let inner = varint(sample, &mut at);
    let payload = sample.get(at..).unwrap_or_default();
    assert_eq!(
        usize::try_from(inner).expect("a small length"),
        payload.len(),
        "the inner length is the payload's"
    );

    let payload = String::from_utf8(payload.to_vec())
        .expect("a JSON payload")
        .replace(from, to)
        .into_bytes();
    let mut value = Vec::new();
    value.push(0x0a);
    encode_varint(payload.len(), &mut value);
    value.extend_from_slice(&payload);

    let mut raw = vec![0x08];
    encode_varint(
        usize::try_from(completion_id).expect("a small id"),
        &mut raw,
    );
    raw.push(0x2a);
    encode_varint(value.len(), &mut raw);
    raw.extend_from_slice(&value);
    raw
}

/// `sample_with` on the committed `account` sample rebuilds the sample
/// itself when nothing is substituted, and a substitution of a different
/// length re-encodes both prefixes.
#[test]
fn sample_with_re_envelopes_the_payload() {
    let account = sample("account");
    assert!(
        String::from_utf8_lossy(&account).contains(&format!("\"endpoint\":\"{SAMPLE_MOCK_URI}\"")),
        "the sample carries SAMPLE_MOCK_URI: {}",
        String::from_utf8_lossy(&account)
    );
    assert_eq!(sample_with(&account, "nothing", "nothing"), account);
    let longer = format!("{SAMPLE_MOCK_URI}0");
    let re_enveloped = sample_with(&account, SAMPLE_MOCK_URI, &longer);
    assert_eq!(re_enveloped.len(), account.len() + 1);
    assert!(
        String::from_utf8_lossy(&re_enveloped).contains(&format!("\"endpoint\":\"{longer}\"")),
        "{}",
        String::from_utf8_lossy(&re_enveloped)
    );
    assert_eq!(
        sample_with(&re_enveloped, &longer, SAMPLE_MOCK_URI),
        account
    );
}
