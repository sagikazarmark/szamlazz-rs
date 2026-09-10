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

The corpus is read by tests, never embedded in them: `crates/szamlazz-agent/tests/upstream`
and `crates/szamlazz-adatkapcsolat/tests/upstream` are workspace-only symlinks into
`fixtures/upstream/agent/` and `fixtures/upstream/adatkapcsolat/`, excluded from the
package (`exclude = ["tests/upstream"]` in each `Cargo.toml`). The tests behind them
(`tests/upstream.rs`, `tests/document.rs`) open the files at run time with `std::fs`
(never `include_bytes!`, which would copy them into the package), and skip with a
message when the directory is absent, as it is in a package built from crates.io.

## Packaged synthetic fixtures

Files under `fixtures/synthetic/` are project-maintained synthetic test data
constructed for this project's parser models. Published packages contain these
purpose-built samples through crate-local `tests/synthetic` symlinks, not the
verbatim official examples or XSD files in the workspace-only corpus.

Files under `crates/szamlazz-agent/tests/golden/` are project-generated
serialization expectations for the crate's own test inputs. They are likewise
project-authored test data, not copies of the official request examples.

## Official corpus provenance — historical acquisition record

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

Not available at the July acquisition:

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
| `agent/xsd/xmlnyugtaget.xsd` | https://docs.szamlazz.hu/agent/querying_receipt/xsd (inline code block; refreshed 2026-08-11: the served file at https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd is stale and lacks `rendelesSzam`. The docs render the block with newlines collapsed; the original 4-space indentation was restored, content unchanged) |
| `agent/xsd/xmlnyugtasend.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd |
| `agent/xsd/xmlnyugtasendvalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd (the URL linked from the docs, http://www.szamlazz.hu/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd, returns 404) |
| `agent/xsd/xmlnyugtavalasz.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd |
| `agent/xsd/xmltaxpayer.xsd` | https://www.szamlazz.hu/szamla/docs/xsds/taxpayer/xmltaxpayer.xsd (the URL linked from the docs, http://www.szamlazz.hu/docs/xsds/agent/xmltaxpayer.xsd, returns 404) |

Extracted verbatim from inline docs code blocks (the download URLs linked from the docs,
http://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdel.xsd and
http://www.szamlazz.hu/docs/xsds/szamladbkdel/xmlszamladbkdelvalasz.xsd, return 404, and no
working variant was found under /szamla/docs/xsds/):

| File | Source URL |
|---|---|
| `agent/xsd/xmlszamladbkdel.xsd` | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xsd (inline code block) |
| `agent/xsd/xmlszamladbkdelvalasz.xsd` | https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response ("XML response scheme" inline code block) |

Each https://docs.szamlazz.hu/agent/&lt;operation&gt;/xsd page also shows the schema inline.
The acquisition originally described the downloads as canonical. That blanket
interpretation is withdrawn: inline and download schemas conflict; see the dated
observations below. Neither source is universally authoritative for server behavior.

**Deliberate deviation:** `agent/xsd/xmlszamla.xsd` was patched by hand (commit `3fc523a`) to add
the `csoportazonosito` (vevő) and `torloKod` (tétel) elements. The docs pages (prose, examples,
and the inline XSD at https://docs.szamlazz.hu/agent/generating_invoice/xsd) document both
fields, but the file served at the download URL above is a stale, older revision that omits them
(verified 2026-08-08: the served file is unchanged since 2026-07-04 and never contained them).
The inline XSD says "The sent XML file must comply with the following XSD schema";
the earlier record treated that as a universal source-precedence rule. It is
evidence for these fields, not a resolution of every schema conflict. This cached
file is a **project-modified schema**, not an unmodified vendor download. Do not
refresh it blindly or the two elements will be silently dropped. Similarly, the served `xmlnyugtaget.xsd`
lacks the documented `rendelesSzam` selector (`agent/xsd/xmlnyugtaget.xsd` was therefore
refreshed from the docs' inline XSD on 2026-08-11, see the table above), and the served
`xmlnyugtaarchiv.xsd` dropped `rendelesSzam` after 2026-07-04; the docs pages remain the
evidence for those order fields. Preserve these supported fields while examining
each disagreement on its own terms.

## 2026-09-10 — response examples and source disagreements (#197)

Unauthenticated GETs on **2026-09-10**, distinct from the July acquisition above.
The docs show site build `v202608271632`, not an acquisition date or a date for
every statement. These are published examples and source observations, not live
Számla Agent exchanges. SHA-256 identifies the acquired bytes, not their truth.

### Structured storno and credit-entry examples

The following files under `upstream/agent/2026-09-10/` are the HTML-decoded text
of the success/error `<pre>` blocks, with one final LF added and no other edits.
The response pages now contain structured examples; the historical "only response
example" descriptions above record the July acquisition and are not current claims.

| File | Source | SHA-256 (stored UTF-8 bytes) |
|---|---|---|
| `reversing_invoice_success.xml` | https://docs.szamlazz.hu/agent/reversing_invoice/response | `7d4efe3326ecb36a1371a506057da30fec0440c539c7ca53261085829fa84008` |
| `reversing_invoice_error.xml` | same page | `d70cb409dc85470d4bba94195f512688884c03b30c165250757913e790b67801` |
| `credit_entry_success.xml` | https://docs.szamlazz.hu/agent/credit_entry/response | `da33ff8914848e50146e7c510cef651a8f7023cc52646ad60e61224c2b20d1b5` |
| `credit_entry_error.xml` | same page | `d70cb409dc85470d4bba94195f512688884c03b30c165250757913e790b67801` |

Acquired HTML hashes: storno
`ae60e06a68de317d1cdcef951b6f97a7a9c7065c714be06c655f8ec05cbcb83c`,
credit entry `5d041bed6d9cc221b6a716c69d72409ca54ff5756205f50db0de94ee2487fb2d`.
Both success examples contain unescaped `&` in `vevoifiokurl`; the storno PDF
also contains `....`. The corpus preserves these defects. Tests separately
label any URL escaping or synthetic PDF substitution used to exercise the
remaining fields; neither transformation reconstructs a real vendor response.
The positive storno gross in this example does not prove reversal of any original.

### NAV link observation

https://docs.szamlazz.hu/agent/querying_taxpayer/response now links the schema
section to [NAV's v3.0 interface PDF](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf),
section 1.8.9 `/queryTaxpayer`. Acquired page SHA-256:
`a4a6e19bceea88b6d3761d69b41939c38cedac7350cb0e42a56db761a4595612`.
This records the link on the Agent page, not a fetch/hash of the linked PDF.
The examples still declare NAV 2.0 api/data namespaces and carry their own
2020-11-04 update date. A current link to v3.0 does not turn them into v3.0 examples.

### Receipt-create acquisition uncertainty

`agent/xsd/xmlnyugtacreate.xsd` contains `torloKod`; today's direct download at
https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd does not
(download SHA-256 `2c6fcda8bd9d48998df77c97413272f033ee381067fe7dd85694156a4b260a6f`).
Git history contains the cached element already in initial commit `a3342f0`,
and no later edit to that file. No original acquisition log or transformation
record was recovered. The mechanism is **unverified**: this does not establish
that the July fetch was wrong, nor that someone patched the element locally.
Keep the cached file and its historical description with this qualification.

### Conflicting invoice schemas

The following are independent acquisitions; hashes of inline blocks refer to
HTML-decoded UTF-8 code-block text, without reindentation or an added newline.
These observations do not replace the cached schemas or merge their contents.

| Source | SHA-256 | Observed `fejlecTipus` tail | Buyer group / item erasure |
|---|---|---|---|
| [Download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` | `szamlaSablon`, `elonezetpdf`, `simpleItems` | both absent |
| [EN inline](https://docs.szamlazz.hu/agent/generating_invoice/xml) | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` | `szamlaSablon`, `simpleItems`, `elonezetpdf` | both present |
| [HU inline](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` | `szamlaSablon`, `simpleItems`, `elonezetpdf` | both present |

EN/HU acquired HTML hashes respectively:
`039d56a5a7f73b4d2cdf9df11ead144a1fe3688a7ade6c44ccf18eac84cae2d0`,
`1863f909162b35b10c1edd7f642addba530a34efd49228d5e6a3c79cfecfd398`.
The table is a project-authored structural observation, not an original XSD.
The complete newly fetched schemas are not substituted into `agent/xsd/`.
Keep original source snapshots separate from the documented local transformation
of `agent/xsd/xmlszamla.xsd` and from `tests/golden` writer expectations.

`agent/simple-items-2026-09-10/` retains separately acquired **header-tail
excerpts**, not complete or merged XSDs, from the three URLs in the table above.
Each starts at `<element name="szamlaSablon"` and ends before the closing
`sequence`; boundary whitespace is stripped and a final newline added. Inline
excerpts are the HTML-decoded `pre` text (adjacent HTML spans concatenated).
Comments and element order are otherwise retained. `tests/upstream.rs` compares
their actual element sequences; `tests/simple_items.rs` independently asserts
the chosen writer output and retention of full monetary/group-id/erasure data.

[#199](https://github.com/sagikazarmark/szamlazz-rs/issues/199) owns the
`simpleItems` writer policy and its independent order/feature regression cases,
including separately retained source snapshots. Its chosen order follows the
download and first-party PHP 2.12.4; this is an implementation policy, not proof
of combined-preview server acceptance. A merged XSD would establish neither.
Keep group-id, erasure and order fields supported. The conflict must inform
#109's drift work and #111's broader validation work; neither is resolved here.

### What the checks establish, and who owns the remaining cases

- `tests/upstream.rs` checks dated examples against operations. Its request
  outline deliberately loses empty containers and surrounding text; matching
  outlines do not establish semantic equivalence or exact XML fidelity.
- `tests/receipt_wire.rs` independently checks actual default `SendReceipt`
  output for exactly one present empty `emailKuldes`, paired/self-closing
  presence, and omitted children versus `Some("")`. The vendor's
  [send XML docs](https://docs.szamlazz.hu/agent/sending_receipt/xml) distinguish
  an absent block (no send) from empty-present (resend); these offline tests
  establish serialization, not delivery or partial-override behavior.
- [#195](https://github.com/sagikazarmark/szamlazz-rs/issues/195) owns the
  independent source-derived error catalogue (`tests/error_classification.rs`).
- [#194](https://github.com/sagikazarmark/szamlazz-rs/issues/194) owns genuine
  NAV mixed-namespace/path cases (`tests/taxpayer_paths.rs`), date/monetary
  lexical assertions (`tests/response_headers.rs`, invoice/receipt unit tests),
  and decoded business text (`tests/business_text.rs`). These landed with
  their fixes; an outline or enum round-trip is not a substitute.
- #199 owns additional taxpayer-field/version fixtures and simplified-image
  order assertions with its capability implementation, rather than deferring
  them to #146/#126's broader test projects.
- `tests/live.rs` implements taxpayer lookup, HUF invoice create/storno,
  proforma create/delete and appearance cases. It does not exercise rejected
  kind combinations or empty/omitted semantics. The current
  [vendor limit](https://docs.szamlazz.hu/agent/basics/error-handling) is 500
  invoices per 10 minutes in the test environment; any slower local pacing is
  a local choice, not the old claimed vendor limit of 100/hour.

Domain guidance in #197 follows the vendor's [VAT rules](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates),
[buyer/waybill annotations](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml),
[queried bank-account annotation](https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak)
and [erasure guidance](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor).
The paid-proforma deletion and possible later queried-buyer changes are bounded
test-account observations in [the behavior notes](../docs/szamlazz-hu-behaviour.md),
not universal account/master-data rules. No live TAHK/OSS, buyer-ID collision,
portal access, erasure or carrier-rendering result is established by this sweep.

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
  pages; those pages contain only the XSDs. Incoming-invoice, bank-transaction and receipt
  pushed-document examples and their receiver-response examples are therefore not included.
