# Fixture sources

## Workspace-only official reference corpus

The files under `fixtures/upstream/` were obtained from szamlazz.hu
documentation and download endpoints on the dates recorded below.
They are retained only as a workspace reference corpus for protocol research and
comparison.

No redistribution license for this official corpus has been identified. These
files must not be copied, symlinked, or otherwise included in published Cargo
packages. The source tables below document only this workspace-local corpus;
they do not describe package fixtures.

## Packaged synthetic fixtures

Files under `fixtures/synthetic/` are project-maintained synthetic test data
constructed for this project's parser models. Published packages contain these
purpose-built samples through crate-local `tests/synthetic` symlinks, not the
verbatim official examples or XSD files in the workspace-only corpus.

Files under `crates/szamlazz-agent/tests/golden/` are project-generated
serialization expectations for the crate's own test inputs. They are likewise
project-authored test data, not copies of the official request examples.

## Official corpus provenance

Unless otherwise noted, files were fetched on 2026-07-04 from
https://docs.szamlazz.hu/ (docs pages, examples extracted verbatim from the
pages' code blocks) and https://www.szamlazz.hu/ (XSD files, downloaded
directly). No values were modified or reformatted, except where noted below.

## agent/requests/

| File | Source URL |
|---|---|
| `agent/requests/xmlszamla.xml` | https://docs.szamlazz.hu/agent/generating_invoice/xml |
| `agent/requests/xmlszamlast.xml` | https://docs.szamlazz.hu/agent/reversing_invoice/xml |
| `agent/requests/xmlszamlakifiz.xml` | https://docs.szamlazz.hu/agent/credit_entry/xml |
| `agent/requests/xmlszamlapdf.xml` | https://docs.szamlazz.hu/agent/querying_pdf/xml (shown without `<?xml ?>` declaration in the docs; kept verbatim) |
| `agent/requests/xmlszamlaxml.xml` | https://docs.szamlazz.hu/agent/querying_xml/xml |
| `agent/requests/xmlszamladbkdel.xml` | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml (first example: delete by invoice number) |
| `agent/requests/xmlszamladbkdel_ordernumber.xml` | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml (second example: delete by order number) |
| `agent/requests/xmlnyugtacreate.xml` | https://docs.szamlazz.hu/agent/generating_receipt/xml |
| `agent/requests/xmlnyugtast.xml` | https://docs.szamlazz.hu/agent/reversing_receipt/xml |
| `agent/requests/xmlnyugtaget.xml` | https://docs.szamlazz.hu/agent/querying_receipt/xml |
| `agent/requests/xmlnyugtasend.xml` | https://docs.szamlazz.hu/agent/sending_receipt/xml |
| `agent/requests/xmltaxpayer.xml` | https://docs.szamlazz.hu/agent/querying_taxpayer/xml |

## agent/responses/

