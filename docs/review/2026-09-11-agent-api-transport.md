# Számla Agent shared-protocol audit — 2026-09-11

## Result and scope

Audited **HEAD `2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81`** against freshly fetched official documentation. The eleven documented multipart actions are present and correctly named. The complete current general error catalogue and numeric operation supplements are represented. No missing route, wrong default endpoint, missing response-version selector, or broken native cookie-isolation implementation was found.

**Actionable findings:**

| ID | Priority | Finding |
|---|---|---|
| T-01 | P2 | The officially supported key-in-both-legacy-fields authentication form leaks its key through `Credentials::Debug`, and consequently client/builder diagnostics. |
| T-02 | P2 | Numbered-56 leniency swallows structural failures of body identity as though they were optional metadata failures, then reports the header number as successful issuance. |
| T-03 | P3 | Error documentation attributes stronger protocol guarantees to the vendor than the freshly fetched sources establish. |

P2 means a concrete, bounded correction; neither T-01 nor T-02 was observed in a live vendor exchange. T-01 is reproducible with a documented credential form. T-02 is an offline response-integrity case, not evidence that szamlazz.hu emits that malformed reply. T-03 is a documentation/evidence correction, not a proposal to change credential-error classification without further evidence.

Owned scope: `crates/szamlazz-agent/src/{client.rs,wire.rs,credentials.rs,error.rs,recovery.md}`, README/platform claims, endpoint/multipart coverage and the shared response entry points. The envelope code was followed where necessary to audit transport evidence. This is not a field-by-field audit of every business model or a review of the Restate worker.

The worktree contained unrelated changes. `git diff --exit-code 2ba5fb86d9e3365a7c2e9bd99c4fce880fa1ab81 -- crates/szamlazz-agent docs/szamlazz-hu-behaviour.md Cargo.lock` passed before report creation. No historical review report supplied findings. The existing behavior note was read as the explicitly requested record of account evidence, with its limitations intact. No source/test edits or live account calls were made. The only repository addition by this audit is this report. Scratch code and freshly downloaded PHP source are under `/tmp/opencode/transport-2ba5fb8-*`.

No further delegation was performed; this session had no subagent tool. Full crate testing belongs to the lead audit. This report records only the targeted checks actually run here.

## Fresh evidence register

All web sources below were fetched on **2026-09-11**. The docs site displayed **`v202608271632`**. A page's current fetch date is not its underlying example's date: in particular, the taxpayer example still explicitly dates itself to 2020-11-04.

### Basics and supplements

