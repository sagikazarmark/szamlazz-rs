//! Keep JSON content raw until the adjacent tag is known. Serde's derived
//! adjacent-tag decoder otherwise buffers content-before-tag into private maps,
//! losing the distinction between monetary number tokens and lookalike objects.
use std::fmt;

use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::value::RawValue;

use super::{CreatedInvoice, InvoiceAcknowledgement, StornoResponse};

// Preserve the ordinary Serde representation for non-JSON formats and caller
// wrappers that have already buffered their content. Only direct JSON needs the
// raw-token route; passing it through Value would erase object identity.
#[derive(Deserialize)]
#[serde(
    remote = "StornoResponse",
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
    struct Exact;

    impl<'de> Visitor<'de> for Exact {
        type Value = StornoResponse;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("a storno response with state and response fields")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
            let raw =
                Box::<RawValue>::deserialize(serde::de::value::MapAccessDeserializer::new(map))?;
            let JsonResponse { state, response } =
                serde_json::from_str(raw.get()).map_err(serde::de::Error::custom)?;
            match state {
                State::Numbered => {
                    serde_json::from_str(response.get()).map(StornoResponse::Numbered)
                }
                State::Unnumbered => {
                    serde_json::from_str(response.get()).map(StornoResponse::Unnumbered)
                }
            }
            .map_err(serde::de::Error::custom)
        }

        fn visit_newtype_struct<D: Deserializer<'de>>(
            self,
            de: D,
        ) -> Result<Self::Value, D::Error> {
            Representation::deserialize(de)
        }
    }

    if !de.is_human_readable() {
        return Representation::deserialize(de);
    }
    // Same RawValue protocol as the monetary scalar decoder: JSON hands us
    // untouched text, ordinary formats unwrap a transparent newtype.
    de.deserialize_newtype_struct("$serde_json::private::RawValue", Exact)
}
