# Independent reviewer A — parser findings, round 2

**2026-09-09 · reviewed HEAD `a804c740eb8446211c1cdca3eea4fb93d298d25d`.**

Scope: F1, F3, R1 and R2 from [REVIEW.md](../REVIEW.md), challenged against the implementation, [adjudication](../ADJUDICATION.md), raw reports [02](../raw/02-query-responses.md), [03](../raw/03-receipts.md), [04](../raw/04-other-operations.md), [05's comma-header exclusion](../raw/05-wire-errors.md#accepted--excluded-from-findings), current primary sources, and new offline countercases. This is specialist evidence for the subsequent judge, not the final adjudication.

## Conclusions

**Retain all four, with narrower implementation contracts than the prior recommendations. No P0/P1 is established.**

| ID | Priority / classification | Confidence finding exists | Confidence in recommended fix | Live occurrence |
|---|---|---|---|---|
| F1 | P2 — supported-response lexical interoperability defect | **High:** current schemas allow timezones and XML whitespace; all three affected reader families fail locally | **High** for contemporary civil-date adaptation; exact grammar candidate tested; no claim to all XSD date values | **Unobserved:** no suffixed-date or whitespace-date incident in the live record |
| F3 | P2 — conditional monetary-header fallback defect | **High:** live-recorded `100,01` fails in the shared fallback; normal body totals mask it | **High:** explicit finite numeric grammar, comma as decimal, tested offline; grouping policy must be stated honestly | **Conditional:** comma create-net format observed; missing-body + comma compound failure unobserved |
| R1 | P3 — document-boundary and taxpayer extraction robustness defects | **High:** independent truncation, failure-to-success, subtree, entity and trailing-content reproductions | **Medium:** boundary mechanism tested and namespace paths established; complete taxpayer rewrite not implemented/verified here | **Unobserved:** malformed/out-of-contract replies only; no normal taxpayer incident established |
| R2 | P3 — response-text fidelity defect | **High:** padding and even NBSP-only content lost; schema string semantics and receipt promise corroborate | **High:** separate optional-text adapter tested through the current XML deserializer; field-by-field migration required | **Unobserved:** no live padded receipt identifier or text case; identity collision is not established |

Priority reflects bounded impact and repair value, not observed frequency. F1/F3 can hide completed writes; R1's striking synthetic failure-to-success case is not evidence of a live service malfunction or an authentication bypass.

### Changes to the prior advice

1. **F1:** Jiff is not simply a *stricter* XSD date parser. It also accepts non-XSD `20260109` and `2026-01-09[foo]`. A fallback that tries Jiff first and only strips a suffix on failure does not validate the whole lexical form. Required credit-entry dates also reject surrounding XML whitespace, just like receipt dates.
2. **F3:** “accept a single decimal comma, reject ambiguous grouping” is underspecified. `1,234` is indistinguishably a decimal or a grouped integer without an external convention. Choose **ungrouped numbers, comma or dot means decimal**; `1,234` means `1.234`, never `1234`. Do not pretend to detect the sender's intent. Preserve scientific notation. Decimal also accepts underscores (`1_234`), so conversion alone is not the promised header grammar.
3. **R1:** version-aware namespace/path extraction is needed, but complete-document checking is a distinct shared responsibility. Consuming until EOF is insufficient unless an open root at EOF, extra roots, and outside-root content are checked. Current taxpayer tests include out-of-schema paths and a fake 3.0 namespace substitution; blindly preserving those tests would retain the defect.
4. **R2:** preserve **decoded XML character content**, not original XML bytes. XML newline normalization and entity/CDATA decoding remain correct. Separate business strings from numerical text, verdict codes, base64 and the deliberately normalized numbered-success envelope.

## F1 — date spelling loses an otherwise useful response

### Evidence and scope

Current [queried-invoice XSD][invoice-xsd] declares eleven date positions as `date`; the current [receipt response][receipt-response] declares `alap/kelt` as `date`. [XSD 1.0 Datatypes][xsd], §§3.2.9.1, 3.2.7.3 and 4.3.6, allows an optional `Z` or `±hh:mm` and fixes date whitespace to `collapse`.

Source surface, relative to `crates/szamlazz-agent/src/`:

| Reader | Fields | Current path |
|---|---|---|
| `ops/query_xml.rs:703–708` | `alap/{kelt,telj,fizh}` | Generic `xml::de::empty_as_none` |
| `ops/query_xml.rs:810–825` | `vevo/fokonyv/{datum,elszDatTol,elszDatIg}` | Same optional helper |
| `ops/query_xml.rs:957–960,994–997` | Item and financial-item `{elszdattol,elszdatig}` | Same optional helper |
| `ops/query_xml.rs:1034–1039` | `kifizetesek/kifizetes/datum` | Direct required Jiff serde |
| `ops/receipt.rs:735` | `nyugta/alap/kelt` | Direct required Jiff serde; create/storno/query share `parse_receipt:651–664` |

New probes independently confirm ordinary dates succeed, and `Z`, `+02:00`, `-00:00`, `+14:00`, `-14:00` fail on optional invoice issue date, required credit-entry date, and receipt issue date. Tab/newline/CR/space padding succeeds on the optional helper but fails on both required readers. Invalid dates and multibyte text return errors today; **no current panic was found**.

Counterexamples to an overly simple fix: all three current readers accept `20260109`, `2026-01-09[foo]`, and year `0000`. Optional dates accept NBSP padding because `str::trim` is Unicode-wide. These are not new ranked incidents, but prove that delegating the entire grammar to Jiff does not establish XSD lexical checking.

### One best solution

Add a **private, explicitly bounded XSD-date-to-civil-date adapter** in `xml::de`, with one pure parser and required/optional serde wrappers. Use it at all twelve positions above. Keep public `Date`/`Option<Date>` and the current optional absence policy.

For the contemporary invoicing domain, state the supported calendar range as **CE years 0001–9999** rather than advertise a complete XSD date value implementation. The adapter checks:

- Trim only XML whitespace `#x20`, `#x9`, `#xD`, `#xA`. Collapse does not permit interior spaces in a date: any interior XML whitespace remains invalid, so edge trimming plus a whitespace-free grammar suffices. NBSP and other Unicode whitespace are content, not date padding.
- Exactly `YYYY-MM-DD`, then nothing, uppercase `Z`, or exactly `[+-]hh:mm`. ASCII digits, two-digit month/day/hour/minute; no bracket annotations, datetime suffix, basic format, trailing junk or interior padding.
- Offset magnitude hours `0..=14`, minutes `0..=59`, and minute **zero when hour is 14**. `+00:00`, `-00:00`, `Z` are all allowed. Do not use a general timezone parser that accepts seconds, `+0200`, lowercase `z`, or offsets through 23 hours.
- Validate the actual Gregorian date through fallible `Date::new`. Optional absent/empty/XML-blank → `None`; required empty/missing → error; invalid nonempty dates → error. No Adatkapcsolat-style invalid-date-to-`None` policy in this crate.
- Check byte slices with `get` and prove ASCII structure before any string indexing, or operate entirely on checked bytes. No unchecked UTF-8 split and no `unwrap`/`expect` on wire content.
- **Return the printed calendar date**, discarding the validated offset. Say so in the public query/receipt date documentation. XSD timezoned date equality/canonical normalization is not preserved by this business projection; shifting to UTC would change a printed fulfillment/issue date.

The range decision should be visible in the implementation/docs review: rejecting previously accepted Jiff year-zero, negative/extended-year spellings or Temporal annotations is an acceptance tightening, not a claim those caused the retained defect. Full astronomical/BCE conversion is unnecessary for this repair and should not be smuggled in via Jiff's ISO year interpretation. A scratch candidate for the declared CE domain passed the grammar/offset/calendar/UTF-8 cases.

**Rejected alternatives:** suffix truncation accepts junk and may panic; direct Jiff-first fallback admits non-XSD annotations; making every date optional loses required receipt/credit data; UTC conversion changes domain dates; replacing public fields with zoned date types is disproportionate and source/serde breaking; a full XSD validator would refuse intentional sparse replies.

**API impact:** no public type/JSON shape change. Broader acceptance of useful date spellings, deliberate tightening of non-XSD spellings and the stated year domain. Request writers and worker storno-date derivation do not change.

**Acceptance criteria:** parameterize the eleven invoice paths and receipt `kelt` independently so a forgotten annotation is caught. Assert the same civil date for plain/Z/positive/negative/zero/±14:00 forms and XML padding. Include leap day, impossible day, ±14:01, ±15:00, `+13:60`, `+2:00`, `+0200`, lowercase z, suffix junk, datetime, annotations, NBSP, short strings and multibyte boundary text. Optional missing/blank stays `None`; required blank/missing fails. Exercise receipt create/storno/query entry points with the same complete canonical fixture, preserving sparse surrounding fields. These tests need no live account.

## F3 — observed header format, conditional failed fallback

### Evidence and scope

`docs/szamlazz-hu-behaviour.md:160` records create net header **`100,01`**, P60-E1/E3. The fresh [invoice response page][invoice-response] calls monetary headers non-URL-encoded and makes XML amounts optional. Its prose also says the same data accompanies XML responses: this argues that the compound trigger is uncommon, not that the implemented fallback can ignore its own accepted channel.

`ops/envelope.rs:146–155,285–303` sends body and header values through the same ordinary Decimal conversion. Used by creation/storno/PDF and by `ops/credit_entry.rs:250–277`. New probes of storno and credit confirm failures for comma net, gross and outstanding; body absent, self-closing or blank activates fallback. Body `9.5` wins over bad/comma header; malformed nonblank body stays an error and does not fall back. A numbered header-only 56 remains success but loses comma net (`None`).

No record establishes header-only fractional storno, credit or PDF emission. Do not call this an observed completed-write failure, and do not count each operation as an independent finding.

I independently reject raw 05's exclusion: supporting the recorded comma format preserves live-backed behavior. The absence of a formal decimal-separator grammar in the current docs limits claims about the vendor, but does not make an independently reproduced defect in the crate's header fallback intentional.

### One best solution

Keep body parsing separate; add **one private ungrouped monetary-header parser** used by `parse_decimal_header` for all three headers. Keep the current body-first and 56 policies. Its explicit finite grammar is:

```text
OWS   := SP | HTAB
digit := ASCII 0..9
value := [+-]? (digit+ ([.,] digit*)? | [.,] digit+) ([eE] [+-]? digit+)?
header := OWS* value OWS*
```

Validate the whole grammar, replace the single mantissa comma with dot when present, then convert through the existing Decimal parser. Retain its finite representability policy; do not pass through `f64`, round to cents, parse grouping or URL-decode numbers. This accepts currently useful integer/dot/exponent cases and `100,01`, `-127,50`, `1,234e2`. The header's OWS is HTTP space/tab, not all Unicode whitespace or line breaks.

**Grouping decision:** `1,234 → 1.234`, `1.234 → 1.234`. Neither can be distinguished from grouping by looking at the characters. Reject mixed/repeated separators (`1,234.56`, `1.234,56`, `1,234,567`), embedded spaces, underscores, currency signs and exponent punctuation. Do not reject exactly three fractional digits as “grouping”: no current source establishes a two-decimal lexical cap for every monetary header. The live two-decimal storage observation is not a general header grammar. The convention is a reader policy; the vendor has not published a formal grammar.

Keep **present blank header = error**, absent header = `None`, as today. There is no evidence blank headers are intended absence; changing that is not necessary to fix commas. On numbered 56, malformed optional metadata still becomes `None`, while now-valid commas survive. Do not weaken the ordinary-success malformed-amount policy as part of this fix.

**Rejected alternatives:** unconditional comma replacement followed by Decimal also accepts underscore grouping; stripping punctuation risks changing amounts by orders of magnitude; guessing locale from currency is wrong; capping comma fractions to two digits invents a rule; refusing exponent syntax regresses existing usable input; dropping all malformed monetary metadata hides a different class of defect; global XML comma support accepts non-XSD monetary bodies.

**API impact:** private implementation only, unchanged public types and body/error precedence. Header acceptance broadens for commas and tightens for undocumented underscore/Unicode-padding syntax. Mention this explicitly in the release note rather than claim every previously parsed string stays accepted.

**Acceptance criteria:** each of net/gross/outstanding on header-only numbered success, signed and zero comma values; ordinary dot/exponent and comma-mantissa exponent; the `1,234` convention; mixed/repeated separators, underscore, missing mantissa/exponent, embedded whitespace, enormous exponent and overflow as controlled errors. Absent/blank header distinction; absent/empty/blank body fallback; valid body precedence; malformed body not rescued; numbered 56 accepts comma metadata and still drops genuinely invalid optional values. One shared grammar table plus operation-level checks of issuance, credit balance and PDF is sufficient—do not clone the entire table into every operation.

## R1 — one complete document, then scoped taxpayer extraction

### Evidence and classification

`xml::response_root:63–108` checks UTF-8 and only the first start tag. Serde consumers (`xml::verdict_text`, `valasz`, `ops/envelope::parse_envelope`, XML query) consume their first value without rejecting a trailing root/text/broken tag. Taxpayer's `TaxpayerResponse::from_body:202–285` then walks local names until EOF, without checking root closure; `set:287–338` assigns leaves irrespective of their path, and unknown general entities disappear at `245–258`.

All original cases reproduced independently. Additional countercases:

- **One root with duplicate `result/funcCode`:** `ERROR`, error code 57, then `OK`, plus validity=true → success. A whole-document-only fix misses this.
- `O&bogus;K` becomes successful `OK`; a name `A&bogus;B` becomes `AB`.
- Taxpayer accepts leading/trailing ordinary text, leading NBSP, and trailing CDATA. Deletion rejects leading ordinary text in serde, but accepts trailing text/CDATA/another root/broken tag. **Do not generalize taxpayer's leading-text or verdict-overwrite behavior to serde paths.**
- Legal comments, PIs, entities and CDATA concatenate correctly today. Preserve that control.
- An empty self-closing address item is ignored while a nonempty-start item allocates a row; the rewrite should make empty-tag handling explicit, not use `Event::Start` as a semantic proxy for presence. This is not another high-impact incident.

[XML 1.0][xml-spec] requires one root with `Misc*` after it: **comments, PIs and literal XML whitespace only**. CDATA or `&#32;` outside the root is not legal merely because it decodes to whitespace. HTTP framing failures are caught before parsing (`client.rs` collects the body); the truncation reproduction requires a successfully collected truncated body or direct `RawResponse`.

### One best solution

Keep quick-xml and introduce a **complete-response boundary in shared XML plumbing plus a small versioned NAV extraction state machine**. These are two responsibilities of the same repair, not interchangeable options.

1. **Shared boundary:** evolve `response_root` to retain the matched root index and scan through EOF before returning text. Track before-root/in-root/after-root and depth, including empty roots. Enable closing-name and comment checks. Reject open depth at EOF, second roots, malformed tails, unmatched ends, ordinary outside text, outside CDATA/references, and misplaced/repeated XML declarations. Permit an initial UTF-8 BOM, legal declaration and surrounding comments/PIs/XML whitespace. Keep existing bounded diagnostics/error types. The root URI check remains exact and prefix-independent.
2. **Do not equate quick-xml's `enable_all_checks` with complete XML validation:** its documented flags only cover comments and end names. Explicitly handle the document phases and references. Where attributes are walked, use checked attribute iteration and proper unescaping; do not silently treat malformed namespace/attribute syntax as an unknown field. Check numeric character references for the XML character domain, not only Rust `char` representability. Keep one reference-decoding policy for the pull-parser path: five XML predefined entities and valid decimal/hex character references, undefined names → parse error. No entity deletion.
3. **Declare DTD-based entities unsupported**, returning a controlled parse error on a DOCTYPE rather than adding a DTD resolver or external I/O. None of the supported response documents relies on DTDs. This is an explicit protocol-parser restriction, not a claim that every XML document with a DTD is malformed. The shared scan must still visit unknown subtree content sufficiently to detect truncation and undefined references; a blind `read_to_end` skip cannot be the only validation.
4. **Taxpayer extraction:** replace the plain `Reader`/`in_address`/single mutable `content` approach with `NsReader`, a root-selected version description and a small stack/state for recognized containers and scalar leaves. Match each recognized **expanded name at its parent path**. Unknown element → suppress extraction for its complete subtree; never promote recognized grandchildren out of that subtree. Known scalar containing child elements → parse error rather than invent concatenated scalar content. Text, CDATA and references belong to that scalar alone; comments/PIs do not clear its buffer.
5. Reject repeated recognized singleton values/containers that would otherwise choose one answer (especially `result`, `funcCode`, validity, taxpayer data/name/tax number); track presence even for empty elements. Only address items repeat. This is projection disambiguation, not full ordering/cardinality/facet validation. Keep absent business fields optional, unknown tokens open, `valid=false` as success, required validity on OK, and non-OK as the existing API error. Build each address in its own context and append at its end so no fields leak between rows.

Versioned extraction paths from the fresh [Számla Agent examples][taxpayer-response], [NAV API schema][nav-api] and [Common 1.0 schema][nav-common]:

| Location relative to root | NAV 2.0 | NAV 3.0 |
|---|---|---|
| Root | `OSA/2.0/api:QueryTaxpayerResponse` | `OSA/3.0/api:QueryTaxpayerResponse` |
| `result/{funcCode,errorCode,message}` | API namespace for container and leaves | `NTCA/1.0/common` for **both result and leaves** |
| `taxpayerValidity`, `taxpayerData`, name, tax-number-detail container, address-list/item/type/address containers | API namespace | API namespace |
| `taxNumberDetail/{taxpayerId,vatCode}` | `OSA/2.0/data` | `OSA/3.0/base` |
| Direct address components inside `taxpayerAddress` | `OSA/2.0/data` | `OSA/3.0/base` |

All abbreviated URIs above have the prefix `http://schemas.nav.gov.hu/`. A schema *type* being from Common does not move the element declared by the API into Common: e.g. `taxpayerName` stays API. Select the layout from the root URI, not prefix spelling, `requestVersion`, or a union permitting every known namespace everywhere.

Do not introduce NAV header/software requiredness or check every supplied headerVersion/requestVersion: those are outside the public projection and sparse official errors omit software. Likewise do not broaden accepted root names to direct NAV GeneralErrorResponse without Számla Agent forwarding evidence.

**Existing-test trap:** `taxpayer.rs:408–417` substitutes `2.0 → 3.0`, leaving nonexistent `3.0/data` as a purported 3.0 layout. Address tests at `465–529` put items directly under the root and components in the API namespace. Replace those synthetic structures with declared parent paths and actual namespace assignments while retaining their field assertions. The public `additional_address_detail` extension can remain as a narrowly recognized leaf under the established address path; its existence does not justify arbitrary nested wrappers or root-level addresses. No live evidence establishes those misplaced layouts as intentional compatibility.

**Rejected alternatives:** only checking EOF/depth leaves path and duplicate overwrites; only switching to NsReader leaves path collisions; serde on local names alone misses expanded-name scoping; a DOM/XSD dependency adds breadth and memory with no need for schema facet enforcement; wildcard namespace matching preserves false matches; blanket strict XSD rejects sparse useful responses; returning the first verdict silently ignores an ambiguous second one.

**API impact:** no public signature/shape change. Malformed/ambiguous replies formerly accepted now return existing parse errors; well-formed foreign/unknown subtrees are ignored, so a wrong-namespace optional field does not populate the result, and a missing recognized verdict/validity still fails. Preserve header-error/down/status precedence and numbered header-56 fallback, which intentionally permits non-XML bodies; the new boundary applies when XML is being interpreted, not as an unconditional guard before header handling.

**Acceptance criteria:** original five taxpayer triggers; duplicate ERROR→OK inside one root; truncate at root/container/scalar boundaries; mismatched closings; empty roots/tags; trailing root/text/broken markup and outside CDATA/reference/NBSP. Genuine 2.0 and 3.0 examples with arbitrary prefixes/default changes; real false result; body-only numeric and symbolic errors; header error over malformed/success body. Unknown subtree at every recognized container depth must not overwrite any result. Wrong namespace must not populate recognized fields. Two differently populated addresses must remain independent. Predefined/numeric entities, literal ampersand in CDATA, CRLF versus `&#13;`, interior comments/PIs, and legal document prolog/epilog controls. Undefined entities, illegal character references and unsupported DTDs must fail without fetching anything. Exercise the shared boundary through query, envelope, receipt and deletion paths; do not infer its integration from a standalone scanner test.

## R2 — optional text must have an explicit preservation policy

### Evidence and scope

`xml::de::empty_as_none:269–283` calls `str::trim` before `FromStr`. For `String`, this irrevocably deletes leading/trailing characters. This affects receipt call/order/original numbers, comments, item ids, bank/ledger strings and tender descriptions; many invoice optional strings share it. Required strings and `query_xml::empty_invoice_number:1064–1071` already preserve nonblank text. Taxpayer's separate `content.trim()` has the same fidelity effect on business strings.

New probes confirm space-padded call id/order/comment become unpadded, NBSP padding is stripped, and an NBSP-only string becomes `None`. [XSD string][xsd] has preserve semantics; [receipt order documentation][receipt-order] expressly promises the same sent value. No receipt live probe establishes normalization, collision or identity aliasing. The recorded invoice create-order trim (`behaviour.md:40–42`) remains a server observation, not a reason to trim arbitrary returned text or alter worker normalization.

### One best solution

Add a private **optional text helper** that deserializes `Option<String>`, returns `None` for absent/empty/**XML-whitespace-only** content, and otherwise returns the original **decoded string**. Migrate business text/identifiers explicitly; keep scalar parsing and protocol normalization separate.

| Field family | Recommended handling |
|---|---|
| Invoice/receipt comments, names, addresses, item/order/call identifiers, invoice references, bank/ledger/tax-id text, tender description | Preserve nonblank decoded text; optional XML-blank remains `None`. Preserve NBSP, including NBSP-only content. Required strings continue their existing direct handling |
| Optional open wire-string wrappers such as queried `PaymentMethod`, `Currency` | Read optional preserved text then map through the existing wrapper conversion; do not normalize unknown string payloads merely because the field has an enum/wrapper |
| `Verdict.hibakod`, taxpayer `funcCode`/`errorCode`, boolean/numeric/date text | Keep explicitly named token/scalar policy; padding acceptance is deliberate and must not depend on the business-text helper |
| Envelope number | Keep `nonblank_invoice_number`'s explicitly documented body/header trim; not part of the receipt call-id fix |
| Envelope raw monetary text/PDF and customer URL | Retain existing scalar/base64/URL-specific policy at the envelope boundary; do not change all `Option<String>` uses mechanically |
| Taxpayer business fields | Preserve scalar decoded text after path extraction; trim only to decide optional blank. Refactor away global `content.trim()` for every field |
| Worker `FoundDocument` order projection | Preserve its separate deliberate normalization unchanged |

The blank-to-`None` convention itself is a deliberate lossy projection, retained here. Do not claim byte-for-byte XML fidelity: `A\r\nB` becomes `A\nB` by XML normalization, while `A&#13;B` retains CR; entities/CDATA become characters. A new helper was tested through quick-xml serde: padding survives, NBSP stays, XML-blank becomes `None`, and newline/entity controls work without deserializer configuration changes.

**Rejected alternatives:** editing the generic helper to preserve everything regresses scalar/verdict consumers; documenting blanket trimming leaves needless identifier/free-text loss; trimming only ASCII space still deletes meaningful space padding; Unicode blank detection still destroys NBSP-only content; changing public strings into raw-XML wrappers is unnecessary; stripping whitespace at the XML reader level breaks text around comments/CDATA.

**Exact surface/API impact:** add helper in `xml.rs`; choose helper annotations/conversions in private `query_xml.rs` and `receipt.rs` wire structs; use the same policy in taxpayer leaf extraction. Audit `xml::totals` VAT-string and envelope uses rather than global replacement. Public `Option<String>`/wrapper shapes stay unchanged, but returned strings/serialized values gain previously discarded characters; NBSP-only business text changes `None → Some`. Release-note the semantic correction. No request serialization or live server-normalization change is warranted.

**Acceptance criteria:** full receipt with padded call/order/original number, item id, comment, ledger value and tender description; queried invoice free text, optional payment-method unknown token and optional reference; taxpayer padded name/address under correct namespace/path. Assert decoded characters exactly, including tabs, leading/trailing spaces, NBSP and entity/CDATA boundaries. Missing/empty/XML-blank stays `None`; NBSP-only stays `Some`; required-string behavior and numeric whitespace still work; padded verdict code still maps to the known error; envelope-number normalization and worker order projection remain deliberate. Do not use a live identity-collision scenario as a prerequisite to repairing demonstrated text loss.

## Verification, source freshness and limits

Read the live-backed behavior record directly and preserved it. Existing concurrent modifications were visible in Cargo.lock and other crates; I changed no production source or existing workspace tests and ran no workspace-wide checks. All executable work was under `/tmp/opencode/reviewer-a-parsers`; only this report is the requested repository deliverable. No live operation, account call or further subagent was used.

Fresh public GETs on 2026-09-09: invoice/receipt response pages, receipt order-number page, queried-invoice XSD, taxpayer response page, NAV API `master` schema and catalog-era Common 1.0 schema, W3C XML 1.0 and XSD Datatypes. Vendor pages report `v202608271632`; taxpayer examples still declare their 2020-11-04 provenance. The NAV declarations fetched match the previously cited `cc7a775…` line ranges for the relevant types; I do not infer live forwarding from that agreement.

Executed:

```sh
cargo run --offline --quiet --manifest-path /tmp/opencode/reviewer-a-parsers/Cargo.toml
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/reviewer-a-parsers/Cargo.toml --bin candidate
cargo run --locked --offline --quiet --manifest-path /tmp/opencode/reviewer-a-parsers/Cargo.toml --bin boundary
```

The first was run before the extra binaries were added; repeat it with `--bin reviewer-a-parsers`. It prints the actual crate parser results listed above, using only `AgentRequest::parse` on synthetic `RawResponse` values. Receipt amount aliases were replaced with canonical receipt tags to isolate date/text behavior. The candidate binaries assert proposed date/header lexical contracts and the complete-document mechanism/serde text-preservation countercases; both passed. Relevant dependencies were explicitly pinned to Jiff 0.2.35, Decimal 1.43.0, quick-xml 0.42.0.

These are mechanism probes, **not implemented production fixes, complete XML well-formedness certification, fresh full-XSD validation, or a full taxpayer rewrite test**. Their limits are why R1 fix confidence is Medium. Existing passing-suite counts from round 1 are not presented as new verification.

[invoice-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[receipt-response]: https://docs.szamlazz.hu/agent/generating_receipt/response
[invoice-response]: https://docs.szamlazz.hu/agent/generating_invoice/response
[receipt-order]: https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number
[taxpayer-response]: https://docs.szamlazz.hu/agent/querying_taxpayer/response
[nav-api]: https://raw.githubusercontent.com/nav-gov-hu/Online-Invoice/master/src/schemas/nav/gov/hu/OSA/invoiceApi.xsd
[nav-common]: https://raw.githubusercontent.com/nav-gov-hu/Common/Common-1.0.RC3/src/schemas/nav/gov/hu/NTCA/common.xsd
[xsd]: https://www.w3.org/TR/xmlschema-2/
[xml-spec]: https://www.w3.org/TR/REC-xml/