| Ref | Primary source | Evidence used |
|---|---|---|
| B1 | [What is Számla Agent?](https://docs.szamlazz.hu/agent/basics/what-is), [how it works](https://docs.szamlazz.hu/agent/basics/how-does), [Agent index](https://docs.szamlazz.hu/agent/), [sitemap](https://docs.szamlazz.hu/sitemap.xml) | Public operation surface; proforma/delivery-note creation share invoice creation. |
| B2 | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests) | One HTTPS POST endpoint; all eleven form field names; XML must be a file; one document per creation XML; XSD/tag-case guidance. |
| B3 | [Authentication EN](https://docs.szamlazz.hu/agent/basics/authentication), [HU](https://docs.szamlazz.hu/hu/agent/basics/authentication) | Agent key preferred; lowercase, case-sensitive; legacy key-in-both-fields alternative; dedicated one-account username/password; no client-side keys. Keys do not expire, deletion is immediate, maximum 17, equal permissions. |
| B4 | [Session cookies](https://docs.szamlazz.hu/agent/basics/session-cookie) | Reuse `JSESSIONID`; expiration after 90 minutes of inactivity; fresh session advised after company/email edits; no-cookie requests reauthenticate. |
| B5 | [Error handling EN](https://docs.szamlazz.hu/agent/basics/error-handling), [HU](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Complete general code table; at most five sends of the same request; no until-success loop; v1 plain-text errors; test rate limit. |
| B6 | [Network/security](https://docs.szamlazz.hu/agent/basics/security) | Current destination and outbound IP information; no documented HTTP timeout or browser CORS guarantee. |
| S1 | [Invoice email](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification) | `attachfile1`–`attachfile5`, 2 MB/file; invalid attachments individually omitted/notified; attachments ignored without requested email; test email redirected to account-configured address. |
| S2 | [Invoice order number](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number) | Toggle, per-type checking, storno/corrective exemptions, reuse after reversal; replay requires matching buyer/gross/three dates and age ≤2 days. |
| S3 | [Receipt order number](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/order-number), [PHP receipt query](https://docs.szamlazz.hu/php/nyugta-lekerdezes) | Separate receipt toggle; number/order selectors; last matching document. |
| S4 | [Receipt amount rules](https://docs.szamlazz.hu/agent/generating_receipt/settings_and_rules/item-amounts) | Codes 261, 363–365; whole HUF gross, ≤2 decimals net/VAT, exact sum; separate receipt tolerance rules. |
| S5 | [Simplified image](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency) | Codes 551–556, including inherited final-invoice restrictions. |
| S6 | [Data erasure code](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | 400 per item, codes 537/539; general catalogue also covers 538. |
| S7 | [Proforma XML EN](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/deleting_pro_forma_invoice/xml) | Number/order selection; **HU explicitly says all matching proformas are deleted**, while EN omits that sentence. |
| S8 | [Receipt-send XML](https://docs.szamlazz.hu/agent/sending_receipt/xml) | Present empty email block requests previous details; absent block is documented as no email; children optional. |
| P1 | [PHP index/download](https://docs.szamlazz.hu/php/), [response processing](https://docs.szamlazz.hu/php/valasz-feldolgozas) | First-party PHP **2.12.4**, 2026-08-12; notification failure need not mean issuance failure. |

All eleven operation request and response pages were fetched, not merely their catalogue entries. Direct links are in the route inventory below. The response pages provide operation-specific errors: invoice XML/PDF 7; deletion 335; receipt creation 336–340; receipt-send's 7 example; taxpayer's 57 example. Receipt-storno describes three refusal cases without assigning numbers. The general table is therefore not the whole public code surface.

### First-party PHP artifact

Fresh download:

`https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip`

SHA-256: `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`.

Extracted under `/tmp/opencode/transport-2ba5fb8-php/PHPApiAgent-2.12.4/`. The following paths are relative to its `szamlaagent/src/szamlaagent/` directory:

- `SzamlaAgentRequest.php:480–499`: POST, XML `CURLFile`, MIME `text/xml`, action as form-field name.
- `SzamlaAgentRequest.php:507–515,527–555`: cookie jar/file, numbered attachment parts, configured total/connect timeouts. Its default timeout constant is **30 seconds** at line 30; the Rust 60-second policy is not copied from PHP.
- `Response/SzamlaAgentResponse.php:147–170`: nonblank `szlahu_down` checked before body processing. The PHP exception's 500 is not evidence that the wire HTTP status must be 500.
- `Response/InvoiceResponse.php:17,314–323,427–431`: code 56 plus a nonblank invoice number overrides the usual error decision. This corroborates the numbered warning; it does not validate arbitrary contradictory XML.
- `Response/InvoiceResponse.php:128–158,348`: number header retained; totals raw; error uses `urldecode`; buyer URL uses `rawurldecode` at assignment and `urldecode` again in its getter. PHP is useful corroboration, not an encoding specification to copy blindly.

### Local evidence

[`docs/szamlazz-hu-behaviour.md`](../szamlazz-hu-behaviour.md) describes one test account, chiefly paper invoices, with dated exceptions and unavailable raw logs (`:3–28,164–172`). Relevant observations:

- Headers/body differ by operation (`:137–145`): create/storno/delete error headers; XML query 7 and credit entry 463 body-only.
- External ids are nonunique, newest-holder queries, never echoed, attached only on actual creation (`:59–71`).
- Repeat invoice storno echoes the existing reversal; proforma/delivery-note storno can be a success-shaped no-op (`:82–98`).
- Deleted/consumed proformas disappear from query; deletion 335 does not distinguish never-existing/already-deleted (`:100–110`).
- Additive/replacing credit-entry behavior (`:129–135`).
- Stalled ≥57-second create, no issuance found in its order query, and failed attempts to trigger 56 (`:147–153`).

These observations are not re-established live by this audit. Some older *design consequences* in that file use stronger retry language than current `recovery.md`; the observation and its historical interpretation must not be conflated.

## Complete endpoint and multipart inventory

All routes: **POST `https://www.szamlazz.hu/szamla/`**, `multipart/form-data`, one main XML file part. `wire.rs:7–14,66–100`; `client.rs:374–405`. The XML filename is the action name; no `.xml` suffix is required by the fresh docs. `text/xml` matches PHP P1. Submit-button fields in sample HTML are not protocol selectors and need not be sent.

Code paths below are relative to `crates/szamlazz-agent/src/`. Each row's official **request/response** pair was read fresh.

| Operation/type | Multipart action | Request root; credential location; version | Response/entry point | Official pages |
|---|---|---|---|---|
| `CreateInvoice` — invoice/proforma/prepayment/final/corrective/delivery note | `action-xmlagentxmlfile` | `xmlszamla`; `beallitasok`; **2** | `xmlszamlavalasz` → issued or preview; `ops/invoice.rs:678–746,927–948` | [Request](https://docs.szamlazz.hu/agent/generating_invoice/request), [response](https://docs.szamlazz.hu/agent/generating_invoice/response) |
| `StornoInvoice` | `action-szamla_agent_st` | `xmlszamlast`; `beallitasok`; **2** | `xmlszamlavalasz` → numbered document; `ops/storno.rs:162–213` | [Request](https://docs.szamlazz.hu/agent/reversing_invoice/request), [response](https://docs.szamlazz.hu/agent/reversing_invoice/response) |
| `RegisterCreditEntry` | `action-szamla_agent_kifiz` | `xmlszamlakifiz`; `beallitasok`; **2** | `xmlszamlavalasz` → balance; `ops/credit_entry.rs:213–277` | [Request](https://docs.szamlazz.hu/agent/credit_entry/request), [response](https://docs.szamlazz.hu/agent/credit_entry/response) |
| `QueryInvoicePdf` | `action-szamla_agent_pdf` | `xmlszamlapdf`; **root**; **2** | `xmlszamlavalasz` → number plus required PDF; `ops/query_pdf.rs:58–93` | [Request](https://docs.szamlazz.hu/agent/querying_pdf/request), [response](https://docs.szamlazz.hu/agent/querying_pdf/response) |
| `QueryInvoiceXml` | `action-szamla_agent_xml` | `xmlszamlaxml`; **root**; no version field | `szamla` or error `xmlszamlavalasz`; `ops/query_xml.rs:535–588` | [Request](https://docs.szamlazz.hu/agent/querying_xml/request), [response](https://docs.szamlazz.hu/agent/querying_xml/response) |
| `DeleteProforma` | `action-szamla_agent_dijbekero_torlese` | `xmlszamladbkdel`; `beallitasok`; no version field | `xmlszamladbkdelvalasz` → `()`; `ops/proforma.rs:60–88` | [Request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), [response](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/response) |
| `CreateReceipt` | `action-szamla_agent_nyugta_create` | `xmlnyugtacreate`; `beallitasok`; no version field | `xmlnyugtavalasz` → receipt; `ops/receipt.rs:189–295` | [Request](https://docs.szamlazz.hu/agent/generating_receipt/request), [response](https://docs.szamlazz.hu/agent/generating_receipt/response) |
| `StornoReceipt` | `action-szamla_agent_nyugta_storno` | `xmlnyugtast`; `beallitasok`; no version field | `xmlnyugtavalasz` → storno receipt; `ops/receipt.rs:340–365` | [Request](https://docs.szamlazz.hu/agent/reversing_receipt/request), [response](https://docs.szamlazz.hu/agent/reversing_receipt/response) |
| `QueryReceipt` | `action-szamla_agent_nyugta_get` | `xmlnyugtaget`; `beallitasok`; no version field | `xmlnyugtavalasz` → receipt; `ops/receipt.rs:420–450` | [Request](https://docs.szamlazz.hu/agent/querying_receipt/request), [response](https://docs.szamlazz.hu/agent/querying_receipt/response) |
| `SendReceipt` | `action-szamla_agent_nyugta_send` | `xmlnyugtasend`; `beallitasok`; no version field | `xmlnyugtasendvalasz` → `()`; `ops/receipt.rs:502–532` | [Request](https://docs.szamlazz.hu/agent/sending_receipt/request), [response](https://docs.szamlazz.hu/agent/sending_receipt/response) |
| `QueryTaxpayer` | `action-szamla_agent_taxpayer` | `xmltaxpayer`; `beallitasok`; no version field | NAV `QueryTaxpayerResponse`, 2.0/3.0 handling; `ops/taxpayer.rs:264–284,317–338` | [Request](https://docs.szamlazz.hu/agent/querying_taxpayer/request), [response](https://docs.szamlazz.hu/agent/querying_taxpayer/response) |

The four selectable response formats all emit the **one** `ops::RESPONSE_VERSION` constant (`ops.rs:27–31`); they do not rely on the vendor's default of v1. The other seven are intrinsically XML operations, not missing a v2 setting. PDF inclusion is separately controlled (`szamlaLetoltes`, query XML's `pdf`, receipt `pdfLetoltes`). Receipt PDF retrieval is a query option, not a twelfth action. Neither the catalogue nor PHP's receipt-PDF convenience surface establishes a receipt-delete action; the receipt error supplement's word “delete” is not enough to invent one.

### Multipart and request encoding

- CRLF framing, blank line between part headers/content, final closing boundary: `wire.rs:66–100`. The main XML has both `name` and `filename`, avoiding documented code 53's non-file problem.
- The base boundary is deterministic, but collision detection scans generated XML **and all attachment bytes**, choosing a bounded numeric suffix (`wire.rs:109–131`). It searches any occurrence, more conservatively than only delimiter-position occurrences. No current payload collision defect found.
- Attachment disposition values strip CR/LF and encode quotes/backslashes; MIME values strip CR/LF (`wire.rs:78–107`). Raw attachment bytes are not XML-escaped or base64-encoded. Unusual filename interpretation by szamlazz.hu is unverified; the implementation is not a guarantee of byte-exact recipient filenames for metacharacters.
- Exactly `attachfile1` through `attachfile5` are generated, in collection order (`ops/invoice.rs:938–948`). `push`, `TryFrom<Vec<_>>`, and serde enforce five files and 2,000,000 bytes/file (`ops/invoice.rs:405–501`); immutable slice access does not bypass the bounds.
- **2 MB** is interpreted conservatively as decimal bytes, explicitly documented in code. The vendor does not give an exact byte count. This can reject a file allowed by a hypothetical 2 MiB interpretation; it is an evidence-backed conservative choice, not a proven compatibility bug.
- Local rejection of an oversized attachment is stricter than the vendor's “issue/send with valid attachments and notify separately for bad files” behavior (S1). A whole-call error before sending is intentional local validation, not evidence that the vendor would refuse the invoice.
- Attachments with `sendEmail=false` remain transmitted but the vendor documents ignoring them. No client claim says that transmitting them guarantees delivery.
- XML is UTF-8 with a 1.0 declaration, written through escaped text (`xml.rs:132–153,561–571`). `to_wire` validates first, then rejects non-UTF-8/XML-1.0 characters before multipart assembly (`wire.rs:402–429`). It does not perform full XSD validation; the vendor recommendation to validate XSD is not a claim that this crate does so.
- `AgentRequest` is intentionally implementable outside the crate. Its arbitrary `ACTION` and `write_xml` are trusted implementer inputs; action-string sanitization and full custom-XML structural validation are not provided. All eleven built-in actions are constant safe tokens. No built-in injection path was found.

## Complete numeric error-code inventory

**43 named numeric codes**, plus `Unknown(String)` and `Absent`. Of these, **30** are in the fresh general table B5, **7** additional codes occur in operation pages, **1** is corroborated by PHP, and **5** are explicitly local account observations. The table below is complete for this audit's public Agent catalogue/supplements, not a claim that the vendor can never send another code. Third-party invoicing's separate access workflow and NAV's full error catalogue are outside this inventory; their unfamiliar tokens must remain open.

`U` = `Unknown`, `R` = `Rejected`, `D` = `DuplicateOrderNumber`, `N` = `NotFound`. “Retry” below is only `is_retryable()`'s potentially-transient read hint. Every row except **1 and 55** has retry=false. Credential=true only for **3,135,136,164**. Variant definitions are `error.rs:37–210`; number-to-variant dispatch is `:269–315`; classification is `:318–420`.

| Code | Rust variant | Class | Current source / qualification |
|---|---|---|---|
| 1 | `Maintenance` | U | B5; retry=true, wait minutes. |
| 3 | `InvalidCredentials` | R | B3/B5; credential. |
| 7 | `MissingData` | N | XML/PDF response pages; receipt-send example means missing `emailTargy`, not necessarily missing document. |
| 14 | `StornoOfReversalInvoice` | R | Behavior note `:88,143`; observed, not in current general table. |
| 53 | `XmlNotAFile` | R | B5; missing proper XML file upload. |
| 54 | `EInvoiceNotEnabled` | R | B5. |
| 55 | `EInvoiceSigningFailed` | U | B5; retry=true is potential timestamp recovery, not proof of issuance; expiry needs remediation. |
| 56 | `InvoiceNotificationDeliveryFailed` | U | P1 source; numbered issuance is a warning in the appropriate parser. Not in B5; not observed live. |
| 57 | `MalformedXml` | R | B5 and taxpayer error example. |
| 71 | `DuplicateOrderNumber` | D | B5/S2. |
| 73 | `PrepaymentInvoiceNotIdentifiable` | R | Behavior note `:119,143`; observed. |
| 135 | `BrowserSessionActive` | R | B5; credential; browser/testing context, not an established agent-key invalidation rule. |
| 136 | `LoginBlocked` | R | B5; credential. |
| 152 | `DuplicateOrderNumberNamed` | D | B5/S2; message names order, not existing document. |
| 164 | `MultipleAccounts` | R | B3/B5; credential. |
| 202 | `UnregisteredPrefix` | R | B5; includes empty/unregistered prefix. |
| 221 | `HasCorrectiveInvoice` | R | Behavior note `:89,143`; observed. |
| 259 | `NetValueMismatch` | R | B5; net = unit price × quantity. |
| 260 | `VatValueMismatch` | R | B5; VAT = net × rate / 100. |
| 261 | `GrossValueMismatch` | R | B5/S4; receipt exact-sum rule is stricter than inferred invoice arithmetic. |
| 262 | `NetValueInvalid` | R | B5 EN says row number, HU says product name; diagnostic retained verbatim. |
| 263 | `VatValueInvalid` | R | B5; offending row. |
| 264 | `GrossValueInvalid` | R | B5; offending row. |
| 335 | `ProformaNotFound` | R | Deletion response example; missing/already-deleted, not successful replay. |
| 336 | `ReceiptPrefixUsedForInvoices` | R | Receipt creation response supplement. |
| 337 | `InvalidReceiptPrefix` | R | Same; uppercase letters/numbers. |
| 338 | `DuplicateReceiptCallId` | R | Same; duplicate refused, original success not recovered. |
| 339 | `ReceiptNotFound` | N | Same; absent receipt number; storno supplement itself gives messages without code numbers. |
| 340 | `ReceiptPaymentMismatch` | R | Same; tender sum differs from gross. |
| 352 | `IssueDateMustBeToday` | R | Behavior note `:90–91,143`; observed storno issue date, not fulfillment date; create may replace date instead. |
| 363 | `ReceiptGrossNotWhole` | R | B5/S4. |
| 364 | `ReceiptNetPrecision` | R | B5/S4. |
| 365 | `ReceiptVatPrecision` | R | B5/S4. |
| 463 | `PaymentOnReversedInvoice` | R | Behavior note `:135,141–143`; observed body-only. |
| 537 | `ErasureCodeLimit` | R | B5/S6. |
| 538 | `ErasureCodesUnavailable` | R | B5; demo/test accounts. |
| 539 | `ErasureCodesDisabled` | R | B5/S6. |
| 551 | `SimplifiedImageAccountIncompatible` | R | B5/S5; OSS/non-Hungarian tax number, including inherited final. |
| 552 | `SimplifiedImageItemLimit` | R | B5/S5; two items/four on final. |
| 553 | `SimplifiedImageVatInvalid` | R | B5/S5. |
| 554 | `SimplifiedImageCannotCorrect` | R | B5/S5. |
| 555 | `SimplifiedImagePrepaymentVatMismatch` | R | B5/S5. |
| 556 | `SimplifiedImageDocumentForbidden` | R | B5/S5. |

Open handling: `ErrorCode::from` trims tokens, normalizes **known** numeric spellings (`007`→7), preserves unknown trimmed strings, including oversized integers/NAV strings, and makes an empty token `Absent` (`error.rs:455–504`). Unknown/absent codes are U, noncredential and not affirmatively retryable. A manually constructed `Unknown("3")` stays unknown; the wire parser constructs the known variant. No general catch-all converts future errors to a settled refusal.

`ClientError::Request` is locally rejected before HTTP; `Transport`, `IncompleteResponse`, `HttpStatus`, parse failure and `ServiceUnavailable` are U (`client.rs:103–137`). `ResponseError` mirrors this (`error.rs:709–761`). The classification describes this exchange; the current recovery text correctly refuses to infer the outcome of earlier sends from a later refusal.

## Shared response, session and feature audit

### Response precedence and encoding

| Check | Current implementation | Assessment |
|---|---|---|
| Body transfer fails after status/headers | `client.rs:385–393`: `IncompleteResponse {status, raw HeaderMap, source}`; no parser call | Correct preservation of incomplete evidence; always U. Repeated cookies/header bytes retained; no partial body exposed. |
| Completed nonblank `szlahu_down` | `wire.rs:291–297` | Wins before other channels. PHP P1 corroborates priority. |
| Completed nonblank error header | `wire.rs:262–270,298–300` | Wins before HTTP; operation handles 56 specially. Whitespace-only code is absent; message decoded once. |
| Known non-2xx, no preceding verdict header | `wire.rs:301–308` | `HttpStatus` before XML. Number/success/unrelated headers do not override status. U, not proof of origin or no effect. |
| Body after those checks | Invoice envelope, XML query, receipt, taxpayer entry points | Body-only 7/463 handled. A 200 body error is an API error; the same body at 500 is an uncertain HTTP error if no verdict header precedes it. |
| Issuing 56 | `ops/envelope.rs:179–251` | Requires a number; readable non-56 body refusal wins. Plain/empty notification bodies allowed with header 56; malformed XML cannot use that plain-body exception. T-02 qualifies identity leniency. |
| Other operations' 56 | Credit/receipt/taxpayer/XML query use ordinary checks | No blanket “56 is success”. PDF query shares issuing envelope handling but still requires PDF (`ops/query_pdf.rs:83–93`); no notification flag projected into its result. |
| Text headers | `wire.rs:239–249,343–351`; payment method `ops/envelope.rs:321–327` | Form-style percent decoding once: `+`→space, `%2B`→`+`; bad UTF-8 escapes fall back to the plus-normalized original. |
| Numeric/code headers | Raw `header()`, dedicated decimal parsing | No form decoding of signs or codes. Decimal comma observed in behavior note `:160`; dot/comma/exponent support is deliberate parser policy. |
| XML body values | `ops/envelope.rs:120–167` | Body before header for number, amounts and buyer URL; XML URL gets entity decoding only. Malformed nonblank body amount does not silently use header; blank amount falls back. |
| Diagnostics | `wire.rs:43–50,155–180`; `client.rs:93–101`; `error.rs:659–695` | Wire body hidden; completed response cookie redacted, other headers visible; incomplete response lists header names. HTTP/unexpected-body excerpts bounded to a char-safe 256-byte text prefix. Other upstream messages are not universally bounded/redacted, as README `:376` states. |

This total precedence is **library policy**, not a complete vendor-specified conflict-resolution rule. Current operation pages say header errors omit number/totals and body carries the same data; they do not specify malformed combinations, conflicting status or repeated verdict headers. PHP supplies down/56 corroboration; the account note supplies body-only errors. The Rust code's conservative handling of ordinary HTTP failures is appropriate, but it must not be presented as proof of vendor HTTP behavior (T-03).

`RawResponse::header` selects the **first** matching repeated header (`wire.rs:227–237`). No fresh vendor source specifies repeated `szlahu_*` semantics. The client preserves repeated values into `RawResponse`, while successful-completion header conversion is lossy UTF-8 (`client.rs:394–403`). This is suitable for documented ASCII/percent-encoded headers; arbitrary non-UTF-8 header byte fidelity is promised only by `IncompleteResponse`, not the parsed `RawResponse` interface.

### Credentials and session ownership

- Authentication XML location/order matches built-in writers; the one helper emits **either** `szamlaagentkulcs` **or** `felhasznalo` then `jelszo` (`xml.rs:603–613`). It neither places keys into an authentication header nor mutates case. Lowercase validation is not required to preserve the vendor's rejection behavior. Empty/uppercase supplied keys are sent as supplied unless they contain forbidden XML characters.
- Fresh default native clients create fresh cookie stores; clones share the original reqwest client/jar (`client.rs:226–238,300–307,315–336`). No globally shared cookie file or account-wide static jar was found.
- An injected HTTP client owns its settings and jar (`client.rs:193–212`). Sharing a client/provider across separately authenticated accounts can share sessions, but this is explicitly documented and locally controllable. Neither vendor key-versus-cookie precedence nor account identity can be tested by a loopback cookie test.
- `session_cookie()` is a transport helper, not a complete jar (`wire.rs:313–339`). It finds exact case-sensitive `JSESSIONID` pairs in repeated case-insensitive `Set-Cookie` fields, skips nonmatches/malformed pairs, preserves later `=`, accepts empty value, and discards attributes. Domain/path/expiry belong to a real jar. No-cookie reauthentication and manual refresh after account edits accurately reflect B4.
- B3's immediate key deletion does **not** establish how an already-authenticated session behaves after key deletion. The crate correctly labels fresh jars on credential changes as ownership policy rather than vendor invalidation proof.
- Default endpoint is HTTPS. Explicit override permits HTTP for mocks/proxies; it validates scheme/host, not production deployment policy (`client.rs:182–186,242–256`). Default TLS verification uses reqwest rustls/platform verification; no insecure certificate bypass appears in production construction. Userinfo is redacted in endpoint/client/builder diagnostics, while custom URL path/query data are not generally secret-redacted.

### Timeouts, retries and platforms

- Native total request timeout is **60 seconds**, including response-body transfer, via reqwest (`client.rs:280–307`). No separate connect deadline is configured; the total bound still applies. XML generation and parsing are synchronous outside the HTTP timeout. Response memory has no explicit cap; this audit found no documented vendor maximum for every response from which to derive a cap.
- Redirects are disabled on the default native client. This protects the POST/body semantics of 301/302/303 and avoids blindly forwarding a credential-bearing body on 307/308. The comment “the endpoint never redirects” (`client.rs:297`) is not established by current docs; the no-redirect policy itself is sound.
- There is no application retry/reconciliation loop. An injected reqwest retry policy remains active and can repeat POST before `AgentRequest::parse` runs; the warning is explicit in `client.rs:199–202`, `recovery.md:4–9`, README `:374`.
- Locked dependency is **reqwest 0.13.4**, not the manifest's minimum 0.13.2. Its `src/retry.rs:195–202,273–317` retains default protocol-NACK retries (up to two), not generic 5xx/API-code retry. The isolated crate feature tree enables cookies/rustls, not HTTP/2 or HTTP/3. Downstream feature unification can enable them. Do not restate “no application loop” as “exactly one physical send under every dependency configuration”. No duplicate-issuance defect is inferred from protocol-refusal retries.
- Fresh B5 EN/HU unambiguously permits **five total sends**, including the initial send, then operator intervention. It does not prescribe exact combined accounting across a write and its different reconciliation queries. The crate exposes this guidance rather than implementing a hidden retry count or a rate limiter.
- `recovery.md:11–55` distinguishes all operation groups: invoice external-id reconciliation; verified original/reversal; persistent receipt call id with 338 as refusal rather than replay; no storno-specific 338 promise; repeatable reads; mutation-state checks for additive/replacing credit entries; all-match order deletion; email delivery uncertainty. Empty queries and elapsed time do not prove nonexecution.
- The 60-second policy deliberately exceeds PHP's 30-second default. Behavior note A4d supplies a stalled call, not a verified “issued later” timeline. Current `recovery.md:50–55` and README `:153` correctly qualify that provenance. A 60-second timeout is not a server cancellation or a permission to resend after 60 seconds.
- No features are enabled by default (`Cargo.toml:14–25`). `client-reqwest` is the only crate feature; the core performs no I/O. Public reqwest re-export names the actual dependency (`lib.rs:75–96`). Filesystem helpers are separately platform-gated.
- **Browser-wasm compilation passed** in this audit. It proves compilation only. Default wasm `Client` is `reqwest::Client::new()` (`client.rs:309–312`), with no configured 60-second deadline. Reqwest 0.13.4 `src/wasm/request.rs:38–48,349–394` defaults credentials to `None`; `src/wasm/client.rs:222–223` sets Fetch credentials only if requested. This crate does not request cross-origin inclusion. CORS, response-header exposure and browser-managed cookies remain external constraints, correctly spelled out in `lib.rs:45–70`, README `:320–328`. An injected client cannot change this request-level setting.
- A browser compile is not a Cloudflare runtime test. Trusted server-side wasm can use the I/O-free core with its own transport. B3 expressly says not to embed account keys in browser client-side code; current public docs repeat that requirement.

## Findings and reproductions

### T-01 — P2: legacy authentication aliases expose the agent key in diagnostics

**Source:** B3 EN and HU both explicitly support supplying the same Agent key in `<felhasznalo>` and `<jelszo>` for legacy integrations.

**Code:** `credentials.rs:88–96` redacts only the password of `UserPassword` and prints the username. `client.rs:149–158,339–345` recursively prints those credentials. `AgentKey::Debug` itself is correctly redacted (`credentials.rs:27–30`).

**Reproduction, executed offline:**

```rust
let key = "legacy-agent-key-secret";
let credentials = szamlazz_agent::Credentials::user_password(key, key);
println!("{credentials:?}");
```

Actual output:

```text
Credentials::UserPassword { username: "legacy-agent-key-secret", password: "…" }
```

**Impact:** logging a valid documented legacy authentication configuration leaks an API-only key with the account's full Agent permissions. This requires the caller to use that authentication form and print diagnostics; normal `Credentials::agent_key` does not exhibit it. The source already tests username visibility (`credentials.rs:105–114`), so existing redaction tests do not catch this documented alias.

**Action:** redact both fields of `UserPassword` in credential/client/builder `Debug`, or otherwise ensure the key-as-username form cannot reveal the secret. Cover the vendor-supported key/key form in a regression test. Do not normalize credentials or remove the wire compatibility form to solve a diagnostic problem.

### T-02 — P2: numbered-56 metadata fallback also hides invalid body identity

**Source:** P1 corroborates successful issuance when 56 accompanies a number. The fresh invoice/storno response XSD declares `szamlaszam` a singleton scalar. It provides no permission to ignore duplicate/nested identity. Current README `:253–276` states that duplicate singleton fields/scalar children remain refused and malformed identity is not usable evidence.

**Code:** `ops/envelope.rs:288–316` retries deserialization as `Identity` when optional metadata fails. That identity read correctly fails on duplicate or nested `szamlaszam`, but its error is passed to `parse_reply`, where **every** payload error under notification failure becomes `Body::default()` (`:208–211`). `Body::invoice_number` then adopts `szlahu_szamlaszam` (`:120–133,214–228`). Identity failure and optional metadata failure have lost their distinction.

**Executed input:** headers `szlahu_error_code: 56`, `szlahu_szamlaszam: I-2`, with the completed body:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres><hibakod>56</hibakod>
  <szamlaszam>I-2</szamlaszam><szamlaszam>I-3</szamlaszam>
</xmlszamlavalasz>
```

`StornoInvoice::new("I-1").parse(&raw)` returned `Ok(CreatedInvoice { invoice_number: "I-2", notification_delivery_failed: true, … })`. Replacing both number elements with `<szamlaszam><bad/></szamlaszam>` also returned success. Without the headers, both controls returned error 56 with `OutcomeClass::Unknown`.

**Impact:** a structurally invalid/ambiguous identity can cross the shared issuance boundary as confirmed success when a number header is available. A caller following the result stops reconciliation and records the selected header number. This is narrower than arbitrary malformed-XML acceptance: the whole XML must pass the lexical/structural envelope reader, and a readable non-56 body refusal is still honored. No such vendor emission was captured.

**Action:** retain the distinction between failure to read optional metadata and failure to establish a unique scalar identity. Permit header fallback when body identity is genuinely absent, and optional-total/PDF leniency when body identity is valid; return uncertain parse failure when the body identity is malformed/duplicated. Extend `tests/response_headers.rs:507–535` with the currently missing **number-header-present** controls. Existing tests only exercise malformed identity without a header number.

### T-03 — P3: qualify protocol guarantees not established by current sources

Two statements in the owned shared error documentation overstate current evidence:

1. `error.rs:3–5`: errors are signaled in-band, “**never via HTTP status codes**”. Fresh response pages document in-band forms; they do not promise that every vendor/platform failure will avoid a non-2xx. PHP stores HTTP status and checks a down header, without proving the universal claim. The crate's own `HttpStatus` path correctly treats non-2xx conservatively.
2. `error.rs:333–340`: credential codes occur “**before it looks at the request (its documentation)**” and “the same request succeeds once the account is fixed”. Fresh B3/B5 EN/HU establish login/access failure meanings, but do not state that exact processing-order guarantee. Correcting authentication also does not guarantee that all subsequent document validation succeeds. Behavior note `:255–261` explicitly records these codes as unobserved.

**Reproduction:** textual comparison of the cited lines with B3/B5 and the operation error pages. No runtime failure is alleged.

**Action:** say that domain failures are reported in-band and HTTP failures remain possible; describe 3/135/136/164 as authentication/access refusals whose `Rejected` classification is the integration's interpretation of their documented meanings. Remove the universal-success claim, or attach a direct source for any stronger guarantee. Retain the important existing caveat that a refusal of this exchange does not settle an earlier send.

## Executed checks and scratch reproduction

Toolchain: `rustc 1.98.0 (88d9e12ae 2026-08-18)`. Locked crate reports package version 0.3.0; the README discusses planned 0.4 changes. `rustup` was unavailable on PATH, but the actual wasm target check succeeded.

```sh
cargo test -p szamlazz-agent --locked --features client-reqwest \
  --test client --test response_headers --test error_classification --test custom_http_client
# 8 + 13 + 3 + 1 = 25 tests passed

cargo test -p szamlazz-agent --locked --features client-reqwest --lib wire::tests
# 13 passed; 174 filtered out

cargo check -p szamlazz-agent --locked --target wasm32-unknown-unknown --features client-reqwest
# passed

cargo tree -p szamlazz-agent --features client-reqwest -e features -i reqwest --locked
# reqwest 0.13.4: cookies/rustls; no http2/http3 in this isolated feature tree
```

The native interrupted-download test reads a real loopback HTTP response claiming 1000 body bytes but delivering only one, with each of credential-error, down, numbered-56 and plain number headers. It asserts incomplete evidence, repeated cookies, redacted diagnostics and U (`tests/client.rs:232–294`). Cookie tests demonstrate native jar behavior, not vendor account selection (`:140–219`). Header tests explicitly identify their combinations as synthetic.

Scratch file `/tmp/opencode/transport-2ba5fb8-probe.rs` was created with `apply_patch`, built and run against the current crate:

```sh
cargo build -p szamlazz-agent --locked --features client-reqwest
rustc --edition=2024 /tmp/opencode/transport-2ba5fb8-probe.rs \
  --extern szamlazz_agent=target/debug/libszamlazz_agent.rlib \
  -L dependency=target/debug/deps -o /tmp/opencode/transport-2ba5fb8-probe
/tmp/opencode/transport-2ba5fb8-probe
```

Equivalent self-contained probe source (uses no account or HTTP calls):

```rust
use szamlazz_agent::{Credentials, OutcomeClass};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::wire::{AgentRequest, RawResponse};

fn main() {
    let key = "legacy-agent-key-secret";
    let diagnostic = format!("{:?}", Credentials::user_password(key, key));
    assert!(diagnostic.contains(key));
    println!("{diagnostic}");
    for identity in [
        "<szamlaszam>I-2</szamlaszam><szamlaszam>I-3</szamlaszam>",
        "<szamlaszam><bad/></szamlaszam>",
    ] {
        let body = format!("<xmlszamlavalasz xmlns=\"http://www.szamlazz.hu/xmlszamlavalasz\"><sikeres>false</sikeres><hibakod>56</hibakod>{identity}</xmlszamlavalasz>");
        let raw = RawResponse::new(
            [("szlahu_error_code", "56"), ("szlahu_szamlaszam", "I-2")],
            body.clone().into_bytes(),
        );
        let parsed = StornoInvoice::new("I-1").parse(&raw);
        println!("{parsed:?}");
        assert!(parsed.is_ok()); // demonstrates the issue at this HEAD
        let raw = RawResponse::new::<&str, &str>([], body.into_bytes());
        let error = StornoInvoice::new("I-1").parse(&raw).unwrap_err();
        assert_eq!(error.outcome_class(), OutcomeClass::Unknown);
    }
}
```

## Evidence-backed deviations, ambiguities and limits

1. **Storno external id:** the fresh storno request page describes it as an optional reference to the original. Behavior note B6/XPRB instead records it attaching to the **new SS**, with the original selected by number (`:69–70`). Current recovery guidance follows the observation and verifies the reversal's original reference. Do not “fix” this to the web prose without renewed vendor evidence.
2. **Code 56 versus ordinary header error prose:** invoice/storno response pages say error headers omit number/totals; PHP explicitly permits numbered 56. The crate's warning exception is justified by this first-party corroboration. It is not evidence for accepting malformed identity (T-02), a promise of email delivery, or a reason to repeat issuance.
3. **Body-only error responses:** generic header lists are permissive, not mandatory. The account's XML-query/credit behavior justifies always examining XML after header/status checks. Missing error headers are not success.
4. **Session cookies:** jar refresh advice is documented; exact key/cookie precedence, revocation of established sessions, expiry under ongoing parallel traffic, and account-switch behavior are unverified. Tests establish only isolated native jar mechanics.
5. **Encoded textual headers:** invoice-number/error encoding and raw monetary headers are explicit in current response tables. Plus handling for the buyer URL and payment method, invalid UTF-8 fallback, repeated verdict-header semantics and conflicting body/header identity have no complete normative vendor specification. The PHP URL path double-decodes, so it is not sufficient authority for exact URL fidelity. Current Rust one-decode header / entity-only XML behavior is the safer defined policy.
6. **Non-2xx/code-56 combinations:** accepting numbered header 56 ahead of status is tested library policy. The fresh docs and retained account evidence do not establish this combination's live emission. Bare success-number headers do not bypass a 500.
7. **Timeout/provenance:** neither an empty order query after a stalled call nor elapsed time proves a call cannot execute later. Current shared recovery docs retain this distinction. The broader historical delayed-issuance assertion is not independently verified here.
8. **Receipt call id:** creation uniqueness/338 is documented. Receipt query is by number or order; no call-ID-only lookup is established. Receipt storno's already-reversed refusal is documented, but exact code and storno-specific 338 semantics remain unspecified. Invoice replay behavior must not be imported into receipt storno.
9. **Deletion scope:** HU S7 explicitly establishes all-match order deletion; EN omits the crucial sentence. Current proforma/recovery text is appropriately stronger than the EN page. Deletion success gives no count/list, and query absence also covers consumption.
10. **Catalogue translations:** EN 262 says row number while HU says product name. The enum's broad “offending row” wording and verbatim runtime message do not depend on parsing that diagnostic. Receipt supplement's “delete” mention does not add a route.
11. **Platform coverage:** browser-wasm compiled, no browser/Worker execution or CORS preflight performed. Native default no-redirect/60-second settings are source-inspected; fast loopback tests inject equivalent settings and do not measure a full 60-second timeout or establish production root-store connectivity.
12. **Recovery guidance versus mechanism:** the crate is a transport/parser library. Five-send budgeting, caller serialization, persistent logical identity, operator reconciliation and time allowed for an earlier request to finish remain caller responsibilities. No hidden exactly-once promise was found in the owned scope.
13. **Audit boundary:** individual XML/XSD business-field coverage, numeric/date/model fidelity beyond shared entry points, all NAV codes, delegated invoicing account-linking, and account-specific execution semantics require their own audits. No live test suite was invoked. The 38 targeted passing tests are useful regression evidence, not a full crate result or proof of vendor behavior.

**Recommended order:** close T-01's secret-bearing diagnostic path; make T-02's identity/metadata distinction explicit; qualify T-03's protocol claims. Preserve the verified route table, open error handling, cookie ownership documentation, incomplete-response evidence and operation-specific recovery guidance.
