# Independent adjudication of the six Számla Agent reviews

**Baseline:** `f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9`

**Adjudicated and primary sources fetched:** 2026-09-10

## Decision

Recommend **four consolidated implementation issues: one P2 and three P3**. None establishes a P0/P1 incident or observed failure on a live account.

| Consolidated issue | Priority | Disposition and evidence class |
|---|---|---|
| **FQ-1 — Repeated response rows depend on raw prefix spelling and adjacency** | **P2** | Confirmed on four invoice lists **and three receipt lists**. Mixed-prefix rows pass current official XSDs but fail parsing. Interleaved ignored extensions are a related breach of the README's extension policy, not XSD-valid examples. |
| **FQ-2 — Normalize reserved namespace bindings before validating them** | **P3** | Confirmed rejection of legal, XSD-valid XML on both XML/PDF query paths. Very unusual spelling; vendor emission unobserved. |
| **FQ-3 — Enforce namespace well-formedness for attributes and declarations** | **P3** | Confirmed malformed-input acceptance. **Merge taxpayer F2 here.** No business-field injection or exploitation demonstrated. |
| **FM1 — An unusable optional diagnostic erases readable code/numbered-56 evidence** | **P3** | Confirmed evidence-retention hardening gap. The diagnostic violates its schema; this is not ordinary conforming-response failure. |

Keep **numberless successful credit registration** as a **P3 contract-clarification task**, outside that defect count. Reclassify **taxpayer F1 (NBSP boolean)** as an optional low-priority normalization-consistency improvement, also outside the count. Its behavior is real, but the claim that current policy requires strict XSD lexical rejection is not established.

The strongest actionable finding is FQ-1: removing a valid alternate prefix makes exactly the same response parse. The receipt report's “no findings” conclusion needs this shared-parser qualification. The invoice and transport reports' earlier numbered-56 closure claims remain true for their tested totals/PDF cases, but do not cover FM1's diagnostic failure.

## Scope, independence and evidence standard

Read all six `2026-09-10-agent-api-f83e5fd-{invoices,mutations,queries,receipts,taxpayer,transport}.md` reports and all 308 lines of `docs/szamlazz-hu-behaviour.md`. Inspected the relevant production paths, README and locked quick-xml source. This adjudication follows reported leads and independently tests their conclusions; it is not a second complete field-by-field audit of all eleven operations.

HEAD matched the baseline. The Agent crate, workspace manifests/lock, fixtures and behavior notes had no diff against it before/after verification. Unrelated ongoing work existed elsewhere. Only this new report was written in the repository. New scratch files were authored with `apply_patch` under `/tmp/opencode`; existing scratch source/specimens were read, not overwritten. No source, repository test or fixture edits; no live account calls. External traffic consisted of unauthenticated documentation/XSD/PHP-archive GETs. PHP was inspected, never executed. No subagent facility was available; verification was conducted directly.

Severity here separates three questions:

1. **Does a supported, conforming response fail?** FQ-1 and FQ-2 do, with different practical breadth. This does not prove the vendor currently emits their spellings.
2. **Does malformed input expose a bounded weakness?** FQ-3 and FM1 do. The former accepts ignored invalid markup; the latter fails conservatively and loses useful evidence. Neither establishes an automatic duplicate write.
3. **Is the report asking for a different projection or normalization policy?** That needs a demonstrated broken promise or a deliberate design decision, not just an XSD that accepts more or fewer strings.

Code references below are relative to `crates/szamlazz-agent/` unless stated otherwise.

## 1. FQ-1: retain P2, expand to receipts, consolidate the manifestations

### Independently reproduced

For the invoice, reused the report's inspected `all-fields.xml` as input, then validated it against a **new download** of [the invoice response XSD][invoice-xsd]. For receipts, reused only the inspected literal one-row record from the receipt scratch source, set its monetary values to zero so duplication preserves money/tender consistency, and validated it against a **new download** of [the receipt response XSD][receipt-xsd]. No published example's invalid PDF placeholder or invoice-style receipt amount aliases entered these controls.

For each listed row, duplicated it and changed only the second row's opening/closing QName to `p:<row>`, declaring `p` as the same protocol namespace. Its children keep their inherited default namespace. Example of the relevant shape (abbreviated here; executed rows were complete):

```xml
<tetelek>
  <tetel><!-- complete row --></tetel>
  <p:tetel xmlns:p="http://www.szamlazz.hu/szamla">
    <!-- same row contents -->
  </p:tetel>
</tetelek>
```

