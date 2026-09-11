# Számla Agent clarification request (draft, not sent)

Updated after the 2026-09-11 review at `837dad0`. Priorities: successful credit
echo (§3), combined preview (§1), then empty replacement (§6) and paid-state
semantics (§7). Executable opt-in probes are described in `docs/testing.md`;
their presence is not a vendor answer or evidence of execution.

## 1. Preview and simplified invoice image

Current EN/HU inline schemas at
<https://docs.szamlazz.hu/agent/generating_invoice/xml> and
<https://docs.szamlazz.hu/hu/agent/generating_invoice/xml> put `simpleItems`
before `elonezetpdf`. The downloadable schema
<https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd> and PHP 2.12.4's
`InvoiceHeader::buildXmlData` put preview first.

- Which order(s) does the deployed Számla Agent accept when both are present?
- Are explicit false values supported for each field in that combination?
- Does `elonezetpdf=true` together with `simpleItems=true` always remain a
  preview, without issuing a document?
- Please align the inline schema, download and PHP writer, or document which
  source defines the deployed contract.

Current Rust policy follows download/PHP. No live combined-option probe was run.

## 2. Invoice layout names

The API template table at
<https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template>
and linked knowledge-base page
<https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok>
invert the traditional/envelope-friendly labels for `SzlaAlap` and `SzlaNoEnv`.

- Which token renders the traditional layout, and which the envelope-friendly one?
- What is the default when no template is sent, and can account settings change it?
- Please provide matching token-labelled previews and correct the conflicting page.

Current Rust tokens remain unchanged pending clarification. No rendering probe
was run. Sources and the acquisition record are in
`docs/review/2026-09-10-agent-api-current.md` and its specialist reports.

## 3. Successful credit-entry registration without an echoed number (highest priority)

The version-2 response schema at
<https://docs.szamlazz.hu/agent/credit_entry/response> and its Hungarian version
<https://docs.szamlazz.hu/hu/agent/credit_entry/response> makes only `sikeres`
mandatory. It says `minOccurs="0"` elements may be absent, and that headers may
also arrive. Successful examples nevertheless include `szamlaszam`.

- Does every successful version-2 registration echo a nonblank invoice number
  in either `szamlaszam` or `szlahu_szamlaszam`?
- Can `<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"><sikeres>true</sikeres></xmlszamlavalasz>`
  be the complete successful body, with no number header?
- If so, are net/gross/outstanding amounts likewise optional on success, and is
  the true verdict alone a definitive registration acknowledgement?

Current Rust requires an echoed number for `InvoiceBalance`. No successful
numberless registration has been observed. We will not substitute the requested
number and present it as vendor-reported evidence.

## 4. Receipt order-number repetition setting

Both Agent locales say the receipt repetition setting is independent of invoices:
<https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number>
and <https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/order-number>.
The linked knowledge-base article <https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan>
says the setting applies to all affected document types and cannot be configured
separately by document type.

- Are invoice and receipt repetition controls independent? Where is each configured?
- What scope does each setting cover, and does receipt storno free an order number?
- Please align the pages and distinguish API behavior from receipt-editor behavior.

Current Rust documents the Agent rule. Invoice-account observations are not
receipt evidence; no receipt lifecycle probe was run.

## 5. Hungarian PDF-query request schema

<https://docs.szamlazz.hu/hu/agent/querying_pdf/xml> requires `szamlaszam` and puts
`valaszVerzio` before `rendelesSzam`, whereas the English page and download
<https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd> make the number
optional and put order before version. The Hungarian inline schema also lacks
whitespace between two namespace attributes. Both request pages describe number,
order and external id as alternative selectors.

- Please confirm the accepted sequence and independent order/external-id selection,
  and repair the Hungarian schema to match the deployed contract.

Current Rust follows the English/download sequence. No alternative-order live
probe was run. This entire document remains an unsent draft.

## 6. Explicit empty credit-entry replacement

<https://docs.szamlazz.hu/agent/credit_entry/xml> and the downloadable request
schema allow zero `kifizetes` elements and say `additiv=false` replaces previous
entries. The Rust client now exposes that exact request as `ClearCreditEntries`.

- Does a successful zero-entry replacement remove every previous credit entry?
- Is the same request accepted when the invoice already has no entries?
- If it is refused or ignored, what is the supported way to clear entries?
- What balance/number acknowledgement and IPN follow each case?
- Does issuer-tax-number selection change these semantics for incoming invoices?

The new `clear_credit_entries_populated` and `clear_credit_entries_already_empty`
probes check those states independently, but have not been run as part of this implementation. Clearing remains a
documentation-derived intent, not an independently observed effect.

## 7. Paid-state omission versus explicit false

The invoice request schema declares optional boolean `fizetve`, with no default.
Both Rust and official PHP 2.12.4 emit it only when true.

- Are omission and explicit false equivalent for every supported document kind,
  payment method (especially cash) and account default?
- Can false suppress automatic paid treatment? If so, under which settings?
- Please document the default and provide contrasting requests/results if they differ.

Current Rust emission remains unchanged pending evidence; absence/false
equivalence is not claimed as a live-tested fact.

## 8. Current NAV taxpayer forwarding

<https://docs.szamlazz.hu/agent/querying_taxpayer/response> shows dated NAV 2.0
examples while delegating to the NAV 3.0 specification. Rust reads both layouts.

- Which version and optional taxpayer fields does Számla Agent currently forward?
- Can `funcCode=OK` legitimately omit `taxpayerValidity`? What does that mean?
- Are generic NAV failure roots forwarded unchanged or wrapped as
  `QueryTaxpayerResponse`? Please provide full root/namespace, HTTP status and
  relevant headers for technical/authentication errors.

The parser continues to require validity on OK and never invents false for an
absent value. Direct NAV possibilities are not treated as Számla Agent guarantees.
