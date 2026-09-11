# Számla Agent receipts — current-revision review

**Revision:** `fbda137e79dc8f5a40016ee03cd5997ed4e0ea78`

**Acquisition/review date:** 2026-09-10

**Scope:** `szamlazz-agent`: create, storno, query and send receipts; request fields, order, types and defaults; receipt data, items, payments, totals, PDF/rendering, email, call identity and order rules.

## Conclusion

**No defect in emitting the documented receipt operations or reading ordinary conforming receipt responses was established. No documented useful receipt capability is missing.** The two documentation findings in the earlier report are **closed** at this revision.

Two **P3 malformed-response hardening concerns** remain, reproduced offline:

| ID | Concern | Confidence |
|---|---|---|
| H1 | Requested PDF can be absent without a diagnostic; whitespace-only PDF becomes `Some(Pdf)` containing **zero bytes** | High in behavior; medium in preferred diagnostic policy |
| H2 | Empty `stornozott` is interpreted as `false`, manufacturing a negative reversal fact from a non-boolean value | High in behavior and schema disagreement; no evidence of vendor occurrence |

Neither is evidence that szamlazz.hu currently returns these malformed values. Neither warrants retrying issuance. The report also records significant **vendor-source disagreements**, including a freshly observed knowledge-base conflict over the receipt order toggle and newer Hungarian NAV-reporting information than the Agent pages provide.

### Method and revision boundary

- Reviewed the implementation and newly fetched official sources independently **before reading** `2026-09-10-agent-api-receipts.md` for closure. That report reviews `382cf761…`; its locations and open/closed judgments are not current evidence.
- HEAD matched the requested SHA at the beginning and again before writing. Tracked working-tree diff was empty. Other untracked review/research files and concurrently appearing Restate files were left untouched.
- Final `git diff --exit-code fbda137e79dc8f5a40016ee03cd5997ed4e0ea78 -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md Cargo.toml Cargo.lock` passed: the reviewed source and dependency lock remained at the pin. Other actors' Restate edits appeared during the session and are outside this report.
- No delegation, live Számla Agent calls, source/test/fixture edits, or account changes. HTTP activity was unauthenticated **GET of documentation, XSDs and the official PHP download**. PHP examples were inspected, never executed.
- The only repository file created by this review is this report. Scratch reproduction is under `/tmp/opencode/current-receipts-fbda137/`.
- `docs/szamlazz-hu-behaviour.md:1–31,33–98,155–172,174–308` was read: its account observations concern invoices/proformas and related operations, **not receipt executions**. Invoice replay, post-storno order reuse, payment removal, timing and stored precision are not receipt proof.
- Below, `receipt.rs` means `crates/szamlazz-agent/src/ops/receipt.rs`; other source basenames mean `crates/szamlazz-agent/src/`. Test paths are under `crates/szamlazz-agent/tests/` unless expanded. Line numbers refer to the pinned revision.

## Findings and actionable hardening

### H1 — Distinguish a requested PDF from a missing or zero-byte artifact

**Severity:** P3 / low, malformed-response hardening. **Confidence:** high in reproduction; medium in choice of public diagnostic. **Live evidence:** none.

**Source rule:** [create response][C-response] says receipt XML accompanies success and base64 `nyugtaPdf` is conditional on `pdfLetoltes`; [storno response][S-response] explicitly promises the **storno** PDF when true; [query XML][Q-xml] says true includes the receipt PDF. The shared XSD makes `nyugtaPdf` optional because it covers both flag values and failures, not because zero bytes are a usable PDF.

**Current code:** `receipt.rs:293–295,364–366,449–451` use the same parser without passing the requested flag. At `690–701`, only the raw **empty string** is filtered. `types.rs:104–117` strips whitespace, decodes an empty base64 string successfully and wraps its empty bytes. Public `Receipt::pdf` is at `595–597`.

**Offline reproduction:** A complete synthetic successful receipt with a real item and matching totals was passed to each public parser with `download_pdf=true`:

```text
nyugtaPdf omitted                 -> Ok, number retained, pdf=None
<nyugtaPdf/>                      -> Ok, number retained, pdf=None
<nyugtaPdf> [newline] </nyugtaPdf> -> Ok, number retained, pdf=Some(0 bytes)
<nyugtaPdf>...</nyugtaPdf>         -> Base64 parse error
<nyugtaPdf>JVBE[newline]Ri0=</nyugtaPdf> -> bytes "%PDF-"
```

The `%PDF-` control establishes decoding only, not a renderable document. The three-operation missing-artifact matrix intentionally uses the same synthetic `NY` body to isolate their shared parser; a separate `SN`/original-reference control also passed.

**Impact:** Checking only `Result::is_ok()` misses an unfulfilled download; even `pdf.is_some()` passes the whitespace-only case and `save_to` can write an empty file. The identity remains available for missing/empty artifacts. Nonempty invalid base64 currently fails the entire parse, an existing strict policy rather than evidence of failed issuance.

