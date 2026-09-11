# Vendor clarification: identity on successful credit registration

**Status:** prepared for submission; no vendor answer received.

## Question to send

For Számla Agent credit-entry registration (`action-szamla_agent_kifiz`) with
`valaszVerzio=2`, does **every successful registration** return a nonempty invoice
number in at least one of these channels?

- XML `xmlszamlavalasz/szamlaszam`
- HTTP header `szlahu_szamlaszam`

Can a successful registration instead return HTTP 200 with no number header and
the following body (possibly including totals but omitting `szamlaszam`)?

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
</xmlszamlavalasz>
```

If a number is guaranteed, please clarify that guarantee specifically for
successful version-2 responses. If omission is supported, please confirm whether
`sikeres=true` alone establishes completion of the requested registration, for
both additive and replacing calls.

## Why clarification is needed

The [English response documentation](https://docs.szamlazz.hu/agent/credit_entry/response)
and [Hungarian response documentation](https://docs.szamlazz.hu/hu/agent/credit_entry/response)
say optional elements may be omitted and additional headers may arrive. Their
schema makes `szamlaszam` optional, but covers success and failure together. The
successful examples include a number; that does not establish a universal
success-path guarantee. The bare envelope above passes the shared XSD.

At reviewed revision `f83e5fd`, `RegisterCreditEntry::parse` requires a reported
number and returns `ParseError::Missing("szamlaszam")` for that body without the
header. Recorded successful observations include the number; no live numberless
success has been captured. See the [independent adjudication](../review/2026-09-10-agent-api-f83e5fd-adjudication.md#6-numberless-credit-success-genuine-contract-ambiguity-not-dismissed-or-promoted).

## Implementation decision pending the answer

Keep the current result contract until the guarantee is settled. If numberless
success is supported, model a successful acknowledgement with optional **reported**
identity, keeping the requested invoice number separate. Never silently substitute
the request's number as a vendor-reported fact. A parse failure is not permission
to resend: additive repetition can duplicate entries and replacement can overwrite
intervening entries.
