//! Caller civil dates: one spelling, without implicit timestamp truncation.

use jiff::civil::Date;
use serde::{Deserialize, Deserializer, de::Error};

const EXPECTED: &str = "expected a calendar date YYYY-MM-DD with year 0001 through 9999";

fn parse<E: Error>(text: &str) -> Result<Date, E> {
    let bytes = text.as_bytes();
    if bytes.len() != 10
        || bytes.iter().enumerate().any(|(index, byte)| match index {
            4 | 7 => *byte != b'-',
            _ => !byte.is_ascii_digit(),
        })
        || bytes.starts_with(b"0000")
    {
        return Err(E::custom(EXPECTED));
    }
    text.parse().map_err(|_| E::custom(EXPECTED))
}

pub(super) fn required<'de, D: Deserializer<'de>>(de: D) -> Result<Date, D::Error> {
    parse(&String::deserialize(de)?)
}

pub(super) fn optional<'de, D: Deserializer<'de>>(de: D) -> Result<Option<Date>, D::Error> {
    Option::<String>::deserialize(de)?
        .map(|text| parse(&text))
        .transpose()
}

#[cfg(feature = "schemars")]
pub(super) fn schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({
        "type": "string",
        "format": "date",
        "minLength": 10,
        "maxLength": 10,
        "pattern": r"^([0-9]{3}[1-9]|[0-9]{2}[1-9][0-9]|[0-9][1-9][0-9]{2}|[1-9][0-9]{3})-[0-9]{2}-[0-9]{2}$",
        "description": "Calendar date YYYY-MM-DD, year 0001 through 9999."
    })
}

#[cfg(feature = "schemars")]
pub(super) fn optional_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
    let mut schema = schema(generator);
    schema.insert("type".into(), serde_json::json!(["string", "null"]));
    schema
}