**Action:** Document explicit caller checking after a requested download and recovery by **querying the known receipt number**. Treat whitespace-only content consistently with empty content. If adding a diagnostic for requested-but-missing/invalid PDF, retain the confirmed receipt and number alongside it; do not turn artifact recovery into permission to create again. A full PDF renderer/validator is not implied. Add a focused absent/empty/whitespace/wrapped-base64 matrix when implementing this policy.

**Prior closure:** Earlier H1 remains a policy-level concern; the zero-byte `Some` case is additional evidence, not a newly discovered missing download capability.

### H2 — Empty reversal text is not evidence that a receipt is unreversed

**Severity:** P3 / low, malformed-response semantic hardening. **Confidence:** high. **Live evidence:** none.

**Source rule:** [shared receipt response][C-response], both EN/HU inline XSD and [download][X-response], declares `alap/stornozott` a **required boolean**: true if reversed, false otherwise, meaningful on `NY`. Empty text is not one of XML Schema boolean's `true`, `false`, `1`, `0` lexical values. Neither current example nor the behavior notes establishes empty as a receipt-specific false value.

**Current code:** `receipt.rs:555–558,764–765` exposes a plain `bool` using `xml::de::flexible_bool`; `xml.rs:587–598` maps `""` (including trimmed whitespace) to `false`. The receipt README's recovery example trusts `!receipt.reversed` at `crates/szamlazz-agent/README.md:186–190`.

**Offline reproduction:** Replace `<stornozott>false</stornozott>` in an otherwise complete `NY` response with `<stornozott/>`. Query succeeds with `reversed=false`. Removing the element entirely, or placing it in a foreign namespace, instead fails with missing `stornozott`. This asymmetry confirms empty is being converted, not merely ignored. The three operations share this decoding path.

**Impact:** A caller can mistake an unknown reversal state for a live receipt during recovery if the vendor emits an empty element. This is conditional on malformed input, not a demonstrated server-side failure or duplicate-issuance path in the library.

**Action:** Use a receipt-specific strict boolean adapter for this required fact, or explicitly model unknown reversal state. Preserve all four valid boolean forms and avoid changing unrelated shared-helper semantics blindly. Test false/0, true/1, missing, paired-empty, self-closing and whitespace-only forms. Any parse failure after a write still leaves its outcome uncertain; it is not permission to retry issuance.

## Comprehensive request coverage

`?` denotes an optional XML child; all other listed children must be present. The crate intentionally leaves business-value validation to szamlazz.hu except for its explicit local gates. Required strings can still be empty; neither the Rust wrapper nor `xs:string` implies a nonempty/pattern-validated value.

