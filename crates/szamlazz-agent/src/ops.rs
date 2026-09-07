//! One module per Számla Agent operation: the request type, its hand-written
//! XML writer, and its response parser live together. A response block more
//! than one operation parses is modelled once in `crate::xml` and converted
//! here into each operation's own public type.

pub mod credit_entry;
pub mod invoice;
pub mod proforma;
pub mod query_pdf;
pub mod query_xml;
pub mod receipt;
pub mod storno;
pub mod taxpayer;
