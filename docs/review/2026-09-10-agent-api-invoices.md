# Számla Agent invoice-creation request review — 2026-09-10

## Verdict

Reviewed **requested HEAD `382cf7615aca1d64a05c7c3f77110248dde51950`**, against freshly retrieved official documentation and downloads. HEAD matched at the start. Concurrent work advanced it to `7da23b44cd006783eb47e60aa54ab8969bdd1c2c` before completion; a final `git diff 382cf7615aca1d64a05c7c3f77110248dde51950 -- crates/szamlazz-agent` was empty, so the reviewed implementation and line references still match the requested commit.

**One confirmed defect: Medium severity, in derived line-item arithmetic.** Decimal's checked operations can silently reduce precision before the requested rounding policy runs, including in `Rounding::Exact`. No additional confirmed invoice-request field, namespace, escaping, or ordinary sequence defect was found. Two previously recorded vendor ambiguities remain unresolved: preview/`simpleItems` ordering and invoice-layout names.

This is a review of the current implementation, not a diff review or a carry-forward of the historical `2026-09-09-agent-api/FINAL.md`. In-scope tracked files were unchanged from HEAD when inspected (`git diff HEAD -- crates/szamlazz-agent fixtures/SOURCES.md docs/research/2026-09-10-agent-vendor-questions.md docs/szamlazz-hu-behaviour.md` was empty). The existing unrelated working-tree changes were present before this review. Only this report was added in the workspace; executable probes and downloaded material were placed under `/tmp/opencode/invoice-audit-382cf761-current/`. No live-account requests were made.

### Scope and evidence boundaries

- Invoice **request**: every `InvoiceKind`, settings, header, seller, buyer/postal/ledger blocks, all item and waybill fields, attachment contribution, shared request tokens, rounding and semantic rustdoc.
- Shared XML/wire code was inspected only for the invoice request's serialization, credentials and XML-character gate. Response envelopes, other operation implementations, transport and error classification belong to the other reviews.
- Read the supplied `CONTEXT.md` domain guidance, `docs/szamlazz-hu-behaviour.md`, `fixtures/SOURCES.md`, and `docs/research/2026-09-10-agent-vendor-questions.md` before classification.
- “Covered” below means compared with the available official specification and current writer/model. It does **not** claim server execution, rendered-PDF verification, or live acceptance of every combination.
- Unimplemented convenience validation is not a defect merely because the server enforces a business rule. A deliberately narrower request model is distinguished from a missing working capability.

## 1. Fresh official sources

All sources below were retrieved during this review on **2026-09-10**, with unauthenticated GETs. Current combined documentation pages display site build `v202608271632`; this is not a per-statement publication date. The still-served legacy EN `/xsd` page displays `v202606031507`.

Source labels used in the coverage tables:

