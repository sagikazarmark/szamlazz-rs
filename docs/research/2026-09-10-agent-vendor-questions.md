# Számla Agent clarification request (draft, not sent)

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
