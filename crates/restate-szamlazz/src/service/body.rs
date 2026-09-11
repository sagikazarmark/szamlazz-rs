//! The JSON body of a handler request, decoded by the handler rather than
//! the SDK, so that a malformed body is the structured `invalid_input` fault.

use bytes::Bytes;
use restate_sdk::serde::{Deserialize, InputMetadata, Json, OutputMetadata, PayloadMetadata};

use super::support::Fault;

/// The JSON body of a handler request, decoded by the handler.
///
/// The SDK decodes a `Json<T>` input before the handler runs and answers a
/// body it cannot decode with a plain-text 400 (`Cannot decode input
/// payload: …`), not the `{ "code", "message" }` fault body.
/// `Body<T>` defers the decode into the handler: its SDK [`Deserialize`]
/// never fails (it keeps the verdict, the request or serde's error), and the
/// handler turns the error into the `invalid_input` fault as its first act,
/// before the prologue, so a malformed request journals nothing. Every
/// malformed body (an unknown field, a wrong type, a missing required field,
/// invalid JSON) is therefore the same structured fault, with serde's
/// message naming the field when there is one.
///
/// Actual JSON objects using `serde_json`'s private arbitrary-precision number
/// key are refused before typed decoding. Generic Serde buffering otherwise
/// makes those objects indistinguishable from genuine numeric tokens.
///
/// Its discovery metadata is exactly [`Json<T>`]'s: the same JSON schema
/// (with the `schemars` feature, `schema_for!(T)`), the same content type,
/// so the `OpenAPI` export does not change with the wrapper.
///
/// Callers of the generated `OrderClient` / `AgentClient` build one with
/// [`Body::new`] (or `From<T>`); it serialises as the request itself.
#[derive(Debug)]
pub struct Body<T>(Result<T, serde_json::Error>);

impl<T> Body<T> {
    /// A body holding the decoded `request`.
    pub const fn new(request: T) -> Self {
        Self(Ok(request))
    }

    /// The decoded request, or the `invalid_input` fault (400) naming what
    /// was wrong with the body.
    pub(super) fn into_request(self) -> Result<T, Fault> {
        self.0
            .map_err(|error| Fault::invalid_input(format!("malformed request body: {error}")))
    }
}

impl<T> From<T> for Body<T> {
    fn from(request: T) -> Self {
        Self::new(request)
    }
}

impl<T> Deserialize for Body<T>
where
    for<'de> T: serde::Deserialize<'de>,
{
    type Error = std::convert::Infallible;

    fn deserialize(bytes: &mut Bytes) -> Result<Self, Self::Error> {
        Ok(Self((|| {
            let raw: &serde_json::value::RawValue = serde_json::from_slice(bytes)?;
            check_json_objects(raw, 0)?;
            serde_json::from_slice(bytes)
        })()))
    }
}

// Serde's arbitrary-precision number travels through visit_map with a private
// key. After buffering it is indistinguishable from a caller's lookalike object.
// Check actual JSON objects before that representation is introduced, retaining
// numeric source tokens and the normal typed decoder's duplicate-field checks.
fn check_json_objects(
    raw: &serde_json::value::RawValue,
    depth: usize,
) -> Result<(), serde_json::Error> {
    use serde::de::Error as _;
    if depth >= 128 {
        return Err(serde_json::Error::custom(
            "request JSON nesting limit exceeded",
        ));
    }
    match raw.get().as_bytes().first() {
        Some(b'{') => {
            let fields: std::collections::BTreeMap<String, &serde_json::value::RawValue> =
                serde_json::from_str(raw.get())?;
            for (key, value) in fields {
                if key == "$serde_json::private::Number" {
                    return Err(serde_json::Error::custom(
                        "expected a decimal string or JSON number, not a number object",
                    ));
                }
                check_json_objects(value, depth + 1)?;
            }
        }
        Some(b'[') => {
            let values: Vec<&serde_json::value::RawValue> = serde_json::from_str(raw.get())?;
            for value in values {
                check_json_objects(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

impl<T: serde::Serialize> restate_sdk::serde::Serialize for Body<T> {
    type Error = serde_json::Error;

    fn serialize(&self) -> Result<Bytes, Self::Error> {
        match &self.0 {
            Ok(request) => serde_json::to_vec(request).map(Bytes::from),
            Err(error) => Err(serde::ser::Error::custom(error)),
        }
    }
}

impl<T> PayloadMetadata for Body<T>
where
    Json<T>: PayloadMetadata,
{
    fn json_schema() -> Option<serde_json::Value> {
        <Json<T>>::json_schema()
    }

    fn input_metadata() -> InputMetadata {
        <Json<T>>::input_metadata()
    }

    fn output_metadata() -> OutputMetadata {
        <Json<T>>::output_metadata()
    }
}