| Response list | Same-QName adjacent rows | Mixed-QName adjacent rows | Unknown/foreign element between same-QName rows |
|---|---|---|---|
| Invoice `tetelek/tetel` | Parses; XSD valid | Duplicate `tetel`; XSD valid | Same duplicate-field failure |
| Invoice `qutetek/qutet` | Parses; XSD valid | Duplicate `qutet`; XSD valid | Same duplicate-field failure |
| Invoice `kifizetesek/kifizetes` | Parses; XSD valid | Duplicate `kifizetes`; XSD valid | Same duplicate-field failure |
| Invoice `osszegek/afakulcsossz` | Parses; XSD valid | Duplicate `afakulcsossz`; XSD valid | Same duplicate-field failure |
| Receipt `tetelek/tetel` | Parses; XSD valid | Duplicate `tetel`; XSD valid | Same duplicate-field failure |
| Receipt `kifizetesek/kifizetes` | Parses; XSD valid | Duplicate `kifizetes`; XSD valid | Same duplicate-field failure |
| Receipt `osszegek/afakulcsossz` | Parses; XSD valid | Duplicate `afakulcsossz`; XSD valid | Same duplicate-field failure |

Comments and processing instructions between rows also parse and validate. The two failing extension controls were `<ext/>` and `<x:ext xmlns:x="urn:future"/>`. They are namespace-well-formed, but **not declared by the current XSDs**. Their support follows the current README `253–259`, not a vendor wildcard declaration. Repeated invoice labels were not needed to prove the issue: the report correctly notes that current `cimke` multiplicity is only 0–1.

All three receipt entry points—`CreateReceipt::parse`, `StornoReceipt::parse`, `QueryReceipt::parse`—were exercised on every receipt variation. They returned the same list failure. A common NY specimen through these entry points isolates parsing; it does not claim that NY is an appropriate successful storno result. The shared failure is already conclusive before such operation semantics.

### Root cause and scope

- `src/xml.rs:196–255` filters foreign subtrees but deliberately preserves protocol QNames. It substitutes `<__szamlazz_foreign/>` for foreign children to prevent scalar concatenation.
- `src/ops/query_xml.rs:588` and `src/xml.rs:402–409` deserialize that text through serde/quick-xml.
- Receipt create/storno/query share `src/ops/receipt.rs:688–701`; receipt vector adapters are at `798–802`, `860–864`, and shared VAT totals at `src/xml.rs:689–697`.
- Locked **quick-xml 0.42.0**, `src/de/map.rs:863–867`, tests sequence membership with `n.name() == start.name()`—the raw QName. Lines `913–919,967–970` explain/implement stopping at another tag without `overlapped-lists`. Workspace `Cargo.toml:29` enables `serialize`, not that feature. Serde subsequently sees another field with the same local name and refuses it.

[Namespaces 1.0 §§2.1, 6.1–6.2][namespaces] defines element identity as namespace name plus local name. Both XSDs allow unbounded rows at the affected paths. The prefix-only transformation changes no declared business field or value. It is therefore a genuine supported-response interoperability failure, not malformed-input hardening.

**Why P2:** a repeated-row serialization choice can make the whole invoice or receipt unavailable, across several business lists. Receipt issuance parsing can additionally lose the typed result after a send. **Why not P1:** ordinary source examples use stable prefixes, no vendor frequency is established, and neither silent row loss nor automatic reissue was demonstrated. If a deployment prioritizes only observed vendor output, operational urgency may be low; the conformance defect remains real.

**Issue acceptance boundary:** read repeated rows by protocol expanded name in order, through ignored container children; retain duplicate-singleton refusal, parent-path isolation and the existing refusal of a child inside scalar text. Enabling overlapped lists alone is not established as a fix for raw-QName grouping. File one issue covering invoice/receipt lists and both manifestations, not seven field tickets or a separate receipt defect.

## 2. FQ-2: retain P3, genuine valid-input rejection

The complete PDF counterexample is:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz"
 xmlns:xml="http://www.w3.org/XML/1998/n&#97;mespace">
  <sikeres>true</sikeres><szamlaszam>I</szamlaszam><pdf>JVBERi0=</pdf>