| Label | Official source and what was checked |
|---|---|
| R | [EN request](https://docs.szamlazz.hu/agent/generating_invoice/request), [HU request](https://docs.szamlazz.hu/hu/agent/generating_invoice/request): POST, multipart action, attachment fields. |
| X | [EN XML example + inline XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml), [HU XML example + inline XSD](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml): both complete examples and schemas, including annotations. |
| L | [Legacy EN XSD page](https://docs.szamlazz.hu/agent/generating_invoice/xsd): independently served older schema, not assumed identical to X. |
| D | [Downloaded invoice XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd): fetched directly, kept separate from inline and project-modified fixtures. |
| K | [Document types](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types): kinds, references, appearance. |
| SI | [Tour-operator simplified image](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency): all documented conditions and kind-specific behaviour. |
| V | [HU VAT rules](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates) and the vendor's [VAT guide PDF](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf): all invoice VAT tokens, meanings, `eusAfa`, K.AFA subtype prose. PDF downloaded and read. |
| Q | [EN rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding), [HU rounding](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding): net-first, gross-first and the eight HUF fractional/integer cases. |
| C | [Currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies): complete currency list, HUF/Ft and bank/rate requirements. |
| T | [Templates and languages](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template), linked [knowledge-base layouts](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok): six layout tokens, omission, 15 language tokens, source conflict. |
| O | [Order-number rules](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number): duplicate toggle, kind-specific checking, exemptions, two-day repeat conditions. |
| DI | [Discounts](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount): negative-price positive-quantity rows; no separate discount field. |
| E | [Email notification](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification): omitted/false/true `sendEmail`, comma-separated recipients, BBCode, five attachments, 2 MB, test delivery. |
| ER | [Erasure codes](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code), linked [HU knowledge-base guidance](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor): count, 400 maximum, account feature and SzlaMost. |
| B | [Sending requests](https://docs.szamlazz.hu/agent/basics/sending-requests), [authentication](https://docs.szamlazz.hu/agent/basics/authentication): one document per XML, exact tag case, credentials. |
| P | [Official PHP download page](https://docs.szamlazz.hu/php/) and [PHP API 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip): freshly downloaded and extracted; inspected invoice header/item/settings and payment tokens as corroboration, not as universal server authority. |

The [settings index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) was fetched to enumerate the current rule pages. The noncanonical `/agent/generating_invoice/` URL returned 403; the actual request, example, schema and rule pages above succeeded. Examples are now inline code blocks; they were extracted from the fresh HTML, not taken only from cached XML fixtures. Layout preview PDFs were not used to infer token-to-layout behaviour; that remains a vendor clarification question.

### Acquisition fingerprints

SHA-256 values for downloaded bytes, or HTML-decoded inline schema text without an added newline:

| Artifact | SHA-256 |
|---|---|
| D: invoice XSD | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` |
| X: EN inline schema | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` |
| X: HU inline schema | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` |
| L: legacy EN inline schema | `508162a8a38ec80648db3d013b7ab1b258532cae914997887192a17dcbada801` |
| P: PHP ZIP | `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| V: VAT PDF | `bb5a52eda87e383be3276870fee937c6a34d87f7a2a3542a03a0bf90a63d9465` |

The first three match the existing provenance record, independently re-established. A matching hash establishes source stability, not server correctness. No cached fixture was replaced or merged.

## 2. Confirmed finding

### INV-01 — Medium: checked Decimal arithmetic can silently round before the selected rounding policy

**Location:** `crates/szamlazz-agent/src/item.rs:189–192` (net multiplication before rounding), `203–207` (VAT multiplication/division), `210–212` (gross addition). The affected contract is stated at `item.rs:11–18`, `24–30`, and `163–180`.

**Confidence:** High for local reproduction and contract violation; no claim of live acceptance or an observed production invoice affected. Normal-scale invoicing is unlikely to reach these boundaries, which limits practical severity. This is not a high-severity ordinary-price rounding failure.

**Problem:** `checked_mul`, `checked_div`, and `checked_add` detect overflow but do not ensure exact representability of the mathematical result. They may reduce scale and round, or underflow to zero, while returning `Some`. The subsequent `Rounding::apply` therefore acts on an already rounded value. `Rounding::Exact` simply returns that value, and gross addition can also lose a fractional component without returning an error.

**Fresh official support:**

- [Rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding): “Multiply the ‘Net unit price’ with the ‘Quantity of item’. If the result is not an integer, you must round it.” The same page defines gross as “the sum of the total value and the VAT value”.
- [Request XML](https://docs.szamlazz.hu/agent/generating_invoice/xml): “Számlázz.hu does not calculate amounts from item data, so all amounts shown on the invoice must be provided explicitly.”
- [Currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies) expressly includes EUR; choosing `Scale(2)` is the crate's advertised minor-unit policy for it. The two-decimal repro below does not depend on a special or unsupported VAT token.
- The stronger “No rounding: exact decimal arithmetic” and “gross = net + VAT holds exactly on the wire” guarantees are the crate's own, not a vendor precision guarantee. They are violated before XML serialization or a server exchange.

**Offline repro against the current crate, resolved rust_decimal 1.43.0** (also the version in the requested commit's Cargo.lock; the manifest permits versions from 1.42.1):

```rust
use rust_decimal::dec;
use szamlazz_agent::{LineItem, Rounding, VatRate};

let item = LineItem::try_calculated(
    "boundary", dec!(0.9999999999999999999999999999), "db",
    dec!(0.005), VatRate::Aam, Rounding::Scale(2),
).unwrap();
assert_eq!(item.net_value, dec!(0.01)); // actual current result
// Exact product: 0.0049999999999999999999999999995.
// Half away from zero at scale 2 is 0.00, not 0.01.
```

The selected rounding should either be calculated from the exact product, or calculation should fail under the documented representability rule. An `Ok` result rounded up by an undisclosed intermediate rounding is neither.

Additional executed controls:

| Inputs / policy | Current result | Mathematical result / broken promise |
|---|---|---|
| Price `1e-28`, quantity `0.1`, AAM, Exact | `Ok`, net `0`, VAT `0`, gross `0` | Net `1e-29` does not fit Decimal but is silently replaced with zero. |
| Price `1e-28`, quantity `1`, 27%, Exact | `Ok`, VAT `0` | VAT is `2.7e-29`; Exact has silently rounded. |
| Price `1e28`, quantity `1`, percentage `1e-28`, Exact | `Ok`, net `1e28`, VAT `0.01`, gross `1e28` | Exact gross sum cannot fit and the `0.01` is lost. This synthetic rate is **not** evidence of a vendor-supported rate; it only challenges the general local invariant. |

**User impact:** callers relying on the derived constructor can receive silently altered amounts despite explicitly choosing a rounding policy, or despite asking for exact calculation and expecting an error when values cannot fit. At a two-decimal midpoint the error can become a whole cent, rather than merely an invisible extra digit. These values are then written as supplied by the invoice serializer (`invoice.rs:887–896`).

**Smallest recommended fix:** introduce loss-aware arithmetic at these three calculation boundaries. Conservatively returning `ArithmeticError` when an intermediate loses information is smaller than adding arbitrary-precision calculation and matches the advertised error contract; an implementation that supports such intermediates must instead apply the chosen rounding to exact coefficient/scale arithmetic. Include the midpoint, underflow and gross-loss cases. Merely replacing operators with the existing `checked_*` calls, or checking only `is_zero`, is insufficient. Keep `LineItem::new` as the caller-supplied escape hatch. No production change was made.

**False-positive challenge:** the documented HUF normalization and observed EUR storage rounding happen at the server and cannot explain this pre-wire loss. Ordinary intentional rounding is appropriate; the defect is unrequested rounding *before* that policy, or inside Exact. Conservative overflow rejection is already an explicit library choice and is not itself flagged.

## 3. Systematic field coverage

Paths below are under `crates/szamlazz-agent/src/`. `I` = `ops/invoice.rs`, `W` = `ops/waybill.rs`. Every leaf and nested child slot in the current EN/HU complex types was inventoried: **118 child declarations**, plus the six root-block declarations. This count includes container declarations and mutually exclusive credentials/kind fields; it is not a count of fields necessarily emitted in one invoice.

### 3.1 Document kinds and references

| Kind / capability | Current mapping and defaults | Assessment / source |
|---|---|---|
| Regular invoice | `Invoice { proforma_number }`; no true kind flag. Optional reference emitted before flags. I:34–40, 772–779. | Covered; K/X/D. |
| Proforma | `dijbekero=true`; other kind flags omitted. I:41–43, 794. | Covered; K. Not a finalized invoice. |
| Delivery note | `szallitolevel=true` plus forced `SzlaFuvarlevelesAlap`, even over a supplied template. I:44–46, 795, 811–817. | Covered; K describes template, X/D and PHP support flag. Override is documented at I:193–196. |
| Prepayment | `elolegszamla=true`; optional `dijbekeroSzamlaszam`. I:47–59, 780. | Covered. Explicit proforma reference is schema-supported; no live explicit-reference claim. |
| Final | `vegszamla=true`, optional `elolegSzamlaszam`, optional proforma reference. I:60–78, 781–788. | Covered; X/K. Local requirement: nonblank prepayment number **or** order number (I:686–701). One final per prepayment is a vendor rule, not duplicated locally. |
| Corrective | `helyesbitoszamla=true` followed by `helyesbitettSzamlaszam`. I:79–84, 790–792. | Covered; K. String wrapper does not establish existence or nonblank content; server validation is intentional. |
| Mixed kind flags / other reference combinations | The enum selects one kind. No proforma reference on Proforma/DeliveryNote/Corrective, and no prepayment reference outside Final. I:21–30, 104–116. | Intentionally unexposed schema combinations, **not a confirmed defect**: XSD independence alone does not establish a meaningful accepted business operation. Rustdoc accurately states this narrower surface. |
| Final deduction | Caller supplies negative prepayment line at the same VAT rate; not automatically netted. I:62–70. | Correct live-behaviour caveat, behaviour notes C6-2. |

### 3.2 Root and settings (`beallitasok`)

Root order is `beallitasok`, `fejlec`, `elado`, `vevo`, optional `fuvarlevel`, `tetelek` (I:738–919). The required empty seller container is retained. One request emits one document; item vector order is retained.

| XML field | Rust representation / emission | Coverage |
|---|---|---|
| `felhasznalo`, `jelszo`, `szamlaagentkulcs` | Credentials enum emits key alone or user/password in that order; `xml.rs:456–465`, I:740. | B/X/D; no simultaneous-credential requirement. Values escaped, not case-normalized. |
| `eszamla` | `e_invoice: bool`, default false, always explicit. I:532–539, 604, 741. | K/X; paper/e-invoice semantics correct. Account capability remains server-side. |
| `szamlaLetoltes` | `download_pdf: bool`, default false, always explicit. I:540–542, 605, 742. | R/X; valid local default, need not match example/PHP true. |
| `szamlaLetoltesPld` | `Option<u8>`, absent by default. I:543–547, 743–745. | Deprecated/ignored per current X. Narrower than `xs:int` but no demonstrated lost current capability. |
| `valaszVerzio` | Always `ops::RESPONSE_VERSION`, `2`. I:746; `ops.rs:27–31`. | X documents 1/2. Version 1 intentionally not offered by this operation. |
| `aggregator` | Optional string, absent by default, no mutation. I:548–549, 747. | X/D/P; wire coverage complete. See semantic documentation opportunity below. |
| `guardian` | Optional bool, absent by default, explicit false retained. I:550–551, 748–750. | X/D; public sources give little operational meaning. No inferred effect/validation. |
| `cikkazoninvoice` | Optional bool, explicit false retained. I:552–553, 751–753. | X/D; exposes item-identifier setting. |
| `szamlaKulsoAzon` | Optional string. I:554–556, 754. | X/O/behaviour notes: query handle, not unique/idempotent; no uniqueness guarantee in rustdoc. |

### 3.3 Header (`fejlec`) beyond kind fields

| XML field(s) | Rust mapping, default / writer | Assessment |
|---|---|---|
| `keltDatum` | `issue_date: Option<Date>`, absent; I:141–150, 758. | X marks optional. Rustdoc correctly qualifies requested vs observed actual date. |
| `teljesitesDatum` | Required `fulfillment_date: Date`; I:151–153, 759. | Correct HU meaning; EN example's “payment date” is a translation error, not grounds to rename. |
| `fizetesiHataridoDatum` | Required `due_date: Date`; I:154–156, 760. | X/Q. No invented today/+8-day default. |
| `fizmod`, `penznem`, `szamlaNyelve` | Required PaymentMethod/Currency/Language; I:157–162, 761–763. | Full token coverage discussed below. |
| `megjegyzes` | Optional `comment`; I:163–164, 764. | X/V/SI: free text and K.AFA wording can be supplied. |
| `arfolyamBank`, `arfolyam` | Optional `ExchangeRate`, bank plus optional Decimal; I:165–166, 765–770. | C requires non-HUF bank/rate; X/D explicitly allow omitted MNB rate. I:719–729 enforces this, including HUF/Ft case-insensitive exemption. |
| `rendelesSzam` | Optional string `order_number`; I:167–169, 771. | O: correct query-key description, no unconditional uniqueness promise. |
| `logoExtra` | Optional `extra_logo`; I:170–171, 797. | P calls it the second logo file name. No upload feature inferred. |
| `szamlaszamElotag` | Optional `number_prefix`; I:172–175, 798. | X/B: account prefix, no local prefix registration lookup. |
| `fizetendoKorrekcio` | Optional Decimal `payable_adjustment`; I:176–177, 799–801. | P: payable correction, not a change to gross; the existing short doc does not claim otherwise. |
| `fizetve` | `paid: bool=false`; only true is emitted. I:178–180, 802–804. | Explicitly documented local omission policy; matches P. Cannot force an explicit false through this field. Account/payment-method effects are not established by mere omission. |
| `arresAfa` | Optional bool `margin_vat`; I:181–182, 805–807. | X/D/P: margin taxation. Does not automatically compute item margin tax. |
| `eusAfa` | Optional bool `eu_vat`; I:183–192, 808–810. | V: no Hungarian VAT, accepted true suppresses NAV submission, seller OSS/non-HU condition, proper item tokens still required. Current rustdoc contains these qualifications. |
| `szamlaSablon` | Optional InvoiceTemplate, absent except forced delivery-note layout; I:193–196, 811–818. | T/K/SI: simplified mode overrides layout server-side. Token labels remain conflicted (§5). |
| `elonezetpdf` | Optional bool `preview_pdf`; I:197–198, 819–821. | X: preview without creating document; omission and false retained as distinct request shapes. Response interpretation not reviewed here. |
| `simpleItems` | Optional bool `simple_items`, defaults absent; I:199–225, 824–826. | SI fully exposed. Current documented order policy matches D/P, conflicts with X (§5). |

Constructor defaults are all explicit at I:228–259, 595–617: no financial/date/currency/language defaults hidden in the constructors; caller supplies required values. Serde optional fields decode absence as `None`, and documented default booleans have `#[serde(default)]`.

### 3.4 Seller, buyer, postal and buyer ledger

| Block / XML fields (complete list) | Rust / writer | Assessment |
|---|---|---|
| Seller `bank`, `bankszamlaszam` | `Seller.bank`, `bank_account`, both optional; I:262–270, 829–830. | X/D/P; defaults to account data rather than allowing seller identity replacement. |
| Seller `emailReplyto`, `emailTargy`, `emailSzoveg` | Optional SellerEmail and optional children; I:271–272, 831–835; `types.rs:1022–1032`. | E: reply-to, subject and BBCode body. LF text and escaped XML work. No separate from-address or attachment XML field exists. |
| Seller `alairoNeve` | Optional signer_name; I:273–274, 836. | X/D: written last. |
| Buyer `nev`, `orszag`, `irsz`, `telepules`, `cim` | Name/ZIP/city/address strings required; country optional. I:296–306, 840–844. | X/D: four mandatory tags, country optional, all in sequence. Empty strings remain caller/server content, not local omissions. |
| Buyer `email`, `sendEmail` | Optional email; tri-state send_email. I:307–315, 845–848. | E: email present + omitted/true sends, false suppresses; comma recipients supported through string. Current doc correct as the normal-account rule. Test delivery is redirected (§5). |
| Buyer `adoalany`, `adoszam`, `csoportazonosito`, `adoszamEU` | Optional taxpayer_status/tax_number/group_id/eu_tax_number. I:316–324, 849–854. | All present in X, correct order. Group field absence from D is vendor drift, not reason to remove it. Tax-number consistency/format remains server validation. |
| Postal `postazasiNev`, `postazasiOrszag`, `postazasiIrsz`, `postazasiTelepules`, `postazasiCim` | `PostalAddress` with five optional strings; I:277–291, 855–861. | X/D: flattened inside vevo, not an extra invented container. Partial blocks preserved. |
| `vevoFokonyv/konyvelesDatum`, `vevoAzonosito`, `vevoFokonyviSzam`, `folyamatosTelj`, `elszDatumTol`, `elszDatumIg` | BuyerLedger: optional date/string/string/bool/date/date; I:120–135, 862–873. | X/D: all six mapped and ordered; explicit false retained; empty-present ledger supported. |
| Buyer `azonosito`, `alairoNeve`, `telefonszam`, `megjegyzes` | Optional id/signer_name/phone/comment; I:329–346, 874–877. | X: partner identifier, signer, contact, comment. Current id rustdoc correctly states partner update and customer-portal exposure consequences; not confused with queried numeric id. |

### 3.5 Items and item ledger

| XML field(s) | Rust / writer | Assessment |
|---|---|---|
| `megnevezes`, `azonosito` | Required name, optional id; `item.rs:91–94`, I:885–886. | X/D: name first, identifier optional. |
| `mennyiseg`, `mennyisegiEgyseg`, `nettoEgysegar` | Decimal quantity, string unit, Decimal net price; `item.rs:95–100`, I:887–889. | X/Q/DI: fractional quantities/prices and negative discount prices representable. |
| `afakulcs` | VatRate, `item.rs:101–102`, I:890. | V; exact special token or normalized numeric percentage. |
| `arresAfaAlap` | Optional Decimal margin_vat_base; `item.rs:103–104`, I:891–893. | X/D/P: before net, not dropped. Calculation from margin is caller-owned. |
| `nettoErtek`, `afaErtek`, `bruttoErtek` | Three explicit Decimals; `item.rs:105–110`, I:894–896. | X/Q: all sent, even zero/negative. `new` preserves supplied values; `try_calculated` has INV-01. |
| `megjegyzes` | Optional string; `item.rs:111–112`, I:897. | X/DI/V: row comment preserved; layout may limit display, distinct from XML transport. |
| `tetelFokonyv/gazdasagiEsem`, `gazdasagiEsemAfa`, `arbevetelFokonyviSzam`, `afaFokonyviSzam`, `elszDatumTol`, `elszDatumIg` | Six optional fields on LineItemLedger; `item.rs:52–72`, I:898–913. | X/D: all six emitted in correct order. Invoice fields fully supported. |
| `torloKod` | `erasure_code_count: Option<u32>`; `item.rs:115–131`, I:914–916; I:703–710 range check. | X/ER: **count**, 0–400; last in row. Feature/account/layout eligibility is documented and remains vendor-side. |
| `tetelek/tetel` | Vec, at least one through to_wire; I:682–685, 882–919. | X: unbounded repeat with minimum one. Order retained, including a discount immediately after its target. |

### 3.6 Complete waybill coverage

All fields below default to absent, except MPL's three required strings. Multiple sub-blocks are representable because the XSD uses a sequence, not a choice; carrier-specific interpretation is the server's. No undocumented carrier block was invented.

| XML paths | Rust and writer | Assessment against X/D |
|---|---|---|
| `fuvarlevel/uticel`, `futarSzolgalat`, `vonalkod`, `megjegyzes` | W:94–104, 133–136. | Complete. Legacy unused destination correctly documented; general barcode fallback correctly documented. |
| `tof/azonosito`, `shipmentID`, `csomagszam`, `countryCode`, `zip`, `service` | TransOFlex, W:11–24, 137–148. | Complete, correct case/order. Five-digit id preserved as string; parcel_count unsigned with `i32::MAX` wire gate in I:711–718. |
| `ppp/vonalkodPrefix`, `vonalkodPostfix` | PickPackPoint, W:28–33, 149–154. | Complete. Vendor 3-character prefix / maximum 7-character suffix not locally enforced. |
| `sprinter/azonosito`, `feladokod`, `iranykod`, `csomagszam`, `vonalkodPostfix`, `szallitasiIdo` | Sprinter, W:37–50, 155–166. | Complete. Vendor id 3 chars, sender 10 digits, suffix 7–13 chars are caller/carrier constraints; no omission caused by model. |
| `mpl/vevokod`, `vonalkod`, `tomeg`, `kulonszolgaltatasok`, `erteknyilvanitas` | Mpl, W:57–84, 167–177. | Complete. First three always emitted; weight is correctly a string per XSD, not forced into a numeric unit. Optional services string and Decimal declared value retained. |
| Carrier tokens | `Waybill.carrier: Option<String>`, W:98–99. | TOF, PPP, SPRINTER, FOXPOST, MPL, GLS, EMPTY all representable. FOXPOST/GLS/EMPTY need no absent bespoke Rust struct to send the documented general fields. |

### 3.7 Domain tokens, arithmetic policies and attachments

| Surface | Coverage / current result |
|---|---|
| Invoice VAT special codes | `types.rs:186–248, 266–324` covers **TAHK, TAM, AAM, EUT, EUKT, F.AFA, K.AFA, HO, EUE, EUFADE, EUFAD37, ATK, NAM, EAM, KBAUK, KBAET** with the correct tokens. V's detailed PDF supports new-means-of-transport meaning of KBAUK and exact K.AFA subtype phrases/default. TAHK is not TAM. |
| Numeric VAT | Every listed value representable: `0, 1, 2, 2.1, 3, 4, 4.8, 5, 5.5, 6, 7, 7.7, 8, 8.1, 9, 9.5, 10, 11, 12, 13, 13.5, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 25.5, 26, 27`. Unsupported numbers are not locally banned. Other numeric text is interpreted by the derived constructor while retaining its wire token; malformed/novel special codes remain server decisions. |
| Shared receipt-only VAT tokens | ÁKK, EU, EUK, MAA are marked as receipt vocabulary at `types.rs:238–246`. Their representability on an invoice does not guarantee invoice acceptance; no separate invoice allowlist is required. Other(String) deliberately keeps the open set. |
| Currency | `types.rs:369–437`: all C codes, including unusual vendor `KSH` and historical codes, can be sent unchanged; HUF/Ft recognized case-insensitively for local policy. Constants cover common currencies, not an exhaustive allowlist. No support gap from omitted named constants. |
| Language | `types.rs:480–585`: all 15 **hu, en, de, it, ro, sk, hr, fr, es, cz, pl, bg, nl, ru, si** match X/T. Vendor's `cz`/`si` should not be “corrected” to ISO `cs`/`sl`. Closed set matches XSD enumeration. |
| Payment method | `types.rs:588–687`: átutalás, készpénz, bankkártya, csekk, utánvét, PayPal, SZÉP kártya match P. PHP also names OTP Simple; `Other("OTP Simple")` already supports it, so missing enum sugar is not missing capability. Free text stays possible. |
| Taxpayer status | `types.rs:690–755`: 7 non-EU business, 6 EU business, 1 Hungarian tax number, 0 unknown, -1 no tax number match X. The parenthetical “private individual” at 703 is shorthand rather than a complete classification rule; no caller-side NAV classification logic is implemented. |
| Invoice templates | `types.rs:983–1020`: six tokens plus Other(String); correct wire case. `Default` means explicit SzlaAlap, not omission, as its doc now says. `Continuous` sends SzlaTomb (retro); no wire defect. Layout labels remain unresolved, below. |
| InvoiceNumber | `types.rs:22–57`: string wrapper preserves invoice/proforma/corrective references. It does not claim to validate vendor numbers. |
| ExchangeRate | `types.rs:943–980`: supplied bank/rate or `automatic_mnb()` bank MNB/rate absent; invoice-specific omission supported in fresh X/D/P. |
| Derived rounding | `item.rs:9–49, 163–224`: explicit Scale or Exact; ordinary midpoint strategy is half away from zero, net first, then VAT, then sum. HUF/Ft 0 digits, EUR 2, KWD 3, etc. are explicitly local choices (`types.rs:412–435`), not universal storage guarantees. INV-01 is the exception to the claimed arithmetic invariants. |
| Explicit amounts / gross-first | `LineItem::new`, `item.rs:133–161`, supports Q's gross-first B2C example (`393.66 × 3`, net 1181, VAT 319, gross 1500) without recalculating it. There is no dedicated gross-price-derived constructor; capability remains available through explicit amounts. |
| Attachments | I:379–516, 569–571, 938–948: all five numbered parts, arbitrary filename/bytes/MIME; max five and decimal 2,000,000 bytes per file, including deserialization. Limits match R/E, and the MB interpretation is expressly qualified. Empty collection defaults. No invented attachment XML nodes. |

## 4. XML fidelity and offline checks

### Static and executed checks

- `xml.rs:19–40`: UTF-8 XML 1.0 declaration, `xmlszamla` with exact namespace `http://www.szamlazz.hu/xmlszamla`. Descendants inherit that namespace. `xmlns:xsi` and `xsi:schemaLocation` from the example are omitted; those are schema-location hints, not required business elements or a namespace mismatch.
- `xml.rs:400–465`: names are fixed by the writer; all business text goes through `BytesText::new`, dates through Date formatting, decimals through locale-independent plain decimal formatting, booleans through `true`/`false`. Optional string `None` omits the tag; `Some("")` emits a present empty tag. Empty optional ledger/carrier structs remain present containers when supplied.
- An executed invoice probe supplied `A&B <tag> "quoted" 'apostrophe' ]]> é\r\nnext` in multiple business fields. Independent XML parsing recovered the exact original text, including CRLF. No markup injection or double escaping observed. `to_wire` rejected an embedded U+0000 for every kind via the existing XML 1.0 gate (`wire.rs:402–429`). Direct `write_xml` is an unchecked lower-level serialization entry point; the complete request path is `to_wire`.
- The scratch Rust executable generated **all six kinds** with every applicable optional header, seller, buyer, postal, buyer-ledger, item-ledger and all four carrier blocks populated. Synthetic records used deliberately rich combinations to check shape, not to claim business eligibility.
- A separate Python stdlib checker extracted each fresh XSD's sequences, minimum/maximum cardinalities and complex-type graph, then compared every generated element's namespace, name and position. **Six common-field user/password instances passed each of D/X-EN/X-HU/L unchanged.**
- Full key-authenticated instances met the known conflicts: D refused `csoportazonosito`; X-EN/X-HU refused preview-before-simple order; L did not recognize `simpleItems`. Removing only the conflict-bearing fields **from synthetic instances**, independently for each source, produced 6/6 structural passes per source. Vendor schemas were never patched for this experiment. All remaining optional fields were still exercised.
- This was **sequence/cardinality/namespace verification, not a full XSD validator**. `xmllint` and Python lxml were unavailable; lexical types were inspected against the Rust types/writers instead. A real schema validator would still not decide the server's combined-preview behaviour.
- Targeted arithmetic repros executed successfully and reproduced INV-01. The first PDF text-extraction attempt lacked `pdftotext`; the downloaded PDF was then read using the file tool, including its table and image.
- Inspected existing request tests at I:1000–1328 and validation tests thereafter, `tests/simple_items.rs:20–112`, and the request-token cases in `tests/numeric_fidelity.rs:15–46`. The complete crate suite was not rerun for this review; only the isolated scratch executable and structural checker were executed. No server or live tests ran.

Scratch commands:

```text
cargo run --quiet --manifest-path /tmp/opencode/invoice-audit-382cf761-current/Cargo.toml
python3 /tmp/opencode/invoice-audit-382cf761-current/audit.py
```

## 5. Intentional deviations, conflicts and non-findings

### Unresolved vendor-source conflicts

1. **Preview and simplified-image sequence remains unsettled.**
   - [EN/HU current inline XSDs](https://docs.szamlazz.hu/agent/generating_invoice/xml): `szamlaSablon → simpleItems → elonezetpdf`.
   - [Download](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) and [fresh PHP 2.12.4](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), `Header/InvoiceHeader.php:398–404`: template → preview → simple.
   - [Legacy `/xsd`](https://docs.szamlazz.hu/agent/generating_invoice/xsd) has template → preview and **no simpleItems**.
   - Current writer I:819–825 intentionally follows download/PHP; I:220–222 tells callers about the conflict. The official quote “the order of the fields is fixed, they cannot be interchanged” makes this a genuine interoperability question, but does not identify which published sequence the deployed server enforces. **Not a newly confirmed Rust defect.** Do not silently switch order or merge schemas as a supposed resolution. Existing vendor-question draft §1 asks exactly the right order/false/preview-no-issue questions.

2. **Layout labels conflict.**
   - [API template table](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template): `SzlaAlap` = “Tradicionális számlakép”, `SzlaNoEnv` = “Borítékbarát számlakép”. Fresh PHP agrees.
   - [Linked knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok): “Tradicionális: … SzlaNoEnv”; “Borítékbarát: … SzlaAlap”. It also calls SzlaMost the default.
   - `types.rs:991–995` follows the API's explicit SzlaAlap/traditional association; omission is separate. **No token swap recommended without clarification.** Existing vendor-question draft §2 remains relevant. Lowercase `<szamlasablon>` and lowercase delivery-layout value in the knowledge-base snippet are not grounds to change the XSD's case-sensitive writer.

3. **Downloaded schema missing newer fields.** D lacks buyer `csoportazonosito` and item `torloKod`; current X, L, P and ER support them. Preserve both. `fixtures/SOURCES.md:126–140` correctly labels the cached invoice schema as project-modified. A download-only validation failure is not a writer regression.

4. **Example prose contradicts schemas.** X says “All fields shown in the example are mandatory”, while its own `minOccurs="0"` elements and instructions permit omission. The page also says those `minOccurs="0"` elements “may be omitted”. Current omission follows the explicit schema and PHP behaviour; do not force empty optional dates, numbers or flags merely to imitate the example.

5. **General foreign-rate requirement vs MNB exception.** C asks for both bank and rate, while X/D explicitly state automatic current MNB rate when bank is MNB and rate absent. Current invoice support is justified by the specific annotation. The conservative requirement for a rate specification on every non-HUF kind is a known local policy in the behaviour notes; no live claim that a foreign proforma without it would fail.

6. **HUF rounding / English strength of wording.** EN Q says B2B/B2C “have to” use the respective net/gross approaches; HU says “valószínűleg” (probably). Both publish server normalization for fractional HUF totals. The crate permits explicit amounts and calls its minor-unit rounding a local policy. No blanket claim that all fractional HUF input must be rejected, or that all currencies store ISO minor units, is justified.

### Supported live deviations respected

- Request issue date may be replaced with today; current header rustdoc already states the bounded test-account evidence (behaviour notes P48-P5).
- A proforma reference may be ignored when the referenced proforma is consumed/deleted; implicit order-number linking also occurs. The model provides the reference and correctly qualifies unprobed explicit prepayment/final use. A Rust field cannot enforce the vendor's relationship semantics.
- A final invoice does not automatically deduct the prepayment; current rustdoc instructs callers to supply the negative line.
- External ids are non-unique, attach only on actual creation and can be hidden by newer holders. No request-model uniqueness check is possible from a string, and none is promised.
- Observed net arithmetic tolerance and independent EUR server rounding are not errors in `LineItem::new`; explicit monetary fields must remain possible. Numeric VAT spellings `27.00`/`27.0` were accepted; normalization to `27` is hygiene.
- Buyer data updates belong to partner identification semantics; current buyer-id documentation captures the official risk. An update of partner data is not evidence that the serializer supplied the wrong buyer.

### Intentionally unsupported or caller-owned capabilities — not defects

- Arbitrary combinations of kind flags/reference elements; a dedicated response-version-1 mode; explicit false `fizetve`; a dedicated gross-price-derived constructor; automatic final deduction or automatic discount computation. The practical gross-first, discount and final-deduction operations are representable using explicit line values.
- Account configuration, prefix creation, certificate selection, OSS state, seller identity replacement, custom logo upload, payment-method configuration, legal choice of VAT category, carrier code generation and portal permissions. These are not missing XML fields in the reviewed schema.
- Semantic checks such as maximum simpleItems rows, allowed VATs, inherited final setting, simplified-original correction ban, country/taxpayer consistency, reference existence, erasure feature/layout eligibility, carrier id lengths, positive exchange-rate business requirements and date chronology. Current types write supplied values; szamlazz.hu answers the content rules. SI's restrictions are documented at I:199–218; no local rejection is required merely to mirror PHP.
- Future Currency/VatRate/PaymentMethod/InvoiceTemplate tokens are intentionally pass-through. Unknown language/status values are refused because those request sets are presently finite/documented; no current required value is missing.
- Decimal excludes non-finite IEEE `double` values and is narrower than the XSD number space. There is no meaningful invoicing requirement to emit NaN/INF. The actionable problem is returning silently rounded `Ok` arithmetic inside the chosen Decimal domain, not refusing unsupported numeric input.

### Semantic rustdoc opportunities / bounded uncertainties

These are not included in the confirmed-defect count:

- **Aggregator description:** I:548 says “for contracted integrations”; fresh official `SzamlaAgentSetting.php:74–81` describes the web-shop engine name, with examples WooCommerce/OpenCart/PrestaShop. The wire works, but “integration/platform identifier; consult vendor if unsure” would avoid implying a contract prerequisite not established by the fetched public sources. Guardian's contract-only meaning is likewise not explained by the schema or PHP's bare bool; leave its operational semantics qualified rather than inventing them.
- **Line-item shorthand:** `item.rs:105–108` says net/VAT “must equal” their unrounded formulas, although its own derived constructor and Q use rounding. Qualifying “subject to the selected rounding/server checks” would prevent literal readers from treating the per-field docs as an exact unrounded invariant. This is distinct from INV-01's actual unrequested precision loss.
- **Taxpayer `-1`:** the XSD says “no tax number”, whereas `types.rs:703` adds “private individual”. The variant name and token are right, but the parenthetical should not be treated as an exhaustive rule for organizations without a tax number; the VAT PDF explicitly discusses such organizations. No runtime misclassification code was found.
- **Test email delivery:** E now expressly says test-account notification goes to the account-configured email, not the XML buyer address. `Buyer.email` describes normal recipient selection without this exception. Existing behaviour-note D6 uncertainty about where test emails go should not be promoted into a universal no-send claim. Adding a concise test-account caveat would improve the request docs.
- **Legacy/default bool claims:** `ops.rs:18–21` asserts absent flags are “exactly false”. PHP mirrors true-only emission of paid/kind flags, but this does not prove that every account/payment-method-derived paid status is false when omitted. Current invoice field docs accurately describe emission; the broader server-default statement could be narrowed to emission policy.
- **Dates outside normal invoicing years:** Jiff's domain is broader than present-day business dates. This review did not claim vendor acceptance of astronomical/zero/negative years, or demand local validation of them. Normal date formatting is correct; unusual years remain a lexical/business boundary for broader validation work.
- **Source semantics not fully specified:** guardian, item-id account interactions, logo lookup failures, payable-adjustment display details, exact carrier rendering and barcode selection with partial carrier data lack enough public evidence for stronger correctness assertions. The request exposes the fields in their documented types and order. No live conclusion was invented.

## 6. Recommended disposition

1. Fix **INV-01** with loss-aware arithmetic and targeted boundary tests.
2. Keep the existing preview/simpleItems and layout vendor questions open; current source contradictions remain reproducible. No unsupported source-precedence rule should decide them.
3. Consider the small semantic-doc clarifications above separately from functional defects. No ordinary invoice request field addition or wholesale writer rewrite is justified by this review.
