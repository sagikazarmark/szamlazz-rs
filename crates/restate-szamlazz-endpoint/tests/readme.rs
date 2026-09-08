//! The endpoint README is the caller reference: what a backend developer who
//! has never opened the Rust source builds against. These tests hold its
//! examples and tables to the contract types, so the reference cannot drift
//! from the worker:
//!
//! - every fenced `json` block names its contract type in the fence's info
//!   string (`` ```json CreateResponse ``) and round-trips through that type:
//!   a request deserializes and re-serializes to itself, a response
//!   re-serializes to exactly the bytes shown (nulls included: the README
//!   shows what the worker emits, not a trimmed sketch);
//! - the fault examples are the ingress envelope with the worker's fault
//!   (the contract's `Fault`, which every field shown must be a field of)
//!   inside `message`, and the envelope's `code` is the status the fault's
//!   `TerminalCode` pins (the e2e harness's `Reply::fault` in
//!   `crates/restate-szamlazz/tests/e2e/harness/ingress.rs` decodes the same
//!   type out of a live reply);
//! - the `conflict_reason` table has a row per `ConflictReason`, and the
//!   README shows a `CreateResponse` example per `Outcome` (`issued` in the
//!   quick start, the rest in the response reference).
//!
//! The fault table's rows are held to `TerminalCode::ALL` by the `config`
//! module's `every_terminal_code_is_in_every_fault_table`; the TOML blocks by
//! its `every_documented_example_loads`.

use std::fmt::Debug;

use restate_szamlazz::contract::{
    CheckAccountResponse, ConflictReason, CorrectRequest, CreateRequest, CreateResponse,
    DeleteProformaRequest, DeleteProformaResponse, Fault, OrderStatus, Outcome, QueryRequest,
    QueryResponse, QueryTaxpayerRequest, QueryTaxpayerResponse, SetPaymentsRequest,
    SetPaymentsResponse, StornoRequest, StornoResponse,
};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

const README: &str = include_str!("../README.md");

/// A fenced code block of the README: the info string after the opening
/// fence, the body, and the line the fence is on (for messages).
struct Block {
    info: String,
    body: String,
    line: usize,
}

/// Every fenced block whose info string's first word is `language`.
fn fenced(document: &str, language: &str) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut open: Option<Block> = None;
    for (index, text) in document.lines().enumerate() {
        match &mut open {
            Some(_) if text.starts_with("```") => {
                let block = open.take().expect("a block is open");
                if block.info.split_whitespace().next() == Some(language) {
                    blocks.push(block);
                }
            }
            Some(block) => {
                block.body.push_str(text);
                block.body.push('\n');
            }
            None if text.starts_with("```") => {
                open = Some(Block {
                    info: text.trim_start_matches('`').trim().to_owned(),
                    body: String::new(),
                    line: index + 1,
                });
            }
            None => {}
        }
    }
    assert!(open.is_none(), "an unclosed fence in the README");
    blocks
}

/// One JSON example of the README: the contract type its fence names, the
/// parsed value and the line it is on.
struct Example {
    kind: String,
    value: Value,
    line: usize,
}

/// The JSON examples of the README, each with the contract type it names.
fn json_examples() -> Vec<Example> {
    fenced(README, "json")
        .into_iter()
        .map(|block| {
            let mut words = block.info.split_whitespace();
            assert_eq!(words.next(), Some("json"));
            let kind = words.next().unwrap_or_else(|| {
                panic!(
                    "README.md:{}: a JSON example names its contract type in the fence (```json CreateResponse)",
                    block.line
                )
            });
            let value: Value = serde_json::from_str(&block.body).unwrap_or_else(|error| {
                panic!("README.md:{}: not JSON ({error}):\n{}", block.line, block.body)
            });
            Example {
                kind: kind.to_owned(),
                value,
                line: block.line,
            }
        })
        .collect()
}

