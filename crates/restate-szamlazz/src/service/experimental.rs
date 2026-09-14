//! Isolated #247 actual-service experiment; never selected by compilation target.

use super::{Agent, Order, support::Fault};

impl Order {
    /// Enable the isolated `RequestResponse` ordinary-issuance experiment (#247).
    /// Requires enabled provider duplicate-order checking. Only fresh ordinary
    /// invoices and reads are supported; other mutations (including recovery)
    /// are refused before provider I/O. Matching issuance does not prove uniqueness
    /// or exclude a delayed old execution. Not a production deployment option.
    #[must_use]
    pub fn experimental_request_response(mut self) -> Self {
        self.experimental_request_response = true;
        self
    }

    pub(super) fn require_supported_mutation(&self) -> Result<(), Fault> {
        require_supported(self.experimental_request_response)
    }

    pub(super) fn require_initial_invoice(
        &self,
        request: &crate::contract::CreateRequest,
    ) -> Result<(), Fault> {
        if self.experimental_request_response
            && (request.options.reissue.is_some()
                || matches!(
                    request.options.proforma,
                    crate::contract::ProformaLink::Number(_)
                ))
        {
            return Err(Fault::invalid_input(
                "experimental RequestResponse supports only initial ordinary invoices without proforma conversion",
            ));
        }
        Ok(())
    }
}

impl Agent {
    /// Restrict this Agent to reads for the isolated #247 `RequestResponse` endpoint.
    /// Enable alongside [`Order::experimental_request_response`].
    #[must_use]
    pub fn experimental_request_response(mut self) -> Self {
        self.experimental_request_response = true;
        self
    }

    pub(super) fn require_supported_mutation(&self) -> Result<(), Fault> {
        require_supported(self.experimental_request_response)
    }
}

fn require_supported(experimental: bool) -> Result<(), Fault> {
    if experimental {
        Err(Fault::invalid_input(
            "mutation unsupported by the experimental RequestResponse endpoint",
        ))
    } else {
        Ok(())
    }
}