| File | Source URL |
|---|---|
| `agent/responses/generating_invoice_text_error.txt` | https://docs.szamlazz.hu/agent/generating_invoice/response ("Text response" example) |
| `agent/responses/xmlszamlavalasz.xml` | https://docs.szamlazz.hu/agent/generating_invoice/response (XML response, success) |
| `agent/responses/xmlszamlavalasz_pdf.xml` | https://docs.szamlazz.hu/agent/generating_invoice/response (XML response, success with base64 `pdf`; the base64 is abbreviated with `....` in the docs themselves) |
| `agent/responses/xmlszamlavalasz_error.xml` | https://docs.szamlazz.hu/agent/generating_invoice/response (XML response, login error) |
| `agent/responses/reversing_invoice_text_error.txt` | https://docs.szamlazz.hu/agent/reversing_invoice/response (text error example; only response example on the page) |
| `agent/responses/credit_entry_text_error.txt` | https://docs.szamlazz.hu/agent/credit_entry/response (text error example; only response example on the page) |
| `agent/responses/querying_pdf_text_error.txt` | https://docs.szamlazz.hu/agent/querying_pdf/response ("PDF response" text error example) |
| `agent/responses/querying_pdf_xmlszamlavalasz.xml` | https://docs.szamlazz.hu/agent/querying_pdf/response (XML response, success; base64 `pdf` abbreviated with `....` in the docs) |
| `agent/responses/querying_pdf_xmlszamlavalasz_error.xml` | https://docs.szamlazz.hu/agent/querying_pdf/response (unsuccessful request example) |
| `agent/responses/szamla_query.xml` | https://docs.szamlazz.hu/agent/querying_xml/response (successful request example, `<szamla>` document) |
| `agent/responses/xmlszamladbkdelvalasz.xml` | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response (success) |
| `agent/responses/xmlszamladbkdelvalasz_error.xml` | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response (unsuccessful deletion) |
| `agent/responses/xmlnyugtavalasz.xml` | https://docs.szamlazz.hu/agent/generating_receipt/response (XML response example) |
| `agent/responses/xmlnyugtasendvalasz.xml` | https://docs.szamlazz.hu/agent/sending_receipt/response (success) |
| `agent/responses/xmlnyugtasendvalasz_error.xml` | https://docs.szamlazz.hu/agent/sending_receipt/response (failed) |
| `agent/responses/taxpayer.xml` | https://docs.szamlazz.hu/agent/querying_taxpayer/response (success) |
| `agent/responses/taxpayer_error.xml` | https://docs.szamlazz.hu/agent/querying_taxpayer/response (failed) |
| `agent/responses/taxpayer_invalid_taxnumber.xml` | https://docs.szamlazz.hu/agent/querying_taxpayer/response (invalid tax number) |

Not available:

- https://docs.szamlazz.hu/agent/querying_receipt/response and
  https://docs.szamlazz.hu/agent/reversing_receipt/response contain no example bodies; both state
  the response matches the one for generating new receipts (`agent/responses/xmlnyugtavalasz.xml`).
- https://docs.szamlazz.hu/agent/querying_taxpayer/response has an "XML response scheme" heading
  but no schema/code block under it (the payload is the NAV Online Számla `QueryTaxpayerResponse`,
  namespace `http://schemas.nav.gov.hu/OSA/2.0/api`).

## agent/xsd/

Downloaded directly (verified to be XML, not HTML error pages):

