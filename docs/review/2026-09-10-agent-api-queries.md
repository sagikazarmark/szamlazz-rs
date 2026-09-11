# Számla Agent query API review — 2026-09-10

## Conclusion

**No new normative implementation bug or missing documented query-response field was confirmed.** The current XML model covers all **125 element declarations across the 19 complex structures** of the freshly downloaded `szamla.xsd` (reused address structures counted once). Both query writers cover all three selectors. The PDF result now exposes every successful payload field in its operation-specific response schema.

Retained observations:

| ID | Classification | Priority | Conclusion | Confidence |
|---|---|---|---|---|
| Q-D1 | API documentation omission | P3 | XML-query rustdoc omits the vendor's internal-outgoing-only retrieval restriction, while the response model documents externally issued records. | High gap / High remedy |
| Q-V1 | Vendor ambiguity / source defect | P3 clarification | Current HU PDF inline XSD contradicts EN/download selector order and presence, and is not well-formed XML. Keep the working writer policy. | High conflict / Medium deployed-contract certainty |

**No P0/P1/P2 finding. No new unsupported-capability addition recommended. No robustness-only issue is elevated to a normative bug.** The limits and permissive behaviors below are explicit, including exact Decimal representability and the legacy civil-date domain. Offline counterexamples establish parser behavior, not vendor emission frequency.

## Baseline, scope, and method

- Reviewed baseline `382cf7615aca1d64a05c7c3f77110248dde51950`, which was HEAD at the start. During the review another session advanced HEAD to `7da23b44cd006783eb47e60aa54ab8969bdd1c2c` (`chore: use published restate-e2e-harness crate`). A final diff against the requested baseline confirmed **no changes to `crates/szamlazz-agent`, fixture provenance, vendor questions, or behavior notes**. The reviewed code and line citations therefore remain the requested baseline. Existing unrelated workspace changes were present.
- Owned scope: `crates/szamlazz-agent/src/ops/query_xml.rs`, `ops/query_pdf.rs`, and relevant numeric/date/text/totals helpers in `src/xml.rs`; followed `src/number.rs`, `types.rs`, and PDF projection through `ops/envelope.rs` where needed. Below, abbreviated source paths are relative to `crates/szamlazz-agent/src/`.
- Read `docs/szamlazz-hu-behaviour.md`, applicable vocabulary in `CONTEXT.md`, `fixtures/SOURCES.md`, and `docs/research/2026-09-10-agent-vendor-questions.md`. The last document covers preview/layout questions, not query selectors. The untracked old `docs/review/2026-09-09-agent-api/FINAL.md` was a prior-finding checklist only, never current-code evidence.
- Fetched both languages of the six current query request/XML/response pages, all four applicable downloadable XSDs, legacy XSD pages, and EN/HU annotations on the shared outgoing-invoice document. Per-field comparison used fresh sources, not merely the fixture corpus.
- All network activity was unauthenticated documentation GETs. No account calls. No production or test source changes. Scratch acquisition/probes live under `/tmp/opencode/query-audit-382cf761/`; this report is the sole repository artifact created by this review.
- No subagent tool was available; this report is a direct review, not an independently delegated judgment.

## Confirmed findings

### Q-D1 — Document the XML operation's retrieval boundary

**P3 · Documentation omission · High confidence in gap and bounded remedy.**

**Current code:** `ops/query_xml.rs:1–2` describes fetching “the full data of a previously issued invoice”; `:46–50` describes the operation and full-data response without its retrieval boundary. `InvoiceInfo::source` at `:236–237` then describes a source code “for externally issued invoices.” This model field is correct for the shared schema, but its presence does not establish that the Számla Agent query can retrieve such records.

**Current official sources:**

- <https://docs.szamlazz.hu/agent/querying_xml/request>: “only the data of internal outgoing invoices (issued in Számlázz.hu) can be retrieved via this interface.”
- <https://docs.szamlazz.hu/hu/agent/querying_xml/request>: “csak belső (Számlázz.hu-ban kiállított) kimenő számlák adatait lehet lekérni.”
- <https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd>, `forras` annotation: “csak abban az esetben szerepel, ha a kérdéses számla nem a Számlázz.hu-val lett kibocsátva” (only when the invoice was not issued with Számlázz.hu).

**User impact:** A consumer reading the Rust query API and its `source` field can reasonably plan to retrieve NAV-imported or incoming records from the billing account through `QueryInvoiceXml`; that capability is explicitly outside the documented operation. Failures would be discovered only during integration, and the returned shared-schema model can obscure the reason.

**Bounded remedy:** Add one sentence on `QueryInvoiceXml` stating the vendor's internal-outgoing-only scope, and qualify `InvoiceInfo::source` as a field retained from the shared `szamla` schema whose presence in the model does not expand retrieval coverage. Keep `source`, sparse fields, proforma/storno/prepayment/final/corrective/delivery-note representations, and unknown document types. Do not reject `forras` client-side or claim a specific error code for imported/incoming lookups without evidence. This is documentation of an upstream restriction, not a request for a new operation.

### Q-V1 — HU PDF schema conflicts with independently usable selectors

**P3 clarification · Vendor ambiguity/source defect, not a confirmed Rust bug. High confidence in source conflict; Medium confidence about all deployed selector/order combinations.**