/// A request example deserializes as `T` and re-serializes to a value that
/// deserializes to the same request (defaults may be omitted in the example).
fn request<T>(example: &Example)
where
    T: DeserializeOwned + serde::Serialize + PartialEq + Debug,
{
    let parsed: T = serde_json::from_value(example.value.clone()).unwrap_or_else(|error| {
        panic!(
            "README.md:{}: not a {} ({error}):\n{:#}",
            example.line,
            std::any::type_name::<T>(),
            example.value
        )
    });
    let again = serde_json::to_value(&parsed).expect("serialize");
    let back: T = serde_json::from_value(again).expect("deserialize what was serialized");
    assert_eq!(
        back, parsed,
        "README.md:{}: the request round-trips",
        example.line
    );
}

/// A response example deserializes as `T` and re-serializes to **exactly** the
/// value shown: the README shows the wire body as the worker emits it, every
/// `null` included.
fn response<T>(example: &Example) -> T
where
    T: DeserializeOwned + serde::Serialize + Debug,
{
    let parsed: T = serde_json::from_value(example.value.clone()).unwrap_or_else(|error| {
        panic!(
            "README.md:{}: not a {} ({error}):\n{:#}",
            example.line,
            std::any::type_name::<T>(),
            example.value
        )
    });
    let again = serde_json::to_value(&parsed).expect("serialize");
    assert_eq!(
        again,
        example.value,
        "README.md:{}: the example is exactly what the worker emits for a {}",
        example.line,
        std::any::type_name::<T>()
    );
    parsed
}

/// The Restate ingress error envelope (server 1.7.8): the HTTP status in
/// `code`, a string `message`, and `source`: `invocation` for a completed
/// invocation's terminal error, `ingress` for an error Restate itself raised.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    code: u16,
    message: String,
    source: String,
}

fn envelope(example: &Example) -> Envelope {
    serde_json::from_value(example.value.clone()).unwrap_or_else(|error| {
        panic!(
            "README.md:{}: not the ingress error envelope ({error}):\n{:#}",
            example.line, example.value
        )
    })
}

/// A fault example: `source: invocation`, `message` a JSON string carrying the
/// worker's fault, whose `code` pins the envelope's `code`.
fn fault(example: &Example) -> Fault {
    let envelope = envelope(example);
    assert_eq!(envelope.source, "invocation", "README.md:{}", example.line);
    let fault: Fault = serde_json::from_str(&envelope.message).unwrap_or_else(|error| {
        panic!(
            "README.md:{}: the envelope's message is not the worker's fault ({error}): {}",
            example.line, envelope.message
        )
    });
    // `Fault` is a response type and tolerates unknown fields; the README
    // shows what the worker emits, so the example must re-serialise to
    // exactly what it shows.
    let shown: Value = serde_json::from_str(&envelope.message).expect("the message is JSON");
    assert_eq!(
        serde_json::to_value(&fault).expect("json"),
        shown,
        "README.md:{}: the fault example carries only the fault's fields",
        example.line
    );
    assert_eq!(
        envelope.code,
        fault.code.status(),
        "README.md:{}: the envelope's code is the status `{}` pins",
        example.line,
        fault.code
    );
    assert!(
        !fault.message.is_empty(),
        "README.md:{}: a fault carries a message",
        example.line
    );
    fault
}

/// A killed invocation's example: `source: invocation`, 500, and a message that
/// is the last retryable error's text, not JSON.
fn killed(example: &Example) {
    let envelope = envelope(example);
    assert_eq!(envelope.source, "invocation", "README.md:{}", example.line);
    assert_eq!(envelope.code, 500, "README.md:{}", example.line);
    assert!(
        serde_json::from_str::<Value>(&envelope.message).is_err(),
        "README.md:{}: a killed invocation's message is plain text, not the worker's fault",
        example.line
    );
}