</xmlszamlavalasz>
```

Both this envelope and the full invoice with the same declaration passed libxml2 validation against freshly downloaded schemas. Both public query parsers rejected them with `InvalidXmlPrefixBind("http://www.w3.org/XML/1998/n&#97;mespace")`. Replacing the character reference with literal `a` made both parse.

[Namespaces §2.3][namespaces] explicitly compares the **normalized attribute value**, after replacing character/entity references. Section 3 permits declaring `xml` with its reserved URI. This is XML attribute normalization, not URI percent-decoding or URL canonicalization.

`src/xml.rs:73,81` invokes `NsReader` before `namespace_uri` at `258–274` can unescape the binding. Locked quick-xml `src/name.rs:679–685` compares the raw value with its reserved constant and errors first. The second reader in `protocol_text` has the same dependency boundary.

**Severity:** P3 is appropriate: legal input fails, but only an unusual, unnecessary explicit declaration spelling was demonstrated. There is no current-vendor emission evidence. The general README normalization exceptions do not justify this rejection: those concern field reading, not rejecting an equivalent namespace declaration.

FQ-2 and the escaped-reserved-binding part of FQ-3 share a correction seam. They may be implemented together, but retain separate acceptance criteria: accepting legal declarations and refusing illegal ones are opposite failure modes.

## 3. FQ-3: retain P3; merge taxpayer duplicate attributes

Inserted each independent specimen into otherwise successful invoice XML, PDF, NAV 2.0 and NAV 3.0 taxpayer responses:

```xml
<extension xmlns:a="urn:same" xmlns:b="urn:same" a:x="1" b:x="2"/>
<extension xmlns:a="urn:same" xmlns:b="urn:s&#97;me" a:x="1" b:x="2"/>
<extension xmlns:x="http://www.w3.org/XML/1998/n&#97;mespace"/>
<extension xmlns="http://www.w3.org/XML/1998/namespace"/>
<extension xmlns="http://www.w3.org/2000/xmlns/"/>
<extension xmlns:x=""/>
```

**All 24 public-parser cases succeeded; Python ElementTree rejected each specimen as namespace-invalid.** Controls with distinct namespace URIs and the same attribute local name succeeded correctly. Same-QName duplicate attributes, a used undeclared attribute prefix and a literal forbidden binding of another prefix to the reserved XML namespace were refused correctly.

[Namespaces §§3, 5, 6.3, 8][namespaces] supplies the controlling rules: reserved bindings/defaults, no prefix undeclaring in 1.0, unique expanded attribute names, and reporting namespace-well-formedness violations. Empty prefixed bindings are a **Namespaces 1.0** finding; do not claim they are prohibited in 1.1. Quick-xml's resolver source itself references Namespaces 1.1, so its acceptance is not by itself proof of a dependency defect against its own chosen specification.

`src/xml.rs:96–109` checks attribute syntax and unknown prefixes but has no expanded-name uniqueness set. Its lexical tokenizer (`173–194`) does not fill that namespace gap. Quick-xml `src/name.rs:661–678,694–720` allows these default/empty bindings and checks reserved named bindings against raw strings. Unescaping later cannot retroactively enforce the missed constraints.

**Merge taxpayer F2:** its NAV entry point calls this same preflight; it is not a NAV path-mapping defect. This adjudication independently confirms the merge in both NAV layouts. Shared code suggests wider reach, but the executed namespace-invalid matrix is the four response shapes above, not every operation.

**Severity:** P3 hardening/conformance. The attributes/declarations are ignored metadata in these reproductions. They neither supply a foreign success verdict nor overwrite document identity. This is a narrower namespace gap than the old illegal-character/undefined-entity finding, whose existing regression tests still pass. Do not inflate it into a security exploit or “valid invoices parse incorrectly.”

**Issue acceptance boundary:** enforce normalized expanded attribute uniqueness and reserved-binding rules even in ignored subtrees; preserve legal distinct attributes, namespace aliases and sparse business content. Full business-XSD validation is not the remedy.

## 4. FM1: retain P3 as evidence-retention hardening

Recompiled and ran the mutation report's inspected six-test scratch program against the newly workspace-built library. A body-only response, or the same response with numbered code-56 headers, reproduces:

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>false</sikeres><hibakod>56</hibakod>
  <hibauzenet><bad/></hibauzenet><szamlaszam>I-2</szamlaszam>
</xmlszamlavalasz>
```