**Current code:** `ops/query_pdf.rs:68–78` writes a selected invoice/order number, then `valaszVerzio`, then an external id if that is the selector. `types.rs:1034–1052` models exactly one selector. This matches the EN inline and downloadable schemas.

**Current official sources and quotes:**

- <https://docs.szamlazz.hu/hu/agent/querying_pdf/xml>: `szamlaszam ... minOccurs="1"`; its sequence is `szamlaszam`, `valaszVerzio`, `rendelesSzam`, `szamlaKulsoAzon`.
- <https://docs.szamlazz.hu/agent/querying_pdf/xml> and <https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd>: `szamlaszam ... minOccurs="0"`; sequence is `szamlaszam`, `rendelesSzam`, `valaszVerzio`, `szamlaKulsoAzon`.
- <https://docs.szamlazz.hu/hu/agent/querying_pdf/request>: identification by “számlaszám ... rendelésszám ... **vagy** külső azonosító” (invoice number, order number, **or** external identifier). Thus HU prose also supports independent selectors.
- The HU inline schema literally contains `...xmlszamlapdf"xmlns:tns=...`, with no separating whitespace. Python's independent XML parser refuses the unmodified extracted block at line 1, column 140. The legacy <https://docs.szamlazz.hu/hu/agent/querying_pdf/xsd> repeats the same structure and defect.

**User impact:** A caller validating an order-only or external-id-only PDF request against the HU schema would reject the crate's output (after first repairing that schema's XML syntax). A consumer implementing HU order would send a different sequence. This prevents a universal claim that the writer complies with every current official schema; it does not show the deployed service rejects the crate's order.

**Bounded remedy:** Retain current serialization and independent selectors. Ask the vendor to align HU syntax, optional invoice-number presence, and `rendelesSzam` order with EN/download, or explicitly identify accepted alternatives. Preserve separate source evidence if adding a drift check. Do not insert a fake/empty invoice number, combine selectors, reorder to HU by default, or auto-retry another request shape. Existing test-account XML/PDF external-id observations (`docs/szamlazz-hu-behaviour.md:63–68`) support keeping that capability, without proving every PDF order-number combination.

## Complete request and PDF-response coverage

Sources: [XML request][xr], [XML example/schema][xx], [XML download][xd], [PDF request][pr], [PDF example/schema][px], [PDF download][pd], [PDF response][ps], [response download][sd]. HU counterparts were checked, with Q-V1 the substantive selector/schema conflict.

| Wire element / surface | Official type/presence | Current implementation and disposition |
|---|---|---|
| XML action/root/namespace | `action-szamla_agent_xml`; `xmlszamlaxml`; `http://www.szamlazz.hu/xmlszamlaxml` | Exact match, `ops/query_xml.rs:530–538`. |
| PDF action/root/namespace | `action-szamla_agent_pdf`; `xmlszamlapdf`; `http://www.szamlazz.hu/xmlszamlapdf` | Exact match, `ops/query_pdf.rs:58–65`. |
| `felhasznalo`, `jelszo`, `szamlaagentkulcs` | Optional strings directly under query root, in that order | `xml.rs:456–465`; credential variant writes key or username/password. No `beallitasok` wrapper. |
| `szamlaszam` | Optional string in EN/download | `InvoiceSelector::InvoiceNumber`; exact value escaped, no query-side trimming. |
| `rendelesSzam` | Optional string; last matching order document | `InvoiceSelector::OrderNumber`; exact case preserved. Shared selector rustdoc says last, `types.rs:1046–1049`. |
| `szamlaKulsoAzon` | Optional string, usable if attached on creation | `InvoiceSelector::ExternalId`, serialized last in both operations. Neither query claims external-id uniqueness. |
| XML `pdf` | Optional boolean | `include_pdf: bool`, constructor false, always writes explicit true/false, `ops/query_xml.rs:54–69,547`. HU example is empty but its comment prescribes true/false; emitting a valid boolean is correct. |
| PDF `valaszVerzio` | Required `int` in schemas; response prose also describes omitted/1 legacy mode | Always shared `RESPONSE_VERSION` = 2, `ops/query_pdf.rs:75`. The typed operation deliberately chooses structured mode. No missing PDF-download capability. |
| XML success | `szamla`, namespace `http://www.szamlazz.hu/szamla` | `ops/query_xml.rs:558–603`; full model below. |
| XML error | `xmlszamlavalasz`, `sikeres=false`, `hibakod`, `hibauzenet`; unknown selector code 7 | Body-only errors handled, `:563–581`; a successful generic envelope is refused as the wrong response shape. |
| PDF `sikeres`, `hibakod`, `hibauzenet` | Required boolean verdict; optional string error fields | Consumed via `parse_issued`; verdict belongs to the operation result/error, not `InvoicePdf` fields. |
| PDF `szamlaszam` | Optional string in shared envelope (also covers errors) | Required successful result identity, body then header; `InvoicePdf::invoice_number`, `ops/query_pdf.rs:40–41,87`. No valid-success evidence supports inventing an absent number. |
| PDF `szamlanetto`, `szamlabrutto` | Optional doubles | `net_total`, `gross_total: Option<Decimal>`, `:42–45,88–89`; absence retained; no recomputation. |
| PDF `kintlevoseg` | Optional double | `outstanding: Option<Decimal>`, `:46–49,90`; body then header, absence is not zero. Prior missing exposure is fixed. |
| PDF `vevoifiokurl` | Optional string | `customer_account_url: Option<String>`, `:50–53,91`; body then decoded header. Prior missing exposure is fixed. |
| PDF `pdf` | Optional `base64Binary` in shared envelope | Required on successful PDF query, `:54–55,92`; absent/blank is `Missing("pdf")`, malformed base64 is an error. |