| Surface | Complete field/type/default check | Current implementation and verdict |
|---|---|---|
| Operation identity | Multipart file fields `action-szamla_agent_nyugta_create`, `_storno`, `_get`, `_send`; roots `xmlnyugtacreate`, `xmlnyugtast`, `xmlnyugtaget`, `xmlnyugtasend`; namespace `http://www.szamlazz.hu/<root>` | Match all four request pages and [sending requests][requests]. `receipt.rs:189–191,223–231,340–352,420–431,502–511`. |
| Transport packaging | HTTPS POST to `https://www.szamlazz.hu/szamla/`; one XML file/document | `wire.rs:7–14,66–99,390–408` builds a multipart file part with `text/xml`; receipt operations contribute no extra attachments. `to_wire` validates before construction. No per-receipt response-version selector exists; do not add invoice `valaszVerzio`. |
| XML writing | UTF-8 declaration, root namespace, escaped text, true/false, plain decimal notation | `xml.rs:19–41,414–442`. Schema-location attributes in examples are unnecessary hints, not business inputs. `wire.rs:402–419` rejects forbidden XML 1.0 characters on the complete-wire path; direct `write_xml` is lower level. |
| Credentials | Settings hold `felhasznalo?`, `jelszo?`, `szamlaagentkulcs?` | Each writer invokes `xml.rs:456–465`: one credential form, username before password or agent key. Order meets storno/send sequences. Authentication cannot be established by XSD alone; no credentials were used. |
| PDF setting/default | Create/storno/query require boolean `pdfLetoltes`; send has none | All three explicitly write it; constructors and serde default false (`receipt.rs:150–152,173–184,317–319,330–335,396–398,410–415`). PHP convenience defaults do not define an XML server default. |
| Create header strings | `hivasAzonosito?`, `elotag`, `fizmod`, `penznem`, `devizabank?`, `megjegyzes?`, `pdfSablon?`, `fokonyvVevo?`, `rendelesSzam?` | Complete fields `117–149`; output `232–249`. Prefix/method/currency explicit constructor inputs; optional values default absent. Call ID goes first, order last, matching example order. |
| Prefix and method rules | Prefix required, receipt-only (336), capital letters/numbers (337); `fizmod` free text | Prefix string sent unchanged; vendor validates it. `PaymentMethod::Other` covers all unnamed documented methods (`types.rs:588–644`). PHP's empty-prefix default comment is not sufficient to invent a crate default or a strict new prefix alphabet beyond the vendor rule. |
| Currency/rate | `penznem` string; `devizabank?` string and `devizaarf?` double; foreign currency needs bank/rate per prose | `receipt.rs:208–218,237–242`; finite Decimal sent as decimal, correct receipt spellings. Non-HUF requires rate object, nonblank/unpadded bank, and numeric rate unless bank exactly `MNB`. Zero/negative explicit rates are not rewritten or locally restricted. Automatic MNB retained on receipt-specific PHP evidence (below). |
| Currency openness | HUF/Ft aliases; full supported list includes legacy codes and KWD | `Currency` preserves any wire code (`types.rs:369–410`). All listed codes representable. Local HUF detection ignores case but sends original text; lower-case server acceptance was not established here. Minor-unit scale is local policy (`412–435`), not vendor precision. |
| Create items | Required `tetelek`, 1+ `tetel`: `megnevezes`, `azonosito?`, `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar`, `afakulcs`, `netto`, `afa`, `brutto`, `fokonyv?`, `megjegyzes?`, `torloKod?` | All written at `receipt.rs:250–277`. Quantities/unit price/values are Decimal; VAT token is string; names/unit/id/comment are strings. Empty vector rejected (`193–196`). No invoice `*Ertek` spelling in requests. |
| Item ledger | `fokonyv/arbevetel?`, `afa?`, both strings | Both emitted (`265–270`). Invoice-only margin base, economic event, VAT economic event and two settlement dates are refused (`655–685`), not silently discarded. `item.rs:52–71,103–126` documents the split. |
| Erasure code | Optional nonnegative int **count**, max 400; account feature required, unavailable in demo/test | `item.rs:115–131`; `receipt.rs:200–207,272–274`. `u32` plus validation to 400 fits the XSD int domain. Current EN/HU rule and PHP confirm it despite stale download. 537/538/539 named. No receipt requirement for invoice template `SzlaMost` inferred. |
| Item amounts | HUF/Ft gross whole, net/VAT at most two decimals, exact sum; price×quantity and VAT relation each have documented 2-HUF tolerance | Correctly described at `receipt.rs:104–108`; explicit `LineItem::new` preserves amounts (`item.rs:133–160`), derived constructor rounds net then VAT and adds them (`183–222`). `Scale(2)` need not yield whole gross; minor-unit HUF is stricter, all-whole local policy. Validation does not claim to enforce these vendor arithmetic rules. |
| Payments | Optional `kifizetesek`, 1+ `kifizetes`: `fizetoeszkoz` string, `osszeg` double, `leiras?` string; sums must equal receipt gross | API `receipt.rs:61–88,156–160`, writer `278–288`; empty vector omits block, nonempty writes all entries. Description correctly string despite misleading “double” example comment. No invoice five-entry cap, credit-entry date, replace/additive semantics or mutation endpoint imported. Sum left to server, with 340 supported. |
| Storno | `beallitasok` → `fejlec`; header `nyugtaszam` → `pdfSablon?` → `hivasAzonosito?` | Exact `xs:sequence` at `344–362`, defaults `328–337`. Required original number, optional strings. No caller-set date, amounts, payments, resulting receipt number or invoice flags. |
| Query | Settings/header required; header `nyugtaszam?`, `rendelesSzam?`, `hivasAzonosito?`, `pdfSablon?` | `ReceiptSelector` and writer (`369–443`) offer one number/order selector, matching prose's either/or rule rather than XSD's permissive zero/both. Optional call ID is emitted only if supplied and has no asserted selector semantics. No call-ID-only lookup. |
| Send | `beallitasok` → `fejlec/nyugtaszam` → `emailKuldes?`; children `email?` → `emailReplyto?` → `emailTargy?` → `emailSzoveg?` | Exact sequence at `506–525`. `email=None` still writes present-empty block, requesting resend; `Some(default)` same. `None` children omitted, `Some("")` empty, each a string. Complete first-send details and one recipient recommended (`454–499`), no unsupported splitting or merging. |
| Template/rendering | `A` standard A4, `N` 80 mm, `J` ticket, `L` ticket with logo; omitted/empty defaults to A4, example also says invalid defaults to A | `receipt.rs:25–59`, all named variants plus `Other(String)`; usable on create/query/storno. XML tokens correct. Serde snake-case enum representation is an SDK JSON choice, not Agent XML. Send has no template override. Actual rendering not tested. |

### Order versus `xs:all`

Create and query singleton groups use `xs:all`, despite the pages' blanket “order … cannot be interchanged” warning. The Rust create writer follows the **example**, including bank before rate; the XSD lists rate before bank but does not require that order. PHP `ReceiptHeader.php:184–193` uses the same create order, but query template before order. PHP `ReceiptItem.php:65–75` emits comment before ledger; Rust follows the XML example's ledger-before-comment. These are not XSD-order defects. Storno and send use actual sequences and Rust matches them.

## Comprehensive response coverage

