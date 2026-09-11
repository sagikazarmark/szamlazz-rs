# Számla Agent receipt conformance review — 2026-09-09

## Verdict and scope

Reviewed all four receipt operations at commit `a804c740eb8446211c1cdca3eea4fb93d298d25d`, against **freshly fetched official documentation**, including the current inline schemas and separately downloaded XSDs. The working tree was clean at the start.

**Five findings:** two medium-severity implementation gaps, one low-severity lossy projection, one medium-severity recovery-documentation defect, and one low-severity rounding-documentation defect. No high/critical finding. Automatic MNB rate lookup on receipts is an **unverified extension**, not a demonstrated server failure. The request field inventory, operation routing, schema order, ordinary response parsing, PDF decoding and send verdict all otherwise conform within the explicitly stated limitations below.

Production scope: `crates/szamlazz-agent/src/ops/receipt.rs`, receipt-relevant `item.rs`, `types.rs`, `xml.rs`, `wire.rs`, `error.rs`, credential serialization and `client.rs::send`. Test/corpus scope: receipt unit tests, `tests/upstream.rs`, synthetic/golden receipt XML, `fixtures/SOURCES.md`, and `fixtures/upstream/agent`. Read all of `docs/szamlazz-hu-behaviour.md`.

No production edits, account calls, issuing, reversal, email sending, or live tests. No delegated agents. The only repository artifact added by this review is this report.

Severity: **medium** = a supported integration can lose an answer, misclassify a documented outcome, or receive unusable recovery advice; **low** = limited data fidelity or documentation drift. Confidence distinguishes a reproduced local result from an unobserved vendor behaviour.

## Sources inspected and freshness

All URLs below were fetched on **2026-09-09**. The currently linked request/XML/response pages report footer version **v202608271632**. The still-accessible separate `/xsd` pages and `/other` report **v202606031507**. A successful fresh fetch is not evidence that every URL is maintained at the same revision.

### Operation prose, examples and inline XSDs

| ID | URLs inspected | Coverage |
|---|---|---|
| C1 | https://docs.szamlazz.hu/agent/generating_receipt/request | Create endpoint, multipart field, PDF template, HUF rules, NAV notice |
| C2 | https://docs.szamlazz.hu/agent/generating_receipt/xml | Current create example **and** inline request XSD |
| C3 | https://docs.szamlazz.hu/agent/generating_receipt/response | Create response prose, 336–340, full receipt example and inline response XSD |
| C4 | https://docs.szamlazz.hu/agent/generating_receipt/xsd | Legacy standalone request-XSD page; compare with C2 |
| S1 | https://docs.szamlazz.hu/agent/reversing_receipt/request | Storno request, required receipt-number selector |
| S2 | https://docs.szamlazz.hu/agent/reversing_receipt/xml | Current storno example and inline request XSD |
| S3 | https://docs.szamlazz.hu/agent/reversing_receipt/response | SN response, PDF, missing/already-reversed/storno-original failures |
| S4 | https://docs.szamlazz.hu/agent/reversing_receipt/xsd | Legacy standalone request-XSD page |
| Q1 | https://docs.szamlazz.hu/agent/querying_receipt/request | Query routing and two supported selectors |
| Q2 | https://docs.szamlazz.hu/agent/querying_receipt/xml | Current example, order selector, call identifier, template, inline XSD |
| Q3 | https://docs.szamlazz.hu/agent/querying_receipt/response | Explicit reuse of create response format |
| Q4 | https://docs.szamlazz.hu/agent/querying_receipt/xsd | Legacy standalone request-XSD page |
| E1 | https://docs.szamlazz.hu/agent/sending_receipt/request | Send routing; already-issued receipt prerequisite |
| E2 | https://docs.szamlazz.hu/agent/sending_receipt/xml | Current send example, email fallback, optional email block, sequence XSD |
| E3 | https://docs.szamlazz.hu/agent/sending_receipt/response | Send success/failure examples and inline response XSD |
| E4 | https://docs.szamlazz.hu/agent/sending_receipt/xsd | Legacy standalone request-XSD page |
| HU1 | https://docs.szamlazz.hu/hu/agent/querying_receipt/xml | Hungarian cross-check of selectors/call identifier |
| HU2 | https://docs.szamlazz.hu/hu/agent/sending_receipt/xml | Hungarian cross-check of missing email details versus missing block |

### Rules and shared protocol

| ID | URL inspected | Coverage |
|---|---|---|
| R0 | https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules | Current rule-page inventory |
| R1 | https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number | Receipt-specific repetition toggle, request/response order number |
| R2 | https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template | A/N/J/L, omission/default |
| R3 | https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code | `torloKod`, 0–400, account feature |
| R4 | https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts | HUF exact sum, whole gross, net/VAT precision, tolerance, 261/363–365 |
| R5 | https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies | Receipt and invoice currency codes and exchange-rate requirements |
| HU3 | https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/currencies | Hungarian exchange-rate requirement cross-check |
| OLD | https://docs.szamlazz.hu/agent/generating_receipt/other | Legacy rules; conflicts with R1 on account toggles |
| G1 | https://docs.szamlazz.hu/agent/basics/error-handling | Errors, retry limits, plain-text operations, receipt amount errors |
| G2 | https://docs.szamlazz.hu/agent/basics/authentication | Agent key or username/password |
| G3 | https://docs.szamlazz.hu/agent/basics/sending-requests | One XML per document, HTTPS POST, multipart names, validation |
| I1 | https://docs.szamlazz.hu/agent/generating_invoice/xml | **Only** the contrast for automatic MNB lookup: annotation expressly about invoice creation |
| W1 | https://www.w3.org/TR/xmlschema-2/#date | Meaning of the vendor's `xs:date`; §3.2.9.1, plus string/boolean/double/whiteSpace definitions in the same fetched specification |