All 12 generated combinations (two credential variants × three selectors × two operations) passed independent checks against fresh download element sequences, namespaces, required presence, maximum-one occurrences, and selector exclusivity. This was a structural comparison using Python ElementTree, **not a full XSD validator**; neither lxml nor xmllint was available. Escaped `&` in selectors was included. Existing unit tests independently cover `include_pdf=true` and omission of the other selectors.

Unexposed legacy/raw mode (`valaszVerzio=1`) is a deliberate format choice, not a missing document capability. No PDF-operation-specific source was found requiring public `document_id`, `payment_method`, or notification-warning fields: the operation's own schema lists exactly the payload above. Envelope-wide behavior and HTTP/error policy remain separately owned.

## Full queried invoice model: every block, field, and type

Primary structural source: fresh [szamla.xsd][sx]. Semantic cross-check: [EN outgoing document annotations][ae] and [HU annotations][ah], which describe the same `szamla` structure. They do **not** expand the Agent's retrieval surface (Q-D1).

Notation: `?` after a Rust type means `Option<T>`; **R** = XSD `minOccurs=1`, **O** = `minOccurs=0`; unspecified maximum is one. “Business text” means decoded nonblank content preserved, with only empty/XML-whitespace-only text mapped to None. Every listed mapping was traced through the private XML struct and its public conversion.

### Root and shared address/bank structures

| XML path / declaration | Schema | Public mapping / parsing |
|---|---|---|
| `szamla/szallito` | R `szallitoTipus` | `supplier: Supplier`, required. |
| `szamla/alap` | R `alapTipus` | `info: InvoiceInfo`, required. |
| `szamla/vevo` | R `vevoTipus` | `buyer: BuyerInfo`, required. |
| `szamla/tetelek` → `tetel` | R wrapper; 1..unbounded rows | `items: Vec<DocumentItem>`; wrapper required, empty rows accepted. |
| `szamla/qutetek` → `qutet` | O wrapper; 0..unbounded rows | `financial_items: Vec<FinancialItem>`; omitted/empty → empty vector. |
| `szamla/cimkek` → `cimke` | O wrapper; O string, max 1 in XSD | `labels: Vec<String>`; multiple labels preserved as a permissive extension. Same reusable structure on financial items. |
| `szamla/osszegek` | R `osszegekTipus` | `totals: Totals`, required. |
| `szamla/kifizetesek` → `kifizetes` | O wrapper; 1..unbounded rows when present | `credit_entries: Vec<RecordedCreditEntry>`; omitted/empty → empty vector. No five-row query cap. |
| `szamla/pdf` | O string | `pdf: Pdf?`; absent/blank → None, otherwise base64 decode. Actual encoding comes from operation documentation. |
| `cimTipus/orszag` | O string | `Address.country: String?`, business text. |
| `cimTipus/irsz`, `telepules`, `cim` | R strings | `Address.zip`, `city`, `address: String`; present empty strings accepted, omission refused. Shared by supplier billing/postal and buyer billing addresses. |
| `cimpostaTipus/nev`, `orszag`, `irsz`, `telepules`, `cim` | O strings | `BuyerPostalAddress.name`, `country`, `zip`, `city`, `address: String?`; all business text. |
| `bankTipus/nev`, `bankszamla` | O strings | `Bank.name`, `account: String?`, business text. |

Code: public `ops/query_xml.rs:73–144,319–334`; wire/conversion `:583–698,797–821,910–914,995–999,1040–1051`. The different supplier and buyer postal-address types exactly follow their different schema types.

### Supplier (`szallitoTipus`)

| Element | Schema | Public field/type |
|---|---|---|
| `id` | R int | `Supplier.id: i64?`; missing/empty tolerated, malformed integer refused. |
| `nev` | R string | `name: String`. |
| `cim` | R `cimTipus` | `address: Address`. |
| `postacim` | O `cimTipus` | `postal_address: Address?`. |
| `adoszam` | R string | `tax_number: String?`, business text. |
| `csoportazonosito` | O string | `group_id: String?`, business text. |
| `adoszameu` | O string | `eu_tax_number: String?`, business text. |
| `bank` | O `bankTipus` | `bank: Bank?`. |

Code: `ops/query_xml.rs:122–144,667–698`. Retaining an optional supplier id is intentional; its stability is not established as an account identity.

### Core invoice data (`alapTipus`)

| Element | Schema | Public field/type and behavior |
|---|---|---|
| `id` | R int | `id: i64`, required lexical integer. |
| `szamlaszam` | R string | `invoice_number: InvoiceNumber`, required element, wire string newtype. |
| `gazdEsemAzon` | R int | `economic_event_id: i64?`, empty/missing tolerated. |
| `forras` | O int | `source: i64?`; retains codes, including unknown ones. See Q-D1. |
| `iktatoszam` | O string | `registration_number: String?`. |
| `tipus` | R string | `document_type: DocumentType`, open token set. |
| `eszamla` | R int | `appearance: InvoiceAppearance`, retains i64 code. |
| `hivszamlaszam` | O string | `referenced_invoice_number: InvoiceNumber?`; nonblank decoded text retained. |
| `hivdijbekszam` | O string | `referenced_proforma_number: InvoiceNumber?`; same. |
| `kelt` | R date | `issue_date: Date?`. |
| `telj` | R date | `fulfillment_date: Date?`. |
| `fizh` | R date | `due_date: Date?`. |
| `fizmod` | R string | `payment_method: PaymentMethod?`, arbitrary token retained via `Other`. |
| `fizmodunified` | R restricted string | `unified_payment_method: String?`; open, does not reject the example's `other`. |
| `keszpenz` | R boolean | `cash_payment: bool`; absent/empty → false. |
| `rendelesszam` | O string | `order_number: String?`; decoded business text, not trimmed. Note response lowercase spelling versus request `rendelesSzam`. |
| `nyelv` | R restricted string | `language: String?`; open to future language tokens. |
| `devizanem` | R string | `currency: Currency?`; open string value, no forced HUF/Ft equivalence. |
| `devizabank` | O string | `exchange_bank: String?`. |
| `devizaarf` | O double | `exchange_rate: Decimal?`; zero preserved, no automatic rate lookup. |
| `megjegyzes` | O string | `comment: String?`. |
| `afatipus` | O string | `vat_type: String?`, invoice-level value separate from line VAT. |
| `penzforg` | R boolean | `cash_accounting: bool`; absent/empty → false. |
| `kata` | R boolean | `kata: bool`; absent/empty → false. |
| `katafokonyv` | R boolean | `kata_ledger: bool`; absent/empty → false. |
| `email` | O string | `email: String?`, document-associated address, separate from buyer email. |
| `teszt` | R boolean | `test: bool?`; absent/empty is unknown, not live/false. |
| `sztornozott` | O boolean | `reversed: bool?`; explicit true/false retained; absent remains None. |

Code: public `ops/query_xml.rs:146–317`; wire/conversion `:700–795`; reference helper `:1084–1091`. Optional textual entries use business-text semantics unless a more specific rule is noted.

Appearance is correctly **0 not invoice; 1 paper; 2/3 electronic; other integer unknown** (`:197–204`). HU annotation says “0: nem számla, 1: papír számla, 2: e-számla, 3: e-számla”; EN agrees. The annotation's adjacent `REQ string` comment conflicts with its XSD `int`, not with the implemented interpretation. Historical P73 observations support 1 and 3; 2 remains documented, not locally observed.

### Buyer and buyer ledger

| Element | Schema | Public field/type |
|---|---|---|
| `vevo/id` | O int | `BuyerInfo.id: i64?`, distinct from partner identifier. |
| `vevo/nev` | R string | `name: String`. |
| `vevo/azonosito` | O string | `identifier: String?`. |
| `vevo/cim` | R `cimTipus` | `address: Address?`, omission tolerated. |
| `vevo/postacim` | O `cimpostaTipus` | `postal_address: BuyerPostalAddress?`. |
| `vevo/email` | O string | `email: String?`. |
| `vevo/adoszam` | R string | `tax_number: String?`. |
| `vevo/csoportazonosito` | O string | `group_id: String?`. |
| `vevo/adoszameu` | O string | `eu_tax_number: String?`. |
| `vevo/lokacio` | R int | `location: i64?`; no closed enum gate. Documented 1 domestic, 2 EU, 3 outside EU, -1 unknown. |
| `vevo/privatePersonIndicator` | R boolean | `private_person: bool`, absent/empty → false. |
| `vevo/fokonyv` | O complex | `ledger: BuyerLedgerInfo?`. |
| `fokonyv/vevo` | O string | `BuyerLedgerInfo.account: String?`. |
| `fokonyv/vevoazon` | O string | `buyer_id: String?`, buyer ledger identifier. |
| `fokonyv/datum` | O date | `date: Date?`. |
| `fokonyv/folyamatostelj` | O boolean | `continuous_fulfillment: bool?`, all four XSD boolean spellings supported. |
| `fokonyv/elszDatTol`, `elszDatIg` | O dates | `settlement_from`, `settlement_to: Date?`; correct mixed-case wire names. |

Code: `ops/query_xml.rs:336–393,823–908`. Optional strings use business-text semantics. The qualified buyer-mutability warning at `:355–360` correctly distinguishes one test-account observation from immutable at-issue data.

### Printed items and item ledger