| Surface | Complete data/type check | Current implementation and verdict |
|---|---|---|
| Create/storno/query envelope | `xmlnyugtavalasz`: required `sikeres`, optional `hibakod`, `hibauzenet`, `nyugtaPdf`, `nyugta` | Root/namespace `receipt.rs:20–23`; shared parser `688–721`. Error verdict checked before document payload (`xml.rs:314–393`); success without `nyugta` refused. Header unavailability/error/status precedence in `wire.rs:273–310`; no success assumed from missing error headers. |
| Basic identity | `id` int, `hivasAzonosito?` string, `nyugtaszam` string, `tipus` NY/SN | `receipt.rs:544–554,727–730,754–763`; signed i64 widens XSD int safely, string number retained; call ID blank becomes None. Open `ReceiptType` preserves future tokens (`types.rs:861–940`). |
| Reversal | Required `stornozott` boolean; optional `stornozottNyugtaszam` names original on SN | Correct receipt spelling and reference direction (`555–561,731–732,764–771`). Storno returns reported SN data/PDF, not a constructed original. Parser does not assert SN or compare reference with request; caller must verify reported identity/type/reference. H2 covers empty boolean. |
| Remaining basic data | `kelt` date, `fizmod` string, `penznem` string, `devizabank?`, `devizaarf?` double, `megjegyzes?`, `fokonyvVevo?`, `teszt` boolean, `rendelesSzam?` | All mapped (`562–584,733–741,772–795`). Date uses civil-date helper; timezone suffix retained as printed date, no UTC conversion. Missing/empty test marker deliberately remains None. Optional business strings preserve meaningful padding/NBSP; blank XML whitespace is None. |
| Items | `megnevezes`, `azonosito?`, `nettoEgysegar`, `mennyiseg`, `mennyisegiEgyseg`, `netto`, `afatipus?`, `afakulcs`, `afa`, `brutto`, `fokonyv?` | Complete public/private mapping (`600–652,798–858`). Quantity/prices/values finite Decimal; VAT raw string retained and optional special type takes precedence. Canonical receipt amounts plus example's `nettoErtek/afaErtek/bruttoErtek` aliases accepted (`821–826`). |
| Item ledger | `fokonyv/arbevetel?`, `afa?` strings | `636–644,831–856`: both retained, empty block can remain an all-None ledger. Returned row comment and erasure codes/count are **not in either current response XSD or example**; no established missing response field. |
| Payments | Optional block, repeated tender name/amount/description | `receipt.rs:588–591,743–746,860–882`; amount Decimal, text string, empty/absent collection tolerated; no recomputation or invoice-payment interpretation. |
| Totals | `osszegek/afakulcsossz*`: `afatipus?`, numeric `afakulcs`, net/VAT/gross; `totalossz`: net/VAT/gross | `xml.rs:638–721`, public `types.rs:1055–1107`. Raw numeric VAT token retained; all amounts decoded; grand total required. No requirement that inconsistent published sample totals be silently replaced by item sums. |
| Send | `xmlnyugtasendvalasz`: `sikeres`, `hibakod?`, `hibauzenet?` | `receipt.rs:527–533` returns `()`, correctly a verdict-only XML acknowledgement. “Plain acknowledgement” in rustdoc does not mean a text/plain parser. No response PDF promised by the dedicated contract. |
| Scalar boundaries | Boolean `0/1/true/false`; padded/exponent numbers; civil dates; open strings | Shared helpers at `xml.rs:475–615`; numeric fidelity tests and scratch controls pass. Decimal intentionally refuses nonfinite/unrepresentable XSD doubles rather than silently rounding. Date domain is finite and deliberately not full arbitrary-year XSD validation. These are explicit model boundaries, not evidence of a currently emitted receipt that fails. |
| Envelope integrity | Complete expected root/namespace through EOF; protocol fields only | `xml.rs:63–229`; existing query completion/send namespace controls pass. Scratch foreign-namespace reversal cannot supply the required field. Unknown receipt types/extensions are not converted into known facts. This is not a claim of exhaustive XML-validator equivalence. |

### Response tolerance and identity boundaries

- Empty item lists, absent VAT subtotals and missing/empty `teszt` are deliberate leniency, covered at `receipt.rs:1309–1383`, despite response XSD cardinalities. They do not lose valid supplied values; missing `teszt` is never assumed live.
- Read aliases are justified by the **current official example**, not by invoice live behavior. The request writer still emits canonical receipt amount names.
- No parser-level selector equality, call-ID equality, SN-reference check or accounting consistency check is promised. Callers can inspect these fields; the README recovery example does. A success-shaped synthetic NY accepted by `StornoReceipt::parse` does not prove vendor storno behavior.
- PDFs are decoded bytes, not validated rendering. Template selection, persistence across queries, logo availability, exact dimensions/content and byte-for-byte PDF stability require vendor clarification or separately authorized receipt testing.

## Receipt operational rules and evidence boundaries

