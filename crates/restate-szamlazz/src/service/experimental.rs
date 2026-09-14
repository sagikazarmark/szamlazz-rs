//! Isolated #247 actual-service experiment; never selected by compilation target.

use super::{Agent, Order, support::Fault};

impl Order {
    /// Enable the isolated `RequestResponse` ordinary-issuance experiment (#247).
    /// Requires enabled provider per-type duplicate-order checking. Proformas,
    /// ordinary/prepayment/final invoices, including exact-target reissue and pinned references, reads and
    /// evidence-carrying recovery, Order storno and exact-target proforma deletion are supported. Other mutations are refused
    /// before provider I/O. Matching issuance does not prove uniqueness
    /// or exclude a delayed old execution. Not a production deployment option.
    #[must_use]
    pub fn experimental_request_response(mut self) -> Self {
        self.experimental_request_response = true;
        self
    }

    pub(super) fn require_supported_mutation(&self) -> Result<(), Fault> {
        require_supported(self.experimental_request_response)
    }
}

impl Agent {
    /// Enable reads and existing unmanaged storno for the isolated #247
    /// `RequestResponse` endpoint. Credit-entry writes remain refused.
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
