# Számla Agent invoice queries: fresh full-source conformance review

> **Final adjudication:** the P2 Q-01 candidate below was downgraded to presence-model/clarification work; Q-02 is a shared P3 malformed-verdict hardening item, including an Unknown → Rejected consequence with known codes. See the [adjudication](2026-09-11-agent-api-eec57fc-adjudication.md) and [final report](2026-09-11-agent-api-eec57fc.md) for final classifications and executed suite/schema results.

**Reviewed commit:** `eec57fcf3036d93cd68c9cfc017338cd3020e7dd` (`HEAD`).

**Acquired/reviewed:** 2026-09-11. **Scope:** invoice XML/PDF queries and the complete `InvoiceDocument` response tree, including shared XML, numeric, envelope, value-type and HTTP-response handling.

## Result

**No missing `InvoiceDocument` element was found against the freshly downloaded response schema.** All **134 descendant element paths** (135 including `/szamla`, counting containers and expanding reused types at each location) have a wire/public-model destination. An independently schema-generated, fully populated synthetic document parsed through the current public operation, and its JSON retained the populated fields. This is field/model coverage, not proof of full XSD validation or live emission of every field.

Two findings remain:

| ID | Severity | Finding |
|---|---|---|
| Q-01 | P2 / medium | Five document booleans collapse missing/empty content into a factual `false`; absence cannot be recovered from `InvoiceDocument`. |
| Q-02 | P3 / low | An empty envelope `sikeres` is interpreted as a vendor refusal instead of an unreadable verdict. |

No P0/P1 finding. Request actions, selector placement, namespaces, the v2 PDF choice, nested field mappings, ordinary date/number parsing and body-before-header PDF metadata precedence conform to the sources identified below. There are explicit local domain restrictions and unresolved success/header guarantees; those are separated from confirmed defects.

### Method and limits

- Started from the current source and fresh official pages/downloads. Earlier review reports were not used as finding inputs. `fixtures/SOURCES.md:90–109` was used only to locate download URLs; their current bytes were independently fetched.
- Read the complete production implementations in `src/ops/query_xml.rs`, `query_pdf.rs`, `envelope.rs`, `src/xml.rs`, `src/number.rs`; followed `src/types.rs`, `src/wire.rs`, `src/client.rs` and associated tests where they affect queries. Paths below are relative to `crates/szamlazz-agent/` unless prefixed otherwise.
- Read `docs/szamlazz-hu-behaviour.md` and the dated credit-clearing research. Tests describe coverage and implementation expectations, **not vendor truth**. The vendor clarification draft explicitly says no answer was received; it is not a vendor source.
- Only unauthenticated documentation/XSD GETs were made. No credentials were read, no account endpoint was called, and no production/test files were edited.
- Repository Cargo tests belong to the parent run and were not run here. Scratch Rust/Python experiments stayed under `/tmp/opencode/queries-eec57fc/`. The only repository output is this report.
- Final verification still reported the reviewed HEAD. Concurrent changes appeared in the Restate worker and sibling review reports during this review; they were not used to change the Agent verdict or edited here. `git diff --check` reported no tracked whitespace errors.
- No XSD validator was available in this shell (`xmllint` absent, Python `lxml` absent). The fresh-schema experiment checks declaration structure, recursive inventory and parser output; **it is not an independently validated XSD instance**. The repository's separate schema-check runner was inspected, not executed.

## 1. Primary sources acquired

All official web pages below were fetched during this review. The docs footer reported **`v202608271632`**; that is a site build label, not an observation date or a version guarantee for every schema.

| Ref | URL | Relevant authoritative statement/content |
|---|---|---|
| X0 | https://docs.szamlazz.hu/agent/category/query-document-xml | XML query category; links to request, response and XML/XSD. |
| X1 | https://docs.szamlazz.hu/agent/querying_xml/request | “only the data of internal outgoing invoices (issued in Számlázz.hu)”; action `action-szamla_agent_xml`; three selectors, last document for a shared order. |
| X2 | https://docs.szamlazz.hu/agent/querying_xml/response | Success is “Full `szamla` XML document”; error is `xmlszamlavalasz` with `sikeres=false`, code/message; unknown selector returns 7. |
| X3 | https://docs.szamlazz.hu/agent/querying_xml/xml | Ordered request schema, optional boolean `pdf`, external id after `pdf`. |
| P0 | https://docs.szamlazz.hu/agent/category/query-document-pdf | PDF query category and its three linked pages. |
| P1 | https://docs.szamlazz.hu/agent/querying_pdf/request | Action `action-szamla_agent_pdf`; number/order/external-id selectors. |
| P2 | https://docs.szamlazz.hu/agent/querying_pdf/response | Version 2 is “Structured `xmlszamlavalasz` with base64-encoded PDF inside `<pdf>`”; “additional parameters may also arrive” in HTTP headers; inline response schema. |
| P3 | https://docs.szamlazz.hu/agent/querying_pdf/xml | Required integer `valaszVerzio`; order selector before it, external id after it. |
| S1 | https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd | Complete queried-document schema; target namespace `http://www.szamlazz.hu/szamla`, `elementFormDefault="qualified"`. |
| S2 | https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd | Shared v2 envelope schema, nine child declarations. |
| S3 | https://www.szamlazz.hu/szamla/docs/xsds/agentxml/xmlszamlaxml.xsd | Downloaded XML-query request schema. |
| S4 | https://www.szamlazz.hu/szamla/docs/xsds/agentpdf/xmlszamlapdf.xsd | Downloaded PDF-query request schema. |
| H1 | https://docs.szamlazz.hu/hu/agent/querying_xml/xml | Hungarian XML-query request sample/schema; sample has empty `pdf`, annotation asks for true/false. |
| H2 | https://docs.szamlazz.hu/hu/agent/querying_pdf/xml | Hungarian PDF-query schema conflict, detailed below. |
| H3 | https://docs.szamlazz.hu/hu/agent/querying_xml/response | Same response shape and sparse sample as X2; minor sample prose differences. |
| H4 | https://docs.szamlazz.hu/hu/agent/querying_pdf/response | Same v2 envelope and optional-field declarations as P2. |
| A1 | https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak | Independently published `szamla` schema and annotated field semantics. Used for the shared document shape, not to expand Számla Agent retrieval scope. |
| A2 | https://docs.szamlazz.hu/agent/generating_invoice/xml | Followed from X2's schema link. This page actually describes the create **request** (`xmlszamla`), not the queried `szamla` response. |
| A3 | https://docs.szamlazz.hu/agent/generating_invoice/response | Named `szlahu_*` headers; net/gross and error code are not URL encoded; invoice number and error text are. This is creation documentation, not a guarantee those headers occur on queries. |
| B1 | https://docs.szamlazz.hu/agent/basics/authentication | “either an Agent key … or a username and password”; legacy key-as-both-fields supported. |
| B2 | https://docs.szamlazz.hu/agent/basics/sending-requests | HTTPS POST with XML file, action table; element names case-sensitive. |
| B3 | https://docs.szamlazz.hu/agent/basics/error-handling | Error-code and response-version-1 text-error guidance. |
| W1 | https://www.w3.org/TR/xmlschema-2/#boolean | Boolean legal literals `{true, false, 1, 0}`. |
| W2 | https://www.w3.org/TR/xmlschema-2/#double | Mantissa plus optional exponent; includes `INF`, `-INF`, `NaN`. |
| W3 | https://www.w3.org/TR/xmlschema-2/#date | XSD civil-date lexical/value domain, including optional timezone. |

The Adatkapcsolat landing page (https://docs.szamlazz.hu/penzugyi-adatkapcsolat/) was also fetched to locate A1. Its historical v5.1 ZIP is labelled last updated 2023-06-07; it was not used as current query authority. The current self-contained S1–S4 downloads have **no includes/imports**. A guessed `/szamla/docs/xsds/szamla.xsd` URL returned 404; guessed `/agent/basics/response` and `/penzugyi-adatkapcsolat/outgoing-invoice` routes returned 403. The working routes above supplied the needed content.

### Fresh download identity and source comparison

Hashes cover original downloaded bytes. Inline hashes cover HTML-decoded `<pre>` text without reformatting or an added newline.

| Source | Bytes | SHA-256 |
|---|---:|---|
| S1 `szamla.xsd` | 20,049 | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` |
| S2 `xmlszamlavalasz.xsd` | 1,242 | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |
| S3 `xmlszamlaxml.xsd` | 1,039 | `06cd34ce07ca8f3c0919cf7c4e6505bbda66ddf6b72d60736c849e695f7e19f3` |
| S4 `xmlszamlapdf.xsd` | 1,035 | `b9b161d1356bcd10791605f74c390a0b2b347fdc19a4cf074f76f8a91fe3cfdf` |
| X3 inline | — | `08d56e1318fdabcd40bf4541a11623d8bc6db1343692b3516bad925291a6e7d9` |
| P3 inline | — | `24dcfe7f5ea673907061560a70800bf284aa24112971691803ad74ff99db6953` |
| H1 inline | — | `9d67609c8b565215555e1edfe8f2a3ea815d174e78e551c96da5e9ce04351dc2` |
| H2 inline | — | `1c50375b586ddfeac1867af3c6b3b5427c60ded9772352456274d6ddcbda113c` |
| P2 inline response | — | `3d88ee85e90eeb19237dcc3c66ac8637fa009d3de8d02b53aff2621765c77e6f` |
| A1 inline response | — | `2efe5f0ae7a3212d5504df7bf160ee10a10fe094390ed3d6a1807eb908d050f3` |

Python ElementTree comparison of ordered element tags and attributes (ignoring comments/formatting) found X3=S3, H1=S3, P3=S4, P2=S2 and A1=S1. H2 is not well-formed XML: parse failure at line 1, column 140. No merged or locally patched schema was used for this comparison.

## 2. Request and response-channel inventory

| Surface | Implementation | Review result |
|---|---|---|
| XML action/root/namespace | `src/ops/query_xml.rs:535–557` | Matches X1/X3/S3 exactly. |
| PDF action/root/namespace | `src/ops/query_pdf.rs:58–81` | Matches P1/P3/S4 exactly. |
| Root-level authentication | `src/xml.rs:628–637` | Correct root placement and sequence: username/password **or** agent key. No `beallitasok` wrapper. B1 and both request schemas support it. |
| Number selector | `query_xml.rs:545–547`, `query_pdf.rs:68–70` | `szamlaszam`, retained as caller text and XML-escaped. |
| Order selector | `query_xml.rs:549`, `query_pdf.rs:72` | Exact request spelling `rendelesSzam`; response spelling is separately `rendelesszam`. No silent trim/case fold. |
| External selector | `query_xml.rs:553–555`, `query_pdf.rs:76–78` | `szamlaKulsoAzon` at schema tail. The caller must have set it at creation (X1/P1). |
| Selector exclusivity | `src/types.rs:1034–1053` | Enum emits exactly one selector tag; all strings remain unbounded and may be empty. Schemas impose no length/minLength facet. The enum guarantees tag choice, not a nonblank business identifier. |
| XML PDF switch | `query_xml.rs:60–72,552` | `bool`, default false, emitted explicitly. X3/S3 allow omission or a boolean; false is conforming. |
| PDF response version | `query_pdf.rs:75` | Always `super::RESPONSE_VERSION` = 2. No raw-PDF-v1 parser needed for a request that always asks for v2. |
| Transport envelope | `src/wire.rs:66–99,405–411`; `src/client.rs:374–405` | XML file multipart part under the correct action; POST and content type supplied. XML escaping occurs before multipart assembly. |
| XML success/error dispatch | `query_xml.rs:563–588` | Checks headers/status, then accepts `szamla` or the error envelope in their exact namespaces. A successful `xmlszamlavalasz` is rejected as an unexpected XML-query body. Body-only code 7 is read. |
| PDF response dispatch | `query_pdf.rs:83–93`; `envelope.rs:179–249,275–279` | Shared envelope; requires reported number and decoded PDF, optional totals/balance/URL. Numberless success and code 56 need the qualifications in §7. |
| PDF retention | `query_xml.rs:604–607`; `src/types.rs:104–116,142–175` | Standard base64 decoded to original bytes; whitespace wrapping removed. JSON uses base64. XML missing/blank PDF is `None`, even if requested; PDF-query missing/blank PDF fails. No PDF structural validation is claimed. |
| HTTP/error precedence | `src/wire.rs:291–310` | Nonblank down header → error-code header → non-2xx status → body. Known status without header evidence is not bypassed by success headers. Query body-only 7 at HTTP 200 is typed. |

## 3. Complete `InvoiceDocument` field inventory

**Notation:** `?` in the **schema** column means `minOccurs=0`; otherwise exactly one. All scalar `maxOccurs` are 1 unless explicitly stated. `Opt<T>` means `Option<T>`. Text type is `String` unless another type is named. Every row below was checked against S1 and A1, not inferred from a test fixture. Wire/public positions cite the implementing source.

### Root and reusable blocks

| Schema path/type | Public destination and optionality | Source |
|---|---|---|
| `szallito`, `alap`, `vevo` | Required `supplier: Supplier`, `info: InvoiceInfo`, `buyer: BuyerInfo` | `query_xml.rs:80–86,619–622` |
| `tetelek/tetel` (1..unbounded) | `items: Vec<DocumentItem>`; wrapper required, zero rows accepted | `query_xml.rs:87–88,623,915–919` |
| `qutetek? / qutet` (0..unbounded) | `financial_items: Vec<FinancialItem>`; absent/empty → empty list | `query_xml.rs:89–90,624–625,1000–1004` |
| `cimkek? / cimke?` (max 1 in S1) | `labels: Vec<String>`; accepts more than schema maximum | `query_xml.rs:91–92,626–627,1045–1050` |
| `osszegek` | Required `totals: Totals` | `query_xml.rs:93–94,628`; `xml.rs:835–842` |
| `kifizetesek? / kifizetes` (1..unbounded if wrapper present) | `credit_entries: Vec<RecordedCreditEntry>`; absent/empty → empty list | `query_xml.rs:95–96,629–630,1052–1056` |
| `pdf?` (`string`, not `base64Binary` in S1) | `pdf: Opt<Pdf>`; interpretation follows X2's base64 meaning, not unrestricted string semantics | `query_xml.rs:97–98,604–607,631–632` |
| `cimTipus`: `orszag?`, `irsz`, `telepules`, `cim` | `Address { country: Opt<String>, zip, city, address }` | `query_xml.rs:104–113,636–651` |
| `cimpostaTipus`: `nev?`, `orszag?`, `irsz?`, `telepules?`, `cim?` | `BuyerPostalAddress { name, country, zip, city, address }`, all optional | `query_xml.rs:328–339,803–824` |
| `bankTipus`: `nev?`, `bankszamla?` | `Bank { name, account }`, both optional | `query_xml.rs:118–123,656–669` |

The address types are deliberately different: supplier `postacim` is **`cimTipus`**, with no recipient `nev`; buyer `postacim` is **`cimpostaTipus`**, including optional recipient name. Reusing buyer postal optionality for supplier postal data would misread S1.

### Supplier (`szallito`)

| Schema element/type | Public destination/type | Source |
|---|---|---|
| `id: int` | `id: Opt<i64>` (reader relaxation) | `query_xml.rs:130–131,674–675` |
| `nev: string` | `name: String` | `query_xml.rs:132–133,676` |
| `cim: cimTipus` | `address: Address` | `query_xml.rs:134–135,677` |
| `postacim?: cimTipus` | `postal_address: Opt<Address>` | `query_xml.rs:136–137,678–679` |
| `adoszam: string` | `tax_number: Opt<String>` (blank/absent relaxation) | `query_xml.rs:138–140,680–681` |
| `csoportazonosito?: string` | `group_id: Opt<String>` | `query_xml.rs:141–142,682–683` |
| `adoszameu?: string` | `eu_tax_number: Opt<String>` | `query_xml.rs:143–144,684–685` |
| `bank?: bankTipus` | `bank: Opt<Bank>` | `query_xml.rs:145–146,686–687` |

### Core data (`alap`)

| Schema element/type | Public destination/type | Source |
|---|---|---|
| `id: int` | `id: i64` | `query_xml.rs:232–233,709` |
| `szamlaszam: string` | `invoice_number: InvoiceNumber` | `query_xml.rs:234–235,710` |
| `gazdEsemAzon: int` | `economic_event_id: Opt<i64>` | `query_xml.rs:236–238,711–716` |
| `forras?: int` | `source: Opt<i64>` | `query_xml.rs:239–242,717–718` |
| `iktatoszam?: string` | `registration_number: Opt<String>` | `query_xml.rs:243–244,719–720` |
| `tipus: string` | `document_type: DocumentType` (open token) | `query_xml.rs:245–247,721`; `types.rs:768–858` |
| `eszamla: int` | `appearance: InvoiceAppearance` (open integer code) | `query_xml.rs:248–252,722,196–222` |
| `hivszamlaszam?: string` | `referenced_invoice_number: Opt<InvoiceNumber>` | `query_xml.rs:253–254,723–724` |
| `hivdijbekszam?: string` | `referenced_proforma_number: Opt<InvoiceNumber>` | `query_xml.rs:255–256,725–726` |
| `kelt`, `telj`, `fizh`: `date` | `issue_date`, `fulfillment_date`, `due_date`: `Opt<Date>` | `query_xml.rs:257–264,727–732` |
| `fizmod: string` | `payment_method: Opt<PaymentMethod>` (open token) | `query_xml.rs:265–267,733–734` |
| `fizmodunified: fizmodunifiedTipus` | `unified_payment_method: Opt<String>` | `query_xml.rs:268–270,735–736` |
| `keszpenz: boolean` | `cash_payment: bool`, missing/empty → false (Q-01) | `query_xml.rs:271–272,737–738` |
| `rendelesszam?: string` | `order_number: Opt<String>` | `query_xml.rs:273–274,739–740` |
| `nyelv: nyelvTipus` | `language: Opt<String>` | `query_xml.rs:275–276,741–742` |
| `devizanem: string` | `currency: Opt<Currency>` (arbitrary currency text retained) | `query_xml.rs:277–279,743–744` |
| `devizabank?: string` | `exchange_bank: Opt<String>` | `query_xml.rs:280–281,745–746` |
| `devizaarf?: double` | `exchange_rate: Opt<Decimal>` | `query_xml.rs:282–284,747–748` |
| `megjegyzes?: string` | `comment: Opt<String>` | `query_xml.rs:285–286,749–750` |
| `afatipus?: string` | `vat_type: Opt<String>` | `query_xml.rs:287–288,751–752` |
| `penzforg`, `kata`, `katafokonyv`: `boolean` | `cash_accounting`, `kata`, `kata_ledger`: `bool`, missing/empty → false (Q-01) | `query_xml.rs:289–295,753–758` |
| `email?: string` | `email: Opt<String>` (document-associated email) | `query_xml.rs:296–297,759–760` |
| `teszt: boolean` | `test: Opt<bool>` (does not default absent to live) | `query_xml.rs:298–304,761–762` |
| `sztornozott?: boolean` | `reversed: Opt<bool>` | `query_xml.rs:305–321,763–764` |

All 28 `alap` declarations are present. `eszamla` 0/1/2/3 handling matches A1's quote “0: not an invoice, 1: paper invoice, 2: e-invoice, 3: e-invoice”, independently corroborated by the P73 observations (`docs/szamlazz-hu-behaviour.md:105–107`). It is not the create request's boolean. `forras` is schema-supported even though X1 excludes externally issued invoices from this query surface.

### Buyer and buyer ledger

| Schema path/type | Public destination/type | Source |
|---|---|---|
| `vevo/id?: int` | `buyer.id: Opt<i64>` | `query_xml.rs:370–372,867–868` |
| `vevo/nev: string` | `buyer.name: String` | `query_xml.rs:373–374,869` |
| `vevo/azonosito?: string` | `buyer.identifier: Opt<String>` | `query_xml.rs:375–377,870–871` |
| `vevo/cim: cimTipus` | `buyer.address: Opt<Address>` (relaxed wrapper presence) | `query_xml.rs:378–379,872–873` |
| `vevo/postacim?: cimpostaTipus` | `buyer.postal_address: Opt<BuyerPostalAddress>` | `query_xml.rs:380–381,874–875` |
| `vevo/email?: string` | `buyer.email: Opt<String>` | `query_xml.rs:382–383,876–877` |
| `vevo/adoszam: string` | `buyer.tax_number: Opt<String>` | `query_xml.rs:384–386,878–879` |
| `vevo/csoportazonosito?`, `adoszameu?`: string | `buyer.group_id`, `eu_tax_number`: `Opt<String>` | `query_xml.rs:387–390,880–883` |
| `vevo/lokacio: int` | `buyer.location: Opt<i64>` | `query_xml.rs:391–393,884–885` |
| `vevo/privatePersonIndicator: boolean` | `buyer.private_person: bool`, missing/empty → false (Q-01) | `query_xml.rs:394–395,886–891` |
| `vevo/fokonyv?: fokonyvvevoTipus` | `buyer.ledger: Opt<BuyerLedgerInfo>` | `query_xml.rs:396–397,892–893` |
| `fokonyv/vevo?`, `vevoazon?`: string | `account`, `buyer_id`: `Opt<String>` | `query_xml.rs:346–349,830–833` |
| `fokonyv/datum?: date` | `date: Opt<Date>` | `query_xml.rs:350–351,834–835` |
| `fokonyv/folyamatostelj?: boolean` | `continuous_fulfillment: Opt<bool>` | `query_xml.rs:352–353,836–837` |
| `fokonyv/elszDatTol?`, `elszDatIg?`: date | `settlement_from`, `settlement_to`: `Opt<Date>` | `query_xml.rs:354–357,838–849` |

The numeric buyer `id` and textual `azonosito` are distinct. Buyer and item settlement-date **capitalization differs in the schema**, and the code correctly preserves that distinction.

### Printed items and item ledger

| Schema path/type | Public destination/type | Source |
|---|---|---|
| `tetel/nev: string` | `name: String` | `query_xml.rs:405–406,923` |
| `tetel/azonosito?: string` | `id: Opt<String>` | `query_xml.rs:407–408,924–925` |
| `tetel/mennyiseg: double` | `quantity: Decimal` | `query_xml.rs:409–410,926–927` |
| `tetel/mennyisegiegyseg: string` | `unit: String` | `query_xml.rs:411–412,928` |
| `tetel/nettoegysegar: double` | `unit_price: Decimal` | `query_xml.rs:413–414,929–930` |
| `tetel/afatipus?: afatipusTipus` | `vat_type: Opt<String>` | `query_xml.rs:415–417,931–932` |
| `tetel/afakulcs: double ≥ 0` | `vat_rate_code: String`; interpreted by `vat_rate()` | `query_xml.rs:418–421,456–462,933` |
| `tetel/netto: double` | `net_value: Decimal` | `query_xml.rs:422–423,934–935` |
| `tetel/arresafaalap?: double` | `margin_vat_base: Opt<Decimal>` | `query_xml.rs:424–425,936–937` |
| `tetel/afa`, `brutto`: double | `vat_value`, `gross_value`: `Decimal` | `query_xml.rs:426–429,938–941` |
| `tetel/megjegyzes?: string` | `comment: Opt<String>` | `query_xml.rs:430–431,942–943` |
| `tetel/sztetordering: int` | `ordering: Opt<i64>` | `query_xml.rs:432–433,944–945` |
| `tetel/fokonyv?: fokonyvtetelTipus` | `ledger: Opt<DocumentItemLedger>` | `query_xml.rs:434–435,946–947` |
| `fokonyv/arbevetel?`, `afa?`: string | `revenue_account`, `vat_account`: `Opt<String>` | `query_xml.rs:442–445,973–976` |
| `fokonyv/gazdasagiesemeny?`, `gazdasagiesemenyafa?`: string | `economic_event`, `vat_economic_event`: `Opt<String>` | `query_xml.rs:446–449,977–980` |
| `fokonyv/elszdattol?`, `elszdatig?`: date | `settlement_from`, `settlement_to`: `Opt<Date>` | `query_xml.rs:450–453,981–984` |

### Financial items, labels, totals, credit entries

| Schema path/type | Public destination/type | Source |
|---|---|---|
| `qutet/nev: string` | `FinancialItem.name: String` | `query_xml.rs:475–476,1008` |
| `qutet/afatipus?: afatipusTipus` | `vat_type: Opt<String>` | `query_xml.rs:477–478,1009–1010` |
| `qutet/afakulcs: double ≥ 0` | `vat_rate_code: String`, `vat_rate()` helper | `query_xml.rs:479–480,499–504,1011` |
| `qutet/netto`, `afa`, `brutto`: double | `net`, `vat`, `gross`: `Decimal` | `query_xml.rs:481–486,1012–1017` |
| `qutet/elszdattol?`, `elszdatig?`: date | `settlement_from`, `settlement_to`: `Opt<Date>` | `query_xml.rs:487–490,1018–1021` |
| `qutet/afalevon: int` | `deductible_vat: i64` | `query_xml.rs:491–494,1022–1023` |
| `qutet/cimkek? / cimke?` | `labels: Vec<String>` | `query_xml.rs:495–496,1024–1025,1045–1050` |
| `osszegek/afakulcsossz` (1..unbounded) | `Totals.by_vat_rate: Vec<VatTotal>` (zero accepted) | `xml.rs:835–841`; `types.rs:1061–1065` |
| `afakulcsossz/afatipus?: afatipusTipus` | `VatTotal.vat_type: Opt<String>` | `xml.rs:847–849`; `types.rs:1073–1075` |
| `afakulcsossz/afakulcs: double ≥ 0` | `vat_rate_code: String`, `vat_rate()` helper | `xml.rs:850–851`; `types.rs:1076–1078,1087–1093` |
| `afakulcsossz/netto`, `afa`, `brutto`: double | `net`, `vat`, `gross`: `Decimal` | `xml.rs:852–860`; `types.rs:1079–1084` |
| `osszegek/totalossz` with `netto`, `afa`, `brutto`: double | Required `GrandTotal { net, vat, gross }` | `xml.rs:863–875`; `types.rs:1100–1106` |
| `kifizetes/datum: date` | `RecordedCreditEntry.date: Date` (required, unlike optional core dates) | `query_xml.rs:514–515,1060–1061` |
| `kifizetes/jogcim: string` | `title: PaymentMethod` (open token) | `query_xml.rs:516–520,1062` |
| `kifizetes/osszeg: double` | `amount: Decimal` | `query_xml.rs:521–522,1063–1064` |
| `kifizetes/megjegyzes?: string` | `comment: Opt<String>` | `query_xml.rs:523–524,1065–1066` |
| `kifizetes/bankszamlaszam?: string` | `bank_account: Opt<String>` | `query_xml.rs:525–528,1067–1068` |
| `kifizetes/banktranzid?: int` | `bank_transaction_id: Opt<i64>` | `query_xml.rs:529–530,1069–1070` |
| `kifizetes/devizaarf?: double` | `exchange_rate: Opt<Decimal>` | `query_xml.rs:531–532,1071–1072` |

No schema declaration gives `afalevon` a percentage unit/range; the code correctly retains an integer without assuming one. A1 explains `bankszamlaszam` as the actual sender account **or** the invoice account if unknown; the public documentation correctly preserves that ambiguity.

### Explicitly checked alleged omissions

S1/A1 have **no** queried `fuvarlevel`, carrier sub-blocks (`tof`, `ppp`, `sprinter`, `mpl`), root `arfolyam` object, `torloKod`, buyer telephone/comment/signer, seller email configuration, `simpleItems`, invoice template, external-id echo, or `kintlevoseg` element. These are not missing schema fields. Waybill and `ExchangeRate` request types belong to A2's `xmlszamla` create request. Queried exchange data are instead `alap/devizabank`, `alap/devizaarf` and credit-entry `devizaarf`, all modeled. `JS` is mentioned as a credit note in A1's sample annotation; `DocumentType::Other("JS")` retains it losslessly even though there is no named variant.

## 4. PDF envelope and retained headers

S2/P2 declare `sikeres: boolean` required; all eight following fields optional. All are consumed by the shared envelope:

| Element | Handling and retention | Source |
|---|---|---|
| `sikeres` | Verdict, not a public data field; empty handling is Q-02 | `xml.rs:478–499,769–778` |
| `hibakod`, `hibauzenet` | Typed/open API error and decoded diagnostic on refusal. Missing code is `ErrorCode::Absent`. Malformed optional diagnostic can be discarded without discarding the readable code. | `xml.rs:483–498,506–516` |
| `szamlaszam` | Body first, decoded `szlahu_szamlaszam` fallback; nonblank required by PDF parser | `envelope.rs:123–132,275–279` |
| `szamlanetto`, `szamlabrutto` | `InvoicePdf.net_total`, `gross_total`: optional Decimal, body first/header fallback | `envelope.rs:226–237`; `query_pdf.rs:42–45,88–89` |
| `kintlevoseg` | `InvoicePdf.outstanding`: optional Decimal, absent is not zero | `envelope.rs:238–243`; `query_pdf.rs:46–49,90` |
| `vevoifiokurl` | `InvoicePdf.customer_account_url`: body first/decoded-header fallback | `envelope.rs:137–143`; `query_pdf.rs:50–53,91` |
| `pdf` | Standard base64 bytes; required to produce `InvoicePdf` | `envelope.rs:145–148`; `query_pdf.rs:92` |

Header retention is intentionally **not complete raw-response retention**:

- `RawResponse` retains bytes, status and header pairs; `header()` is case-insensitive and returns the first match (`wire.rs:149–153,183–198,227–249`). The public caller can keep it when using the wire API.
- The bundled `Client::send` returns only the typed result (`client.rs:403–405`), so ignored body fields/header data cannot subsequently be recovered from a successful call.
- PDF queries retain number, net/gross, outstanding and customer-account URL via header fallback. The shared parser also reads `szlahu_id` and `szlahu_fizetesmod`, but `query_pdf.rs:86–93` drops `CreatedInvoice.document_id`, `payment_method` and `notification_delivery_failed` from the PDF projection.
- XML queries check down/error/status headers, but the constructed `InvoiceDocument` uses body fields only (`query_xml.rs:590–608`). There is no header-only outstanding/customer-account-URL field. The body already supplies document id and payment method; it does **not** supply outstanding or customer-account URL.
- Numeric headers use raw text, HTTP space/tab trimming, comma→dot, and the exact numeric reader (`envelope.rs:351–370`). Encoded textual headers decode once: `+`→space, percent escapes→UTF-8 (`wire.rs:343–350`). XML URL text is not form-decoded. Nonblank body values take precedence; malformed numeric body text fails instead of falling back. Blank body values allow fallback.

**Assessment:** no PDF-envelope field is missing. A3 documents payment-method headers on **creation**; P2 only says extra parameters may arrive. Neither fresh query page enumerates query-specific headers. Thus dropping PDF id/payment method and XML outstanding/URL is a confirmed implementation limitation, but query emission/required typed exposure is **unresolved**, not a schema-confirmed missing-field defect. Historical `docs/szamlazz-hu-behaviour.md:89,155` says outstanding is observable via headers and lists success headers on create/storno/credit; it does not supply a captured query-header inventory.

## 5. Findings

### Q-01 — P2: absent document booleans become asserted negative facts

**Exact locations:** `src/ops/query_xml.rs:737–738,753–758,886–891`; public nonoptional fields at `:271–272,289–295,394–395`; conversion at `:784,792–794,909`; shared empty conversion `src/xml.rs:769–778`.

**Primary evidence:** [S1](https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd) declares, for example:

```xml
<element name="katafokonyv" type="boolean" maxOccurs="1" minOccurs="1"></element>
<element name="privatePersonIndicator" type="boolean" maxOccurs="1" minOccurs="1"></element>
```

There is no `default="false"`. Nevertheless [X2's official successful query example](https://docs.szamlazz.hu/agent/querying_xml/response) omits `keszpenz`, `katafokonyv` and `privatePersonIndicator` altogether. [A1](https://docs.szamlazz.hu/penzugyi-adatkapcsolat/kimeno-szamlak) describes `privatePersonIndicator` as “customer is a private individual” and `katafokonyv` as whether, in the issuer's opinion, the invoice should receive KATA accounting treatment. [W1](https://www.w3.org/TR/xmlschema-2/#boolean) permits only `{true, false, 1, 0}`, not empty text.

**Observed implementation:** `#[serde(default)]` invents `false` on omission, and `flexible_bool` also returns false for empty text. This affects `cash_payment`, `cash_accounting`, `kata`, `kata_ledger` and `buyer.private_person`. An omitted indicator and an explicit negative indicator produce identical public/JSON data. In contrast, `test`, `reversed` and buyer-ledger `continuous_fulfillment` preserve unknown presence via `Option<bool>`.

**Reproduction:** in an otherwise complete synthetic response, start with `<keszpenz>true</keszpenz>`. Removing it, or replacing it with `<keszpenz/>`, both parsed successfully with `info.cash_payment == false`. The same default/helper is used at the other four locations. The official sparse example establishes actual documentation-level omission; the synthetic mutation establishes current parser behavior, not live omission frequency.

**Impact:** a consumer serializing queried accounting/private-person indicators cannot distinguish “not reported” from “reported false”. P2 because this is silent semantic loss in accounting data, not a query crash. The current model is complete by field name but not by reported-presence semantics.

**Recommendation:** preserve absence as `Option<bool>` (and choose an explicit empty-value policy), or obtain and document a vendor guarantee that omission means false for each affected field. Merely accepting the sparse example is justified; asserting a negative value is not justified by it. No checked live observation establishes this false-default rule.

### Q-02 — P3: an empty success verdict is classified as a vendor refusal

**Exact locations:** `src/xml.rs:479–482,506–516,769–778`; used by XML error-envelope parsing at `src/ops/query_xml.rs:580–586` and PDF parsing through `src/ops/envelope.rs:289–294`.

**Primary evidence:** [S2](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) requires `<element name="sikeres" type="boolean" maxOccurs="1" minOccurs="1">`; [P2](https://docs.szamlazz.hu/agent/querying_pdf/response) says errors contain `<sikeres>false</sikeres>`, and [W1](https://www.w3.org/TR/xmlschema-2/#boolean) lists the four valid lexical literals.

**Executed reproduction:** parse this with `QueryInvoicePdf::new(InvoiceSelector::InvoiceNumber("I-1".into()))`, HTTP 200/no error headers:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres/>
  <szamlaszam>I-1</szamlaszam>
  <pdf>JVBERi0=</pdf>
</xmlszamlavalasz>
```

Result: `szamlazz.hu error absent: `, i.e. `ResponseError::Api` with an absent code. An empty fact has been turned into `false` before `Verdict::api_error`. Whitespace-only content follows the same branch. A missing tag or unrelated invalid token instead fails deserialization.

**Impact:** malformed upstream verdicts appear as known negative verdicts rather than parse failures, obscuring protocol diagnostics. Low severity: this is not false success, `ErrorCode::Absent` remains outcome-unknown, and no live empty verdict was observed.

**Recommendation:** use the existing required-boolean lexical reader (`src/xml.rs:759–766`) for envelope `sikeres`. Do not infer a vendor decision from empty content. Existing tolerant test definitions are not evidence for that interpretation.

## 6. Dates, numerics, namespaces and justified deviations

### Evidence-backed deviations from a literal schema model

1. **Sparse official query responses:** X2/H3 omit schema-required `gazdEsemAzon`, several booleans, buyer `lokacio`, and item `sztetordering`. Optional representations avoid refusing the official sample. `gazdEsemAzon`, `location`, `ordering` preserve absence; Q-01 identifies the different consequence of false-default booleans.
2. **Empty business values:** X2/H3 show `<bankszamla></bankszamla>`, `<adoszam></adoszam>` and empty ledger strings. Optional business text treating XML-whitespace-only values as absent is reasonable. Nonblank decoded characters, including padding/NBSP, are retained (`xml.rs:745–756`, `query_xml.rs:1089–1096`). This is a semantic model, not lossless source XML.
3. **Open token sets:** X2 itself uses `fizmodunified=other`, outside S1's Hungarian enum. Retaining arbitrary `String`, `PaymentMethod::Other` and `DocumentType::Other` is necessary for forward compatibility. No known token is silently mapped to a different one. `afatipus` precedence over numeric `afakulcs` matches A1's special-VAT examples.
4. **Reversal and credit-entry absence:** historical B1/B8 observations (`docs/szamlazz-hu-behaviour.md:86–89`) support absent `sztornozott` on live originals, true after reversal, no marker on the storno itself, and disappearing credit entries on reversed originals. The empty-list/optional-marker model supports these observations. New clearing evidence (`docs/research/2026-09-11-credit-clearing-live.md:20–30,64–73`) supports empty queried credit entries after explicit clearing but does not record raw response channels.
5. **Buyer data may change:** `docs/szamlazz-hu-behaviour.md:120` records updated partner data on a later query. `query_xml.rs:360–365` correctly avoids promising an issuance-time snapshot; `alap/email` stays distinct from `vevo/email`.
6. **Exact selectors:** historical observations at `docs/szamlazz-hu-behaviour.md:49–54,72–80` corroborate case-sensitive/exact order queries, newest shared external-id holder, consumed/deleted proforma code 7 and no external-id echo. None requires adding a response external-id field absent from S1.
7. **Monetary header comma:** historical P60-E1/E3 at `docs/szamlazz-hu-behaviour.md:170` records `szlahu_nettovegosszeg=100,01`. Supporting comma headers is justified. It does not justify comma in XML double text, which is correctly refused.

### Explicit local policies, not vendor-proven domain guarantees

- **Integers:** S1 currently uses `int` for every document integer. All are widened to signed `i64`, not narrowed or made unsigned (`query_xml.rs:10–25`). This accepts the full XSD-int range, plus extensions. Scratch parsing accepted padded `+002` for id and `+001` for appearance. Above-`i64` text is not supported; S1 does not require it.
- **Decimal instead of IEEE double:** all money/quantity/rate fields use exact finite `Decimal` (`xml.rs:647–665`, `number.rs:63–140`). Scientific notation, optional signs, leading/trailing zeroes, dot fractions and exact representable values work. Overprecision, underflow, overflow, `INF`/`-INF`/`NaN` fail instead of rounding. This deliberately supports less than S1/S2's unrestricted double domain; `README.md:441–446` states the policy. Scratch `1e-2` became `0.01`; `1e-29` failed. Sensible for financial integrity, but no vendor source guarantees every future query fits Decimal. Tests cannot establish that guarantee.
- **Numeric VAT text:** `afakulcs` is retained as text at printed rows, financial rows and subtotals, despite S1's nonnegative-double restriction. Scratch `junk` is retained and `vat_rate()` returns `Other("junk")`; negative/nonfinite/unrepresentable text is not uniformly gated as an XSD numeric value. This is permissive response interpretation, not full XSD validation. The original token remains available, avoiding silent numeric truncation.
- **Dates:** all 11 date positions use shared civil-date parsing: three `alap`, three buyer-ledger, two item-ledger, two financial-item, one credit-entry date (`xml.rs:667–726`). Ten are optional; credit-entry `datum` is required. Complete `Z`/`±hh:mm` suffixes are accepted through ±14:00 and discarded without shifting the printed date. Impossible dates, partial/junk suffixes and invalid offsets fail; UTF-8 slicing is checked. Optional blank dates become `None`; nonblank malformed dates fail the query. **Do not import Adatkapcsolat's invalid-date→None behavior into this crate.**
- **Legacy date domain:** the initial Jiff parse (`xml.rs:671–674`) also accepts spellings beyond XSD `date`; scratch `2026-09-11T12:34:56` became `2026-09-11`. The parser refuses valid XSD BCE spelling `-0001-09-11` and year `10000-09-11`; tests retain Jiff year-zero/basic-date/signed-six-digit-year spellings. `README.md:249–253` explicitly says this is not strict XSD lexical validation. This is a documented local compatibility domain, not evidence those spellings occur on invoices. No practical present-day query failure was demonstrated from these boundaries.
- **Cardinality/order relaxation:** absent optional list wrappers become empty vectors; required item/subtotal wrappers may hold zero rows; both label lists accept multiple `cimke` although S1 says max 1. Known fields can arrive out of sequence; unknown well-formed children are ignored. These extensions do not lose data from a schema-conforming response, but neither the source nor existing tests establish live emission of every extended form.
- **Required strings:** missing required names/identity/unit fields fail, but empty `String` fields—including an empty queried invoice number—are accepted where the XSD string has no `minLength`. The PDF envelope instead requires a nonblank number. This is an intentional difference in model gates, not an XSD omission.
- **PDF policy:** standard base64 accepts wrapped text and retains bytes. `Pdf` is a byte wrapper, not a PDF syntax checker. Unicode whitespace removal is broader than XSD whitespace. S1 declares PDF as string, but X2 describes base64; malformed nonblank base64 fails the full XML query. Optional missing/blank PDF still returns the document, which can be re-queried for the artifact. P2's PDF operation fails without the artifact.

### Namespace and XML-shape review

`src/xml.rs:201–275` validates one complete expected root through EOF, rejects wrong/missing root namespaces, truncation, extra roots, illegal outside text, DTDs and misplaced declarations. `NamespaceReader` checks declared prefixes, reserved bindings and duplicate expanded attributes (`:46–153`); the lexical tokenizer adds XML name/character/reference checks (`:280–300`). `protocol_text` canonicalizes equivalent protocol prefixes and suppresses whole foreign subtrees (`:310–377`) before serde field matching, retaining an unknown-child marker so mixed text cannot become a fabricated scalar.

The schema's qualified-element requirement is respected independently of prefix spelling. Parent paths remain significant: a nested foreign/unknown reversal cannot set `alap/sztornozott`. Unknown extensions are ignored only after complete XML checking. Scratch malformed multi-colon element/attribute QNames and a local name starting with a digit were refused. Existing prefix/list tests are useful coverage, not the basis for accepting foreign elements as protocol facts.

No namespace-specific defect was reproduced in the reviewed query paths. The parser is not an XSD validator and does not preserve namespace declarations/schema-location metadata in the public document.

## 7. Source conflicts and unresolved questions

### U-01: Hungarian PDF-request XSD conflicts with working sources

[H2](https://docs.szamlazz.hu/hu/agent/querying_pdf/xml) publishes this malformed adjacency:

```xml
targetNamespace="http://www.szamlazz.hu/xmlszamlapdf"xmlns:tns="http://www.szamlazz.hu/xmlszamlapdf"
```

It also declares `szamlaszam minOccurs="1"` and places `rendelesSzam` **after** `valaszVerzio`. P3 and S4 instead make `szamlaszam` optional and put `rendelesSzam` **before** `valaszVerzio`; P1 documents order/external-id alternatives. The implementation follows the two agreeing, well-formed sources, which is justified. Do not “fix” the writer to the malformed Hungarian schema or claim universal source agreement. Ask the vendor to correct the page and identify the effective schema. H1's empty boolean sample is likewise invalid against its own XSD; emitting false rather than copying that placeholder is correct.

### U-02: numberless successful PDF query

P2/S2 make `szamlaszam` optional across success/error envelopes and only say headers “may” arrive. `envelope.rs:275–279` requires a nonblank reported number even when a valid PDF is present. Scratch `<sikeres>true</sikeres><pdf>JVBERi0=</pdf>` returned `missing szamlaszam in response`.

The published success example contains a number; there is no live numberless PDF success in the checked observations. Clarify whether every successful PDF query guarantees that number in body or header. This is a **schema-permitted shape versus operation-success invariant question**, not a demonstrated vendor incompatibility. Do not substitute the requested order/external id as a reported invoice number.

### U-03: query-specific headers and raw evidence retention

Obtain an operation-specific header contract or captured XML/PDF query headers before declaring `InvoicePdf.payment_method`, `InvoicePdf.document_id`, or XML-query outstanding/URL mandatory model additions. Raw-response retention and typed exposure are separate design choices (§4). Existing tests explicitly checking those fields are absent from PDF JSON (`tests/response_headers.rs:385–387`) are implementation expectations, not evidence that the vendor never sends them.

### U-04: numbered code 56 on a PDF query

PDF parsing reuses issuing-operation handling: with code 56, a usable number and PDF, `envelope.rs:179–247` returns a document and `query_pdf.rs:86–93` drops the notification-failure flag. Scratch `<sikeres>false</sikeres><hibakod>56</hibakod><szamlaszam>I-1</szamlaszam><pdf>JVBERi0=</pdf>` therefore returned ordinary `InvoicePdf` success. P2 describes false verdicts as errors and does not document a query notification operation/exception. No observed query code 56 exists in the checked records; even creation code 56 could not be triggered (`docs/szamlazz-hu-behaviour.md:163,190–191`).

Clarify whether this shared-parser exception is intended for read-only queries. A separate PDF verdict policy would avoid silently discarding a warning if this anomalous combination arrives. This is a reproduced defensive edge case with unestablished vendor reachability, not evidence a normal PDF query sends notifications.

### U-05: finite value domains and boolean absence

The schemas provide no financial magnitude/scale bound or positive-year date restriction for responses. Current local Decimal/Jiff policies are explicit and conservative, but do not establish full XSD-domain conformance. Separately, ask what omitted `keszpenz`, `katafokonyv` and `privatePersonIndicator` mean in X2's own sparse example (Q-01). No existing local test or unsigned clarification draft supplies those answers.

### Official example defects are not parser defects

- X2/H3's PDF is literal prose: “The receipt .pdf can be found here in BASE64 encoding”. P2/H4's encoded PDF contains `....`. Neither is executable valid base64 as published. Rejecting them is correct; replacing the placeholder with synthetic bytes tests other fields but does not recover an official PDF.
- X2 sends `fizmodunified=other` and omits required S1 fields. This justifies a lenient/open model, not a false-default rule for omitted facts.
- X2 links to A2 when referring to the full `szamla` schema. A2's `xmlszamla` schema is a different root and direction; importing its request-only waybill fields into a missing-query-field list would be erroneous.

## 8. Associated test coverage reviewed

| Source | Query-relevant coverage and limit |
|---|---|
| `src/ops/query_xml.rs:1098–1845` | Canonical request and all selectors; body errors; special VAT precedence; sparse totals; basic and all-section response mapping; 11 date positions; invalid/empty dates; JSON round-trip; appearance/reversal/test markers; UTF-8/root errors; embedded PDF. The “every … field” comment at `:1300–1301` is a fixture claim, independently checked here against S1. |
| `src/ops/query_pdf.rs:97–187` | Golden request, order/external-id selectors, success/error, PDF JSON round-trip, missing PDF. |
| `src/ops/envelope.rs:373–725` | Shared success/56/error/status table; body/header precedence; optional metadata and PDF failures. Issuance/reversal assertions do not establish query-specific 56 semantics. |
| `src/xml.rs:909–1113` | Writer escaping/order, expected roots/namespace, verdict table and header-first handling. |
| `tests/business_text.rs:9–47` | Decoded optional invoice text, XML references/line endings, NBSP, whitespace-only absence, order/reference preservation. |
| `tests/numeric_fidelity.rs:49–186` | Numeric VAT token retention; financial-row special-code precedence; scale retention; exact representable exponents and refusal of unrepresentable numbers. |
| `tests/response_namespaces.rs:16–357` | Repeated rows under equivalent aliases/interleaved extensions; invoice identity/reversal isolation; foreign verdicts; undeclared/reserved bindings; expanded-attribute uniqueness; scalar anti-splicing; singleton checks. PDF path is covered indirectly through the shared storno envelope, not every test's direct request type. |
| `tests/response_completion.rs:12–193` | Complete roots/prolog/epilog, lexical checking even in ignored subtrees, bounded diagnostics. Direct XML query and shared-envelope entry points. |
| `tests/response_headers.rs:148–602` | Error/status order, textual decoding, comma monetary headers across PDF/storno/credit, PDF outstanding/URL fallback and JSON defaults, 56 evidence rules. PDF numberless failures are asserted, not independently justified by those assertions. |
| `tests/upstream.rs:294–310,426–429,547–597,685–833,1140–1150` | Historical official request/response samples, named placeholder repair, value assertions and request outline comparison. The corpus may skip when absent; its historical data is not today's source acquisition. Outline comparison ignores empty placeholders/attributes and is not XSD validation. |
| `tests/golden/xmlszamlaxml.xml`, `xmlszamlapdf.xml` | Project-authored writer expectations, exercised by unit tests; not primary protocol evidence. |
| `tests/schema_requests.rs:58–90,537–558`; `scripts/check-agent-schemas.py:88–174` | Generated requests for both auth forms, all three selectors and both XML PDF settings; runner validates separate cached EN/download schemas with negative controls and path accounting. Request-only, not `InvoiceDocument` response validation; not executed in this review. |
| `tests/live.rs:44–168` | Defined invoice/proforma lifecycle assertions: query by number and external id, embedded PDF, totals, credit entries, reversal and consumed/deleted absence. Read as test source only; no execution claimed. |
| `tests/probes.rs` and `tests/live_support/mod.rs` query uses | Supporting query observations/cleanup for mutations. Execution claims came only from the dated behavior/research records, not the presence of a probe definition. |
| `src/types.rs` value tests and wire/client tests | Supplementary shared type/transport checks. Complete transport behavior outside query response ingestion is not re-audited as a separate surface here. |

**Coverage gaps relevant to this report:** unknown-versus-false preservation (Q-01); empty required verdict classification (Q-02); live query-specific header inventory; numberless PDF success guarantee; XSD-wide response scalar-domain validation. Existing all-section fixtures alone cannot settle those gaps.

## 9. Independently executed scratch checks

Commands (no account/network calls from the Rust program):

```sh
python3 /tmp/opencode/queries-eec57fc/sources.py
cargo run --offline --manifest-path /tmp/opencode/queries-eec57fc/Cargo.toml
python3 /tmp/opencode/queries-eec57fc/full.py | cargo run --offline --manifest-path /tmp/opencode/queries-eec57fc/Cargo.toml -- --stdin
```

The Python scripts GET only public docs/XSDs. The Rust program uses the repository crate as a path dependency with **no client feature**, builds in its own scratch target directory, and calls `AgentRequest::parse` / `write_xml` with synthetic data and dummy credentials. Resolved core versions matched the repository lock for the relevant readers: quick-xml 0.42.0, Jiff 0.2.35, rust_decimal 1.43.0, serde 1.0.229, serde_json 1.0.151. This was a scratch run, not `cargo test` or a claim that the entire workspace dependency graph was locked identically.

| Experiment | Observed result |
|---|---|
| Fresh schema inventory | 134 descendant paths; all four schemas self-contained; inline/download structural comparison as in §1. |
| Populate every S1 declaration recursively | Parsed; all supplier/core/buyer/item/financial/label/total/credit/PDF destinations retained their synthetic values in printed JSON. All dates used `2026-09-11+02:00` and yielded `2026-09-11`. |
| Required id and appearance ` +002 ` / ` +001 ` | Accepted as integer 2 / appearance 1. |
| Quantity `1e-2` / `1e-29` | `0.01` / parse failure, no underflow-to-zero. |
| Date `2026-09-11junk` | Parse failure. |
| Date `2026-09-11T12:34:56` | Accepted as the printed date by the legacy Jiff branch. |
| Dates `-0001-09-11`, `10000-09-11` | Parse failures; confirms local date-domain limitation. |
| Omitted/empty `keszpenz` | Successful document with `cash_payment=false` (Q-01). |
| `afakulcs=junk` | Retained as text; helper returned `Other("junk")`. |
| Blank / wrapped / malformed XML PDF | `None` / exact bytes `%PDF-` / base64 failure. `%PDF-` is a synthetic byte marker, not a complete PDF. |
| Malformed element/attribute QNames in ignored extensions | Parse failures, including multiple colons and invalid local-name start. |
| PDF header balance and opaque URL | `1,5` became `1.5`; URL decoded once; id/payment-method omitted by public PDF projection. |
| PDF numberless / numbered 56 / empty verdict | Missing-number error / ordinary PDF success / absent-code API error, respectively (§5/§7). |
| All selector writers with `<`, `&` and padded order text | Correct escaped tag text and sequence; padding preserved; credentials at root; v2 on PDF, false PDF switch on default XML. |

### Overall conclusion

The response model is **element-complete against today's published `szamla` schema**, and the query writers align with the agreeing English/download request contracts. Completeness should be stated as typed document coverage with deliberate parsing policies, not lossless HTTP/XML retention or acceptance of every XSD value. Preserve uncertainty in absent document booleans, require a real lexical envelope verdict, and resolve the specific vendor/header questions without turning synthetic expectations into claimed live guarantees.