| Topic | Established rule and implementation | Boundary / next action |
|---|---|---|
| Creation call ID | [C-response] recommends unique `hivasAzonosito`; reuse fails with 338 and prevents duplicate issuance, not replay-success. `receipt.rs:94–102,118–122`; `recovery.md:15` and README `157–194` require persistence before send and prohibit fresh-ID escape during unresolved recovery. | No documented retention, normalization, prefix/account/operation scope or concurrent-request guarantee beyond the stated duplicate rule. Keep a nonempty logical ID; no automatic UUID generation in the library. |
| Storno repetition | [S-response] EN/HU explicitly describes errors for nonexistent original, already reversed original and target itself SN. `receipt.rs:298–325`, `recovery.md:16`, README `196` now say so. | Error table gives messages, **not numeric mappings**. A 339 synthetic storno test is not proof all three errors use 339. Optional storno call ID does not establish a storno-specific 338 guarantee. |
| Order identity | [Order] says optional `fejlec/rendelesSzam`, echoed in `alap/rendelesSzam`; [PHP-query]/[PHP-PDF] say **last matching document**. Code emits/reads it verbatim apart from blank optional response text. | Exact last criterion, SN selection, receipt order reuse after storno, case/padding normalization and invoice-style two-day replay are unestablished. Verify identity and type. |
| Order repetition | [Order] EN/HU and invoice cross-link say separate receipt toggle, ON forbids reuse, OFF allows multiple matches. Current code/docs accurately cite that rule. | Linked knowledge base contradicts toggle independence (V3 below). No library enforcement or automatic configuration. Do not change the current supported behavior solely on that contradiction. |
| Email | [E-xml] distinguishes absent block (no email) from details absent in a present block (resend). Current send always requests an email operation. | First-send minimum beyond the demonstrated subject failure, partial overrides, empty-string meaning, multi-recipient syntax, HTML/BBCode and final delivery evidence remain unspecified. Result `()` records vendor acknowledgement, not inbox receipt. Querying receipt existence cannot reconcile a lost email acknowledgement. |
| Foreign currency | [Currencies] asks for bank/rate; PHP receipt-specific source explicitly documents automatic MNB when rate omitted. `types.rs:966–979` accurately qualifies this. | No live omitted-rate receipt proof. Custom-data example `41–44` comments on omission but actually supplies 300. MNB availability/currency/date behavior remains external. Preserve supported omission, not an explicit-rate-only regression. |
| Amount validation | [Amounts] gives HUF/Ft whole gross, ≤2 net/VAT decimals, exact sum and 2-HUF arithmetic tolerances. Explicit fractional-net/VAT wire example passes. | Not enforced by `validate`; this is documented server-owned validation, not a hidden automatic rounding policy. Foreign-currency precision is not inferred from P60 invoice storage. |
| Error catalogue | 336/337 prefix, 338 call ID, 339 receipt absent, 340 payment mismatch; 363/364/365 precision; 537/538/539 erasure constraints all named (`error.rs:154–189,243–255,295–307,383–410`). | Code 7 on send is the official missing-**subject** example, not receipt absence. Generic `OutcomeClass::NotFound` is operation-dependent and documented as such (`recovery.md:17`). Open codes remain uncertain. |
| Retry rules | [Errors] allows at most five total sends of the same request, then operator involvement; no tight loop. `recovery.md:4–9,36–55` records it and the supplied transport retry boundary. | Exchange classification cannot settle earlier unanswered sends. Library has no receipt recovery/retry loop. Storno/email are not granted invoice retry semantics. |
| NAV reporting | [NAV] describes computer-generated receipts, not e-cash-register e-receipts; no additional XML request/response field. | Agent pages are behind linked HU knowledge-base information as of today (V4). A successful receipt response contains no NAV reporting acknowledgement. Do not claim end-to-end NAV reporting success from `Receipt`. |

## Vendor disagreements and preserved deviations

### V1 — Downloaded XSDs lag the documented field sets

1. **Create:** [X-create] lacks `torloKod`; EN/HU [C-xml], [Erasure] and PHP `Item/ReceiptItem.php:73–75` include it. Keep `receipt.rs:272–274`. The cached schema includes it at `fixtures/upstream/agent/xsd/xmlnyugtacreate.xsd:41–47`; its acquisition mechanism is already explicitly uncertain in `fixtures/SOURCES.md:182–191`.
2. **Query:** [X-query] lacks `rendelesSzam`; EN/HU [Q-xml], request prose, PHP query/PDF pages and `ReceiptHeader.php:191–194` support it. Keep `ReceiptSelector::OrderNumber`. `fixtures/SOURCES.md:103–108,135–140` records the cached schema's inline refresh; it is not an unchanged current download.
3. **VAT response enumeration:** [X-response] includes `TEHK`, absent from EN/HU inline schema. The open string/`VatRate::Other` representation retains it, so no information is lost.
4. **Broken embedded locations:** storno/query/send examples and send response point to `/docs/xsds/...` URLs returning 404. Working variants are registered below. None of the six downloaded schemas imports/includes another XSD.

These are source disagreements, not a blanket rule that inline always wins or downloads are always wrong. No cached schemas were replaced and no merged schema was used to claim conformance.

### V2 — Illustrative examples are not valid execution traces

