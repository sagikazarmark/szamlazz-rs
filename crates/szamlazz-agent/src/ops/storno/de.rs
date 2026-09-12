//! Keep JSON content raw until the adjacent tag is known. Serde's derived
//! adjacent-tag decoder otherwise buffers content-before-tag into private maps,
//! losing the distinction between monetary number tokens and lookalike objects.
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

use super::{CreatedInvoice, InvoiceAcknowledgement, StornoResponse};

// Preserve the ordinary Serde representation for non-JSON formats and caller
// wrappers that have already buffered their content. Only direct JSON needs the
// raw-token route; passing it through Value would erase object identity.
#[derive(Deserialize)]
#[serde(
    remote = "StornoResponse",
    rename = "StornoResponse",
    tag = "state",
    content = "response",
    rename_all = "snake_case"
)]
enum Representation {
    Numbered(CreatedInvoice),
    Unnumbered(InvoiceAcknowledgement),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum State {
    Numbered,
    Unnumbered,
}

#[derive(Deserialize)]
struct JsonResponse {
    state: State,
    response: Box<RawValue>,
}

pub(super) fn response<'de, D: Deserializer<'de>>(de: D) -> Result<StornoResponse, D::Error> {
    if !crate::number::de::is_json::<D>() {
        return Representation::deserialize(de);
    }
    // Only serde_json receives its private RawValue protocol.
    let raw = Box::<RawValue>::deserialize(de)?;
    let JsonResponse { state, response } =
        serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
    match state {
        State::Numbered => serde_json::from_str(response.get()).map(StornoResponse::Numbered),
        State::Unnumbered => serde_json::from_str(response.get()).map(StornoResponse::Unnumbered),
    }
    .map_err(serde::de::Error::custom)
}
