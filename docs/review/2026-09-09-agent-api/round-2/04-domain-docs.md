# Round 2 — reviewer D: domain and public documentation

**Reviewed:** 2026-09-09. **HEAD:** `a804c740eb8446211c1cdca3eea4fb93d298d25d`.

## Decision

**Retain D1, D2, D5, D6, D7 and D9 as substantive documentation corrections. Treat D8 as a housekeeping note.** These findings do not establish a defective serializer, parser mapping, or automatic action. No runtime change is warranted for them.

The two P2 corrections are **D1 (wrong VAT category)** and **D2 (omitted NAV-submission effect)**. D7 is consequential identity guidance, but remains P3: the crate makes no isolation promise and does not assign buyer identifiers itself. D6 removes an implied paid-state restriction; it is not a request to add that restriction to the low-level operation.

Residual review retains **D10–D12 as small, exact contract/provenance corrections**, and **D13 as one combined waybill/template guidance finding**. Merge buyer-data temporal wording into D7. Keep `afalevon`'s unit as an unresolved source note, not a proven incorrect-percentage finding. Merge `simpleItems` template precedence into C1's implementation brief rather than count it again.

### Ranking and dispositions

Severity is the impact of the documentation defect, not the worst conceivable downstream incident. P2 means normal-priority consequential guidance; P3 means narrower guidance/maintenance. “Note” means no separate defect ticket is justified; exact suggested wording is still supplied where useful.

| ID / subject | Disposition | Severity / priority | Practical impact | Runtime change |
|---|---|---|---|---|
| D1 — TAHK | Retain | Medium / P2 | Can direct a caller to a different zero-VAT category than intended | No |
| D2 — `eusAfa` | Retain | Medium / P2 | Hides an accepted flag's effect on NAV submission and its prerequisites | No |
| D5 — credit-entry bank account | Retain | Low / P3 | Can reverse sender/recipient labels in exports or reconciliation screens | No |
| D6 — unpaid proforma | Retain | Low / P3 | Suggests a deletion restriction callers cannot rely on | No |
| D7 — buyer identifier | Retain; merge queried-buyer wording | Low / P3, ahead of cosmetic work | Omits documented partner updates and customer-account access consequences | No |
| D8 — currency count | Note; remove stale count in documentation sweep | Informational / P3 | Obsolete inventory statement; every listed token is already representable | No |
| D9 — erasure codes | Retain | Low / P3 | Misleads test-account setup and error diagnosis | No |
| D10 — credentials placement | Retain as housekeeping | Low / P3 | Describes the wrong XML location for two operations; built-in requests work | No |
| D11 — defaults called absent | Retain as housekeeping | Low / P3 | Confuses constructor defaults with XML omission | No |
| D12 — proforma-reference XSD claim | Retain as provenance correction | Low / P3 | Attributes a Rust-model restriction to an XSD that does not encode it | No |
| D13 — waybill scope and field precedence | Retain; merge related carrier/template omissions | Low / P3 | Hides ordinary invoice waybills, barcode fallback and delivery-note template override | No |
| `qutet/afalevon` percentage | Note; no new D ID | Informational / P3 | Unit is unverified; no demonstrated misinterpretation in crate computation | No |
| Buyer live/master-data claim | Merge into D7, not a separate finding | Low / P3 | Query data must not be advertised as an immutable issuance snapshot | No |
| `simpleItems` overrides template | Merge into C1; no additional D ID | C1's priority | The feature's existing documented rendering rule belongs with its support | C1 owns the feature change |

No P0/P1 or High-severity finding is established. The small corrections are worth making together, not eleven independent urgent work streams.

## Evidence and scope

Read [REVIEW.md](../REVIEW.md) and raw reports [01](../raw/01-invoice-requests.md), [02](../raw/02-query-responses.md), [04](../raw/04-other-operations.md), [05](../raw/05-wire-errors.md), then inspected the actual current source declarations, defaults, writers and response projections. Code references below are current line numbers relative to **`crates/szamlazz-agent/src/`**. The earlier reports are leads, not authority for the second-round conclusions.

Read all of `docs/szamlazz-hu-behaviour.md`, including its scope and unverified list. Its observations concern one TEST account on 2026-09-03/06/07. In particular:

- D3 at line 110 establishes that a fully paid proforma was deleted on that account. It refutes an unpaid-only guarantee; it does not prove that every paid proforma under every configuration can always be deleted.
- D6 at line 111 records queried buyer data changing after a later create with the same buyer name. It does not prove a universal name-matching algorithm, an explicit `azonosito` collision, or customer-account access. Those identifier consequences have separate official documentation.
- Lines 234–240 expressly leave an explicit proforma reference on prepayment/final invoices unverified. Implicit consumption by order number is not a test of explicit references.
- P60 is invoice arithmetic, predominantly HUF/EUR. It establishes neither receipt behavior, the full accepted-currency inventory, nor VAT-code legal meanings.
- There are no recorded TAHK, OSS/`eusAfa`, carrier rendering, `afalevon`-unit or erasure-code probes establishing the disputed semantics.

All official sources below were fetched afresh by unauthenticated GET on 2026-09-09. The documentation pages showed build `v202608271632`; this is a site build label, not a date for every claim. The linked VAT PDF was downloaded afresh and read with the PDF reader (the local `pdftotext` command was unavailable).

| Key | Primary source | What it establishes here |
|---|---|---|
| V-EN | [VAT rates, English](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates) | TAHK/TAM distinction; `eusAfa` effect and prerequisites |
| V-HU | [Áfa értékek, Hungarian](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates) | Original wording: “áfa tárgyi hatályán kívül”; accepted `eusAfa` suppresses submission |
| V-PDF | [Vendor VAT table, 2025-11-04, p. 1](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf), linked from the [official knowledge-base article](https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere) | TAM's public-interest/special-activity wording; TAHK → ATK mapping |
| I-HU | [Invoice XML + inline XSD, Hungarian](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | Partner warning, required settings flags, independent references, waybill and barcode annotations |
| K-HU | [Document types, Hungarian](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types) | Delivery-note layout; proforma-reference prose; paper/e-invoice flags |
| T-HU | [Templates, Hungarian](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template) | Template names/default selection and `simpleItems` precedence |
| SI-HU | [Tour-operator image, Hungarian](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/travel-agency) | Simplified image overrides template; final/storno inherit state |
| Q-XML / Q-PDF | [XML-query XML + XSD](https://docs.szamlazz.hu/agent/querying_xml/xml), [PDF-query XML + XSD](https://docs.szamlazz.hu/agent/querying_pdf/xml) | Root-level credentials in these two operations |
| Q-RESP | [XML-query response](https://docs.szamlazz.hu/agent/querying_xml/response) | Query returns `szamla`, governed by `szamla.xsd` |
| OUT-HU | [Outgoing invoices, Hungarian Adatkapcsolat](https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/kimeno-szamlak) | Annotation of the same `szamla/kifizetesek/kifizetes/bankszamlaszam`; financial-item schema |
| S-DOC | [Downloaded `szamla.xsd`](https://www.szamlazz.hu/szamla/docs/xsds/szamla/szamla.xsd) | `qutet/afalevon` is an unannotated `int`; no percentage facet |
| IN-HU | [Incoming invoices, Hungarian Adatkapcsolat](https://docs.szamlazz.hu/hu/penzugyi-adatkapcsolat/bejovo-szamlak) | Additional cross-check: its financial-item schema also does not specify `afalevon`'s unit |
| DEL | [Deletion introduction](https://docs.szamlazz.hu/agent/category/deleting-a-pro-forma-invoice), [request](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/request), [XML + XSD](https://docs.szamlazz.hu/agent/deleting_pro_forma_invoice/xml) | Existing proforma, selectors/credentials; no documented paid-state condition |
| CUR | [Supported currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) | Current list and `HUF`/`Ft` alias |
| ERR-HU | [Error catalogue, Hungarian](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Exact 537/538/539 conditions |
| ERA-HU | [Erasure-code field, Hungarian](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | Nonnegative count, cap 400, feature setting |
| ERA-KB | [Vendor erasure-code knowledge base](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor) | `SzlaMost`, count rather than identifier, uploaded stock before vendor-supplied codes |

OUT-HU is used for a field annotation of the same document schema, **not** to transfer Adatkapcsolat authentication, delivery, Ack or query-availability rules to the Számla Agent operation. IN-HU is only a search for further unit documentation; its distinct invoice direction supplies no new query guarantee.

## Assigned findings

### D1 — retain: `VatRate::Tahk` describes the wrong VAT category

**Code:** `types.rs:190–196`; correct token output at `258–264`, input mapping at `289–294`.

The existing “tárgyi adómentes, a tevékenység közérdekű vagy sajátos jellegére tekintettel” is not an awkward synonym. V-HU calls TAHK **“áfa tárgyi hatályán kívül”**, while TAM is **“tárgyi adómentes.”** V-PDF puts the public-interest/special-nature sentence under TAM (§§85–86) and TAHK under outside-scope transactions (§§2–3), mapping TAHK to ATK for NAV.

**Impact/rank:** Medium/P2. Someone selecting the variant from its definition can select TAHK for a TAM activity. Both can yield zero derived VAT, so a correct numerical total does not eliminate the category distinction. No actual misissued document was observed here.

**Existence confidence: High.** Current code and both official language versions directly disagree; the detailed PDF independently explains the distinction.

**Solution confidence: High.** Replace only the definition with the vendor's term. Avoid unnecessarily making the NAV mapping part of the minimal variant description or implying that the crate rewrites TAHK to ATK.

Exact replacement for `types.rs:194–195`:

```rust
    /// `TAHK`: áfa tárgyi hatályán kívül (outside the subject-matter scope
    /// of VAT).
```

**Runtime:** none. Keep `Tahk`, its exact `TAHK` serialization and its distinction from `Tam`. Neither a token substitution nor local tax-category validation follows from this finding.

### D2 — retain: `eu_vat` omits a consequential operation effect

**Code:** `ops/invoice.rs:181–182`, default `214`, writer `755–757`.

V-HU/V-EN explicitly say the flag indicates **no Hungarian VAT**, and **when accepted**, the invoice does not trigger NAV Online Invoice submission. They limit true to an OSS-registered seller or a seller with a non-Hungarian tax number, retain item-level VAT-code obligations, and state that retroactive submission is unavailable. I-HU repeats the submission and item-code distinction.

**Impact/rank:** Medium/P2. The current one-line label hides a processing switch. A caller can mistake it for a descriptive EU-transaction marker. This is conditional on setting true and upstream accepting it; no evidence shows that all ineligible requests are accepted, and absence of Hungarian VAT alone is not a sufficient usage rule.

**Existence confidence: High.** The omission is visible and its significance is stated directly by the vendor.

**Solution confidence: High.** Attribute the rule to szamlazz.hu, preserve “when accepted,” and avoid claiming either automatic rejection or a general rule that every zero-VAT transaction needs this flag.

Exact replacement for `ops/invoice.rs:181`:

```rust
    /// Indicates that the invoice contains no Hungarian VAT (`eusAfa`).
    /// When `Some(true)` is accepted, szamlazz.hu does not submit the invoice
    /// to NAV Online Invoice. The vendor permits this only for an
    /// OSS-registered seller or a seller with a non-Hungarian tax number.
    ///
    /// This does not replace the correct VAT code on each line item. The
    /// vendor states that retroactive submission is not possible if this
    /// setting was wrong. `None` omits the element; `Some(false)` sends false.
    /// See the [eusAfa documentation](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates#the-eusafa-field).
```

**Runtime:** none. The `Option<bool>` and current writer represent all three states correctly. This struct has no authoritative OSS/account-state information for a local eligibility gate. Do not infer the meaning of omission or false beyond what is serialized.

### D5 — retain: bank-account direction and fallback are incorrectly described

**Code:** `ops/query_xml.rs:511–512`; wire read/projection `1042–1043,1057`.

OUT-HU's exact annotation is **“A kifizetés ténylegesen erről a bankszámláról érkezett, vagy a számlán szereplő bankszámlaszám (ha a küldő bankszámlaszám nem ismert)”**: from this account, or the account on the invoice when the sender's is unknown. Q-RESP and S-DOC establish the same queried `szamla` field. “Arrived on” states the opposite direction and omits the fallback.

**Impact/rank:** Low/P3. A consumer may label a sender account as the destination. Conversely, merely changing “on” to “from” would remain incorrect for the fallback. No bank-linked live entry was examined.

**Existence confidence: High.** An explicit Hungarian field annotation contradicts the current rustdoc.

**Solution confidence: High.** Copy the documented conditional meaning, with no inferred role discriminator.

Exact replacement for `ops/query_xml.rs:511`:

```rust
    /// Sender's bank account, when known; otherwise the bank account shown
    /// on the invoice (`bankszamlaszam`). The field does not identify which
    /// of these two sources supplied the value.
```

**Runtime:** none. Keep the neutral `bank_account` name and optional string. Do not rename it to an unconditional `sender_account`, manufacture the fallback client-side, or alter the response's text normalization as part of this semantic fix.

### D6 — retain, with bounded live wording: deletion is not an unpaid-only API

**Code:** `ops/proforma.rs:1–2,26–31,48–76`.

DEL says **“delete an existing pro forma invoice.”** Its request defines credentials and a selector, with no unpaid-only promise. That silence alone would not disprove an upstream business-state check; the recorded D3 observation at `docs/szamlazz-hu-behaviour.md:110` supplies the counterexample: a fully paid proforma was deleted. There is no client-side balance check.

**Impact/rank:** Low/P3. The one adjective can be read as an eligibility restriction. A caller must not rely on that restriction. Describing this as an observed production data-loss incident would overstate the evidence; the probe was deliberate and on a test account.

**Existence confidence: High.** Current prose implies a restriction the implementation does not enforce and the recorded test-account result contradicts.

**Solution confidence: High.** Say what the operation targets, what the client does not check, and identify the scope of the counterexample rather than promise unconditional deletion everywhere.

Exact replacement for the module introduction:

```rust
//! Proforma deletion (`xmlszamladbkdel`): removes an existing proforma
//! (díjbekérő) from the account.
//!
//! This operation does not check payment status before sending. Deletion of
//! a fully paid proforma was observed on the test account. If paid proformas
//! must be retained, the caller must enforce that policy before deletion.
```

**Runtime:** none. Adding a query/paid guard would change this operation's scope and cannot be justified as conformance repair. The worker's policy is a separate interface.

### D7 — retain: buyer identifiers select partner identity, not just metadata

**Code:** request `ops/invoice.rs:288–289,816`; response distinction `ops/query_xml.rs:355–365` (`id: Option<i64>` versus `identifier: Option<String>`).

I-HU tells callers not to send an `azonosito` already assigned to another buyer. It names two effects: **“a vevői számlafiók link birtokában az összes, az adott vevői fiókhoz tartozó bizonylatot elérhetik”** and **“automatikusan frissítjük a partner adatait.”** Thus the customer-account link can expose that partner account's documents, and recognized identifiers cause supplied billing data to update the partner.

**Impact/rank:** Low/P3, but ahead of cosmetic work. An order-local/recycled value can identify an existing different partner. The documentation needs to expose that consequence. It is not proof of arbitrary account access or a vulnerability in this crate: the caller supplies the identifier, and the vendor's access statement is conditional on possession of the customer-account link.

**Existence confidence: High.** The current sentence omits an explicit vendor warning about the very field it documents. It does not itself make a false uniqueness/isolation guarantee, so classify this as missing consequential guidance, not a broken guarantee.

**Solution confidence: High.** State account-local one-partner use and the two vendor-documented effects, without assuming a particular partner lookup algorithm or trying to validate uniqueness locally.

Exact replacement for `ops/invoice.rs:288`:

```rust
    /// Partner identifier in the billing account's partner records
    /// (`azonosito`). Use an identifier for only one partner within that
    /// account; do not reuse one assigned to another buyer.
    ///
    /// When szamlazz.hu recognizes the identifier, it updates that partner
    /// with the billing data supplied in this request. Buyers sharing an
    /// identifier can access the documents of that customer account through
    /// its customer account link. This is distinct from szamlazz.hu's
    /// internal numeric buyer `id` returned by an XML query.
```

#### Merge: queried buyer data versus an issuance snapshot

`ops/query_xml.rs:355` says **“as recorded on the invoice.”** This can reasonably mean the returned invoice record; it is not an explicit immutable-snapshot promise. Do not open a second defect solely over that phrase. Nevertheless, D6's recorded change after another create makes an added clarification useful in the same buyer-semantics patch.

The official identifier-update warning corroborates mutability of partner records, but does not establish that *every* queried buyer field is always current master data. OUT-HU's “all current data” delivery prose is also not a guarantee of every query field's storage model. The recorded query changed after a same-name create; it did not test every identity path, field, account setting or PDF.

**Existence confidence: Medium** for a misleading temporal implication: the phrase is ambiguous rather than explicitly false. **Solution confidence: High** for the qualified replacement below, which states only the observed possibility.

Exact replacement for `ops/query_xml.rs:355`:

```rust
/// Buyer data (`vevo`) returned by the invoice XML query.
///
/// Do not assume this is an immutable snapshot from issuance: on the test
/// account, a later create changed buyer data returned for an earlier
/// document. The document-associated email is a separate field on
/// [`InvoiceInfo`].
```

**Runtime for both parts:** none. Keep the input string identifier and the two distinct response identifiers. No local uniqueness database, re-query, payload-equality test, partner-renaming rule or reconstructed issuance snapshot is warranted.

### D8 — note: remove the currency count, not a capability defect

**Code:** `types.rs:361–364`; open construction `381–384`, output `386–390`.

CUR currently lists 47 tokens, including `HUF` and its `Ft` alias (46 currency entries after collapsing that alias). The previous “37 ISO-style codes” is stale. The table includes historical codes and vendor spelling `KSH`; do not silently replace the vendor list with today's ISO active-currency list.

**Impact/rank:** Informational/P3. No current listed token is unrepresentable, and the existing next sentence already says the set is open. Treat as housekeeping rather than a separately counted interoperability defect.

**Existence confidence: High.** The present table contradicts 37. **Solution confidence: High.** Removing the count avoids another frozen inventory and makes server acceptance distinct from local string construction.

Suggested complete type introduction:

```rust
/// A currency code (`pénznem`).
///
/// See szamlazz.hu's [supported currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies).
/// Hungarian forint may be written `HUF` or `Ft`. This type is open:
/// [`Currency::new`] and `From` retain any code without checking whether
/// szamlazz.hu accepts it.
```

**Runtime:** none. No new currency enum, whitelist, count assertion or minor-unit change. P60 cannot establish storage precision for the entire list.

### D9 — retain: separate the three erasure-code conditions

**Code:** `item.rs:107–114`; the already-correct variants at `error.rs:162–167`, mappings `210–212,249–251`; local cap check `ops/invoice.rs:650–657`.

ERR-HU is unambiguous: **537** is the 400-per-item cap, **538** forbids demo/test accounts, **539** is the disabled setting. ERA-HU repeats 537/539. ERA-KB requires the recommended `SzlaMost` invoice layout but assigns no error code to a wrong layout. Thus the current **“Requires the account feature; on invoices the `SzlaMost` template (errors 537–539 otherwise)”** misbundles separate conditions.

An additional correction belongs in the same paragraph: **“szamlazz.hu generates this many codes”** is too specific. ERA-KB says an uploaded account stock is used first; szamlazz.hu supplies codes if there is no stock. “Requests this many codes” accurately covers both.

**Impact/rank:** Low/P3. Someone following the field documentation can arrange the feature/template and still be surprised by 538 on a test account. The typed error itself remains correct and actionable. No lost erasure code or accepted malformed count was observed.

**Existence confidence: High.** The catalogue directly distinguishes all three conditions; ERA-KB also establishes the stock/supply wording correction.

**Solution confidence: High.** Describe count, prerequisites and actual codes separately. Preserve the invoice-only template qualification: this shared item also appears in receipts, which have a different template surface.

Exact replacement for `item.rs:107–111`:

```rust
    /// Number of data erasure codes requested for this row (`torloKod`), at
    /// most [`MAX_ERASURE_CODE_COUNT`]. This is a count, not a code identifier.
    ///
    /// The account must enable the feature (otherwise code 539); demo and
    /// test accounts cannot use it (538). Code 537 denotes exceeding the
    /// per-item count limit. For invoices, the vendor requires the `SzlaMost`
    /// template. Codes may come from the account's uploaded stock or be
    /// supplied by szamlazz.hu.
```

**Runtime:** none. Existing error mappings and count validation are correct. Do not introduce a local test-account gate (the request carries no authoritative account mode), force a template, or assign a guessed wrong-template code. Fixing the error wording is not a new error-enum capability.

## Residual findings retained with new IDs

### D10 — retain, housekeeping: credentials do not always live in `beallitasok`

**Code:** `credentials.rs:45–48`; root-level writes in `ops/query_xml.rs:523–540` and `ops/query_pdf.rs:54–70`; invoice settings write at `ops/invoice.rs:685–687`.

Q-XML and Q-PDF put credential elements directly in the root sequence. I-HU puts them in `beallitasok`. The broad **“block of every request”** sentence is therefore false even though the serializer is correct.

**Impact/rank:** Low/P3. Relevant to readers inspecting the protocol or implementing a custom request. It does not cause built-in query authentication to fail.

**Existence confidence: High:** code and two operation-specific XSDs prove the exception. **Solution confidence: High:** use operation-specific placement language and name the exceptions.

Replace `credentials.rs:45–48` with:

```rust
/// Credentials injected into each request's XML at the location required
/// by the operation: directly under the root for invoice XML/PDF queries,
/// and in `beallitasok` for the other built-in operations.
///
/// Credentials are client state, not document data: request types do not carry
/// them; they are supplied when a request is serialized to the wire.
```

**Runtime:** none. Do not move the query credentials into a settings block to agree with the old prose.

### D11 — retain, housekeeping: constructor defaults are not all absent

**Code:** `ops/invoice.rs:523–526,549–563`; explicit flags at `688–689`; seller container at `770–779`. Related header constructor `190,199–216`, paid writer `749–751`.

The constructor documentation explicitly includes `e_invoice`, `download_pdf` and `seller` among fields defaulting to “absent.” The two flags are false and always sent, and `elado` is present with optional children omitted. I-HU requires those flags and the seller container; it does not declare server defaults for them.

**Impact/rank:** Low/P3. This is a directly inaccurate statement about the Rust/wire contract, but the constructor and writer already do the right thing. It does not prove an omission-versus-false server bug.

**Existence confidence: High:** defaults and writer are explicit. **Solution confidence: High:** describe Rust values and emitted presence independently.

Replace `ops/invoice.rs:523–526` (keep its example) with:

```rust
    /// An invoice-creation request with the supplied blocks and line items.
    /// `e_invoice` and `download_pdf` default to false and are sent explicitly.
    /// Optional settings default to `None`; seller fields and attachments
    /// start empty. The seller XML container is still emitted.
    /// Set fields on the returned value, or use functional update:
```

For the related header default, replace `ops/invoice.rs:190` with:

```rust
    /// A header with the required fields. `Option` fields default to `None`
    /// and `paid` to false; the writer omits `fizetve` when `paid` is false.
```

**Runtime:** none. In particular, do not change `paid` to `Option<bool>` or promise that omitted `fizetve` and explicit false have the same upstream effect. That behavioral question remains unverified in the first-round evidence.

### D12 — retain: the XSD does not restrict proforma references to three kinds

**Code:** `ops/invoice.rs:21–30,102–114,719–743`.

I-HU's `fejlecTipus` is a sequence of independent optional `dijbekeroSzamlaszam` and kind elements. It has no conditional assertion, choice or three-kind restriction. K-HU documents referencing a proforma while creating an invoice but does not enumerate exactly three schema-permitted kinds. **“the three kinds the XSD lets carry it”** assigns the Rust model's restriction to the wrong authority.

**Impact/rank:** Low/P3, provenance rather than a missing capability. No additional useful documented combination was established. The practical correction is to make future readers distinguish schema expressibility, Rust interface design and observed acceptance.

**Existence confidence: High:** the complete header sequence contradicts the claimed restriction. **Solution confidence: High:** describe what this enum exposes, without expanding it or presenting unprobed combinations as guaranteed server behavior.

Replace `ops/invoice.rs:23–30` with:

```rust
/// The wire uses independent boolean flags (`dijbekero`, `elolegszamla`, …)
/// and reference elements. This enum selects one document kind and attaches
/// the references exposed for that kind.
///
/// It exposes the proforma reference (`dijbekeroSzamlaszam`) on regular,
/// prepayment and final invoices; [`InvoiceKind::proforma_number`] reads it
/// uniformly. The XSD declares that reference independently of the kind
/// flags, rather than restricting it to these three kinds.
```

Replace the method introduction at `102–104` with:

```rust
    /// The proforma reference (`dijbekeroSzamlaszam`) carried by this value:
    /// available on regular, prepayment and final invoice variants, and
    /// `None` for the other variants.
```

**Runtime:** none. Preserve the three reference-bearing variants and the prepayment variant's explicit live-evidence qualification. The existing C1-3 observation is implicit linking; it cannot upgrade explicit ES/VS references to live-verified status.

### D13 — retain as one finding: waybill scope, template override and barcode fallback

**Code:** module `ops/waybill.rs:1–5`; fields `91–96`; pass-through writer `127–130`; template field `ops/invoice.rs:183–184`; kind-dependent template writer `758–764`; unconditional-by-kind waybill writer `821–823`; template tokens `types.rs:971–997`.

I-HU's root annotation says the optional waybill should be used with an invoice layout that can display it: **“olyan számlaképet használsz ami meg tudja jeleníteni.”** It does not restrict the block to `szallitolevel=true`. The module's **“only the delivery-note use needs”** is therefore misleading. This is not simply translating “fuvarlevél” versus “szállítólevél”: a waybill block and the document's kind are independent in both the XSD and writer.

Related field interactions belong in the same correction:

1. **Client-side template override:** a delivery-note kind always emits `SzlaFuvarlevelesAlap`, even when `header.template` is another value. K-HU calls for that layout. The override is intentional, but the template field's one-line description does not expose it.
2. **Server-side barcode fallback:** I-HU says generic `fuvarlevel/vonalkod` is used if the carrier sub-block does not supply the data needed to generate its barcode. The current “General barcode” omits precedence. This is not an overwrite performed by the Rust writer: it sends both fields when present.
3. **Unused destination:** I-HU marks `uticel` unused and names `sprinter/iranykod` as its replacement. “Legacy destination” can be more precise. Do not generalize Sprinter's replacement into a routing-code field for every carrier.

**Impact/rank:** Low/P3. Caller-facing consequences are an unexpected layout or barcode selection, or assuming a waybill requires issuing a delivery note. No PDF/carrier delivery failure was observed. Several minimally documented related fields warrant one cohesive correction, not separate inflated findings.

**Existence confidence: High:** module claim, writer override and schema barcode annotation are directly inspectable. **Solution confidence: High:** distinguish the library's template selection from documented upstream rendering, without guessing carrier-specific defaults or enforcing undocumented block exclusivity.

Exact module replacement for `ops/waybill.rs:1–5`:

```rust
//! Optional carrier waybill data (`fuvarlevel`) for the invoice-creation
//! operation, including four carrier-specific sub-blocks. The block is not
//! limited to delivery notes; use a document layout that can display it.
//! Re-exported from [`ops::invoice`](crate::ops::invoice), where it is a field
//! of [`CreateInvoice`](crate::ops::invoice::CreateInvoice).
```

Exact field replacements in `ops/waybill.rs`:

```rust
    /// Legacy destination (`uticel`), documented as unused. For Sprinter
    /// routing, use [`Sprinter::routing_code`].
```

```rust
    /// General barcode (`vonalkod`), used by szamlazz.hu when the selected
    /// carrier's sub-block does not supply the data needed to generate one.
```

Exact template-field replacement at `ops/invoice.rs:183`:

```rust
    /// Requested invoice PDF template (`szamlaSablon`). `None` leaves the
    /// element absent, except for [`InvoiceKind::DeliveryNote`]: that kind
    /// always sends `SzlaFuvarlevelesAlap`, overriding this field.
```

Related exact clarification for `types.rs:973`:

```rust
    /// `SzlaAlap`, the traditional invoice layout. This selects a named
    /// template; it is not the same as omitting `szamlaSablon`.
```

**Runtime:** none. Preserve the intentional delivery-note template override and pass-through carrier blocks. No source establishes that every carrier token needs a new dedicated block, that inconsistent optional sub-blocks must be rejected locally, or that a layout setting alone should become the document-kind flag.

#### Merge into C1: simplified-image template precedence

T-HU and SI-HU explicitly say `simpleItems=true` overrides `szamlaSablon`; SI-HU says final/storno inherit simplified-image state. The present crate cannot set `simpleItems`, so do not add a second missing-feature finding here. C1's implementation/documentation should include:

```rust
/// When simplified-image mode applies, szamlazz.hu renders that image
/// regardless of `szamlaSablon`. Final and storno invoices inherit the
/// relevant earlier document's simplified-image setting.
```

**Existence confidence: High** for the documented interaction. **Solution confidence: High** for merging this wording into C1; **no confidence is claimed here about C1's unresolved combined preview/`simpleItems` XML order**. Neither this rule nor the erasure-code layout requirement proves how those two features interact.

## Residual source note: `qutet/afalevon` is not proven to be a percentage

**Disposition: note, not a retained false-semantics finding; no D14 assigned.**

**Code:** `ops/query_xml.rs:453–480,982–1016`; synthetic example `1300`.

S-DOC, OUT-HU and IN-HU all declare `afalevon` as `int` without a unit, range, enum or annotation explaining its scale. `qutetek` is annotated **“pénzügyi tételek”** (financial items). These sources do not establish the current **“Deductible VAT percentage”**, but their silence does not disprove it either. The locally authored sample `50` is not primary evidence that 50 means 50%. The crate simply retains the integer and computes nothing from it.

**Impact/rank:** Informational/P3. A consumer may need the unit before using it in bookkeeping. There is no demonstrated incorrect calculation, observed value distribution or upstream unit definition to justify a stronger defect claim.

**Existence confidence: Medium** for unsupported precision in the rustdoc; **Low** for an assertion that “percentage” is actually wrong. The fetched sources are clear about type but incomplete about meaning. **Solution confidence: High** for qualifying the unit as unverified; **Low** for replacing it with a guessed boolean, amount, coded state, or guaranteed percentage interpretation.

Suggested replacement for `ops/query_xml.rs:479` if making the documentation sweep:

```rust
    /// VAT-deductibility value (`afalevon`), retained as the reported integer.
    /// The published schema does not specify its unit or range; the crate
    /// does not interpret it as a percentage or use it in calculations.
```

The `FinancialItem` introduction's possible “QUiCK tétel” origin is already explicitly tentative. Do not promote it into a settled translation; no naming/runtime change follows. In particular, do not add a 0–100 gate, divide the value by 100, or alter the `i64` based on the current evidence.

## Completion and implementation guidance

1. Correct D1/D2 first; D7 and D6 are the next most useful caller guidance. D5/D9 and the small D10–D13 fixes fit the same documentation patch. D8 and the `afalevon` qualification can accompany it as notes, without being counted as new functional defects.
2. Every retained finding has an exact proposed rustdoc replacement above. These recommendations preserve current runtime behavior. The only referenced feature work is C1, owned separately; its template interaction is merged there.
3. No new unit test is needed to assert comment strings. When applying the recommendations, build the relevant rustdoc to check links and retain the existing code examples. Existing parsing/serialization tests cannot verify OSS status, portal access, VAT classification, bank-account direction or real carrier rendering.
4. This pass ran no Rust tests: the deliverable is this review, with no production changes. Verification consisted of source inspection, fresh official-source comparison, line-reference checks and report review. No live Számla Agent call or further agent was used.
5. Concurrent modifications were present at the start in the worker, CLI, Adatkapcsolat and `Cargo.lock`; they were left untouched. The report is the only repository file written by this reviewer. The downloaded VAT PDF is under `/tmp/opencode/round2-reviewer-d-vat.pdf`.