- Shared response [C-response] has `nyugtaPdf=...`, not base64; second item uses invoice amount aliases; item grosses total 50,800, payments total 4,000, grand gross is 254; NY/false reversal still carries an SN-style original reference. Both locales retain these semantic inconsistencies. Historical corpus shows them at `fixtures/upstream/agent/responses/xmlnyugtavalasz.xml:7,14–16,35–52,56–79`.
- Create sample's second row uses `ÁKK` with a positive VAT amount. Its free-text annotations also mislabel `leiras` as double and `devizaarf` as string; XSD and PHP establish string description and numeric rate. No server arithmetic conclusions are drawn from it.
- [Amounts] EN/HU prints `787.40157480315 + 212.59842519685` as `999.999999...`, although those exact decimal strings sum to **1000**. Its decimal-place rule independently excludes them; exact decimal arithmetic cannot reproduce the claimed 261 ordering. Treat the stated check order as vendor documentation, not a proven floating-point implementation. The accepted rounded example remains unambiguous.
- PHP `examples/document/receipt/send_receipt.php:17–18` claims a PDF after send. Dedicated [E-response]/[X-send-response] define only verdict/error; do not add a send PDF type based on this example comment.
- EN `cash` versus HU `készpénz` and translated tender/item names are free-text examples, not different type systems. The HU storno XML comment's “sztornózott” is less precise than the response page; both response locales clearly say the newly created **SN** data/PDF.

### V3 — Linked order knowledge base contradicts both Agent locales

[Order] EN/HU explicitly says receipt and invoice toggles are independent. The linked [order knowledge-base article][KB-order], fetched independently today, locates the switch in receipt-editor settings but then says:

> “A beállítás valamennyi érintett bizonylattípusra … érvényes; bizonylattípusonként nem állítható be külön.”

That says the setting applies to all affected document types and cannot be configured separately by type. It also says repetition is forbidden by default and order-number entry is available in #start/#digital/#profi, while its surrounding #free discussion concerns web receipt issuance. The Agent pages do not establish equivalent API subscription gates/defaults.

**Disposition:** vendor ambiguity, not a reopened D2 implementation/documentation omission. Current crate faithfully cites the explicit Agent rule. Ask the vendor to align the sources and establish actual target-account behavior; do not import invoice probe results, assume account defaults, or add local plan validation.

### V4 — NAV reporting readiness differs across current official pages

[NAV] EN/HU says automation is being developed and “nothing you need to do for now.” Linked [English knowledge base][KB-NAV-EN] says automatic reporting from September 1 and connection to NAV required. Linked [Hungarian knowledge base][KB-NAV-HU] now explicitly says **September 10**, retroactive for receipts issued after September 1, with a connected account and technical-user permission **“Hozzáférés az nyugtaadat-szolgáltatási interfészhez”**. It also contains other rollout prose that is not fully internally aligned.

**Disposition:** record the newer dated HU statement and prerequisites; do not repeat the Agent page's no-action statement as a settled current operational fact. None of these pages adds a receipt XML field or NAV verdict to the response, so no missing Rust wire capability is established. This review did not query NAV or verify any account's reporting state.

## Tests, fixtures and provenance

- `fixtures/SOURCES.md:15–39,52–55,74–85` separates workspace-only upstream files (historical July acquisition unless annotated) from packaged synthetic data and project-generated golden XML. Both upstream and synthetic crate directories are symlinked corpora. Current GETs above were independent of those cached files.
- Golden files `tests/golden/xmlnyugtacreate.xml`, `xmlnyugtast.xml`, `xmlnyugtaget.xml`, `xmlnyugtasend.xml` (each line 1) cover canonical default-shaped writes, not every optional combination or server acceptance.
- Receipt unit tests `receipt.rs:929–1478` cover dates across three operations, defaults/goldens, automatic MNB emission, unsupported invoice fields, optional metadata ordering, order selector, templates/call ID, response fields/tenders/totals, test marker, PDF decoding and errors. They are synthetic/local, not live receipt tests.
- `receipt_wire.rs:39–149` independently observes **container presence**, omitted children and empty strings; `152–190` preserves the documented fractional-net/VAT whole-gross example. This closes the empty-container blind spot of the outline comparator.
- `upstream.rs:328–400,852–962,1189–1203` rebuilds the receipt requests and parses historical examples. The response test first rejects placeholder base64, then **explicitly substitutes** a synthetic `%PDF-` payload; no restored real PDF is claimed. Request outlines ignore empty containers and trim text (`1225–1234`); they are not byte fidelity or XSD validation.
- `business_text.rs:49–85` asserts receipt call ID/order/reference, bank, ledger and tender description preservation. `numeric_fidelity.rs:50–95,112–150` covers receipt numeric VAT and exact-domain failures. `error_classification.rs:49–99,163–183,185–240` checks source-derived receipt codes and missing-data semantics.
- Existing completion/namespace suites reach receipt query/send entry points; some checks are invoice-specific. Passing those checks is not a new proof of every receipt namespace/path permutation. Scratch adds receipt-specific foreign-reversal and SN/PDF controls.
- There is no receipt live-account fixture or executed create→query→storno→query→send lifecycle in this evidence. `fixtures/SOURCES.md:251–256` names the actual live-suite operations; none establishes receipt execution, email delivery or rendering.

### Checks run

