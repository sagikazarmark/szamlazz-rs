# Vendor clarification: identity on successful credit registration

**Status:** background brief; no vendor answer received. Updated 2026-09-11:
the [combined Hungarian clarification](2026-09-11-agent-vendor-clarification.md)
is now the send-ready message, covering registration and explicit clearing.

## Why clarification is needed

The [English response documentation](https://docs.szamlazz.hu/agent/credit_entry/response)
and [Hungarian response documentation](https://docs.szamlazz.hu/hu/agent/credit_entry/response)
say optional elements may be omitted and additional headers may arrive. Their
schema makes `szamlaszam` optional, but covers success and failure together. The
successful examples include a number; that does not establish a universal
success-path guarantee. A bare `xmlszamlavalasz` containing only `sikeres=true`
passes the shared XSD.

At reviewed revision `f83e5fd`, `RegisterCreditEntry::parse` requires a reported
number and returns `ParseError::Missing("szamlaszam")` for that bare body without the
header. Recorded successful observations include the number; no live numberless
success has been captured. See the [independent adjudication](../review/2026-09-10-agent-api-f83e5fd-adjudication.md#6-numberless-credit-success-genuine-contract-ambiguity-not-dismissed-or-promoted).

## Implementation decision pending the answer

Keep the current result contract until the guarantee is settled. If numberless
success is supported, model a successful acknowledgement with optional **reported**
identity, keeping the requested invoice number separate. Never silently substitute
the request's number as a vendor-reported fact. A parse failure is not permission
to resend: additive repetition can duplicate entries and replacement can overwrite
intervening entries.