/// An error the ingress itself raised: `source: ingress`.
fn ingress_error(example: &Example) {
    let envelope = envelope(example);
    assert_eq!(envelope.source, "ingress", "README.md:{}", example.line);
    assert!(
        serde_json::from_str::<Value>(&envelope.message).is_err(),
        "README.md:{}: an ingress error's message is Restate's text",
        example.line
    );
}

/// Every JSON example round-trips through the contract type its fence names.
#[test]
fn every_json_example_round_trips_through_its_contract_type() {
    let examples = json_examples();
    assert!(
        examples.len() >= 12,
        "the README carries the request, response and fault examples"
    );
    for example in &examples {
        match example.kind.as_str() {
            "CreateRequest" => request::<CreateRequest>(example),
            "CorrectRequest" => request::<CorrectRequest>(example),
            "StornoRequest" => request::<StornoRequest>(example),
            "DeleteProformaRequest" => request::<DeleteProformaRequest>(example),
            "QueryRequest" => request::<QueryRequest>(example),
            "QueryTaxpayerRequest" => request::<QueryTaxpayerRequest>(example),
            "SetPaymentsRequest" => request::<SetPaymentsRequest>(example),
            "CreateResponse" => {
                response::<CreateResponse>(example);
            }
            "StornoResponse" => {
                response::<StornoResponse>(example);
            }
            "DeleteProformaResponse" => {
                response::<DeleteProformaResponse>(example);
            }
            "OrderStatus" => {
                response::<OrderStatus>(example);
            }
            "SetPaymentsResponse" => {
                response::<SetPaymentsResponse>(example);
            }
            "QueryResponse" => {
                response::<QueryResponse>(example);
            }
            "QueryTaxpayerResponse" => {
                response::<QueryTaxpayerResponse>(example);
            }
            "CheckAccountResponse" => {
                response::<CheckAccountResponse>(example);
            }
            "Fault" => {
                fault(example);
            }
            "KilledInvocation" => killed(example),
            "IngressError" => ingress_error(example),
            other => panic!(
                "README.md:{}: `{other}` is not a contract type this test knows",
                example.line
            ),
        }
    }
}

/// The README shows one `CreateResponse` per `Outcome`.
#[test]
fn a_create_response_example_exists_per_outcome() {
    let shown: Vec<Outcome> = json_examples()
        .iter()
        .filter(|example| example.kind == "CreateResponse")
        .map(|example| response::<CreateResponse>(example).outcome)
        .collect();
    for outcome in Outcome::ALL {
        assert!(
            shown.contains(&outcome),
            "the README shows a CreateResponse with outcome `{outcome}`"
        );
    }
}

/// The fault examples cover the three envelope cases (a structured fault, a
/// killed invocation, an ingress error), and among the structured ones there
/// is one carrying the document identity and one carrying a `szamlazz_code`,
/// so both optional parts of the inner object are shown.
#[test]
fn the_fault_examples_cover_every_envelope_case() {
    let examples = json_examples();
    for required in ["Fault", "KilledInvocation", "IngressError"] {
        assert!(
            examples.iter().any(|example| example.kind == required),
            "the README shows the `{required}` envelope case"
        );
    }
    let faults: Vec<Fault> = examples
        .iter()
        .filter(|example| example.kind == "Fault")
        .map(fault)
        .collect();
    assert!(
        faults.iter().any(|fault| {
            fault.order.is_some() && fault.kind.is_some() && fault.external_id.is_some()
        }),
        "one fault example carries the order, kind and external id"
    );
    assert!(
        faults.iter().any(|fault| fault.szamlazz_code.is_some()),
        "one fault example carries a szamlazz_code"
    );
}

/// The `conflict_reason` table has a row per `ConflictReason`.
#[test]
fn the_conflict_reason_table_covers_every_variant() {
    for reason in ConflictReason::ALL {
        let row = format!("| `{reason}` |");
        assert!(
            README.contains(&row),
            "the conflict_reason table has a row for `{reason}`"
        );
    }
}