```text
cargo test --locked --offline -p szamlazz-agent --lib ops::receipt::tests
  27 passed

cargo test --locked --offline -p szamlazz-agent --test receipt_wire --test numeric_fidelity --test business_text --test response_completion --test response_namespaces --test error_classification --test upstream
  receipt_wire: 4; numeric_fidelity: 6; business_text: 2
  response_completion: 2; response_namespaces: 6
  error_classification: 3; upstream: 11 — all passed

cargo run --offline --manifest-path /tmp/opencode/current-receipts-fbda137/Cargo.toml --target-dir /tmp/opencode/current-receipts-fbda137/target
  H1/H2 reproductions and control assertions passed

cargo tree --locked --offline -p szamlazz-agent --depth 1
  Compared workspace direct runtime versions with scratch resolution
```

**61 existing tests passed**, including shared/invoice cases, not 61 receipt-only tests. Upstream corpus was present. Scratch used the pinned path source with the same direct runtime versions (quick-xml 0.42.0, jiff 0.2.35, rust_decimal 1.43.0, serde 1.0.229, base64 0.23.1, percent-encoding 2.3.2, thiserror 2.0.20); it has an independently resolved lock and is not claimed identical in every transitive feature/version.

The first PHP-fetch command used unavailable `python`; it was rerun successfully with `python3`. Python `lxml` was unavailable. **No XSD-validator execution is claimed**: field sets/types/cardinalities/sequences were compared directly with freshly fetched schemas. No live tests or PDF renderer ran. Normal Cargo build artifacts are not source/test edits.

## Closure-check of the stale report

The earlier `docs/review/2026-09-10-agent-api-receipts.md` was read only after independent source comparison and initial checks.

| Earlier item | Current result / precise evidence |
|---|---|
| D1: repeated receipt storno refusal missing from docs | **Closed.** `receipt.rs:304–310`, `recovery.md:16`, README `196` explicitly state both already-reversed and SN-target refusals and retain unknown storno-specific 338 behavior. |
| D2: independent receipt order toggle omitted | **Closed as requested.** `receipt.rs:142–148,376–380`, `recovery.md:31–34`, README `194` explain/link the receipt setting. V3 records a newly checked vendor contradiction, not absence of this guidance. |
| H1: requested PDF missing without diagnostic | **Still applicable as optional hardening**, current locations `receipt.rs:690–701`; expanded with whitespace-only zero-byte `Some` reproduction. |
| F1: required receipt date | Still closed: `receipt.rs:772–773`, date matrix `929–980` passed. |
| F2: receipt error codes | Still closed: named catalogue and source-derived error tests passed. |
| R2: business text | Still closed: adapters at `receipt.rs:756–795,807–819,831–836,871–872`; receipt business-text test passed. |
| R1: complete XML / namespace | Existing fixes still exercised successfully, including receipt query/send. No exhaustive shared-parser re-audit inferred. |
| D3: operation-specific recovery | Still closed: `recovery.md:15–20,22–34`, README `155–217`. |
| D4: receipt rounding | Still closed: `receipt.rs:104–108`, `item.rs:15–18,84–87,173–174`, `receipt_wire.rs:152–190`. |
| B.E03: query call ID/order interpretation | Still closed: `receipt.rs:373–404`, normal query emission test; no unsupported call-ID-only selector. |
| B.E04: email assumptions | Still closed: `receipt.rs:454–499`, README `217`; no independent merge/multiple-recipient promise. |
| B.E05: MNB provenance | Still closed: `types.rs:966–973` matches independently fetched receipt-specific PHP comments. |
| E.E6: fixture provenance | Still qualified correctly: `fixtures/SOURCES.md:121–140,182–191,230–247`. No transformation inferred from an unexplained cached difference. |
| Receipt item restrictions | Still closed: all five invoice-only fields refused, supported receipt ledger fields accepted (`receipt.rs:1081–1148`). |
| Earlier NAV uncertainty row | **Needs current source qualification**, supplied by V4: newer linked HU information now describes automation starting today and its permission prerequisite. No missing XML capability follows. |

## Official source acquisition register

All cited URLs were fetched during **this** review. Docs pages reported build `v202608271632`; that footer is not an acquisition timestamp or a guarantee that linked knowledge-base content shares the same revision. Code blocks were read in the fetched pages, including example and inline-XSD tabs.

### Entry points and operation pages