`StornoInvoice::parse` returns `Parse(Xml(XmlError(MixedContent("bad"))))`; two diagnostic elements instead return a duplicate-field error for `hibauzenet`. The error does not retain `I-2`. A body-only credit refusal with code 463 and the nested diagnostic similarly becomes `Parse`, with `OutcomeClass::Unknown`, instead of the readable refusal code.

**Why it happens:** `src/ops/envelope.rs:292–294` fully deserializes `xml::Verdict` before its separate payload/identity recovery. `src/xml.rs:347–355` couples the verdict/code to `hibauzenet: Option<String>`. The early error at `envelope.rs:195` bypasses the identity salvage at `298–315`. Generic verdict entry points have the same coupling at `xml.rs:389–393`.

**Source and counterargument:** [the shared response XSD][envelope-xsd] makes the message optional, but scalar and at most once. Optionality does **not** make a nested or repeated message conforming. A reader may reasonably reject such a response, especially to avoid promoting malformed XML. This finding is actionable specifically as a bounded extension of the library's existing evidence-retention policy, not as a general API violation.

The current README `234–237` and `envelope.rs:173–177` promise numbered-56 metadata tolerance. Fresh [first-party PHP response guidance][php-response] says invoice issuance may succeed despite notification failure. A fresh PHP 2.12.4 archive independently confirms `INVOICE_NOTIFICATION_SEND_FAILED = 56` (`Response/InvoiceResponse.php:17`) and the number-plus-notification condition clearing the failure verdict (`319–322`). Neither source establishes malformed diagnostics in vendor output.

**Controls passed:** nested totals and duplicate/invalid PDF under numbered 56 preserve identity; ordinary success remains strict; a readable non-56 refusal wins over provisional header 56 despite bad optional payload; malformed complete XML/verdict is not promoted; duplicate/nested/foreign body identity without a usable header remains unusable. The old totals/PDF fix is closed, not reverted.

**Priority:** P3, not P2. The practical loss is avoidable reconciliation/diagnostic evidence. There is no observed malformed vendor message, no conforming-response failure, and no parser retry loop. A safe correction would establish unique scalar verdict/code independently of diagnostic decoding, retain usable numbered evidence, and keep diagnostic failure separate. Preserve whole-document syntax checks and conservative treatment of malformed/duplicate verdict, code or identity. Do not restore broad header-56 salvage of broken XML.

## 5. Taxpayer F1: behavior confirmed; downgrade to normalization-policy improvement

Both NAV layouts accept `<taxpayerValidity>&#160;true&#160;</taxpayerValidity>` as `valid: true`. `src/ops/taxpayer.rs:524–525` uses Unicode `trim()` before the exact token match at `570–581`. A separate libxml2 `xs:boolean` control rejects NBSP padding and accepts XML whitespace. The inspected NAV 2.0/3.0 source schemas declare this field `xs:boolean` (`invoiceApi.xsd:1682` / `1566`); [XSD Datatypes §§3.2.2, 4.3.6][datatypes] permits only the four tokens after XML whitespace handling.

That establishes **broader-than-XSD normalization**, not an incorrect interpretation of a conforming record. It does not turn a legal false into true, infer validity from missing data, or bypass a non-OK verdict in these probes.

The report's proposed expected boundary needs qualification: README `244–251` expressly excludes **numeric/verdict parsing** from business-text preservation, and `258–259` disclaims XSD business validation. This is not a promise that all scalar lexical domains are exactly XSD's. The README does not spell out NBSP validity normalization specifically; aligning it with `required_bool`'s XML-only trim, or documenting the specific difference, would be reasonable **P3 optional consistency work**. It should not be counted as an established ordinary-response defect or an undocumented violation of the business-text fidelity promise.

This policy assessment does not excuse FQ-3: XML namespace well-formedness and the chosen scalar value domain are distinct boundaries.

## 6. Numberless credit success: genuine contract ambiguity, not dismissed or promoted

The minimal `<xmlszamlavalasz …><sikeres>true</sikeres></xmlszamlavalasz>`:

- passes the freshly downloaded shared success/error XSD;
- returns `Parse(Missing("szamlaszam"))` through `RegisterCreditEntry::parse`;
- succeeds when a usable `szlahu_szamlaszam` header is supplied.