| Element | Schema | Public field/type |
|---|---|---|
| `tetel/nev` | R string | `DocumentItem.name: String`. |
| `tetel/azonosito` | O string | `id: String?`. |
| `tetel/mennyiseg` | R double | `quantity: Decimal`. |
| `tetel/mennyisegiegyseg` | R string | `unit: String`. |
| `tetel/nettoegysegar` | R double | `unit_price: Decimal`. |
| `tetel/afatipus` | O restricted string | `vat_type: String?`, open special code. |
| `tetel/afakulcs` | R double, minInclusive 0 | `vat_rate_code: String`, raw token retained; `vat_rate()` interprets it only if no special type exists. |
| `tetel/netto` | R double | `net_value: Decimal`. |
| `tetel/arresafaalap` | O double | `margin_vat_base: Decimal?`. |
| `tetel/afa` | R double | `vat_value: Decimal`. |
| `tetel/brutto` | R double | `gross_value: Decimal`. |
| `tetel/megjegyzes` | O string | `comment: String?`. |
| `tetel/sztetordering` | R int | `ordering: i64?`, omission tolerated. XSD int takes precedence over example's `REQ double` annotation. |
| `tetel/fokonyv` | O complex | `ledger: DocumentItemLedger?`. |
| `fokonyv/arbevetel` | O string | `DocumentItemLedger.revenue_account: String?`. |
| `fokonyv/afa` | O string | `vat_account: String?`; ledger account, not numeric tax. |
| `fokonyv/gazdasagiesemeny` | O string | `economic_event: String?`. |
| `fokonyv/gazdasagiesemenyafa` | O string | `vat_economic_event: String?`. |
| `fokonyv/elszdattol`, `elszdatig` | O dates | `settlement_from`, `settlement_to: Date?`; correctly lowercase, unlike buyer ledger. |

Code: `ops/query_xml.rs:395–458,916–993`. Values are reported, not recalculated; negative storno quantities/totals survive. Neither ordering metadata nor quantity sign causes sorting or filtering.

### Financial items, labels, totals, credit entries

| Element | Schema | Public field/type |
|---|---|---|
| `qutet/nev` | R string | `FinancialItem.name: String`. |
| `qutet/afatipus` | O restricted string | `vat_type: String?`. |
| `qutet/afakulcs` | R double, minInclusive 0 | `vat_rate_code: String`; special type takes precedence in `vat_rate()`. |
| `qutet/netto`, `afa`, `brutto` | R doubles | `net`, `vat`, `gross: Decimal`. |
| `qutet/elszdattol`, `elszdatig` | O dates | `settlement_from`, `settlement_to: Date?`. |
| `qutet/afalevon` | R int | `deductible_vat: i64`; trimmed integer, no invented percentage/range. |
| `qutet/cimkek` → `cimke` | O wrapper, O string max 1 | `labels: Vec<String>`; same permissive list as invoice labels. |
| `osszegek/afakulcsossz` | 1..unbounded complex | `Totals.by_vat_rate: Vec<VatTotal>`; omission accepted as empty. |
| `afakulcsossz/afatipus` | O restricted string | `VatTotal.vat_type: String?`. |
| `afakulcsossz/afakulcs` | R double, minInclusive 0 | `vat_rate_code: String`; same special-code precedence. |
| `afakulcsossz/netto`, `afa`, `brutto` | R doubles | `net`, `vat`, `gross: Decimal`. |
| `osszegek/totalossz` | R complex | `Totals.total: GrandTotal`, required. |
| `totalossz/netto`, `afa`, `brutto` | R doubles | `GrandTotal.net`, `vat`, `gross: Decimal`. |
| `kifizetes/datum` | R date | `RecordedCreditEntry.date: Date`, required/nonempty. |
| `kifizetes/jogcim` | R string | `title: PaymentMethod`, open token, required element. |
| `kifizetes/osszeg` | R double | `amount: Decimal`. |
| `kifizetes/megjegyzes` | O string | `comment: String?`. |
| `kifizetes/bankszamlaszam` | O string | `bank_account: String?`; sender if known, otherwise account printed on invoice. |
| `kifizetes/banktranzid` | O int | `bank_transaction_id: i64?`. |
| `kifizetes/devizaarf` | O double | `exchange_rate: Decimal?`. |

Code: `ops/query_xml.rs:460–528,995–1082`; `xml.rs:638–721`; `types.rs:1055–1107`. The corrected bank-account wording exactly matches HU “erről a bankszámláról érkezett ... ha a küldő bankszámlaszám nem ismert.” The `afalevon` unit remains undocumented; neutral integer wording is correct. `qutet` is retained without claiming a vendor-established English expansion.

**Coverage boundary:** No `fuvarlevel`, carrier sub-block, request-style `arfolyam` block, erasure code, preview/layout option, or `szamlaKulsoAzon` response element occurs in this fresh response schema or query example. Those request capabilities are not missing response fields. Here exchange-rate data is `alap/devizabank`, `alap/devizaarf`, and credit-entry `devizaarf`. There is no `kintlevoseg` element in `szamla`; do not fabricate a header-backed field in the document model merely by copying the PDF envelope.

## Deserialization audit and supported deviations

### Numeric helpers

`xml.rs:475–493` routes required/optional amounts through `number::parse`; `number.rs:6–84` recognizes finite signed decimal/exponent spellings and converts exactly, without f64. Required absence/empty content is an error; optional absent/empty/blank is None; malformed nonempty values are errors. All amount positions above use these helpers. Integer metadata uses i64, with optional empty handling at `xml.rs:559–571` and required financial `afalevon` at `:623–631`; direct `alap/id` and appearance use serde's integer reader.

