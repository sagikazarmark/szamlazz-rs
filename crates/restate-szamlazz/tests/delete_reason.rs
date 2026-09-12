//! Unknown deletion reasons stay unclassified at the public JSON boundary.

use restate_szamlazz::contract::{DeleteProformaResponse, DeleteReason};
use serde_json::json;

#[test]
fn unfamiliar_deletion_reasons_preserve_text_without_inventing_vendor_provenance() {
    for token in ["document_locked", "335", "FUTURE_CODE"] {
        let wire = json!({"deleted": false, "reason": token});
        let response: DeleteProformaResponse =
            serde_json::from_value(wire.clone()).expect("response");
        assert_eq!(response.reason, Some(DeleteReason::Other(token.to_owned())));
        assert_eq!(response.reason.as_ref().expect("reason").as_str(), token);
        assert_eq!(serde_json::to_value(response).expect("encode"), wire);
    }
}