### Downloaded schemas and broken links

These six working URLs returned XML and were inspected in full; none imports/includes an additional schema:

| ID | URL | Comparison with current inline schema |
|---|---|---|
| X1 | https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd | **Stale:** lacks C2's `tetel/torloKod` |
| X2 | https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd | Same operative fields, types, multiplicities and sequence as S2 |
| X3 | https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd | **Stale:** lacks Q2's `rendelesSzam` |
| X4 | https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd | Same operative schema as E2; fewer explanatory comments |
| X5 | https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd | Same response fields; additionally enumerates VAT token `TEHK`, absent from C3's inline enumeration |
| X6 | https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd | Same operative response schema as E3 |

The following literal schema locations from examples were also checked, via HTTPS, and returned **404**:

- https://www.szamlazz.hu/docs/xsds/nyugtast/xmlnyugtast.xsd
- https://www.szamlazz.hu/docs/xsds/nyugtaget/xmlnyugtaget.xsd
- https://www.szamlazz.hu/docs/xsds/nyugtasend/xmlnyugtasend.xsd
- https://www.szamlazz.hu/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd

Initial guesses https://docs.szamlazz.hu/agent/generating_receipt and https://docs.szamlazz.hu/agent/reversing_receipt returned **403**; their actual `/request`, `/xml`, `/response` pages were successfully fetched, so this did not leave operation coverage missing.

**Source precedence:** the current pages explicitly say “The sent XML file must comply with the following XSD schema.” Prefer their inline schema and operation-specific prose over demonstrably stale downloads and contradictory sample comments. The cached create XSD already contains `torloKod` (`fixtures/upstream/agent/xsd/xmlnyugtacreate.xsd:41–47`), unlike today's X1 download; the cached query XSD already contains `rendelesSzam` (`xmlnyugtaget.xsd:14–20`). `fixtures/SOURCES.md:103–108,124–135` documents the query drift but does not fully describe the create-XSD discrepancy. Do not overwrite either cached schema blindly from its download URL. No byte-for-byte freshness assertion is made for the whole cached corpus.

## Field-by-field request comparison

Notation: **R** = XSD `minOccurs=1`; **O** = `minOccurs=0`; unmarked scalar maximum is 1. `string` means XML Schema string, not a closed token list. Code line references in this report are repository-relative; `receipt.rs` abbreviates `crates/szamlazz-agent/src/ops/receipt.rs`, and other source basenames are in the same crate's `src/`.

### Shared transport and settings

| Surface | Official contract | Implementation and result |
|---|---|---|
| Target/body | C1/S1/Q1/E1, G3: HTTPS POST `https://www.szamlazz.hu/szamla/`, one XML file in multipart | `wire.rs:14,66–100,380–393`; `client.rs:262–288`: **pass**, exact target and file-part encoding, no operation-level retry loop |
| Multipart names | `action-szamla_agent_nyugta_create`, `_storno`, `_get`, `_send` | `receipt.rs:171,314,387,466`: **pass**; storno action is not inferred from root suffix `st` |
| XML declaration/root | UTF-8, exact operation root, corresponding `http://www.szamlazz.hu/<root>` namespace | `receipt.rs:205–208,318–321,391–394,470–473`; `xml.rs:21–40`: **pass**; namespace stays HTTP even over HTTPS transport |
| Authentication | All four: O string `felhasznalo`, `jelszo`, `szamlaagentkulcs`; G2 requires one usable credential method | `xml.rs:251–260`: **pass**, key alone or username then password; escaped as XML text |
| PDF request | Create/storno/query: R boolean `pdfLetoltes`; send: no such field | `receipt.rs:131–133,163,211,291–293,306,324,363–365,379,397,474`: **pass**; explicit `false` constructor default, not omission of a required tag |
| Response version | No receipt request schema has `valaszVerzio` | **Pass**, none emitted; receipts use structured XML, including PDF as base64, without the invoice version selector |
| Additional fields | No receipt buyer/seller, issue-date override, invoice-kind flags, invoice external id or attachments | **Pass**, not borrowed from invoice operations; default `multipart_files()` is empty |
| XML escaping/characters | Proper XML text | `xml.rs:209–237`; `wire.rs:387–415`: **pass** for escaping, non-scientific decimal output and XML 1.0 forbidden-character refusal through `to_wire` |

### Create (`xmlnyugtacreate`)