Reproduced: `1e-2` → 0.01; `100e-30` → 1e-28; `+.5` → 0.5; `1.` → 1. Values `1e-29`, excessive significant precision, `NaN`, `INF`, underscores, and XML comma decimals are refused. These are **explicit representational/grammar limits**, not proof the vendor emits such monetary content. XML `xs:double` has a wider domain than Decimal; preserving exact money and failing rather than silently rounding is the current intentional policy. No switch to floating point or unrestricted acceptance is recommended.

`afakulcs` is deliberately raw text on rows and subtotals. Unknown special tokens survive; numeric interpretation accepts exponent/XML padding while retaining the raw token (`types.rs:296–323`). The reader does not enforce the XSD nonnegative facet or finite/numeric interpretation on that raw field. This is open/lossless representation, not silent amount correction. Unknown `TEHK`, `JS`, or future tokens need not have named enum variants to be supported.

### Dates: all eleven positions

`xml.rs:495–555` is used for ten optional positions (three core dates; three buyer-ledger dates; two item-ledger dates; two financial-item dates) and the required credit-entry date. Empty optional dates are None; malformed **nonempty** dates refuse the Agent query. This differs deliberately from Adatkapcsolat's content-tolerant optional-date rule; importing that receiver policy here would be a separate API decision.

- Complete XSD timezone suffixes `Z`, `±HH:MM` through ±14:00 are accepted and discarded **without moving the printed civil date**. `+14:01`, `+01:60`, arbitrary suffixes, malformed UTF-8-adjacent text, and a datetime in place of a date are refused.
- Checked slicing at `:505–508` avoids the multibyte boundary panic. The old timezone/required-date-padding finding is fixed, with all eleven positions under tests at `ops/query_xml.rs:1328–1464`.
- The initial Jiff parse at `xml.rs:499–502` intentionally retains the legacy accepted domain: year zero, six-digit signed BCE representation, compact `YYYYMMDD`, and Jiff-compatible annotations. An offline input `2024-02-29[u-ca=hebrew]` returns civil `2024-02-29`; `-2024-02-29` is refused while `-002024-02-29` is accepted; `10000-01-01` is outside this Date representation. These are not fresh regressions or ordinary invoice examples. Do not describe the helper as a complete `xs:date` validator or impose a new CE-year gate under this review.

### Text and XML shape

- `xml.rs:573–585` preserves decoded nonblank business text, including surrounding spaces and NBSP; only XML-whitespace-only text becomes None. Reference numbers use equivalent behavior at `ops/query_xml.rs:1084–1091`. Required strings remain required elements but may contain empty strings, as `xs:string` permits.
- CRLF normalization, character references, CDATA, and unknown-entity handling are delegated to quick-xml. Fresh probes: literal `A\r\nB` becomes `A\nB`; undefined entities in recognized text fail; a nested child inside a scalar fails rather than concatenating `A<extra/>B` into `AB`.
- A whole document and the expected root/namespace are required (`xml.rs:63–167`). Protocol-namespace projection (`:169–229`) prevents foreign same-local-name fields from supplying identity or reversal. Valid namespace aliases work; foreign subtrees and unknown fields are ignored; a recognized singleton duplicate fails. Probed duplicate `alap/id`, identity moved under an unknown wrapper, foreign reversal, and a trailing second root.
- The reader is not a schema-order validator: known fields can arrive in a different order, unknown additions are skipped, missing optional wrappers become empty vectors, and several XSD-required content fields are nullable/defaulted. No strict-XSD retrofit is recommended. Duplicate known singletons are distinct from multiple intentionally list-valued rows.
- Numeric/boolean helpers still use Rust Unicode `trim`, more permissively than XSD whitespace; optional dates pre-trim similarly. Probes accept NBSP around a number or optional date. Business text does **not** lose NBSP. Classify scalar permissiveness as robustness/compatibility behavior, not a meaningful-character-loss regression.

### PDF behavior and JSON

- XML query returns absent/blank PDF as None even when `include_pdf=true`; it decodes a present PDF even if false was requested. No unsupported promise is made that every requested PDF must arrive in the XML result.
- Nonblank malformed XML-query PDF fails the result (`ops/query_xml.rs:599–602`); dedicated PDF query requires a decoded PDF (`ops/query_pdf.rs:92`). This all-or-error behavior is observable, but no conforming real PDF counterexample was established. A proposed partial-document result would be a new design, not a bug fix proved by the vendor's placeholder example.
- `Pdf::from_base64` (`types.rs:110–116`) supports standard base64 with wrapped whitespace. It does not inspect PDF magic/content, repair abbreviated base64, or stream bytes. It also tolerates Unicode whitespace beyond XML's four whitespace characters. These are bounded robustness/representation choices, not newly confirmed protocol defects.
- Public results serialize Rust field names, Decimal strings, integer appearance codes, Date strings, token enums as their wire strings, and PDFs as base64. Private `*Xml` structs own the Hungarian names/empty-element rules. Same-version JSON round trips are tested. This is not a cross-release guarantee that arbitrary partial JSON reconstructs every required vector or scalar. The two newly added optional PDF metadata fields support older JSON without them (`ops/query_pdf.rs:48,52`; `tests/response_headers.rs:355–359`).

### Sparse shapes and live-backed facts to preserve

The current query example itself omits XSD-required `gazdEsemAzon`, `keszpenz`, `katafokonyv`, buyer `lokacio`, buyer `privatePersonIndicator`, and `sztetordering`; it has empty business fields and `fizmodunified=other`, outside the downloaded enumeration. It cannot justify making all XSD-required fields mandatory. Conversely, not every tolerated omission has a live observation: some are existing compatibility choices and are recorded as such here.

Preserve the bounded historical observations in `docs/szamlazz-hu-behaviour.md`: exact/case-sensitive query selectors (`:40–45`); newest shared external-id holder and no echoed external id (`:63–71`); absent reversal marker versus true on the original, absent on the storno, and credit-entry removal on reversal (`:77–80`); optional `telj` despite mandatory XSD (`:96`); appearance 1/3 observations (`:97–98`); possible later buyer changes (`:111`); unordered returned credit entries (`:134`); and body-only code 7 (`:141–142`). These are existing test-account observations, not new live verification.

## Current vendor inconsistencies and unsupported claims

Apart from Q-V1, these do not establish new code defects:

1. **Example PDFs are placeholders in both languages.** [XML response][xs] contains “The receipt .pdf can be found here in BASE64 encoding”; [PDF response][ps] contains `....` in base64. Unmodified examples correctly fail base64 decoding. Replacing only their PDF content with synthetic `JVBERi0=` makes all four success samples parse. Do not weaken decoding to accept documentation ellipses or call these successful live responses.
2. **XML response schema navigation is misleading.** EN/HU response prose says `szamla.xsd` but links “generating invoices,” whose request schema is `xmlszamla.xsd`. The correct full response download is [szamla.xsd][sx], also linked from the shared outgoing-document annotations. The two schemas are not interchangeable.
3. **PDF version presence differs between prose and XSD.** Response prose describes `1 or not set`; XSD requires `valaszVerzio`. Always writing 2 satisfies both intended successful modes and avoids the disagreement.
4. **HU XML request example has an empty boolean**, but its own comment says true if PDF needed, otherwise false. The actual writer's false/true and the XSD boolean are coherent.
5. **Published examples are not arithmetically authoritative.** The XML example's one printed row is 380/76/456 while totals are 464/93/557. The reader correctly retains each reported value; it should not reject or repair the example by recalculating.
6. **Shared schema includes externally sourced documents**, whereas the Agent XML request page explicitly excludes them. Retain shared fields but document retrieval scope (Q-D1). The annotation's `JS` code is safely retained as an open document type; there is no evidence requiring a named query enum variant.
7. **`afalevon` unit and `cimke` multiplicity:** the schema says int without unit/range and permits one label. Neutral integer plus permissive vector is appropriate; neither percentage semantics nor actual multiple-label emission was established.

The old `/xsd` pages remain fetchable with footer `v202606031507`, while current `/xml`, request, and response pages show `v202608271632`. Those build identifiers are not acquisition dates or proof that every sentence was updated then.

## Prior findings rechecked against current code

| Old issue area | Current disposition |
|---|---|
| XSD timezone dates and required-date whitespace | Fixed: shared civil-date adapter and eleven-position tests; `xml.rs:495–555`. |
| Optional business strings losing surrounding characters | Fixed for queried document fields and references: `xml.rs:573–585`, `ops/query_xml.rs:1084–1091`; fresh text tests pass. |
| Missing PDF outstanding amount / customer account URL | Fixed: `ops/query_pdf.rs:46–53,90–91`; body/header precedence and older JSON verified. |
| Silent numeric rounding / exponent interpretation | Exact conversion active at `number.rs:6–84`; representable exponents accepted and unrepresentable values refused. |
| Incomplete/trailing XML and namespace-local-name confusion | Whole-document and namespace projection guards active; applicable regression tests and fresh counterexamples pass. No repetition of the old broad finding. |
| Queried bank account called recipient account | Fixed at `ops/query_xml.rs:520–523`; sender-or-printed-account annotation confirmed EN/HU. |
| Queried buyer as immutable snapshot | Qualified observation now documented at `ops/query_xml.rs:355–360`. |
| `afalevon` described as a percentage | Fixed to reported integer with unknown unit/range at `ops/query_xml.rs:486–489`. |
| Appearance interpreted as a flag | Current code and test-account evidence agree: numeric code 1 is paper, 2/3 electronic. No fix recommended. |

## Verification record

Executed on the current tree, default crate features, entirely offline for Rust operations:

```text
cargo test --offline -p szamlazz-agent --lib ops::query_
  29 passed
cargo test --offline -p szamlazz-agent --test business_text --test numeric_fidelity --test response_namespaces --test response_completion --test response_headers
  24 passed (2 + 6 + 6 + 1 + 9)
cargo test --offline -p szamlazz-agent --test upstream responses::every_response_example_parses_through_its_operation -- --nocapture
  1 passed; corpus present, 18 response files visited
```

**54 tests passed, zero failures.** Some selected shared-helper integration suites contain controls for other operations; those passes do not claim a review of the separately owned operations. The corpus test explicitly handles source defects; it is not a claim that all unchanged official success examples decode.

Scratch commands and retained evidence:

```text
python3 /tmp/opencode/query-audit-382cf761/acquire.py
python3 /tmp/opencode/query-audit-382cf761/validate.py
python3 /tmp/opencode/query-audit-382cf761/structure.py
```

`validate.py` runs the path-dependent Rust probe with `cargo run --offline`, records `results.txt`, and reports unavailable lxml rather than claiming XSD validation. `structure.py` independently verifies the 12 generated request structures and rejects the unmodified HU schema's XML syntax. The Rust probe exercises fresh EN/HU examples, synthetic PDF-only substitutions, actual public metadata projection, malformed/valid dates and numbers, and identity/text boundaries. Scratch dependencies matched the workspace's relevant versions: quick-xml 0.42.0, Jiff 0.2.35, rust_decimal 1.43.0, serde 1.0.229, base64 0.23.1.

No full workspace test suite, live account tests, external XML-schema engine, fuzzing campaign, or general XML-conformance certification is claimed.

## Acquisition ledger

All successful sources below were acquired **2026-09-10**. HTML and decoded code blocks were saved only in scratch; no vendor corpus was redistributed or replaced. All four fresh downloads proved byte-identical to the corresponding cached workspace XSDs, an observed comparison rather than an assumption about fixture freshness.

| Source | SHA-256 of acquired bytes |
|---|---|
| [XML request EN][xr] | `f07eed320ee1863a207594229ef201be57535cb21020b3692679ddb67c5c7ca7` |
| [XML XML/XSD EN][xx] | `dd0619827e25249eb60f81e7f16cc916b5f813b01060150e1ba43559df568735` |
| [XML response EN][xs] | `c0a657660994b75800a24f843e1d8da3563e5be3be4d6fc20147db3c44346961` |
| [PDF request EN][pr] | `8200629c5c742b6a08d6e080be4c67a001241c8dea62b75e574d649cd1ce6145` |
| [PDF XML/XSD EN][px] | `bc1f0711108ab37ec15f58003b6e53789e3d27874ce3de5197e581c2391df666` |
| [PDF response EN][ps] | `0deb718e8edc26b4acdadacb9d7038754830bccffc2d67fee1ab99fca5d1ec51` |
| [XML request HU](https://docs.szamlazz.hu/hu/agent/querying_xml/request) | `ecfd680247a29f40d843486dc85107eb12ba69fa920056b1a53b7c67b43ec811` |
| [XML XML/XSD HU](https://docs.szamlazz.hu/hu/agent/querying_xml/xml) | `c7e0272cc98d3f1c2d71f42cf7317017c7442fbeef55332a175f69a5df041c4c` |
| [XML response HU](https://docs.szamlazz.hu/hu/agent/querying_xml/response) | `6c57213687c8cc6c851d39c7f18f88c488b0264c0a62b2ae30367c3cfef65989` |
| [PDF request HU](https://docs.szamlazz.hu/hu/agent/querying_pdf/request) | `66e2e4fc2cfb5b96d73388fe9fb8213c8c27b0c9c0644900a3c6db2339fbd9ec` |
| [PDF XML/XSD HU](https://docs.szamlazz.hu/hu/agent/querying_pdf/xml) | `04fb76c65a80106a45df9bf8d8b3bfc05dd0cece62e7a2c067627b47d60ebc95` |
| [PDF response HU](https://docs.szamlazz.hu/hu/agent/querying_pdf/response) | `b5640050a9a5718036fae849e79ec1af2016d5ccb1f2e0239a2d54be02fb95b1` |
| [XML request XSD download][xd] | `06cd34ce07ca8f3c0919cf7c4e6505bbda66ddf6b72d60736c849e695f7e19f3` |
| [PDF request XSD download][pd] | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` |
| [Full queried document XSD download][sx] | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` |
| [PDF envelope XSD download][sd] | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |

For Q-V1, decoded inline XSD block hashes (no added newline/reindentation): EN `24dcfe7f5ea673907061560a70800bf284aa24112971691803ad74ff99db6953`; HU `1c50375b586ddfeac1867af3c6b3b5427c60ded9772352456274d6ddcbda113c`. These identify separate source documents, not a merged “canonical” schema.

Additional fresh reads: [EN shared-document annotations][ae], [HU shared-document annotations][ah], [legacy XML XSD](https://docs.szamlazz.hu/agent/querying_xml/xsd), [legacy EN PDF XSD](https://docs.szamlazz.hu/agent/querying_pdf/xsd), [legacy HU PDF XSD](https://docs.szamlazz.hu/hu/agent/querying_pdf/xsd). Initially guessed `/agent/querying_invoice_xml` and `/agent/querying_invoice_pdf` returned 403; the actual documented `querying_xml`/`querying_pdf` URLs above succeeded.

[xr]: https://docs.szamlazz.hu/agent/querying_xml/request
[xx]: https://docs.szamlazz.hu/agent/querying_xml/xml
[xs]: https://docs.szamlazz.hu/agent/querying_xml/response
[xd]: https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd
[pr]: https://docs.szamlazz.hu/agent/querying_pdf/request
[px]: https://docs.szamlazz.hu/agent/querying_pdf/xml
[ps]: https://docs.szamlazz.hu/agent/querying_pdf/response
[pd]: https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd
[sx]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[sd]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd
[ae]: https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak
[ah]: https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak
