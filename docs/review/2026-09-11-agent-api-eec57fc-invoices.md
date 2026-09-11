# Számla Agent API conformance — invoice creation at eec57fc

**Review date:** 2026-09-11. **Reviewed commit:** `eec57fcf3036d93cd68c9cfc017338cd3020e7dd` (verified HEAD; clean working tree at the start). **Method:** fresh full-source review, not a diff.

Path shorthand throughout: `src/…` and `tests/…` are relative to `crates/szamlazz-agent/`; `invoice.rs` means `src/ops/invoice.rs`, and `behaviour.md` means `docs/szamlazz-hu-behaviour.md`. PHP source ID **P1** below denotes a source, not a finding severity.

## Executive conclusion

**No confirmed implementation defect found in the scoped invoice request surface.** Every element declared by the current EN/HU inline invoice XSD is represented, injected, or deliberately derived. The six `InvoiceKind` variants, ordinary element sequences, required containers, buyer/seller blocks, all four carrier sub-blocks, shared line items and supported value sets conform, subject to the explicit vendor-source conflicts below.

| Classification | Result |
|---|---|
| Confirmed P0/P1/P2/P3 implementation defects | **0** |
| P2-priority vendor clarification | Combined preview/simple-items order; two fields missing from the downloadable XSD |
| P3-priority vendor clarification | Template labels; PHP Trans-O-Flex `shippingID` versus XSD `shipmentID`; paid omission versus explicit false |
| Convenience limitation | No gross-first calculation helper; the documented XML is fully representable with `LineItem::new` |
| Verification in this review | Full source/doc inspection and executable, in-memory comparison of freshly fetched schemas and PHP source; **no Cargo tests run** |

These investigation priorities are not severities assigned to proven Rust bugs. A statement that every generated request validates against **the** official XSD would be incorrect: the vendor currently publishes mutually incompatible schemas. Current code follows identified first-party sources and discloses the principal order conflict.

No production or test file was changed. No vendor operation was invoked, account page opened, or secret accessed. Public documentation and downloads were fetched with GET only. The parent session owns the Cargo suite; this review neither started Cargo nor reused another review's scratch outputs. The only authored file is this report.

## Evidence and source inventory

The documentation entry point and all ten invoice settings/rules pages were read live. Request, response and XML pages, and all ten rules pages, were also read in Hungarian to check terminology and ambiguity. The site identified itself as **v202608271632**. Links below denote the sources actually inspected, not an assumption that repository fixtures are current.

### Primary operation sources

| ID | Source | Relevant requirement / quote |
|---|---|---|
| S1 | [Generating invoice](https://docs.szamlazz.hu/agent/category/generating-invoice), [settings/rules index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) | Entry point and complete ten-page rules inventory. |
| S2 | [Request EN](https://docs.szamlazz.hu/agent/generating_invoice/request), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/request) | POST to `https://www.szamlazz.hu/szamla/`, `multipart/form-data`; “`action-xmlagentxmlfile` (main file); optional attachments: `attachfile1` … `attachfile5`”. |
| S3 | [XML/XSD EN](https://docs.szamlazz.hu/agent/generating_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | Full examples, complex types, sequence, cardinality, language enumeration and annotations. “the order of the fields is fixed, **they cannot be interchanged**”; “Elements marked `minOccurs="0"` may be omitted.” |
| S4 | [Downloadable request XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Actual schema URL in the official request example's `xsi:schemaLocation`. |
| S5 | [Response EN](https://docs.szamlazz.hu/agent/generating_invoice/response), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/response), [downloadable response XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Version 2 = structured `xmlszamlavalasz`, optional base64 PDF; header table and response optionality. |
| S6 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | One document per XML; case-sensitive tags; “A request containing a mistyped tag may fail with error code `57` … or the document may be created without the setting … being applied.” |
| S7 | [Error handling](https://docs.szamlazz.hu/agent/basics/error-handling) | Prefix, arithmetic, erasure and simplified-image errors; maximum five attempts and no automatic retry-until-success loop. |

### All invoice rules

| ID | Rule, EN / HU | Checked content |
|---|---|---|
| R1 | [Document types](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types) | All six create kinds; separate storno; flags/references; e-invoice boolean; “one prepayment invoice can have exactly one final invoice”. |
| R2 | [Travel agency](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/travel-agency) | `simpleItems` exact spelling; per-document selection; default, seller prerequisites, item/rate limits, inheritance, template override, prohibited kinds. |
| R3 | [VAT rates](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates) | All percentage and special tokens; `eusAfa` signals no Hungarian VAT/NAV submission, not a replacement for each item's VAT code. |
| R4 | [Rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding) | Net-first and gross-first HUF calculations, fractional HUF correction table, HTTP totals. |
| R5 | [Currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/currencies) | Full currency list; `HUF`/`Ft`; foreign-currency bank/rate rule. |
| R6 | [Templates/languages](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template) | Six template tokens, fifteen languages, omitted-template default, language effects on email/portal. |
| R7 | [Order number](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/order-number) | Optional order; per-type account toggle, corrective/storno exemption, reuse after storno, two-day replay conditions. |
| R8 | [Discount](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/discount) | Negative unit price, positive quantity, same VAT rate, adjacent ordered item; no percentage/total discount field. |
| R9 | [Notification email](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/email-notification) | Email + omitted/true `sendEmail` sends; false suppresses; comma-separated recipients; BBCode/newlines; five attachments, 2 MB each; invalid-file behavior and test-account routing. |
| R10 | [Erasure count](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code) / [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | Optional `int >= 0`, maximum 400 codes per row, account enablement. |

### Followed first-party explanatory links and PHP

| ID | Source | Evidence used |
|---|---|---|
| K1 | [Erasure-code knowledge base](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor) | “a `<torloKod>` mezőben az igényelt kódok **darabszámát**” (number of requested codes), at the end of `tetel`; `SzlaMost`; stock allocation. |
| K2 | [VAT knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere), [detailed first-party table image](https://tudastar.szamlazz.hu/hs-fs/hubfs/GYIK/AFA-kulcsok_NOSZ-segedlet_2025-11-04.png?width=1340&name=AFA-kulcsok_NOSZ-segedlet_2025-11-04.png) | Visually inspected KBAUK/new means of transport, KBAET/supply of goods, and K.AFA subtype wording/default. The [linked PDF](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf) was downloaded in memory, but `pdftotext` was unavailable; the readable image is the evidence used. |
| K3 | [Travel-agent knowledge base](https://tudastar.szamlazz.hu/gyik/utazasszervezoi-szamla-kiallitasa), [K.AFA text](https://tudastar.szamlazz.hu/gyik/kulonbozeti-afas-szamlazas) | Feature prerequisites/inheritance and mandatory printed wording; internal documentation inconsistencies noted below. |
| K4 | [OSS configuration](https://tudastar.szamlazz.hu/gyik/szamlazas-oss-rendszer-ala-bejelentkezve), [OSS reporting](https://tudastar.szamlazz.hu/gyik/oss-rendszerbe-torteno-adatszolgaltatas) | Agent callers set `eusAfa` themselves; setting it is not automatic reporting to OSS. The [linked OSS background blog](https://www.szamlazz.hu/blog/2021/07/az-oss-es-ami-mogotte-van-erre-figyelj-ha-mas-eu-s-orszagba-is-ertekesitesz/) was fetched, but its long navigation-heavy extraction was truncated; no finding relies on the unread portion. |
| K5 | [Template examples/token list](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok), [foreign-language invoices](https://tudastar.szamlazz.hu/gyik/idegen-nyelvu-szamla) | Traditional/envelope-friendly label conflict, default `SzlaMost`, invoice-plus-waybill layout, 80 mm item-comment display cap, fifteen languages. No PDF rendering experiment was performed. |
| K6 | [Discount knowledge base](https://tudastar.szamlazz.hu/gyik/kedvezmeny-szamla-partner), [dynamic email fields](https://tudastar.szamlazz.hu/gyik/szamlaertesito-egyedi-mezok) | Negative-row representation; literal dynamic labels and BBCode can travel in existing text fields. |
| P1 | [PHP introduction](https://docs.szamlazz.hu/php/), [invoice generation](https://docs.szamlazz.hu/php/szamla-generalas), [official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip) | Public release 2.12.4 (2026-08-12). ZIP members inspected in memory: invoice/header kind classes, buyer/seller and ledgers, invoice item, generic and four carrier waybills. PHP paths below are relative to `PHPApiAgent-2.12.4/szamlaagent/src/szamlaagent/`. Its implementation is supporting evidence, not proof of vendor execution. |

Login/configuration destinations in the documentation were not followed; they are not public protocol specifications. Other operations linked as navigation (storno, PDF/XML query, receipt) are outside this request review, except the shared response/header boundary discussed here.

### Fresh machine-readable source comparison

An in-memory Python `HTMLParser` collected text inside each page's `pre` blocks, selected the `<schema>` block, and parsed it with `xml.etree.ElementTree`. The downloadable XML was parsed directly. Comparing every complex type's ordered element attributes produced:

```text
EN inline: 125 element declarations
HU inline: 125 element declarations
Download:  123 element declarations
EN == HU complex-type element declaration attributes: True
Differing complex types versus download: vevoTipus, tetelTipus, fejlecTipus
```

These counts include containers/root and are declaration counts, not a count of independently exercised combinations. Fresh byte fingerprints:

| Input | SHA-256 |
|---|---|
| EN schema text extracted from `pre`, UTF-8 encoded without reformatting | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` |
| HU schema text extracted the same way | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` |
| Downloaded request-XSD bytes (S4) | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` |

The download fingerprint matches `fixtures/upstream/agent/request-xsd-2026-09-11/sources.json:9`. Inline hashes depend on extraction/formatting; the repository stores formatted schema artifacts and their hashes should not be compared blindly to these extracted text hashes. This executable comparison establishes source disagreement, **not XSD validation of generated Rust requests**.

## Confirmed implementation findings

**None.** In particular, the prior nonpositive-date finding does not apply to HEAD. `crates/szamlazz-agent/src/xml.rs:19–35` refuses years `<= 0`; `src/ops/invoice.rs:683–703` visits all eight invoice date positions (header, buyer ledger and item ledger); `src/wire.rs:405–411` calls that validation before serialization, and `src/client.rs:374–382` uses `to_wire` before POST. `tests/request_dates.rs:42–132` covers negative, zero, normal and boundary positive years, leap day, and a later item. The unchecked `write_xml` method is explicitly documented as unchecked at `src/wire.rs:367–373`; its existence is not a remaining checked-boundary bug.

The earlier absence of a full-XSD test is also no longer an accurate finding: `tests/schema_requests.rs` and `scripts/check-agent-schemas.py` exist at HEAD, inspect separately labeled, unmodified schemas and expect only identified source conflicts. Their execution belongs to the parent suite, not this report.

The prior invoice report was consulted **after** independently reading current implementation and primary sources, to avoid re-reporting fixed findings. Its test results are not counted as results for this HEAD.

## Source conflicts and unresolved behavior

### C1 — P2 clarification: preview and simple-items have incompatible authoritative orders

**Rust:** `crates/szamlazz-agent/src/ops/invoice.rs:837–847` writes `szamlaSablon → elonezetpdf → simpleItems`; public caveat at `220–222`.

**Primary requirements:** S3 says the order “cannot be interchanged”, and both EN/HU inline `fejlecTipus` end with:

```xml
<element name="szamlaSablon" type="string" maxOccurs="1" minOccurs="0"></element>
<element name="simpleItems" type="boolean" maxOccurs="1" minOccurs="0"></element>
<element name="elonezetpdf" type="boolean" maxOccurs="1" minOccurs="0"></element>
```

S4 instead ends with `szamlaSablon → elonezetpdf → simpleItems`. P1 `Header/InvoiceHeader.php:398–404` agrees with the download: `$data['szamlaSablon']`, then `$data['elonezetpdf']`, then `$data['simpleItems']`.

**Reproducer by source inspection:** on an otherwise ordinary request, set `header.preview_pdf=Some(true)` and `header.simple_items=Some(true)`. Rust emits `<elonezetpdf>true</elonezetpdf><simpleItems>true</simpleItems>`. An XSD 1.0 validator using S3 must reject the latter element in that sequence; S4 permits that tail. Reversing the sequence exchanges which schema rejects it. Presence matters even for `Some(false)`; omitting either element removes this conflict. This is a deterministic schema conclusion, not a claim that a new validator or vendor call was run.

**Disposition:** source-backed Rust policy, not a confirmed runtime defect. `tests/simple_items.rs:20–91` checks all nine presence combinations for all six kinds; `tests/schema_requests.rs:38–43,305–312` and the schema script explicitly encode/report the inline conflict. Neither proves the server will honor preview when both fields are supplied. Obtain the effective schema and combined-preview behavior from the vendor; do not silently choose one source as universally authoritative.

### C2 — P2 clarification: download omits buyer group id and erasure count

**Rust:** `src/ops/invoice.rs:873–875` emits `adoszam → csoportazonosito → adoszamEU`; `935–937` emits `torloKod` last in the item. Models: `invoice.rs:321–322`, `src/item.rs:115–131`.

**Primary requirements:** both S3 inline schemas declare optional string `csoportazonosito` between the two tax-number fields; they declare `torloKod` after `tetelFokonyv`, restricted from `int` with `<minInclusive value="0"/>`. S4 has neither declaration. R10 says “At most **400** data erasure codes can be assigned per item.” K1 explicitly calls this field the requested **count** and places it at the end of the item. P1 `Buyer.php:285–287` and `Item/InvoiceItem.php:67–72` support Rust's field names and placement.

**Reproducer by source inspection:** `buyer.group_id=Some("12345678".into())` or `items[0].erasure_code_count=Some(1)` on a minimal valid invoice adds an element not allowed by S4. Both are accepted structurally by S3. The vendor's own current example includes `torloKod=123`, so it too does not conform to the linked download.

**Disposition:** documentation/download conflict, not evidence to delete a supported feature. Tests explicitly expect download invalidity at `tests/schema_requests.rs:44–52,381–388`. Group-id processing and actual erasure allocation are not established by the local observations. In particular, the vendor says erasure codes cannot be used on demo/test accounts (S7, code 538).

### C3 — P3 clarification: official template labels are reversed

**Rust:** `src/types.rs:991–995,1011–1014` names `Default` as traditional `SzlaAlap` and maps `NoEnvelope` to `SzlaNoEnv`. This agrees with R6 EN/HU: `SzlaAlap` = “Tradicionális számlakép”, `SzlaNoEnv` = “Borítékbarát számlakép”.

**Conflicting primary evidence:** K5 instead gives “Tradicionális: … `SzlaNoEnv`” and “Borítékbarát: … `SzlaAlap`”. P1 `Document/Invoice/Invoice.php:39–46` defines `INVOICE_TEMPLATE_DEFAULT='SzlaMost'`, `INVOICE_TEMPLATE_TRADITIONAL='SzlaNoEnv'`, `INVOICE_TEMPLATE_ENV_FRIENDLY='SzlaAlap'`.

**Impact:** users selecting by semantic name may receive the other layout if the knowledge-base/PHP mapping is the operational truth. All six literal tokens are nevertheless supported, and Rust explicitly distinguishes its `Default` variant from omitting the template. No token-labelled rendered output in the inspected live evidence resolves the disagreement. Do not relabel this as a confirmed Rust mapping bug or swap its tokens without confirmation. Also avoid copying K5's lowercase `<szamlasablon>` or `szlafuvarlevelesalap`: S3/P1 use the camel-cased spellings already sent by Rust.

### C4 — P3 clarification: PHP's Trans-O-Flex shipment element disagrees with every XSD

**Rust:** `src/ops/waybill.rs:14–15,139–140` sends `shipmentID`.

**Schema quote:** S3 EN, S3 HU and S4 all declare `<element name="shipmentID" type="string" maxOccurs="1" minOccurs="0">`.

**PHP quote:** P1 `Waybill/TransoflexWaybill.php:109` instead writes `$data['tof']['shippingID'] = $this->getShippingId();`.

This is a concrete first-party serializer/schema conflict. Rust matches all three schemas; changing it to mirror PHP would make the request schema-invalid. No TOF execution observation settles which spellings the deployed processor understands. General/carrier data and all other inspected PHP carrier writer names/order agree with Rust. This finding is about source inconsistency, not a missing Rust carrier field.

### C5 — P3 clarification: `fizetve=false` cannot be explicitly emitted

**Rust:** `src/ops/invoice.rs:178–180,823–825` uses a bool and omits the element when false. S3/S4 declare optional boolean `fizetve` **without an XSD default**. P1 `Header/InvoiceHeader.php:393` also emits it only when true. `src/ops.rs:18–22` accurately says universal equivalence of omission and explicit false is unverified.

Literal false is a schema-valid representation the high-level model cannot select. That is a representational limitation, but no fetched rule or recorded execution demonstrates distinct behavior for omission versus false on cash, another payment method or a particular kind. Therefore it is **not** a confirmed lost business capability. The existing [unsent question backlog](../research/2026-09-10-agent-vendor-questions.md#7-paid-state-omission-versus-explicit-false) is the appropriate follow-up. No paid-state experiment is claimed.

### Other contradictions requiring careful interpretation

- **“Every field is mandatory” versus the schema:** S3 EN/HU says every field in the example must be present, immediately alongside optional-field explanations and `minOccurs="0"` declarations. Rust follows the explicit schema cardinalities; omitted placeholder strings and unused kind flags are not defects. `elado` itself is required and is emitted even when empty.
- **Delivery note versus invoice-plus-waybill:** R1 explains delivery notes using the `SzlaFuvarlevelesAlap` template; S3/S4 separately declare `szallitolevel`; P1 `Header/DeliveryNoteHeader.php:17` selects the flag. K5 calls the layout “Fuvarlevél + számla”. Rust distinguishes the kind from the layout: a delivery-note kind sends the flag and forces the documented template (`invoice.rs:816,832–839`); an ordinary invoice can select that template and supply a waybill without changing its kind. No evidence justifies treating the template alone as the document discriminator.
- **Automatic MNB exception:** R5 requires bank and rate for foreign-currency documents; S3/S4 expressly say bank `MNB` with omitted `arfolyam` uses the current rate. `ExchangeRate::automatic_mnb()` and `invoice.rs:740–751,786–791` implement this explicit exception. No observation proves additional exemptions for AAM, proforma or delivery note; see `docs/szamlazz-hu-behaviour.md:227–234`.
- **PHP required fields/defaults are not XSD cardinality:** its documentation lists issue date and buyer-ledger booking/continuous fulfillment as mandatory, while S3/S4 make them optional. PHP's own buyer ledger conditionally emits the fields (`BuyerLedger.php:118–123`). Rust correctly permits omission. PHP default currency/date/payment-due values are convenience defaults, not mandatory defaults for this Rust constructor.
- **PHP simplified-image gate:** PHP throws before sending a simplified delivery note/corrective (`Header/InvoiceHeader.php:333–336,402–404`); Rust delegates content rules to the vendor and documents that at `invoice.rs:199–222`. This is a different validation boundary, not an incorrect allowed-document claim.
- **Travel-agent knowledge-base self-contradiction:** K3 has a dedicated section explaining how Agent `simpleItems` works, but later says the feature is currently only available for manually issued UI invoices (“csak … manuálisan kiállított számlák”). R2, S3/S4 and PHP all explicitly support Agent use. Do not remove `simple_items` based on that stale sentence. The knowledge base describes both account-level enablement and a per-document UI checkbox; the Agent request is independently per-document.
- **Rounding wording:** R4 EN says B2B/B2C “have to” use the corresponding method; HU says “valószínűleg” (probably). The worked gross-first example labels `(1500−319)/3=393.66` as net value in prose, but its XML correctly sends **unit price 393.66, net total 1181**. Use the actual arithmetic/XML, not the mistranslated label.
- **VAT names/wording:** R3's abbreviated KBAUK/ET English descriptions are misleadingly geographic. K2's detailed table confirms the Rust new-means-of-transport interpretation. Its K.AFA table uses `gyűjtemény darabok és régiségek`, while K3's legal wording uses `gyűjteménydarabok és régiségek`; the table describes exact matching but does not explain normalization or multiple matches. `types.rs:203–210` correctly attributes its wording to that table. No local normalization of these business strings is justified.
- **Email recipient separators:** R9 explicitly documents commas; PHP's invoice page additionally permits semicolons and spaces. `Buyer.email` is an unmodified string, so both described forms are representable. Its comma advice is not a parser restriction. Test-account mail routing to the configured account address (R9) limits what a bad buyer-email probe can establish.

### Remaining runtime questions

1. Which complete schema and tail order does the deployed processor use, including explicit false? Does combined `simpleItems=true`/`elonezetpdf=true` always produce only preview? [The clarification draft](../research/2026-09-11-agent-vendor-clarification.md) (lines 52–75) is explicitly unsent, with no vendor answer.
2. Which template token actually renders which layout? Does an account-specific setting affect omission? No current rendering evidence was found.
3. Are explicit proforma references accepted on prepayment/final invoices, beyond the observed implicit order-number linking? `invoice.rs:47–54,69–70` states the evidence limit; `behaviour.md:245–251` leaves these executions unverified.
4. Are there meaningful mixed flags/reference combinations beyond the six modeled kinds? XSD independence alone does not establish, for example, a supported corrective-plus-prepayment workflow. No such documented workflow was found. Corrective/proforma/delivery-note variants do not expose the proforma reference; this restriction is disclosed at `invoice.rs:23–30,104–117`.
5. Lowercase `huf`/`ft`, padded or unusual bank tokens, carrier-specific formatting and arbitrary open currency/VAT tokens: local representation/classification is not vendor acceptance. The documented spellings all work through the model; content remains server-validated.
6. Margin-scheme arithmetic: `arresAfa`, `arresAfaAlap` and comments are represented, but no complete margin-tax formula is specified by the invoice XSD. The derived helper explicitly treats nonpercentage codes as zero VAT; use explicit amounts when the applicable calculation differs. No live margin-scheme calculation was observed.
7. HUF fractional correction, zero-/three-decimal foreign-currency storage, combined feature interactions, actual carrier barcode rendering and attachment delivery remain outside the available execution evidence.

## Field coverage matrix

All source references below are under `crates/szamlazz-agent/` unless prefixed otherwise. **R** = element required by S3/S4, **O** = optional; an optional Rust member starts absent unless noted. Required **element presence** is not a nonempty-string or account-validity guarantee. Within each block rows follow the writer's sequence. `String` preserves caller text with XML escaping; `Decimal` supplies finite plain decimal notation valid for the request's `double`; `Date` is checked to the supported positive-year civil-date domain.

### Envelope and settings — S2–S6, R9

| Wire field / feature | Cardinality, representation, behavior | Source |
|---|---|---|
| HTTP operation | POST, main file `action-xmlagentxmlfile`, multipart | `src/ops/invoice.rs:678–680`; `src/wire.rs:66–100`; `src/client.rs:374–383` |
| `xmlszamla` | R fixed root, namespace `http://www.szamlazz.hu/xmlszamla`, UTF-8 XML 1.0 | `src/ops/invoice.rs:759`; `src/xml.rs:157–179` |
| Root children | `beallitasok` R → `fejlec` R → `elado` R → `vevo` R → `fuvarlevel` O → `tetelek` R | `src/ops/invoice.rs:760–941` |
| `felhasznalo`, `jelszo`, `szamlaagentkulcs` | O schema strings; inject username then password, or agent key | `src/xml.rs:628–638` |
| `eszamla` | R bool; `e_invoice`, explicit false by default | `src/ops/invoice.rs:532–539,762` |
| `szamlaLetoltes` | R bool; `download_pdf`, explicit false by default | `src/ops/invoice.rs:540–542,763` |
| `szamlaLetoltesPld` | O int; `download_copies: Option<u8>`, deprecated/ignored per inline annotation | `src/ops/invoice.rs:543–547,764–766` |
| `valaszVerzio` | O int, deliberately fixed to 2 | `src/ops/invoice.rs:767`; `src/ops.rs:28–32` |
| `aggregator` | O string; contracted integration input | `src/ops/invoice.rs:548–549,768` |
| `guardian` | O bool; absent/false/true preserved | `src/ops/invoice.rs:550–551,769–771` |
| `cikkazoninvoice` | O bool; `item_identifiers_on_invoice` | `src/ops/invoice.rs:552–553,772–774` |
| `szamlaKulsoAzon` | O string; `external_id`, settings location, no invented uniqueness | `src/ops/invoice.rs:554–556,775` |
| `attachfile1` … `attachfile5` | O multipart files; filename/content/MIME, private bounded collection; <=5, <=2,000,000 bytes each | `src/ops/invoice.rs:379–516,959–969` |
| XML characters / multipart delimiters | `to_wire` rejects XML 1.0 forbidden characters; delimiter chosen outside XML and file content; escaped disposition values | `src/wire.rs:102–125,405–441` |

The sample's `xsi:schemaLocation` is not required instance data. Omitting it does not alter the qualified element names. Attachment size uses the explicitly documented conservative decimal reading of “2 MB”; larger files are refused locally even though R9 says the vendor may still issue/send with other valid attachments. This intentional early bound does not remove a documented **valid** attachment. Sending no email leaves attachments unprocessed server-side; the Rust writer need not infer their processing from the presence of files.

### Header — S3/S4, R1–R7

| Wire element | Representation / default / audit | Source in `src/ops/invoice.rs` |
|---|---|---|
| `keltDatum` | O Date, `issue_date`; omitted normally; observed replacement caveat | `141–150,779` |
| `teljesitesDatum` | R Date, `fulfillment_date`, not payment date | `151–153,780` |
| `fizetesiHataridoDatum` | R Date, `due_date` | `154–156,781` |
| `fizmod` | R `PaymentMethod`; free text through `Other` | `157–158,782` |
| `penznem` | R `Currency`; full vendor list through open string | `159–160,783` |
| `szamlaNyelve` | R `Language`; all fifteen schema tokens | `161–162,784` |
| `megjegyzes` | O string, `comment`; empty and absent distinct | `163–164,785` |
| `arfolyamBank`, `arfolyam` | O string / O double, grouped in `ExchangeRate`; bank before rate; explicit rate or automatic MNB | `165–166,740–751,786–791` |
| `rendelesSzam` | O string, `order_number`; no local account-toggle or worker-key policy | `167–169,792` |
| `dijbekeroSzamlaszam` | O reference from invoice/prepayment/final kind; before flags | `795–798` |
| `elolegszamla` | O bool derived true for prepayment only | `801` |
| `vegszamla`, `elolegSzamlaszam` | O bool/reference, final flag then optional explicit prepayment number | `802–810` |
| `helyesbitoszamla`, `helyesbitettSzamlaszam` | O schema fields, required reference member on corrective variant | `811–814` |
| `dijbekero` | O derived true for proforma | `815` |
| `szallitolevel` | O derived true for delivery note | `816` |
| `logoExtra` | O string, `extra_logo` | `170–171,818` |
| `szamlaszamElotag` | O string, `number_prefix`; pre-registration restriction documented | `172–175,819` |
| `fizetendoKorrekcio` | O Decimal, `payable_adjustment`; not a discount substitute | `176–177,820–822` |
| `fizetve` | O wire bool, `paid=false` omits; C5 | `178–180,823–825` |
| `arresAfa` | O bool, `margin_vat`; explicit false retained | `181–182,826–828` |
| `eusAfa` | O bool, `eu_vat`; explicit false retained; OSS/non-Hungarian seller caveat | `183–192,829–831` |
| `szamlaSablon` | O template token; delivery note forces `SzlaFuvarlevelesAlap`; C3 | `193–196,832–839` |
| `elonezetpdf` | O bool, `preview_pdf`; C1 | `197–198,840–842` |
| `simpleItems` | O bool, `simple_items`; exact case, complete monetary rows still emitted; C1 | `199–225,843–847` |

### All InvoiceKind variants — R1/R2, S3/S4

| Kind | Mapping and supported references | Validation / assessment |
|---|---|---|
| `Invoice` | No kind flag; optional proforma number | `invoice.rs:35–40,800`; regular invoice and explicit proforma conversion supported. |
| `Proforma` | `dijbekero=true` | `41–43,815`; uses same full invoice request blocks. |
| `DeliveryNote` | `szallitolevel=true`, forced waybill template | `44–46,816,832–839`; optional waybill; C3/source ambiguity does not erase the explicit discriminator. |
| `Prepayment` | `elolegszamla=true`, optional proforma reference before it | `47–59,795–801`; explicit reference representable, runtime evidence limited. |
| `Final` | `vegszamla=true`, optional prepayment and proforma references | `60–78,802–810`; `707–723` requires a nonblank prepayment number **or** order number. Does not falsely require both. One final/one prepayment is server state; negative deduction is caller-supplied and documented. |
| `Corrective` | `helyesbitoszamla=true` plus corrected number | `79–84,811–814`; reference is a mandatory member; blank contents remain server validation. Negative monetary rows supported. |

Storno is properly a separate operation. Independent XSD flag declarations do not by themselves require a client to expose contradictory/mixed document kinds. The enum supports every meaningfully described R1 kind. R2's final/storno inheritance and corrective prohibition depend on server/original state; Rust does not claim request-local knowledge of them.

### Seller, buyer and buyer ledger — S3/S4, R9

| Wire element(s), in order | Representation / cardinality | Source in `src/ops/invoice.rs` |
|---|---|---|
| `elado/bank`, `bankszamlaszam` | O strings, `Seller.bank/bank_account` | `262–275,849–851` |
| `emailReplyto`, `emailTargy`, `emailSzoveg` | Independent O strings inside O `SellerEmail`; no extra XML wrapper | `852–856`; `src/types.rs:1022–1032` |
| `elado/alairoNeve` | O string, seller signer | `857` |
| `vevo/nev` | R string, `Buyer.name` | `296–298,861` |
| `orszag` | O string, country | `299–300,862` |
| `irsz`, `telepules`, `cim` | R strings, ZIP/city/address; ZIP not numeric | `301–306,863–865` |
| `email` | O string; multiple recipients representable | `307–310,866` |
| `sendEmail` | O bool; default absent, explicit false preserved | `311–315,867–869` |
| `adoalany` | O integer token from `TaxpayerStatus` | `316–317,870–872` |
| `adoszam` | O string, Hungarian tax number | `318–320,873` |
| `csoportazonosito` | O string, VAT-group id, C2 | `321–322,874` |
| `adoszamEU` | O string, EU tax number | `323–324,875` |
| `postazasiNev`, `postazasiOrszag`, `postazasiIrsz`, `postazasiTelepules`, `postazasiCim` | Five O strings, flattened from O `PostalAddress` | `277–291,876–882` |
| `vevoFokonyv` | O ledger container, all children optional | `327–328,883–894` |
| `konyvelesDatum` | O positive-year Date, accounting date | `122–124,885` |
| `vevoAzonosito`, `vevoFokonyviSzam` | O strings, buyer identifier and account | `125–128,886–887` |
| `folyamatosTelj` | O bool, explicit false retained | `129–130,888–890` |
| `elszDatumTol`, `elszDatumIg` | O positive-year Dates, settlement period | `131–134,891–892` |
| `vevo/azonosito` | O string, partner association identifier, not queried numeric id | `329–340,895` |
| `vevo/alairoNeve`, `telefonszam`, `megjegyzes` | O strings, buyer signer/phone/comment | `341–346,896–898` |

The invoice-creation seller block has no seller name/address/tax-number elements to map: those are account-owned. Their absence from `Seller` is not a feature omission. Buyer `azonosito`'s partner-update/document-access consequences are accurately documented at `329–340`. No fabricated local uniqueness check can establish whether an id belongs to another partner. Empty optional ledger blocks are permitted by the schema, as are empty strings; business-required content is a separate vendor validation layer.

### Shared line item and ledger — S3/S4, R3/R4/R8/R10

| Wire element(s), in order | Representation / cardinality | Source |
|---|---|---|
| `tetelek/tetel` | R one-or-more, ordered `Vec<LineItem>` | `src/ops/invoice.rs:704–706,903–940` |
| `megnevezes` | R string, `name` | `src/item.rs:91–92`; `invoice.rs:906` |
| `azonosito` | O string, `id` | `item.rs:93–94`; `invoice.rs:907` |
| `mennyiseg` | R Decimal, `quantity`; fractional/negative allowed | `item.rs:95–96`; `invoice.rs:908` |
| `mennyisegiEgyseg` | R string, `unit` | `item.rs:97–98`; `invoice.rs:909` |
| `nettoEgysegar` | R Decimal, `unit_price`; negative discounts possible | `item.rs:99–100`; `invoice.rs:910` |
| `afakulcs` | R string token from `VatRate` | `item.rs:101–102`; `invoice.rs:911` |
| `arresAfaAlap` | O Decimal, `margin_vat_base` | `item.rs:103–104`; `invoice.rs:912–914` |
| `nettoErtek`, `afaErtek`, `bruttoErtek` | R Decimals, all explicitly supplied | `item.rs:105–110`; `invoice.rs:915–917` |
| `megjegyzes` | O string, comment | `item.rs:111–112`; `invoice.rs:918` |
| `tetelFokonyv` | O `LineItemLedger`, no required children | `item.rs:52–72,113–114`; `invoice.rs:919–934` |
| `gazdasagiEsem`, `gazdasagiEsemAfa` | O strings, economic event / VAT economic event | `invoice.rs:921–925` |
| `arbevetelFokonyviSzam`, `afaFokonyviSzam` | O strings, revenue / VAT ledger account | `invoice.rs:926–930` |
| `elszDatumTol`, `elszDatumIg` | O positive-year Dates | `invoice.rs:698–703,931–932` |
| `torloKod` | O `u32` count, locally <=400, final child; C2 | `item.rs:115–131`; `invoice.rs:724–731,935–937` |

All invoice-only shared-item fields reach the invoice wire. No receipt-specific restriction is accidentally applied here. Discount lines remain in caller order; no grouping/sorting erases their intended adjacency. There is no documented item discount percentage field to add.

### Waybill — full S3/S4 type comparison and P1 cross-check

| Wire element(s), in order | Representation / cardinality / value notes | Source in `src/ops/waybill.rs` |
|---|---|---|
| `uticel` | O string, legacy destination, correctly documented unused | `95–97,133` |
| `futarSzolgalat` | O string, carrier; all `TOF`, `PPP`, `SPRINTER`, `FOXPOST`, `MPL`, `GLS`, `EMPTY` representable | `98–99,134` |
| `vonalkod`, `megjegyzes` | O strings, general fallback barcode and comment | `100–104,135–136` |
| `tof` | O Trans-O-Flex container | `105–106,137–148` |
| `tof/azonosito` | O string; annotated five-digit carrier id, leading zeros retained | `12–13,139` |
| `tof/shipmentID` | O string, shipment id; C4 | `14–15,140` |
| `tof/csomagszam` | O `u32` count checked <=`i32::MAX` | `16–17,141–143`; `invoice.rs:732–739` |
| `tof/countryCode`, `zip`, `service` | O strings, exact case and order | `18–23,144–146` |
| `ppp` | O Pick Pack Pont container | `107–108,149–154` |
| `ppp/vonalkodPrefix`, `vonalkodPostfix` | O strings, annotated agreed three-character prefix / <=7-character suffix | `29–32,151–152` |
| `sprinter` | O Sprinter container | `109–110,155–166` |
| `sprinter/azonosito`, `feladokod`, `iranykod` | O strings, annotated three-character id / ten-digit sender code / routing token | `38–43,157–159` |
| `sprinter/csomagszam` | O `u32` count, same signed-XSD upper bound | `44–45,160–162`; `invoice.rs:732–739` |
| `sprinter/vonalkodPostfix`, `szallitasiIdo` | O strings, annotated 7–13-character id / delivery text | `46–49,163–164` |
| `mpl` | O MPL container; no `Default` because three children required | `52–85,111–112,167–177` |
| `mpl/vevokod`, `vonalkod`, `tomeg` | R strings, customer code/barcode/weight; `tomeg` is truly XSD **string**, including decimal weight text | `58–63,169–171` |
| `mpl/kulonszolgaltatasok` | O string, service-icon configuration | `64–65,172` |
| `mpl/erteknyilvanitas` | O Decimal, declared value | `66–67,173–175` |

The four carrier containers are independent optional sequence members, not an XSD choice. Allowing multiple populated sub-blocks is structurally correct. Carrier identifier lengths appear as annotations, not schema patterns; their valid values are representable, but are not locally verified. Negative parcel counts are in the bare `xs:int` domain but no documented meaningful negative-count operation is lost by `u32`. FOXPOST/GLS/EMPTY need no missing per-carrier class: their documented representation is the generic fields.

### Shared types, calculations and request validation

| Surface | Check / result | Source |
|---|---|---|
| Invoice numbers | Open string, no invented length/alphabet or account-existence check | `src/types.rs:22–57` |
| Currency values | All R5 tokens supported, including `Ft`, vendor `KSH`, legacy currencies; unknown strings can be sent | `src/types.rs:369–410` |
| Languages | Exact `hu,en,de,it,ro,sk,hr,fr,es,cz,pl,bg,nl,ru,si`; do not substitute ISO `cs`/`sl` | `src/types.rs:480–585` |
| Payment methods | Hungarian standard tokens plus PayPal/SZÉP and arbitrary text; no restrictive parser | `src/types.rs:588–688` |
| Taxpayer status | `7,6,1,0,-1`, all annotated values | `src/types.rs:690–755` |
| VAT percentages | Every listed percentage, including `2.1,4.8,5.5,7.7,8.1,9.5,13.5,25.5`, representable as Decimal | `src/types.rs:187–189,251–290` |
| VAT special codes | All sixteen invoice tokens: `TAHK,TAM,AAM,EUT,EUKT,F.AFA,K.AFA,HO,EUE,EUFADE,EUFAD37,ATK,NAM,EAM,KBAUK,KBAET`; extra receipt codes do not restrict invoice use | `src/types.rs:190–248,269–289` |
| VAT openness / arithmetic | Unknown text preserved; numeric `Other` interpreted as percentage; numeric values outside Decimal domain fail calculation rather than becoming zero | `src/types.rs:294–366`; `src/item.rs:194–208` |
| Templates | All six named literal tokens plus `Other`, no missing future/custom token path | `src/types.rs:983–1020` |
| Explicit amounts | `LineItem::new` sends asserted values, including gross-first calculations and negative deductions | `src/item.rs:133–161` |
| Derived amounts | Exact net multiply → selected rounding → exact percentage calculation → rounding → exact sum | `src/item.rs:183–223`; `src/number.rs:6–59` |
| Rounding | Half away from zero; explicit `Scale`/`Exact`; minor-unit is local policy, not universal vendor precision | `src/item.rs:9–49`; `src/types.rs:412–435` |
| Structural validation | Dates, nonempty items, final reference-or-order, erasure count, parcel bound, foreign-currency bank/rate | `src/ops/invoice.rs:682–754` |
| Content checks delegated | Arithmetic consistency on explicit rows, seller/account capabilities, buyer tax validity, prefix existence, carrier formats, simplified-image server rules | `src/item.rs:74–83`; `src/ops/invoice.rs:199–222,682–754` |

The request validator is not an implementation of every business rule. This is not hidden: the shared item explicitly says the server is the authority for arithmetic and simplified-image rustdoc explicitly delegates content validation. Accepting a schema-valid request that the server may reject is not itself an API mapping defect. Conversely, `to_wire` is not a general runtime XSD validator; its operation checks plus typed writer establish a bounded subset, with C1/C2 remaining source-dependent.

**Gross-first limitation:** R4's quantity `3`, unit net `393.66`, net `1181`, VAT `319`, gross `1500` example is directly expressible as `LineItem::new("Könyv", dec!(3), "db", dec!(393.66), VatRate::percent(27), dec!(1181), dec!(319), dec!(1500))`. The only derived helper is net-first; its declared input is net unit price. A gross-first convenience method could be useful but is not a missing XML capability or a defect in the existing helper. No executable Rust reproduction was needed to establish this data representation.

## Creation response and HTTP-header cross-check

This is a focused check of the response selected by invoice requests, not a full review of query or mutation operations.

| Documented response field | Current handling | Source |
|---|---|---|
| `sikeres`, `hibakod`, `hibauzenet` | Namespace-qualified envelope and verdict; preserves open codes, absent diagnostics | `src/ops/envelope.rs:179–216,285–315`; `src/xml.rs:461–523` |
| `szamlaszam` / `szlahu_szamlaszam` | Nonblank body number then decoded header; never substitutes a requested number | `src/ops/envelope.rs:120–133,211–225,326–329` |
| `szamlanetto` / `szlahu_nettovegosszeg` | Optional Decimal, body before header | `src/ops/envelope.rs:158–168,226–231` |
| `szamlabrutto` / `szlahu_bruttovegosszeg` | Optional Decimal, body before header | `src/ops/envelope.rs:232–237` |
| `kintlevoseg` / `szlahu_kintlevoseg` | Optional outstanding amount | `src/ops/envelope.rs:238–243` |
| `vevoifiokurl` / `szlahu_vevoifiokurl` | Optional URL, XML entity decoding versus one textual-header decoding | `src/ops/envelope.rs:135–143,244`; `src/wire.rs:239–249` |
| `szlahu_fizetesmod` | Optional decoded `PaymentMethod`, unknown text preserved; no invented XML element | `src/ops/envelope.rs:49–53,245,318–324` |
| `szlahu_id` | Optional nonnegative i64, auxiliary field tolerates malformed header | `src/ops/envelope.rs:30–38,225,331–342` |
| `pdf` | Optional base64 PDF for issued document; required for an unnumbered requested preview | `src/ops/envelope.rs:145–148,246`; `src/ops/invoice.rs:944–956` |
| Header error / maintenance / status | Nonblank down → error header → known non-2xx → body; operation judges numbered 56 | `src/wire.rs:251–311`; `src/ops/envelope.rs:179–216` |

All fields in S5's header table are now exposed or classified; payment-method header is not missing. Version 1 text/raw-PDF parsing is deliberately not offered by this request: it always asks for version 2 (`src/ops.rs:28–32`). A numberless ordinary success is a parse error; a numberless preview requires the request flag and PDF. A numbered response to a preview request remains an issued document, rather than being concealed as a harmless preview.

The S5 success example contains an unescaped `&` in its URL and an abbreviated base64 payload with `....`; it is illustrative, not a valid complete XML/PDF fixture to demand that the parser accept. S5's blanket sentence that error headers omit number/totals also does not describe the crate's conditional numbered-56 recovery policy. No numbered-56 vendor example was observed in the repository evidence (`behaviour.md:163,190–191`); synthetic tests prove how the parser behaves if such evidence arrives.

## Observed deviations and evidence limits

The following are actual **recorded observations**, not deductions from runnable probe code. Historical raw logs are not in this repository; this review relies on the dated observation record and does not claim to have independently replayed those operations. These facts apply to the recorded test account/dates, not universally to all accounts.

| Observation | Evidence location | Consequence for conformance assessment |
|---|---|---|
| Yesterday's requested create issue date became today | `docs/szamlazz-hu-behaviour.md:99–100` (P48-P5) | Correctly disclosed at `invoice.rs:141–148`; do not invent a local “must equal today” refusal. |
| `e_invoice=true` queried as appearance 3; false as 1 | `behaviour.md:106–107` (P73) | Request bool is correct; no confusion with queried integer codes. |
| Ordinary invoice explicitly converts a proforma; prepayment auto-links by order without explicit proforma reference | `behaviour.md:113–115` (C2-3, C1-3) | Supports current regular conversion and implicit link documentation, not proof of explicit prepayment/final links. |
| Deleted/consumed proforma reference can be silently ignored | `behaviour.md:116–117` (C2-6, D4, D5) | A successful creation does not prove every requested relationship was applied; the request writer cannot perform an existence check from local data. |
| Final auto-links by order but does not subtract prepayment from its totals | `behaviour.md:126–128` (C6) | Current `Final` documentation correctly requires caller deduction lines; 1:1 settlement is vendor state. |
| Corrective has negative totals and references its base | `behaviour.md:135–136` | Negative amounts must remain representable; no blanket positivity check should be added. |
| Duplicate-order check is per kind; edge spaces trimmed on create, case preserved; repeat identity finite/state-dependent | `behaviour.md:46–65` | Client should not promise indefinite idempotency or enforce worker-specific key rules. |
| External ids not unique; newest holder queried; external id attaches only on creating exchange | `behaviour.md:72–80` | `external_id` is accurately an optional query handle, not an idempotency key. |
| HUF net-value discrepancy tolerated at tested small magnitudes | `behaviour.md:169` (P60-H1…H5) | Do not replace server arithmetic policy with exact-equality local validation. Threshold beyond those samples remains unknown. |
| EUR values independently rounded to two decimals, sometimes yielding stored gross unequal to stored net+VAT; unit price retained | `behaviour.md:170–171` (P60-E1…E3) | Explicit net-first rounding addresses the tested EUR problem. `Rounding::Exact` caveat is accurate; it is not evidence of KWD/JPY/HUF behavior. |
| Percentage tokens `27.00` and `27.0` accepted; queries report `27.0` | `behaviour.md:172` | Normalization in `VatRate::as_wire` is hygiene, not a server lexical necessity. |
| Two later clearing probes first created/query-verified ordinary invoices | `docs/research/2026-09-11-credit-clearing-live.md:3–32,36–50` | Concrete later creation evidence exists, but those probes do not test schema-tail ordering, carrier output, templates or mail. Raw HTTP channels were not archived (`64–75`). |

`docs/research/2026-09-11-agent-vendor-clarification.md:3` says **“not sent; no vendor answer received”**. `docs/research/2026-09-10-agent-vendor-questions.md:7–10,108–119` similarly distinguishes opt-in probes from execution evidence. No statement in this report treats either a question draft, a passing mock, or an ignored executable test as a vendor observation.

## Tests inspected and verification boundaries

**No test suite was executed in this review**, as requested by the parent coordination. No previous test pass count is attributed to eec57fc. The in-memory schema/PHP comparisons above did execute. Local `xmllint`, Python `lxml` and `pdftotext` were unavailable; no tools were installed and no shared build was started.

| Tests / machinery inspected | What they check | Important boundary |
|---|---|---|
| `src/ops/invoice.rs:1021–1267` | Canonical XML, corrective flags, regular/prepayment/final references and ordering, kind serde, populated optional blocks/all carriers, delivery-note flag/template | Golden/substrings establish emission, not server processing. |
| `src/ops/invoice.rs:1269–1349` | Exact attachment multipart layout, collection idioms, sixth/oversized attachment | No mail delivery assertion. |
| `src/ops/invoice.rs:1351–1477,1578–1592,1651–1676` | Success/error envelope, document id, numbered 56, preview, JSON and header precedence | Synthetic responses. |
| `src/ops/invoice.rs:1479–1576,1594–1649` | Empty items, final reference, XML characters, erasure limit, both parcel upper bounds, currency/bank/MNB | Does not assert account-dependent valid content. |
| `tests/request_dates.rs:42–132` | Every invoice outbound date through checked public boundary, including later item | Covers the fixed earlier date-domain issue. |
| `tests/simple_items.rs:20–113` | All six kinds × nine preview/simple presence states; exact case/order; monetary fields retained; JSON omission/null/false/true | Includes forbidden business combinations deliberately; tests say rules belong to vendor. |
| `src/item.rs:232–440` | Exactness/underflow/overflow, ordinary docs arithmetic, special-code zero VAT, currency scales, signed rounding | Local math only. |
| `src/types.rs:1115–1225`, `tests/numeric_fidelity.rs:15–47` | Value parsing/normalization, unknown values, currency aliases, numeric `Other` arithmetic/exactness | Does not prove vendor accepts every numeric lexical spelling in a string VAT field. |
| `tests/schema_requests.rs:31–115,123–418,508–535` | Checked-wire matrix, both credentials, full/minimal/independent blocks, all kinds, tail permutations, erasure/group cases, empty containers, explicit false, all languages/templates/statuses, multiple rows/files | Exporter alone does not validate XSD. |
| `scripts/check-agent-schemas.py:59–85,88–174` | Real `xmllint --nonet --schema`, provenance checks, exact known-conflict diagnostics, declared-path coverage earned only by valid cases, negative controls | Present at HEAD; not run here. Path coverage is not exhaustive semantic/combinatorial coverage. |
| `tests/upstream.rs:107–149,182–258,1065–1114,1206–1310` | Stored-source tail conflict; reconstruction of official example; explicit deviations and lossy outline comparison | Corpus may lag current web text; example-outline comparison is not a substitute for the new full-schema matrix. |
| `tests/response_headers.rs:43–105,148–337,406–602`; `src/ops/envelope.rs` tests | Encoded payment method, status/error precedence, money grammar, body/header precedence, numbered-56 evidence retention | Shared parser tests; no vendor emission guarantee. |
| `src/wire.rs:461–516`; `src/xml.rs:916–933` | Multipart boundaries/header metacharacters; escaped/ordered XML | XML text uses actual quick-xml 0.42 escaping, including CR → `&#13;` (`escape.rs:93–110` in the resolved dependency). No CR-loss defect applies. |
| `tests/literals.rs:22–98` | Downstream plain-data construction for header, seller, buyer, item and ledger | Public API usability, not business validation. |
| `tests/live.rs:42–169`; `tests/probes.rs` scenario inventory | Ordinary invoice/proforma lifecycle; appearance mismatch and clearing scenarios | Inspected as opt-in code only; never run. Presence of a scenario is not an observation. |

Residual meaningful gaps are vendor-evidence gaps: C1–C5, actual mail/attachment handling, carrier-generated output, and unusual currency/margin/simplified-image combinations. The offline matrix appropriately exposes source contradictions instead of patching an invented composite “official” schema. A future schema fix should update independently acquired sources and expected conflicts together, rather than weakening the validator until it passes.

**Bottom line:** eec57fc's scoped request model is field-complete against the current inline invoice specification. No new actionable Rust conformance bug was established. Preserve the documented source-backed choices, keep the fixed date/schema-test findings closed, and pursue the specific unresolved vendor contracts above before changing semantics.