C2 uses `all` for the root, settings, header, item and individual payment; only repeated item/payment collections use `sequence`. The prose nevertheless asks for fixed order. The writer follows the example's order, including **bank before rate**, which is valid under `all`; this is not a sequence defect merely because the XSD lists rate first.

| Wire path | Type/presence/default/semantics | Code / assessment |
|---|---|---|
| Root `beallitasok`, `fejlec`, `tetelek` | R containers | `receipt.rs:209–258`: present |
| `fejlec/hivasAzonosito` | O string; C3 recommends unique identifier; reuse → 338 rather than original success | `109,154,214`: optional, omitted by default, sent unchanged; correctly documented at `89–107` |
| `fejlec/elotag` | R string; uppercase letters/numbers, receipt-only prefix (336/337) | `112,155,215`: represented and emitted; format/account uniqueness left to server |
| `fejlec/fizmod` | R string, free text | `114,216`; `types.rs:604–647`: `PaymentMethod` emits known tokens, `Other` supports every other spelling |
| `fejlec/penznem` | R string; HUF/Ft aliases and R5's currencies | `116,217`; `types.rs:369–424`: open `Currency`, no supported currency blocked by enum |
| `fejlec/devizabank`, `devizaarf` | O string/double structurally; R5 requires both for non-HUF/Ft | `117–120,189–199,218–223`: explicit bank/rate correct; **MNB omission extension is Q-A below** |
| `fejlec/megjegyzes` | O string, printed free text | `122,224`: represented, omitted by default |
| `fejlec/pdfSablon` | O string; A/N/J/L; omitted/empty → A4, invalid → default per C2 | `23–57,124,225–227`: exact token mapping; `Other` preserves unknown input |
| `fejlec/fokonyvVevo` | O string | `127,228`: represented |
| `fejlec/rendelesSzam` | O string, order number at end of header | `130,229`: correct case and placement; server's separate receipt toggle is not an XML field |
| `tetelek/tetel` | R, 1..unbounded | `134–136,175–177,231–258`: empty vector refused; every row emitted |
| `tetel/megnevezes`, `azonosito` | R/O string | `237–238`: name then optional item id |
| `tetel/mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | R double, string, double | `239–241`: quantity, unit, unit price correctly named |
| `tetel/afakulcs` | R string; numeric percentage or special code | `242`; `types.rs:178–358`: complete representability, open token set; no forced obsolete code translation |
| `tetel/netto`, `afa`, `brutto` | R double, **not** invoice's `*Ertek` names; R4 adds HUF constraints | `243–245`: correct names and values. Local arithmetic validation is intentionally delegated; see F-05 and limitations |
| `tetel/fokonyv` | O container, O string `arbevetel` then `afa` | `246–251`: both supported, including empty optional ledger |
| `tetel/megjegyzes` | O string, after ledger in example | `252`: supported |
| `tetel/torloKod` | O int ≥0; R3 caps at 400 | `181–188,253–255`; `item.rs:107–119`: `Option<u32>` with ≤400 gate, therefore within signed XSD int range; absent and zero distinct |
| `kifizetesek/kifizetes` | O collection; if present 1..unbounded | `137–141,259–269`: omitted for empty vector; no artificial five-payment cap inherited from invoice credit entries |
| Payment `fizetoeszkoz`, `osszeg`, `leiras` | R string, R double, O **string** | `59–87,263–265`: correct. C2 example's `leiras` “double” comment is a vendor typo; schema and example text say string |
| Payment total | When supplied, payments sum to receipt gross; 340 otherwise | `137–139`: intentionally left to server and expressly documented |

### Storno (`xmlnyugtast`)

S2/X2 use `sequence` throughout. `receipt.rs:317–335` emits exactly:

1. R `beallitasok`: optional credential tags in schema order, R boolean `pdfLetoltes`.
2. R `fejlec`: R string `nyugtaszam`, O string `pdfSablon`, O string `hivasAzonosito`.

All fields represented (`288–309`); defaults are no PDF, no template, no call identifier. The template and call id are correctly **after** the receipt number, with call id after template. No order-number selector is offered, matching S1's required receipt number. S3 says the returned receipt is **SN**, with its own number and `stornozottNyugtaszam` linking the original; the response projection supports both. An already-reversed original is documented as an **error**, unlike the invoice repeat-storno live evidence. The crate does not swallow that error or claim repeat-storno replay success. No request omission/order defect found.

### Query (`xmlnyugtaget`)

Q2 uses `all`. R `beallitasok` and R `fejlec`; settings are credentials plus R `pdfLetoltes`. Header fields are O string `nyugtaszam`, `rendelesSzam`, `hivasAzonosito`, `pdfSablon`, in that displayed order.

`receipt.rs:342–417` represents both documented lookup alternatives, and the enum selects **one** of receipt number or order number; no fake empty companion element is needed. Then optional call id, then optional template. Constructors default to no PDF, no template and no call id. The XSD technically allows neither/both primary selectors; Q1/Q2 prose says use either, so the enum restriction is justified. The absence of `rendelesSzam` in X3 is vendor drift, **not** an unsupported selector. Call-id-only lookup is not promised by Q1/Q2; see Q-B.

### Send (`xmlnyugtasend`)

E2/X4 use `sequence`: R settings (credentials), R header (R string `nyugtaszam`), O `emailKuldes`; its children are O strings `email`, `emailReplyto`, `emailTargy`, `emailSzoveg` in that order. `receipt.rs:469–487` matches all names, types and ordering. No template/PDF/call-id selector is documented for send, and none is invented.

`SendReceipt::new` emits an **empty present email block**, not an omitted block. E2 says missing email details resend the previous email, while its schema comment says omitting the whole block sends no email. The code deliberately requests a resend and documents this at `449–456`; this is a correct, useful narrowing of the send operation. `Some("")` emits an empty child while `None` omits it. The precise treatment of individual empty overrides, first send without details and comma-separated recipients is not established by these pages (Q-C).

## Field-by-field response comparison

C3, S3 and Q3 share `xmlnyugtavalasz` in namespace `http://www.szamlazz.hu/xmlnyugtavalasz`. E3 uses `xmlnyugtasendvalasz` in its corresponding namespace.

| Wire field/block | Official type/presence | Implementation / result |
|---|---|---|
| `sikeres` | R boolean, decides success/failure | `xml.rs:115–160`: accepts true/false/1/0; false → API error before receipt payload decoding |
| `hibakod`, `hibauzenet` | O int/string; returned on failure, examples include empty tags on success | `xml.rs:119–140`: empty code tolerated, code/message retained; missing code → `Absent`; **F-02** for incomplete documented-code classification |
| `nyugta` | O in envelope XSD for failures; C3/S3 require on success | `receipt.rs:653–655,669–684`: mandatory after successful verdict, `Missing("nyugta")` otherwise; correct |
| `nyugtaPdf` | O string containing base64, included when PDF requested | `656–659`; `types.rs:104–117`: whitespace-compacted standard base64 decoded to bytes; absent/empty → no PDF; invalid encoding → parse error |
| `nyugta/alap` | R container | `679`: required |
| `alap/id` | R int | `718`: `i64` safely covers XSD signed int |
| `alap/hivasAzonosito` | O string | `719–724`: optional, empty → None; **F-03** for trimming nonempty identity text |
| `alap/nyugtaszam` | R string | `725,692`: required string into `ReceiptNumber`, preserved |
| `alap/tipus` | R enumeration NY/SN | `726`; `types.rs:850–929`: NY and SN correct, future `Other` retained |
| `alap/stornozott` | R boolean | `727–728,694`: `reversed`, supports numeric booleans |
| `alap/stornozottNyugtaszam` | O string | `729–734,695`: original receipt number on SN, optional wrapper |
| `alap/kelt` | R **date** | `735,696`: required `jiff::civil::Date`; **F-01** for valid lexical forms refused |
| `alap/fizmod`, `penznem` | R string/string | `736–737,697–698`: open payment method/currency, preserves unknown values |
| `alap/devizabank`, `devizaarf` | O string/double | `738–741,699–700`: optional bank/Decimal, absent/empty supported |
| `alap/megjegyzes`, `fokonyvVevo` | O string/string | `742–749,701–702`: represented; trimming follows F-03's shared helper |
| `alap/teszt` | R boolean in XSD | `750–751,703`: deliberate `Option<bool>`; missing/empty unknown, never fabricated as live |
| `alap/rendelesSzam` | O string | `752–757,704`: represented with correct case; **F-03** |
| `tetelek/tetel` | R container, 1..unbounded rows | `680,760–764`: all rows projected; empty container accepted leniently |
| Row `megnevezes`, `azonosito` | R/O string | `768–770,804–805`: represented |
| Row `nettoEgysegar`, `mennyiseg`, `mennyisegiEgyseg` | R double/double/string | `771–779,806–808`: explicit text-to-Decimal conversion for numeric fields |
| Row `netto`, `afa`, `brutto` | R double | `783–788,811–813`: correct; accepts example's `nettoErtek`/`afaErtek`/`bruttoErtek` aliases too |
| Row `afatipus`, `afakulcs` | O special VAT enum, R double ≥0 | `780–782,809–810,609–615`: preserves tokens, special type wins when present; numeric token remains available |
| Row `fokonyv/arbevetel`, `fokonyv/afa` | O block, O strings | `789–799,814–817`: represented |
| Row comment / erasure code | Neither exists in C3/X5 response row schema | Absence from `ReceiptItem` is **not a missing documented response field**, although requests can contain them |
| `kifizetesek/kifizetes` | O block, 1..unbounded if present | `681–682,706–709,822–825`: optional → empty vector; all rows preserved |
| Payment `fizetoeszkoz`, `osszeg`, `leiras` | R string, R double, O string | `828–844`: all fields projected |
| `osszegek` | R container | `683,710`: shared totals parser |
| `afakulcsossz` | 1..unbounded before grand total | `xml.rs:342–368`: every row, empty list tolerated |
| Subtotal `afatipus`, `afakulcs`, `netto`, `afa`, `brutto` | O enum, R double ≥0, three R doubles | `xml.rs:354–368,394–403`; `types.rs:1068–1074`: all represented; same VAT precedence as items |
| `totalossz/netto`, `afa`, `brutto` | R block, three R doubles | `xml.rs:371–413`: all required and projected |
| Send response | Only verdict/error fields; no receipt/PDF | `receipt.rs:490–496`: correct root/namespace and `Result<(), ResponseError>` |

The parser is not a validating XSD processor: the root's expanded name is checked (`xml.rs:63–108`), then serde reads fields without enforcing every descendant namespace, order, multiplicity or numeric facet. This does not prevent parsing the documented qualified responses; broader acceptance is distinguished from the valid-input failure in F-01.

## Findings

### F-01 — Valid `xs:date` receipt replies are refused

- **Severity:** medium. **Confidence:** high for the parser defect; vendor emission frequency unverified.
- **Code:** `receipt.rs:525–526,679–684,735`; `xml.rs:169–176`.
- **Official basis:** C3/X5 declare `<element name="kelt" type="date" ... minOccurs="1">`. W1 §3.2.9.1 says the lexical space is `'-'? yyyy '-' mm '-' dd zzzzzz?`, “where the date and optional timezone are represented exactly the same way as they are for dateTime.” The built-in date whitespace facet is `collapse`.
- **Trigger:** otherwise ordinary successful `xmlnyugtavalasz`, with `<kelt>2026-01-01Z</kelt>`, `<kelt>2026-01-01+02:00</kelt>`, or `<kelt> 2026-01-01 </kelt>`.
- **Observed locally:** plain `2026-01-01` parses; both timezone forms fail with “unparsed input ... remains”; padded date fails parsing the year. The receipt adapter uses direct Jiff deserialization, which is narrower than `xs:date`.
- **Impact:** all three receipt-returning operations lose the entire otherwise usable receipt answer. On create/storno, that can turn a successful issue into `Parse`/unknown outcome and require reconciliation. No live receipt with a timezone suffix was observed in the supplied evidence.
- **Suggested fix:** use a receipt date deserializer that trims XML whitespace and recognizes/validates the optional XSD timezone suffix, then projects the **printed civil date** into `Date`. Do not shift it through UTC. Document that the timezone is discarded, or retain it separately if full XSD value fidelity is required. Never blindly split untrusted UTF-8 at a byte offset.
- **Meaningful regression check:** parameterize a complete successful receipt over plain, Z, positive/negative offset and padded date forms; assert the same issue date through create/storno/query parsing. Include malformed dates, invalid offsets, trailing junk and multibyte text to prove intentional refusal without panic. The existing happy-path date assertion at `receipt.rs:1174` does not cover this.

### F-02 — Seven documented receipt error codes still become unknown outcomes

- **Severity:** medium. **Confidence:** high.
- **Code:** `error.rs:40–44,144–169,220–253,303–348`; receipt entry points `274–275,337–338,415–416,490–495` share this classification.
- **Official basis:** C3 lists 336 “Prefix already used for invoices”, 337 “Prefix format invalid”, 339 “Receipt number does not exist”, and 340 “Paid amount differs from gross amount”. R4/G1 list 363 “gross ... must be a whole number”, 364 net at most two decimal places, and 365 VAT at most two decimal places. These are known missing-document or request-refusal cases.
- **Trigger:** a failure envelope with any of `336,337,339,340,363,364,365` (or the equivalent error header).
- **Observed locally:** each parses as `ErrorCode::Unknown("<code>")` and `OutcomeClass::Unknown`; 338, by contrast, is already `DuplicateReceiptCallId`/`Rejected`. The numeric code and message are **not lost**.
- **Impact:** a generic caller cannot distinguish a documented settled refusal or receipt-not-found result from an unknown “possibly issued” code without its own table. This weakens the public outcome-class interface, rather than causing automatic retries inside the crate. The comment explicitly mentioning unknown 339 demonstrates known incomplete coverage, but is not a domain decision that receipt-not-found must remain inconclusive.
- **Suggested fix:** name these receipt codes and classify 336/337/340/363/364/365 as `Rejected`, 339 as `NotFound`; update the class docs to include receipt not-found. Preserve the conservative fallback for genuinely unknown codes.
- **Meaningful regression check:** feed body-only failures through actual receipt operation parsers, asserting retained message/code and outcome class for all seven; repeat representative header errors. Keep 338 distinct from a successful replay, and keep an invented future code unknown. A round-trip test over only already-named variants cannot detect this missing-code set.

### F-03 — Nonempty receipt identifiers and optional text are silently trimmed

- **Severity:** low. **Confidence:** high for loss of wire text; account-side identifier normalization is unverified.
- **Code:** `receipt.rs:719–724,729–757,769–770,794–798,833–834`; common cause `xml.rs:271–282`.
- **Official basis:** C3/X5 type these fields as `string`, not `token`; R1 states `rendelesSzam` is returned “with the same value you sent in the request.” W1's string datatype preserves whitespace (unlike numeric/date whitespace collapse). No inspected receipt rule authorizes trimming the returned call id or order number.
- **Trigger:** response `<rendelesSzam> ORD-1 </rendelesSzam>` or `<hivasAzonosito> CALL-1 </hivasAzonosito>`; the same helper also trims comments, ledger strings and payment descriptions.
- **Observed locally:** result is `Some("ORD-1")`, `Some("CALL-1")`, and `Some("note")` for a padded comment. The create writer sends those input strings unchanged (`receipt.rs:214,224,229`), so request and returned representations are asymmetric.
- **Impact:** loss of exact identifier correlation and free-text fidelity. In particular, a consumer cannot tell whether the server returned a padded id. No claim is made that the server accepts two receipt orders differing only by surrounding spaces; that is not needed to demonstrate the lossy parse.
- **Suggested fix:** separate optional-string parsing from optional-number parsing: use trimming only to decide whether the element is empty, and return the original nonempty text. If identifier normalization is a deliberate public contract, name and document it explicitly rather than inherit it from a generic scalar helper.
- **Meaningful regression check:** full response with padded call id/order/item id/comment/payment description; assert exact nonempty strings survive while absent/empty cases keep their intended None semantics. Include numeric whitespace to ensure fixing strings does not regress tolerant decimal parsing.

### F-04 — Shared recovery instructions tell receipt callers to query an unavailable external id

- **Severity:** medium, documentation/API-usage defect. **Confidence:** high.
- **Code:** `error.rs:256–270,368–370`; `client.rs:63–72`; contrast `receipt.rs:105–142,342–370`.
- **Official basis:** Q1: “use either `nyugtaszam` (receipt number) or `rendelesSzam` (order number)”; C3: a unique `hivasAzonosito` ensures repeated XML “will not duplicate an existing receipt.” None of C2/Q2's fields is `szamlaKulsoAzon`.
- **Trigger:** receipt create transport/parse failure; user follows `ErrorCode::is_retryable`'s advice: “Before re-sending a create, storno or receipt, query by the external id (`szamlaKulsoAzon`) the request carried”.
- **Impact:** the prescribed recovery cannot be expressed by `QueryReceipt` and does not exist in the documented receipt protocol. Using the invoice query instead addresses a different document surface. This is especially unhelpful when the receipt number is precisely what the lost create reply would have supplied.
- **Suggested fix:** scope external-id instructions to invoice operations; document receipt recovery separately: retain a stable create call id on a repeated logical call, recognize 338 as duplicate prevention rather than recovered success, and reconcile using a known receipt number or deliberately managed order number where available. Do not promise call-id-only querying or invoice-style storno replay without evidence. A receipt with neither known number nor usable order selector may need operator reconciliation after a lost reply. Reuse the documented five-attempt ceiling, not an unbounded loop.
- **Meaningful regression check:** a local scripted create-success/lost-reply/338 scenario that proves no second receipt is assumed issued, plus a compiling recovery example using only `ReceiptSelector` variants. Review the rendered error/client docs to ensure no receipt path requires `InvoiceSelector::ExternalId`.

### F-05 — `Rounding::Exact` documentation promises invoice-side rounding for HUF receipts

- **Severity:** low, documentation defect. **Confidence:** high.
- **Code:** `item.rs:20–26,70–79,151–187`; `receipt.rs:134–139,174–201,243–245`.
- **Official basis:** R4: “`<brutto>` must be a whole number”; “`<netto>` and `<afa>` may contain at most 2 decimal places”; “There is no tolerance ... send already rounded amounts.” It warns that unrounded values are rejected, and explicitly limits these rules to HUF/Ft receipts.
- **Trigger:** a receipt caller selects `Rounding::Exact` after reading its statement that “szamlazz.hu then rounds each value to two decimals on its own”. Even `Scale(2)` can leave a HUF gross fractional: local `1 × 1 @ 27%` produced `net=1, VAT=0.27, gross=1.27`, accepted by `to_wire`, contrary to R4's whole-gross rule.
- **Impact:** misleading expectation of server correction followed by documented rejection (363 in the latter example). The raw-value API itself is intentional: `LineItem` expressly delegates arithmetic validation to the server. The defect is presenting **invoice observations** as general receipt behaviour, not the existence of an explicit raw or exact constructor.
- **Suggested fix:** scope the automatic-rounding observation to the probed invoices. Add receipt-specific amount guidance to `CreateReceipt`/`Rounding`: HUF gross whole, net/VAT at most two decimals, exact sum; `minor_unit(HUF)` satisfies these structural constraints, and explicitly calculated net/VAT cents are also permitted. Do not silently round caller-asserted totals or require whole net/VAT, which would exclude R4's accepted `787.40 / 212.60 / 1000` example.
- **Meaningful regression check:** a documented HUF receipt example with fractional unit price and chosen rounding must emit whole gross, ≤2-decimal net/VAT and exact sum. Keep the official `787.40 / 212.60 / 1000` shape representable and a foreign-currency high-precision case separate. If local validation is later chosen, table-test 261/363/364/365 conditions and HUF/Ft aliases rather than copy the calculation algorithm into a test.

## Intentional deviations, limitations and negative results

1. **Unsupported invoice item fields are refused, not dropped.** `margin_vat_base`, ledger economic-event/VAT-event and settlement dates have no receipt request element in C2. `receipt.rs:178–180,618–649` explicitly refuses them; the public docs at `98–101` and `item.rs:48–53` describe this. Unit tests `invoice_only_line_item_fields_are_refused` cover each. This is an accepted limitation, **not a conformance defect**.
2. **Financial validation is delegated intentionally.** `item.rs:70–79` and `receipt.rs:137–139` explicitly leave arithmetic/payment sums to szamlazz.hu. Prefix registration, supported currency validation, tender semantics and account-dependent switches likewise stay server-side. The review does not turn every invalid value constructible in a plain-data request into a defect. F-05 concerns inaccurate guidance; F-02 concerns the resulting documented errors.
3. **HUF minor-unit calculation is usable.** It produces rounded net/VAT and an exact whole gross; rounding net first and VAT second is within the small monetary tolerances for normal supported percentage rates. The more permissive official two-decimal net/VAT allowance remains available through `LineItem::new`.
4. **PDF handling is correct for documented valid content.** PDF is next to `nyugta`, not inside it, and all three operations use the same parser. Wrapped base64 is supported. The official `...` placeholder is deliberately not valid base64; `tests/upstream.rs:722–739` first asserts refusal, then substitutes synthetic PDF to inspect the remaining example. Its rejection is not a parser defect. PDF bytes are not checked for a `%PDF` signature; that is outside base64 conformance. Missing requested PDF is tolerated as `None`, a permissive API choice rather than proven data loss from a compliant response.
5. **Error detection does not depend on headers alone.** Shared verdict parsing handles body-only false/hibakod/hibauzenet, empty success error tags and unknown codes. Nonempty `szlahu_down` and header errors are read first, then HTTP status, then expected XML envelope. No current receipt page promises raw PDF or invoice-style plain-text error fallback; G1 explicitly lists the other operations that use that format. No receipt response-version omission defect found.
6. **Success payload coverage is complete for the inspected schema.** Includes id, call id, order number, reversal metadata, tender payments, item ledger and both levels of totals. Every operation-specific request field exists in the interface. Receipt item comments/erasure codes are request-only in these published response schemas, so their absence from `ReceiptItem` is not a documented omission.
7. **Open tokens prevent schema-version lockout.** Future receipt type, VAT type/rate, currency and payment-method strings are representable. Download X5's extra `TEHK` becomes `VatRate::Other`, retained rather than refused. There is no requirement to close the enum over an already internally inconsistent vendor list.
8. **Ordinary scientific notation works.** Contrary to a plausible `xs:double` concern, locally `1E3`, `1e-2`, `2.7E1` parse as Decimal 1000, 0.01, 27 through the receipt quantity path; `VatRate::from` recognizes them numerically too. No exponent-format finding. `Decimal` still cannot represent the full IEEE double domain (NaN/infinities/extreme magnitudes); that is a finite-money type limitation, not a demonstrated normal receipt failure. Plain decimal request output is a valid double lexical form.
9. **Lenient optionality is explicit in tests.** Missing/empty `teszt` becomes unknown, absent payments empty, and empty item/subtotal lists are accepted although the XSD wants at least one. Unknown XML fields are ignored. These broaden acceptance; they do not break conforming receipts. There is no new strict-XSD requirement imposed by this review.
10. **Schema-location attributes are not required request data.** The writer's lack of `xmlns:xsi` and `xsi:schemaLocation` is harmless; actual operation/root/namespace identify the request, and several vendor example schema links are broken anyway.
11. **No automatic email on receipt creation is missing.** C2 has no email block. E1 specifically sends an already issued receipt. Send's successful `()` is sufficient for E3's verdict-only response.
12. **Create duplication is not invoice replay.** The stable optional call id is sent and 338 is already named; no implicit random id is generated per retry. This correctly leaves the choice of logical-call identity to the caller. `Rejected` on a repeated call describes that call issuing nothing new, not proof that the prior call issued nothing.

## Open questions and source conflicts

### Q-A — Automatic MNB exchange rate on receipts

`receipt.rs:196,218–223,948–959` explicitly accepts `ExchangeRate::automatic_mnb()` and omits `devizaarf`; `types.rs:932–962` presents it as a shared feature. R5/HU3 require bank **and** rate for a foreign receipt. I1 specifically annotates the **invoice** `arfolyam` element: “If arfolyamBank='MNB' and arfolyam is not set, the current MNB exchange rate is used when creating the invoice”. No analogous receipt annotation appears in C2/X1. Receipt XSD optionality by itself proves only structural validity (HUF needs no rate), not foreign-receipt defaulting.

**Confidence:** high that the published receipt guarantee is missing; **runtime impact unknown**. The local test proves only that the crate emits bank without rate. The behaviour document has no receipt probe establishing automatic MNB. Suggested resolution: obtain vendor confirmation, or separately authorized receipt evidence, before promising the extension. Until then document it as unverified or require an explicit receipt rate; do not automatically insert zero. No live probe was performed here.

### Q-B — Meaning and lookup power of query `hivasAzonosito`

Q2/HU1 call it an optional unique **call identifier**, whereas `QueryReceipt::call_id` (`receipt.rs:368–370`) says “as supplied at creation.” Both primary selectors are optional in the XSD, but Q1/Q2 explicitly describe only receipt/order-number lookup. Does call id identify the original creation for query, identify the query invocation, or do nothing? What is its precedence alongside a primary selector? The current sources do not settle this. Do not label missing call-id-only selector support a confirmed defect; clarify the rustdoc and ask the vendor. Likewise, call-id uniqueness scope/retention and storno cross-operation collisions are unspecified. Storno's optional field exists in S2/X2, but its exact 338 semantics are less explicit than create's prose.

### Q-C — Email defaults and recipient syntax

E2/HU2 distinguish omitted **block** (no send) from omitted **details** (previous email). The implementation is consistent with a resend operation. They do not explicitly guarantee independent per-field merging, comma-separated addresses (`receipt.rs:422–429`), empty-string clearing, or which fields a first send requires. E3's failure example is code 7 for missing subject despite `emailTargy` being schema-optional. These are server semantic requirements/defaults, not proof that all optional fields must become required in the Rust request. A future authorized test should observe first send, resend, partial overrides and multiple recipients without conflating them with invoice email behaviour.

### Q-D — Order lookup when several receipts share an order

R1 explicitly permits repeated receipt order numbers when its separate toggle is off. Q1/Q2 do not say which receipt order lookup returns in that case, or whether the original or SN wins after reversal. Invoice “latest document” behaviour is not receipt evidence. `ReceiptSelector::OrderNumber` appropriately makes no latest-holder promise. Clarify this before relying on order lookup as deterministic lost-reply reconciliation.

### Q-E — Vendor documentation inconsistencies

- C2 prose says order is fixed, but create/query XSDs mostly use `all`. Current writers honor the shown example order and satisfy the schemas; no fix justified.
- C3's second item uses invoice `*Ertek` amount spellings while its XSD uses receipt spellings. Aliases handle both. The example is not a balanced receipt (line totals, payments and summaries differ), and even its NY example has `stornozottNyugtaszam`; it is a **format example**, not live business evidence.
- OLD says receipts use the same account setting as invoices; current R1 expressly says a separate receipt toggle. Prefer current R1. Neither establishes receipt repeat-storno success; S3 says already reversed is an error.
- X1/X3 downloads are stale; X5 and C3 differ on `TEHK`. Broken schema links persist. Current inline schemas are the comparison baseline where they differ.
- `Currency` rustdoc's “37 ISO-style codes” (`types.rs:361–364`) is stale against R5's substantially longer table, but the open wrapper supports every listed code; this is incidental shared documentation drift, not receipt capability loss.

## Relationship to live evidence

`docs/szamlazz-hu-behaviour.md:1–24,164–172` explicitly bounds its probes to one TEST account and named invoice/proforma scenarios. It contains **no receipt-operation live evidence**. In particular:

- Invoice repeat-storno echo/no-op observations (`82–98`) do not override S3's receipt errors.
- Invoice automatic rounding and tolerance observations (`155–162`) do not override R4's HUF receipt rules; this is the basis for F-05's wording correction.
- Invoice missing-data/body-only errors (`137–145`) motivate robust body parsing, but do not prove a particular receipt's header presence or storno numeric code.
- Invoice bad-email probes (`153,171`) cannot establish receipt recipient syntax, defaults or email delivery.
- Foreign-currency defaulting remains explicitly unverified in the behaviour note (`216–223`), and cannot justify a receipt MNB guarantee.

Thus the findings are **current-document conformance and offline parser results**, not claims that live receipt issuing was reproduced.

## Verification performed and remaining regression coverage

Executed:

```text
cargo test -p szamlazz-agent --lib --test upstream
178 unit tests passed; 9 upstream/corpus tests passed; 0 failures.
```

This exercised the receipt unit tests and cached official request/response examples, along with the shared wire/XML/type tests. The corpus tests compare request outlines and parse responses; they **do not validate generated requests against freshly downloaded XSDs**. `xmllint` and Python lxml were unavailable, so the fresh-schema comparison above was manual, field by field; do not read the test result as a fresh-XSD validation certificate.

A temporary standalone executable under `/tmp/opencode/receipt-conformance-20260909` depended on this checkout without `client-reqwest`, used synthetic `RawResponse` bodies, and ran via:

```text
cargo run --offline --manifest-path /tmp/opencode/receipt-conformance-20260909/Cargo.toml
```

Its relevant dependency versions matched the workspace lock: Jiff 0.2.35, rust_decimal 1.43.0, quick-xml 0.42.0. It confirmed F-01's three valid-date failures, F-02's seven unknown classifications, F-03's trimming, F-05's unvalidated fractional HUF gross, Q-A's omitted rate, and the negative scientific-notation result. It performed no network I/O and changed no production/test source in the workspace. The report includes the triggering values so the results are reproducible without retaining that temporary executable.

Useful regression gaps beyond individual findings: run complete NY and SN payloads through **all three** receipt-returning entry points; test default and prefixed root namespaces, wrong root/namespace, true/false/1/0 verdicts, absent success payload, wrapped/empty/bad PDF, and body-only known/unknown errors. For request schema checks use current inline create/query schemas, with all optional fields set, both credential alternatives, both query selectors, and torloKod 0/400/401. Such checks would detect genuine field/order/semantic regressions; merely adding another copy of the canonical happy-path XML would not.