| File | Source URL |
|---|---|
| `agent/xsd/xmlszamla.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd |
| `agent/xsd/xmlszamlavalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd |
| `agent/xsd/xmlszamlast.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd |
| `agent/xsd/xmlszamlakifiz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/agentkifiz/xmlszamlakifiz.xsd |
| `agent/xsd/xmlszamlapdf.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd |
| `agent/xsd/xmlszamlaxml.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd |
| `agent/xsd/szamla.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd (schema of the `<szamla>` document returned by querying_xml; same file as `adatkapcsolat/szamla.xsd`) |
| `agent/xsd/xmlnyugtacreate.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd |
| `agent/xsd/xmlnyugtast.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd |
| `agent/xsd/xmlnyugtaget.xsd` | https://docs.szamlazz.hu/agent/querying_receipt/xsd (inline code block; refreshed 2026-08-11 — the served file at https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd is stale and lacks `rendelesSzam`. The docs render the block with newlines collapsed; the original 4-space indentation was restored, content unchanged) |
| `agent/xsd/xmlnyugtasend.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd |
| `agent/xsd/xmlnyugtasendvalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd (the URL linked from the docs, http://www.szamlazz.hu/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd, returns 404) |
| `agent/xsd/xmlnyugtavalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd |
| `agent/xsd/xmltaxpayer.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd (the URL linked from the docs, http://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd, returns 404) |

Extracted verbatim from inline docs code blocks (the canonical URLs linked from the docs —
http://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd and
http://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd — return 404, and no
working variant was found under /szamla/docs/xsds/):

| File | Source URL |
|---|---|
| `agent/xsd/xmlszamladbkdel.xsd` | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xsd (inline code block) |
| `agent/xsd/xmlszamladbkdelvalasz.xsd` | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response ("XML response scheme" inline code block) |

Note: each https://docs.szamlazz.hu/agent/&lt;operation&gt;/xsd page also shows the schema inline;
the downloaded files above are the canonical versions.

**Deliberate deviation:** `agent/xsd/xmlszamla.xsd` was patched by hand (commit `3fc523a`) to add
the `csoportazonosito` (vevő) and `torloKod` (tétel) elements. The docs pages — prose, examples,
and the inline XSD at https://docs.szamlazz.hu/agent/generating_invoice/xsd — document both
fields, but the file served at the download URL above is a stale, older revision that omits them
(verified 2026-08-08: the served file is unchanged since 2026-07-04 and never contained them).
The docs' inline XSD is the normative one ("The sent XML file must comply with the following XSD
schema"). Do not refresh this file from the download URL without re-checking the docs' inline
XSD, or the two elements will be silently dropped again. Similarly, the served `xmlnyugtaget.xsd`
lacks the documented `rendelesSzam` selector (`agent/xsd/xmlnyugtaget.xsd` was therefore
refreshed from the docs' inline XSD on 2026-08-11, see the table above), and the served
`xmlnyugtaarchiv.xsd` dropped `rendelesSzam` after 2026-07-04 — the docs pages remain the
source of truth over the served XSD files where they disagree.

## adatkapcsolat/

Examples extracted verbatim from docs pages:

| File | Source URL |
|---|---|
| `adatkapcsolat/szamla_example.xml` | https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak ("Outgoing invoice submission (XML)" annotated example) |
| `adatkapcsolat/szamlavalasz_example.xml` | https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak ("Example XML for Expected Response"; shown without `<?xml ?>` declaration in the docs; kept verbatim) |

XSDs downloaded directly (URLs linked from, or discovered via, the adatkapcsolat docs pages):

| File | Source URL |
|---|---|
| `adatkapcsolat/szamla.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd |
| `adatkapcsolat/szamlavalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamlavalasz.xsd |
| `adatkapcsolat/szamlabe.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/szamlabe/szamlabe.xsd (incoming-invoice schema, shown inline on https://docs.szamlazz.hu/penzugyi-adatkapcsolat/bejovo-szamlak) |
| `adatkapcsolat/szamlabevalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/szamlabe/szamlabevalasz.xsd |
| `adatkapcsolat/banktranz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/banktranz/banktranz.xsd |
| `adatkapcsolat/banktranzvalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/banktranz/banktranzvalasz.xsd |
| `adatkapcsolat/xmlnyugtaarchiv.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugta/xmlnyugtaarchiv.xsd (receipt-archive schema, shown inline on https://docs.szamlazz.hu/penzugyi-adatkapcsolat/nyugtak; identical copy also served at https://www.szamlazz.hu/szamla/docs/xsds/nyugtaarchiv/xmlnyugtaarchiv.xsd) |
| `adatkapcsolat/nyugtavalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugta/nyugtavalasz.xsd |

Not available:

- The downloadable ZIP package (description PDF + sample XMLs + XSDs) is not linked anywhere on
  the current penzugyi-adatkapcsolat pages (checked kezdd-el, mukodes, registration, kapcsolat,
  kimeno-szamlak, bejovo-szamlak, banki-tranzakciok, nyugtak, in both the English and the /hu/
  locale). No `docs.pdf` could therefore be saved; the XSDs above were downloaded individually
  instead.
- No example (sample document) XML is shown on the bejovo-szamlak, banki-tranzakciok or nyugtak
  pages — those pages contain only the XSDs. Incoming-invoice, bank-transaction and receipt
  pushed-document examples and their receiver-response examples are therefore not included.
