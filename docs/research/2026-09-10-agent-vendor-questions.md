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
`docs/review/2026-09-10-agent-api/REPORT.md` and its specialist reports.