Current code requires reported identity at `src/ops/credit_entry.rs:253–256`. The fresh [EN][credit-en] and [HU][credit-hu] operation response pages were both read, including hidden XSD tabs. They explicitly say optional elements “may not always be included” / “nem mindig jelennek meg,” and additional headers “may” arrive. Their operation-specific inline schema also makes `szamlaszam` optional. This is stronger evidence for clarification than an unrelated generic schema alone: neither page explicitly guarantees that **every successful version-2 credit registration** echoes a number in at least one channel.

However, **the same schema and prose cover both success and error**. The successful example includes a number; the error example omits it. “May not always be included” is compatible with omission only on failure. There is no conditional schema assertion settling the matter in either direction. Both pages also say XML contains the same data as the headers. Version-1 `DONE` shows that the operation conceptually supports a bare acknowledgement, but does not establish the version-2 wire guarantee. Recorded account notes `145` report successful credit number headers, with no numberless counterexample.

**Adjudication:** preserve this as an open **P3 vendor/API-contract clarification**, with conditional P2 interoperability impact if a supported successful version-2 acknowledgement can omit both number channels. Do not assert either “the vendor guarantees a number” or “schema validity proves this is normal vendor success.” Unlike creation, the request already targets an existing invoice; creation's need to discover a new number is not sufficient justification by analogy.

Ask whether version-2 success always echoes `szamlaszam` in the body or header. If omission is supported, distinguish requested identity from optional reported identity in the result; do not silently manufacture a reported number from the request. Additive repetition can duplicate credit entries; replacement can overwrite intervening entries. This ambiguity and the current parse failure authorize neither automatic retry nor a new send.

## 7. Other merged, rejected and qualified claims

| Claim/lead | Consolidated disposition |
|---|---|
| Receipt report: no demonstrated functional defect | Qualified by newly demonstrated FQ-1 on all three receipt lists and entry points; no separate issue count. Complete field representation does not imply complete serialization-shape support. |
| Invoice/transport: numbered-56 identity loss fixed | Correct for optional totals/PDF; FM1 is a remaining distinct verdict-diagnostic boundary. Do not reopen the old case wholesale. |
| Taxpayer duplicate expanded attributes | Merge into FQ-3; not a second NAV-specific issue. |
| Opaque PDF/customer URL boundary trimming | Confirmed `NBSP + opaque:x + NBSP` becomes `opaque:x`. README `249–251` explicitly excludes URLs from business-text fidelity. Keep as normalization policy; no changed usable vendor link established. |
| Receipt nonblank invalid PDF destroys typed result | Conservative artifact policy / possible result-model enhancement. A published `...` placeholder is not an emitted invalid artifact. Do not count as ordinary conformance failure. Blank PDF identity retention and strict receipt reversal tests pass. |
| Strict optional metadata on ordinary success | Intentional present-value validation; optional in XSD does not mean arbitrary malformed contents must be tolerated. FM1 does not require a general permissive redesign. |
| Old illegal characters, undefined references, malformed-tail acceptance | Selected completion/namespace regressions pass. FQ-3 is namespace-specific; no basis to revive the old lexical finding. |
| Old interrupted-body evidence loss | Source now retains status/headers; transport report supplies focused loopback verification. Not independently rerun here and not reopened. Complete-body parse evidence is a different policy question. |
| Invoice preview/simpleItems ordering; layout labels; HU PDF request XSD; receipt schema/toggle/NAV-rollout disagreements | Keep as reported source conflicts or vendor questions, not confirmed writer defects. No new implementation change is justified merely by choosing one side of conflicting first-party sources. These ancillary source inventories were not fully re-fetched in this focused adjudication. |
| Exhaustive supported fields / no missing operation | No new missing field or route established here. Broad negative conclusions remain bounded to the reports' audit inventories and verification, not independently proven by this adjudication. |

### Live behavior applied before judging deviations

The behavior notes concern one TEST account on September 3/6/7; their raw logs are outside the repository. Preserve the evidenced storno repeat echo/no-op, storno external-id attachment, appearance/date behavior, paid-proforma deletion, final-invoice non-netting, body-only query/credit refusals and comma monetary headers. None is a new defect because a generic example suggests otherwise.

The notes establish neither mixed-prefix emission nor malformed namespace/diagnostic responses; code 56 was not triggered (`153,180–181`). They contain no receipt lifecycle or taxpayer response observations. Do not transplant invoice retry, rounding or storno evidence to receipts. Historical “design consequence” cells are not additional vendor guarantees: for example `63` retains an obsolete account-pin check, and `133,152` retain older worker delay claims. This adjudication uses observed behavior, not those superseded recovery implications.

## 8. Verification and reproducibility

### Executed here

```sh
cargo build --offline --locked -p szamlazz-agent

rustc --edition=2024 /tmp/opencode/f83e5fd-adjudication-probe.rs -L dependency=/home/laborant/szamlazz-rs/target/debug/deps --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -o /tmp/opencode/f83e5fd-adjudication-probe
rustc --edition=2024 --test /tmp/opencode/f83e5fd-mutations-review/probe.rs -L dependency=/home/laborant/szamlazz-rs/target/debug/deps --extern szamlazz_agent=/home/laborant/szamlazz-rs/target/debug/libszamlazz_agent.rlib -o /tmp/opencode/f83e5fd-adjudication-mutations
/tmp/opencode/f83e5fd-adjudication-mutations --nocapture
python3 /tmp/opencode/f83e5fd-adjudication-check.py

cargo test --offline --locked -p szamlazz-agent --test response_namespaces --test response_completion --test response_headers --test receipt_wire --test taxpayer_paths --test business_text
cargo tree --offline --locked -p szamlazz-agent --depth 1
git rev-parse HEAD
git diff --exit-code f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9 -- crates/szamlazz-agent Cargo.toml Cargo.lock fixtures docs/szamlazz-hu-behaviour.md
```

- **40 selected repository tests passed**, zero failures/ignored: namespaces 6, completion 4, headers 12, receipt wire 6, taxpayer paths 10, business text 2. These pass alongside the newly demonstrated gaps; they are not proposed-fix tests.
- **Six inspected mutation scratch assertion groups passed**, including FM1, numbered-56 closure controls, numberless credit/header fallback, identity and refusal precedence.
- **New adjudication matrix passed:** 78 list-variation parser calls across invoice and receipt entry points, plus baseline, namespace, reserved-binding, NBSP and URL controls. Full libxml2 XSD validation was performed for the conforming list variants and FQ-2; extension-gap cases were checked as namespace-well-formed, not presented as XSD-valid.
- Scratch Rust links **the actual newly workspace-built rlib**, not an old independently resolved scratch binary. Locked runtime versions: quick-xml 0.42.0, xmlparser 0.13.6, serde 1.0.229, rust_decimal 1.43.0, jiff 0.2.35, base64 0.23.1, percent-encoding 2.3.2, thiserror 2.0.20.
- Python used installed libxml2 through ctypes and ElementTree; no packages installed. Its one expected schema diagnostic is NBSP's invalid boolean lexical form. All final assertions passed.

### Fresh primary artifacts

| Artifact | SHA-256 of newly fetched bytes |
|---|---|
| [Invoice response XSD][invoice-xsd] | `747b10eb9d92e93004762cbeacd0b0e754b3a4d577194caf9002226ba46323ae` |
| [Receipt response XSD][receipt-xsd] | `73b105be7f718ebbc181c3beefdf8ad63cb4714c4eaa253435e6ca9689689126` |
| [Shared envelope XSD][envelope-xsd] | `47ed8e07bc44686b17a5f2ba492bfa6503ed90285828cd673702ff50158e9d7e` |
| [First-party PHP 2.12.4][php-zip] | `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |

Also fetched both credit response pages, PHP response guidance, W3C Namespaces 1.0 and XSD Datatypes. Their relevant statements are cited above. Vendor pages identify site build `v202608271632`; that is not an individual statement's publication date. Existing NAV source copies were inspected only for the boolean declaration; this was not another fresh full NAV-schema audit.

**Limits:** no vendor emission-frequency measurement, account operation, broad fuzzing/performance campaign, PDF rendering, full workspace/feature matrix or new transport test. Synthetic schema validity establishes the listed serialization counterexamples, not every vendor business rule. The four recommended issues should carry these bounds rather than becoming claims of normal production incidents.

[invoice-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd
[receipt-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/nyugtavalasz/xmlnyugtavalasz.xsd
[envelope-xsd]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd
[credit-en]: https://docs.szamlazz.hu/agent/credit_entry/response
[credit-hu]: https://docs.szamlazz.hu/hu/agent/credit_entry/response
[php-response]: https://docs.szamlazz.hu/php/valasz-feldolgozas
[php-zip]: https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip
[namespaces]: https://www.w3.org/TR/xml-names/
[datatypes]: https://www.w3.org/TR/xmlschema-2/