Fetched all four requested categories: [create category](https://docs.szamlazz.hu/agent/category/generating-a-receipt), [storno category](https://docs.szamlazz.hu/agent/category/reversing-a-receipt), [query category](https://docs.szamlazz.hu/agent/category/querying-a-receipt), [send category](https://docs.szamlazz.hu/agent/category/sending-a-receipt), and [receipt settings/rules index](https://docs.szamlazz.hu/agent/generating_receipt/settings-and-rules).

| Operation | Request | Response | Example and inline request XSD |
|---|---|---|---|
| Create | [EN][C-request] | [EN][C-response], [HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/response), shared response example/XSD | [EN][C-xml], [HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/xml) |
| Storno | [EN][S-request] | [EN][S-response], [HU](https://docs.szamlazz.hu/hu/agent/reversing_receipt/response); no independent response example | [EN][S-xml], [HU](https://docs.szamlazz.hu/hu/agent/reversing_receipt/xml) |
| Query | [EN][Q-request] | [EN][Q-response], refers to create response | [EN][Q-xml], [HU](https://docs.szamlazz.hu/hu/agent/querying_receipt/xml) |
| Send | [EN][E-request] | [EN][E-response], success/error examples and inline response XSD | [EN][E-xml], [HU](https://docs.szamlazz.hu/hu/agent/sending_receipt/xml) |

### Downloadable XSDs

| Download fetched | Result |
|---|---|
| [Create][X-create] | 200 XML, omits `torloKod` |
| [Storno][X-storno] | 200 XML, matches documented sequence |
| [Query][X-query] | 200 XML, omits `rendelesSzam` |
| [Send][X-send] | 200 XML, matches documented sequence |
| [Receipt response][X-response] | 200 XML, adds `TEHK` versus inline enumeration |
| [Send response][X-send-response] | 200 XML, verdict-only |

Embedded example locations fetched over HTTPS and returned **404**:

- <https://www.szamlazz.hu/docs/xsds/nyugtast/xmlnyugtast.xsd>
- <https://www.szamlazz.hu/docs/xsds/nyugtaget/xmlnyugtaget.xsd>
- <https://www.szamlazz.hu/docs/xsds/nyugtasend/xmlnyugtasend.xsd>
- <https://www.szamlazz.hu/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd>

Two attempted send-response alternatives also returned 404: `/szamla/docs/xsds/nyugta/xmlnyugtasendvalasz.xsd` and `/szamla/docs/xsds/nyugtasendvalasz/xmlnyugtasendvalasz.xsd`. The working `/nyugtasend/` location above was fetched successfully.

### Rules and supporting first-party examples

- All five rules, **both EN and HU**: [order][Order] ([HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/order-number)), [template][Template] ([HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/pdf-template)), [erasure][Erasure] ([HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/data-erasure-code)), [amounts][Amounts] ([HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/item-amounts)), [NAV][NAV] ([HU](https://docs.szamlazz.hu/hu/agent/generating_receipt/settings_and_rules/nav-data-reporting)).
- Linked [currencies][Currencies], [VAT rates](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), [invoice order rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number), [general error rules][Errors] and [sending requests][requests]. Invoice-only OSS/`eusAfa`/replay rules were not imported into receipt contracts.
- Linked knowledge-base [order][KB-order], [erasure](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor), [NAV EN][KB-NAV-EN] and [NAV HU][KB-NAV-HU]. Product marketing, account-login links and the separate NAV/e-cash-register products are beyond this XML-contract review.
- PHP [create](https://docs.szamlazz.hu/php/nyugta-generalas), [storno](https://docs.szamlazz.hu/php/sztorno-nyugta-generalas), [send](https://docs.szamlazz.hu/php/nyugta-kuldes), [query][PHP-query] and [PDF][PHP-PDF].
- Fresh [PHP 2.12.4 ZIP][PHP-zip], read in memory with Python `urllib`/`zipfile`: `Header/ReceiptHeader.php`, `Header/ReverseReceiptHeader.php`, `Item/ReceiptItem.php` and **all seven** `examples/document/receipt/*.php` examples (default/custom/erasure create, storno, data query, PDF query, send). Comments are documentary evidence; these are not observed server results.

[C-request]: https://docs.szamlazz.hu/agent/generating_receipt/request
[C-response]: https://docs.szamlazz.hu/agent/generating_receipt/response
[C-xml]: https://docs.szamlazz.hu/agent/generating_receipt/xml
[S-request]: https://docs.szamlazz.hu/agent/reversing_receipt/request
[S-response]: https://docs.szamlazz.hu/agent/reversing_receipt/response
[S-xml]: https://docs.szamlazz.hu/agent/reversing_receipt/xml
[Q-request]: https://docs.szamlazz.hu/agent/querying_receipt/request
[Q-response]: https://docs.szamlazz.hu/agent/querying_receipt/response
[Q-xml]: https://docs.szamlazz.hu/agent/querying_receipt/xml
[E-request]: https://docs.szamlazz.hu/agent/sending_receipt/request
[E-response]: https://docs.szamlazz.hu/agent/sending_receipt/response
[E-xml]: https://docs.szamlazz.hu/agent/sending_receipt/xml
[X-create]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd
[X-storno]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtast/xmlnyugtast.xsd
[X-query]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd
[X-send]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasend.xsd
[X-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[X-send-response]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtasend/xmlnyugtasendvalasz.xsd
[Order]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number
[Template]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/pdf-template
[Erasure]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/data-erasure-code
[Amounts]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts
[NAV]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/nav-data-reporting
[Currencies]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies
[Errors]: https://docs.szamlazz.hu/agent/basics/error-handling
[requests]: https://docs.szamlazz.hu/agent/basics/sending-requests
[KB-order]: https://tudastar.szamlazz.hu/gyik/rendelesszam-a-nyugtan
[KB-NAV-EN]: https://tudastar.szamlazz.hu/en/gyik/mandatory-receipt-data-reporting
[KB-NAV-HU]: https://tudastar.szamlazz.hu/gyik/nyugtaadat-szolgaltatas-kotelezettseg
[PHP-query]: https://docs.szamlazz.hu/php/nyugta-lekerdezes
[PHP-PDF]: https://docs.szamlazz.hu/php/nyugta-pdf
[PHP-zip]: https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip
