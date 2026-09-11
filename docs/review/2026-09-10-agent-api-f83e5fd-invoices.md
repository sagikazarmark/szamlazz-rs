# Independent invoice review against current Számla Agent documentation

**Source revision:** `f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9`

**Reviewed and official sources fetched:** 2026-09-10

**Conclusion:** **No confirmed actionable functional defect found in the scoped implementation.** Prior silent arithmetic loss and numbered-56 identity loss are closed at this revision. The current official sources still disagree about combined preview/simplified-image order and layout labels. These are unresolved interoperability questions, not evidence justifying a change to the chosen writer.

## 1. Boundary and evidence standard

Reviewed the complete invoice creation request and response, all six invoice kinds, settings and options, shared item arithmetic and wire types, waybill/carrier data and email attachments. This is a snapshot review against current official documentation, not an empty diff review against HEAD itself. Source inspection covered `invoice.rs`, `waybill.rs`, `item.rs`, `types.rs`, and the necessary `number.rs`, `envelope.rs`, `wire.rs`, `xml.rs` helpers and tests. Storno-specific requests, receipt operations, full queried-invoice models and HTTP transport lifecycle are outside the substantive finding boundary.

Paths abbreviated below are relative to `crates/szamlazz-agent/src/`: **I** = `ops/invoice.rs`, **W** = `ops/waybill.rs`, **A** = `item.rs`, **T** = `types.rs`, **E** = `ops/envelope.rs`, **N** = `number.rs`, **X** = `xml.rs`. All line numbers refer to the pinned revision, verified against the working tree.

The untracked `2026-09-10-agent-api-current-invoices.md` and `…-current-adjudication.md` were read as leads at their older `fbda137…` revision. Their verdicts, source hashes and execution results were not inherited as current evidence. Official sources were fetched anew, code paths inspected, and scratch probes authored independently. No subagents were available or used.

“Covered” below means the field, order, default, representation and documented restrictions were compared; it does not mean every field combination was accepted by a live vendor account. A missing local duplicate of a server business rule is not automatically a client defect. A source conflict, an XSD-valid hypothetical reply, malformed XML and a live account observation are different kinds of evidence.

Only this report was added in the repository. Source, tests, fixtures and existing reports were preserved. Scratch source was authored with `apply_patch` under `/tmp/opencode/invoices-f83e5fd-independent/`. No credentials or live vendor account operations were used. External requests were unauthenticated documentation/download GETs.

## 2. Fresh source provenance

The [Generating invoice index](https://docs.szamlazz.hu/agent/category/generating-invoice) and [settings index](https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules) were used to enumerate the surface. All EN/HU pairs below were fetched in this review. Pages report site build **`v202608271632`**; this is not a date for every individual rule.

| ID | Official URLs fetched | Evidence used |
|---|---|---|
| R | [EN request](https://docs.szamlazz.hu/agent/generating_invoice/request), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/request) | POST target, multipart field and five attachment slots; linked request example/schema. |
| S | [EN XML/XSD](https://docs.szamlazz.hu/agent/generating_invoice/xml), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | Both full inline request examples and schemas, all complex types, child sequences, cardinalities, scalar types, annotations and language enumeration. |
| D | [Request download XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) | Fresh unmodified download, independently compared with S; URL is in the current example's `xsi:schemaLocation`. |
| P | [EN response](https://docs.szamlazz.hu/agent/generating_invoice/response), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/response), [response download XSD](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamlavalasz.xsd) | Response versions, HTTP headers, both current success/error examples, nine response fields and their optionality. Download URL also checked from the repository's provenance lead. |
| K | [EN kinds](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/document-types) | Kind flags/references, delivery layout, one-prepayment/one-final restriction, e-invoice selection. |
| SI | [EN simpleItems](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/travel-agency) | Per-document selection, exact case, inheritance, forbidden kinds, account/VAT/item limits, template override and full NAV data. |
| V | [EN VAT](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates) | Numeric and special VAT tokens; `eusAfa` meaning and seller prerequisites. |
| Q | [EN rounding](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/rounding) | Net-first and gross-first examples, HUF normalization table, foreign fractional amounts. |
| C | [EN currencies](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/currencies) | Entire currency list, HUF/Ft alias, bank/rate requirements. |
| L | [EN layouts/languages](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/invoice-template) | Six template tokens, omission default, 15 language tokens; language also affects email/portal. |
| O | [EN order numbers](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/order-number) | Optional query handle, account toggle, per-type duplicate checking, exemptions, conditional replay within two days. |
| DI | [EN discount](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/discount) | Negative-price positive-quantity line, same VAT, adjacency; no percentage/total discount field. |
| M | [EN email](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/email-notification) | Send default, comma recipients, BBCode/newlines, five 2-MB attachments, partial attachment failure and test-account redirection. |
| ER | [EN erasure](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code), [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/data-erasure-code) | Nonnegative integer, maximum 400 per item, account enablement. |
| ERR | [EN errors](https://docs.szamlazz.hu/agent/basics/error-handling), [HU](https://docs.szamlazz.hu/hu/agent/basics/error-handling) | Relevant creation codes, 537–539 and 551–556, no automatic retry loop, five-request limit. |

Additional first-party sources fetched:

- [Authentication](https://docs.szamlazz.hu/agent/basics/authentication): “either an Agent key (recommended) or a username and password”; key in both legacy fields also supported. Credential spelling is preserved by X:484–493.
- [PHP download page](https://docs.szamlazz.hu/php/) and its [official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip), downloaded afresh and inspected in memory, never executed. `Header/InvoiceHeader.php:47–54` provides payment tokens; `398–404` gives template → preview → simple order; `SzamlaAgentSetting.php:116,340` describes aggregator and settings sequence.
- [PHP response handling](https://docs.szamlazz.hu/php/valasz-feldolgozas): “an invoice is successfully issued” although notification delivery fails. Fresh ZIP `Response/InvoiceResponse.php:17,319–322` identifies code **56** and conditions success on a document number. The current general error table omits 56; the PHP source is important provenance for this special case.
- [Erasure knowledge base](https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor): requested **count** at the end of `<tetel>`, maximum 400, `SzlaMost`, uploaded stock or vendor-supplied codes.
- [Layout knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-szamlakepek-kozul-valaszthatok): contradictory layout labels, recommended default, carrier layout and UI-only postal service. Linked rendered PDF previews were inventoried but not downloaded/rendered; no rendering conclusion rests on their filenames.
- [Email dynamic fields](https://tudastar.szamlazz.hu/gyik/szamlaertesito-egyedi-mezok): dynamic bracket tokens and BBCode are ordinary string content, not additional XML fields.
- [VAT knowledge base](https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere) and linked [2025-11-04 VAT PDF](https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf). Download redirects to `https://www.szamlazz.hu/dokumentumok/afakulcsok-nosz-ufi-segedlet-2025-11-04.pdf`. Its single page was read via the PDF reader using an existing scratch copy whose SHA-256 was independently checked equal to this review's fresh download. KBAUK means new means of transport, not the UK country; K.AFA subtype selection and default, TAHK/ATK versus TAM, and EU-tax-number restrictions were checked.

### Acquisition identifiers

Hashes were calculated anew. Inline hashes identify HTML-decoded `<pre>` schema text, without reindentation or an added newline. Request-page HTML bytes changed between repeated GETs while extracted schemas remained identical; HTML hashes are not treated as stable schema identities.

| Artifact | SHA-256 |
|---|---|
| Download request XSD | `90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498` |
| EN inline request XSD | `06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4` |
| HU inline request XSD | `09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a` |
| EN inline response XSD | `fac89cd0733c3ea6cef0fb0800e041f73e2abbec0ed40b0499a1b63a15c2b9c2` |
| HU inline response XSD | `3d88ee85e90eeb19237dcc3c66ac8637fa009d3de8d02b53aff2621765c77e6f` |
| PHP ZIP | `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741` |
| VAT PDF | `bb5a52eda87e383be3276870fee937c6a34d87f7a2a3542a03a0bf90a63d9465` |

## 3. Confirmed actionable defects

**None demonstrated in this scope at this revision.** No severity-ranked functional finding is warranted by the verified evidence. This conclusion does not certify every input or vendor execution. The source conflicts and evidence gaps below are material and remain explicitly open.

## 4. Comprehensive request inventory

S has **118 child declarations in named complex types**, plus the six root-block declarations. D has 116, omitting buyer group and erasure count. This count includes containers, credential alternatives and kind-specific slots; no individual request should emit every mutually exclusive flag.

Lists below preserve schema sequence. `?` means optional in the schema; optional Rust values generally default to `None` and are omitted, while `Some("")` is written as an empty element and optional booleans retain explicit false. Every named field below was checked against the writer, not inferred from fixture success alone.

### Root and settings

| Wire field(s), in order | Model, default and current code | Assessment |
|---|---|---|
| `xmlszamla` | I:738; X:19–40: XML 1.0, UTF-8, namespace `http://www.szamlazz.hu/xmlszamla` | Matches S/D. `xsi:schemaLocation` is an optional schema hint, not missing business data. |
| `beallitasok → fejlec → elado → vevo → fuvarlevel? → tetelek` | I:739–920 | Exact root order. Empty seller container is emitted; waybill omitted by default; item order preserved. |
| `felhasznalo? → jelszo? → szamlaagentkulcs?` | I:740, X:484–493; Credentials chooses key or username/password | Correct authentication alternatives; no credentials fabricated. |
| `eszamla → szamlaLetoltes` | I:532–542,604–605,741–742; both default false and are always sent | Valid local paper/no-download defaults; sample true is not a required default. |
| `szamlaLetoltesPld?` | I:543–547,743–745; optional u8 | Inline annotation says deprecated/ignored. Narrower than XSD int, with no demonstrated useful capability loss. |
| `valaszVerzio?` | I:746 and `ops.rs:27–31`: always `2` | Deliberate structured-response selection; no version-1 parser needed for this request. |
| `aggregator? → guardian? → cikkazoninvoice? → szamlaKulsoAzon?` | I:548–556,747–754; string/bool/bool/string | All slots and explicit false preserved. Aggregator/guardian prose caveat in §7. External id is a query handle, not server uniqueness. |

### Header and kinds

| Wire field(s), in order | Model / current code | Assessment |
|---|---|---|
| `keltDatum?` | `issue_date`, I:141–150,758 | Defaults absent; documentation correctly qualifies replacement by today observed on a TEST account. |
| `teljesitesDatum → fizetesiHataridoDatum` | Required civil dates, I:151–156,759–760 | Fulfillment and due date, not the EN example's mistranslated “payment date”. |
| `fizmod → penznem → szamlaNyelve` | Required PaymentMethod/Currency/Language, I:157–162,761–763 | Full current token coverage in §5. |
| `megjegyzes? → arfolyamBank? → arfolyam? → rendelesSzam?` | Comment, ExchangeRate, order number; I:163–169,764–771 | Automatic MNB exception represented; general foreign-currency gate below. Free-text comments support K.AFA descriptions. |
| `dijbekeroSzamlaszam?` | I:104–116,774–777 | Optional proforma reference on regular, prepayment and final invoices; precedes every kind flag. |
| `elolegszamla? → vegszamla? → elolegSzamlaszam? → helyesbitoszamla? → helyesbitettSzamlaszam? → dijbekero? → szallitolevel?` | I:34–85,778–796 | One selected kind, associated reference slots; see table below. |
| `logoExtra? → szamlaszamElotag? → fizetendoKorrekcio? → fizetve?` | I:170–180,797–804 | Logo and registered prefix strings; Decimal adjustment; paid defaults false and only true is emitted. No automatic total recomputation. |
| `arresAfa? → eusAfa?` | I:181–192,805–810 | Optional booleans. `eusAfa` suppresses NAV submission when accepted; OSS/non-Hungarian seller condition and line-VAT requirement are documented accurately. |
| `szamlaSablon?` | I:193–196,811–818 | Optional named/open template; DeliveryNote explicitly overrides it with `SzlaFuvarlevelesAlap`. |
| `simpleItems? → elonezetpdf?` in S; reverse order in D | I:197–225,819–826 | Both options represented, default absent, true and false preserved. Writer deliberately follows D/PHP; unresolved conflict in §7. |

| Kind | Emission / restriction | Assessment |
|---|---|---|
| Regular invoice | No true kind flag; optional proforma number | K/S covered. |
| Proforma | `dijbekero=true` | Covered; payment request, not invoice. |
| Delivery note | `szallitolevel=true` plus forced delivery layout | K describes layout, S/D supplies independent flag. Both correctly sent. Waybill is optional and also usable on actual invoices with compatible layout. |
| Prepayment | `elolegszamla=true`; optional proforma number | Covered; actual explicit proforma-reference acceptance remains unverified in account notes. |
| Final | `vegszamla=true`; optional `elolegSzamlaszam`, proforma number | I:686–701 requires nonblank prepayment number or order number. Single prepayment slot matches K's one-to-one rule. Caller supplies negative deduction lines, I:62–70. |
| Corrective | `helyesbitoszamla=true` and `helyesbitettSzamlaszam` | Reference string required by construction, not guaranteed nonblank/existing. Validity remains vendor-owned. |

Arbitrary combinations of XSD kind booleans and proforma references on corrective/proforma/delivery-note values are intentionally not exposed. Independent optional schema slots alone do not establish a meaningful missing operation. Storno is a distinct operation, not a missing invoice flag.

**Defaults and gates:** I:228–259 and596–618 initialize all optional settings absent, paid/e-invoice/download false, empty seller and attachments. I:682–732 checks one or more items; final identity; erasure 0–400; parcel count ≤ `i32::MAX`; and foreign currency bank/rate. For non-HUF/Ft, missing exchange settings fail, bank must be nonblank/already trimmed, and an absent rate requires exact `MNB`. No account lookup, date chronology check, legal-tax classifier or arbitrary string validator is promised. Lowercase HUF/Ft comparisons are local conveniences, not newly established vendor spelling acceptance.

### Seller, buyer and ledgers

| Wire fields, in order | Model / current code | Assessment |
|---|---|---|
| Seller `bank? → bankszamlaszam? → emailReplyto? → emailTargy? → emailSzoveg? → alairoNeve?` | Seller + flattened SellerEmail, I:262–275,828–837; T:1022–1032 | Six fields complete; omission uses account settings. No seller name/tax-number override is documented in this request schema. |
| Buyer `nev → orszag? → irsz → telepules → cim` | I:296–306,840–844 | Four required strings and optional country; caller supplies content. |
| `email? → sendEmail?` | I:307–315,845–848 | Nonempty email with omitted/true flag sends; false suppresses. Comma-separated recipients supported. |
| `adoalany? → adoszam? → csoportazonosito? → adoszamEU?` | I:316–324,849–854 | Taxpayer enum and three strings; group field is supported by both inline schemas despite D omission. |
| `postazasiNev? → postazasiOrszag? → postazasiIrsz? → postazasiTelepules? → postazasiCim?` | PostalAddress flattened, I:277–291,855–861 | All five fields; no invented wrapper; partial addresses representable. |
| `vevoFokonyv?`: `konyvelesDatum? → vevoAzonosito? → vevoFokonyviSzam? → folyamatosTelj? → elszDatumTol? → elszDatumIg?` | I:120–135,862–873 | Date/string/string/bool/date/date, complete; explicit false and present-empty container supported. |
| Buyer tail `azonosito? → alairoNeve? → telefonszam? → megjegyzes?` | I:329–347,874–877 | Correct sequence. Partner identifier warning correctly explains master-data update and access to account documents; distinct from queried internal numeric id. |

BBCode (`b/i/u/h1…h6/center`, linked knowledge base's `a`) and dynamic `[számlaszám]`, `[összeg]`, `[paylink=…]`, etc. fit ordinary subject/body strings. The crate need not interpolate them itself. Postal service ordering is UI-only according to the linked vendor page; postal-address support is not a missing mailing-operation feature.

### Items and arithmetic-related fields

| Wire fields, in order | Model / current code | Assessment |
|---|---|---|
| `megnevezes → azonosito? → mennyiseg → mennyisegiEgyseg → nettoEgysegar → afakulcs` | A:91–102; I:885–890 | Name/id, exact Decimal quantity/price, unit string, open VAT token. Signs and fractional quantities/prices preserved. |
| `arresAfaAlap? → nettoErtek → afaErtek → bruttoErtek` | A:103–110; I:891–896 | Margin base before the three explicit values, no silent omission or implicit vendor calculation. |
| `megjegyzes? → tetelFokonyv?` | A:111–114; I:897–913 | Text retained; optional ledger emitted. Layout truncation of printed comments is not transport loss. |
| Ledger `gazdasagiEsem? → gazdasagiEsemAfa? → arbevetelFokonyviSzam? → afaFokonyviSzam? → elszDatumTol? → elszDatumIg?` | A:52–72; I:898–913 | Four strings and two dates, all covered. Shared receipt limitation is documented separately. |
| `torloKod?` | A:115–131; I:914–916,703–710 | Last in row, count not identifier, 0–400. `SzlaMost` and account/test restrictions are documented, not silently enforced with guessed account state. |
| `tetelek/tetel` repeated ≥1 | I:682–685,882–919 | Nonempty gate, no upper count imposed except server's simpleItems rules. Discount adjacency preserved. |

### Waybill

| Wire fields, in order | Current code | Assessment |
|---|---|---|
| `uticel? → futarSzolgalat? → vonalkod? → megjegyzes? → tof? → ppp? → sprinter? → mpl?` | W:94–112,133–177 | Complete; unused legacy destination and fallback barcode behavior documented. |
| TOF `azonosito? → shipmentID? → csomagszam? → countryCode? → zip? → service?` | W:11–24,137–148 | Correct case, strings preserve leading zeroes; u32 parcel count checked for XSD int range. Five-digit id restriction left to caller/vendor. |
| PPP `vonalkodPrefix? → vonalkodPostfix?` | W:28–33,149–154 | Three-character prefix/max-seven suffix representable; no undocumented transformation. |
| Sprinter `azonosito? → feladokod? → iranykod? → csomagszam? → vonalkodPostfix? → szallitasiIdo?` | W:37–50,155–165 | Three-character id, ten-digit sender, routing string, count, 7–13 suffix and delivery text all supported. |
| MPL `vevokod → vonalkod → tomeg → kulonszolgaltatasok? → erteknyilvanitas?` | W:57–84,167–176 | Three required strings, including string weight; optional icons and Decimal value. No Default that omits mandatory fields. |

Carrier string supports all annotated tokens: `TOF PPP SPRINTER FOXPOST MPL GLS EMPTY`. No dedicated FOXPOST/GLS child type is declared by the schemas. Carrier subblocks are a sequence of optionals, not an exclusive choice; multiple subblocks are schema-valid. Negative parcel counts permitted by bare XSD int are excluded by the local unsigned count model; no meaningful lost capability was shown. Actual barcode and layout rendering remain untested.

### Attachments and wire fidelity

- I:379–516 bounds attachments at five and **2,000,000 bytes each**, covering push, Vec conversion and Deserialize. The decimal-MB interpretation is explicitly qualified; there is no mutable slice permitting content growth behind the bound.
- I:938–948 supplies `attachfile1`…`attachfile5`, filename, MIME and unmodified raw file bytes. `wire.rs:66–125` builds the multipart body and avoids content-boundary collision. Existing exact-body test I:1249–1280 and scratch five-file/binary/size-boundary controls passed.
- M: “If an attachment is invalid … still sends the notification e-mail with the valid attachments.” Local preflight refusal of oversized files is a deliberate conservative boundary, not a claim the vendor rolls back an invoice for attachment errors. Files sent with `sendEmail=false` are ignored server-side; representing them is not a defect.
- X:442–480 writes escaped text, locale-independent plain Decimal notation, civil dates, and true/false. `wire.rs:402–429` rejects XML 1.0 forbidden characters through `to_wire`. Calling lower-level `write_xml` bypasses that checked boundary.

## 5. Shared types and arithmetic

| Surface | Comparison |
|---|---|
| VAT specials | T:186–248,266–324 covers all V invoice tokens: `TAHK TAM AAM EUT EUKT F.AFA K.AFA HO EUE EUFADE EUFAD37 ATK NAM EAM KBAUK KBAET`. Receipt-oriented `ÁKK EU EUK MAA` are labelled accordingly, not guaranteed invoice tokens. |
| Numeric VAT | Every listed value is representable: `0 1 2 2.1 3 4 4.8 5 5.5 6 7 7.7 8 8.1 9 9.5 10 11 12 13 13.5 14 15 16 17 18 19 20 21 22 23 24 25 25.5 26 27`. Percent normalizes trailing zeros; Other preserves original token. |
| VAT meaning | PDF confirms KBAUK new means of transport, EUT→KBAET and EUKT→EAM, TAHK outside subject matter versus TAM exempt activity, and K.AFA exact wording/default. T:203–210,232–236 agrees. No UK-country “fix” is warranted. |
| Payment method | T:588–687 agrees with first-party PHP for `átutalás készpénz bankkártya csekk utánvét PayPal SZÉP kártya`. PHP's `OTP Simple` remains expressible through Other. |
| Currency | T:369–437 accepts every C token, including historical EEK/HRK/LTL/LVL and vendor spelling KSH. Five named constants do not restrict the set. HUF/Ft exemption checked separately from wire spelling. |
| Language | T:480–585 exactly covers `hu en de it ro sk hr fr es cz pl bg nl ru si`. Vendor `cz`/`si` must not be replaced with ISO `cs`/`sl`. |
| Taxpayer status | T:690–755 emits `7 6 1 0 -1`, corresponding to non-EU/EU/Hungarian-tax-number/unknown/no-tax-number annotations. “Private individual” wording is narrower than the last annotation; see §7. |
| Templates | T:983–1020 exposes all six exact tokens plus Other. Default emits **SzlaAlap**, distinct from absent template. Label conflict remains unresolved. |
| Numbers and document type | T:22–57 preserves reference numbers as caller data; T:768–800 agrees with observed `SZ D ES VS HS SS SL`. Creation selects flags, not `tipus`. |
| Exchange rate | T:943–980 supports explicit bank/rate or MNB/absent rate. Invoice-specific S/D annotation explicitly permits the latter. |

**Calculation:** A:191–211 obtains exact net, applies caller-selected rounding, calculates and rounds percentage VAT, then exactly adds net and VAT. A:42–48 uses half-away-from-zero. N:6–59 uses normalized integer coefficients, cancellation of decimal factors and representability checks. N:63–140 recognizes finite numeric spellings without silently rounding; numeric Other VAT is calculated as its percentage, not zero (A:194–207). Unrepresentable numeric tokens error. Scientific/whitespace token parsing is a local fidelity guarantee, not proof that every such VAT spelling is vendor-accepted.

The exact-intermediate restriction is explicit at A:176–182: “precision loss and underflow, even if later rounding would make it fit.” Refusing an oversized intermediate that a later division could reduce is therefore a documented limitation, not silent incorrect successful calculation. Unknown nonnumeric special codes produce zero VAT as documented; arbitrary tax rules are not inferred.

Q says “all amounts shown on the invoice must be provided explicitly” through S and specifies multiply → round net → calculate/round VAT → add. The implementation matches. B2C gross-first is representable with `LineItem::new`: the official example is quantity 3, net unit price 393.66, net 1181, VAT 319, gross 1500. A missing gross-first convenience constructor is not a missing wire capability. DI's `-2000/-540/-2540` negative row was independently calculated successfully.

Minor-unit rounding is explicitly a **local policy**, T:412–435, A:9–39: HUF whole forints, EUR cents, KWD thousandths, etc. It is not a claim that the vendor stores every ISO currency at that precision. Exact is likewise not a promise against vendor-side rounding; recorded EUR evidence shows independent rounding can produce stored gross ≠ stored net + VAT.

## 6. Invoice response inventory and behavior

P says version 2 is “Structured `xmlszamlavalasz` with optional base64 PDF in `<pdf>`”; version 1/omission yields DONE text or raw PDF. I:746 always selects version 2, so rejecting ordinary version-1 bodies is coherent.

| Source field / behavior | Current code | Assessment |
|---|---|---|
| Root/namespace/UTF-8/full XML | E:288–318 → X:63–194 | Requires correct expanded root and a complete lexical XML document. Prefix aliases/foreign extensions are handled without admitting foreign identity/verdict fields. No XSD sequence validation is promised on responses. |
| Required `sikeres` boolean | X:348–373,625–655 | true/false/1/0 supported. Missing/malformed refuses; legacy blank→false cannot manufacture success. |
| Optional `hibakod`, `hibauzenet` | X:351–372 | Refusal carries code/message; unknown code preserved, missing code represented as Absent. Success flag is authoritative for body verdict; contradictory true-plus-code emission is not documented. |
| `szamlaszam?` | E:106–107,123–133,329–332; I:927–935 | Body then decoded header, trimmed/nonblank. Issued result requires number. Schema optionality covers errors/previews and does not establish a numberless issued document. |
| `szamlanetto?`, `szamlabrutto?`, `kintlevoseg?` | E:108–113,158–168,229–246,347–374 | Optional exact Decimals, body before raw numeric header, empty XML may fall back; scientific syntax supported, comma separator only in HTTP money. Zero remains present. |
| `vevoifiokurl?` | E:114–115,135–143,247 | Body then once-decoded header. XML entities decoded, no URL percent decoding. Boundary trimming is a documented normalization-policy note, not demonstrated broken link. |
| `pdf?` | E:116–117,145–148,249; T:104–117 | Optional standard base64 with whitespace wrapping; returns decoded bytes. No PDF rendering/content certification. |
| `szlahu_fizetesmod` | E:49–53,248,321–327 | Now exposed as optional open PaymentMethod, decoded once from header. P lists it; P's actual XSD has **no XML payment-method element**, despite generic “same data … XML” wording. |
| `szlahu_id` | E:30–38,228,334–345 | Auxiliary internal document id from header; invalid/negative absent. Supported by recorded account observation, not declared in P's XML schema. |
| HTTP error code/message, status/down | `wire.rs:262–310`; E:180–203 | Nonblank down → header code → known non-2xx → body, with numbered-56 special handling. Synthetic precedence is local policy, not vendor-origin authentication or observed every-status behavior. |
| Notification failure 56 | E:173–251,295–315 | Numbered reply yields Issued + warning; malformed optional metadata does not erase unique body identity. Other body refusal still wins. Numberless 56 remains error/uncertain, never automatic resend. First-party PHP corroborates numbered rule. |
| Preview | I:621–675,927–935 | Requested preview plus unnumbered success must contain PDF. A numbered response remains Issued even if preview was requested, preserving reported identity instead of inventing no issuance. Actual preview response combinations remain unobserved here. |

**Actual fetched examples:** Both current EN/HU successful P examples contain unescaped `&partguid` / `&szfejguid` and abbreviated base64 `....`. Passing each untouched example to `CreateInvoice::parse` returns `Parse(Xml(…UnclosedReference…))`. Escaping just the two ampersands still fails on the abbreviated PDF. After explicitly escaping those ampersands **and replacing** the PDF with synthetic `JVBERi0=`, both return number `XXX-2012-3`, net 30000, gross 38100, outstanding 0. This is a source-example defect, not vendor evidence of malformed production replies or a reason to weaken the parser. Both untouched login-error examples parse to `InvalidCredentials` with the complete Hungarian message.

**Optional artifacts:** A numbered success may have `pdf=None` even when requested; that absence remains visible without discarding issuance identity. I:625–627 explicitly says totals/PDF are optional as reported. No fatal missing-download finding is raised. Missing/empty/whitespace-only preview PDF fails because a preview's useful result is its PDF. No ordinary download failure was reproduced.

## 7. Ambiguities, deliberate policies and evidence gaps

### A1 — Combined preview/simpleItems order: vendor contradiction

**Potential impact:** Medium interoperability risk; **not a confirmed implementation defect**.

- S says “The order of the fields is fixed, they cannot be interchanged” / “kötött, nem felcserélhetők”. Its EN/HU header tails are **template → simpleItems → preview**.
- D and freshly inspected PHP `InvoiceHeader.php:398–404` say **template → preview → simpleItems**.
- I:819–826 deliberately follows D/PHP, and I:220–222 explicitly describes the disagreement.

**Reproduction:** Set `header.template=Some(Most)`, `preview_pdf=Some(true)`, `simple_items=Some(true)` on a valid populated create. Emitted tail is:

```xml
<szamlaSablon>SzlaMost</szamlaSablon>
<elonezetpdf>true</elonezetpdf>
<simpleItems>true</simpleItems>
```

The fresh real libxml2 XSD validator rejects `simpleItems` after preview against both inline schemas, code 1871. The download accepts that order. The same conflict applies when both fields are present with false values. Existing tests cover all nine option-presence/value combinations across all six kinds. Switching order would violate D/PHP instead. Vendor clarification or separately authorized server verification is needed before choosing a new order; no such operation ran here.

### A2 — Download request schema omits documented group/erasure fields

**Potential impact:** Low schema-tooling drift; **not missing Rust capability**. S contains `csoportazonosito` and `torloKod`; D does not. ER and its linked knowledge base explicitly document erasure count. I:853,914–916 correctly preserves them.

**Reproduction:** A full generated request containing both fields fails D with unexpected `csoportazonosito` and `torloKod`. Removing only those fields from the synthetic instance yields valid D output, retaining the selected header order and every other optional block. Against S, removing only `simpleItems` resolves the separate order conflict, retaining group/erasure. Six kinds × three independent sources yielded **18 successful conflict-isolated XSD controls**; none of the vendor schemas was patched or merged.

`fixtures/SOURCES.md:126–140` records that the cached request schema was hand-patched to add these fields. Passing that cached schema would not resolve today's three-source comparison.

### A3 — Layout labels contradict the linked knowledge base

**Potential impact:** Low layout-selection ambiguity; **no confirmed wrong wire token**. L labels `SzlaAlap` traditional and `SzlaNoEnv` envelope-friendly. Its linked knowledge base explicitly says `Tradicionális … SzlaNoEnv` and `Borítékbarát … SzlaAlap`, and also uses inconsistent lowercase XML spellings. T:991–1017 follows L's token inventory and traditional label; the enum name `NoEnvelope` follows the token spelling. No rendered PDF was inspected to resolve the disagreement. Preserve exact schema case and seek clarification rather than swapping tokens from one conflicting prose source.

### Deliberate policies / non-findings

- **simpleItems restrictions:** I:199–218 faithfully describes independent selection for regular/proforma/prepayment, inherited final state and rates, corrective/delivery exclusion, simplified-original correction refusal, OSS-off/Hungarian seller, two-item/four-final-item limits, allowed VATs, template override and K.AFA comment requirement. The server owns these content rules. The field does not remove monetary data from the outgoing XML/NAV data. Scratch schema-valid combinations are deliberately not claims of legal/account eligibility.
- **Foreign currency gate:** Blanket bank/rate requirement applies even to proformas/delivery notes; account notes at `docs/szamlazz-hu-behaviour.md:216–223` expressly identify this conservative boundary and untested relaxation. S/D's specific automatic-MNB annotation qualifies C's general “bank and rate” wording.
- **HUF arithmetic:** Q documents server normalization of fractional HUF totals; refusing all fractional caller-supplied invoice amounts locally is not mandated. Whole-forint derived policy is valid without being the only representable request.
- **Gross-first advice:** EN Q says B2B/B2C “have to”; HU says “valószínűleg” (probably). Explicit line values support either computation. No business-type classifier belongs implicitly in the arithmetic constructor.
- **No automatic final deduction/discount:** Both are caller-supplied negative lines. Missing automatic netting is not a defect, and the account notes show the danger of assuming vendor deduction.
- **Omitted paid flag:** I:178–180,802–804 deliberately emits only true. `ops.rs:18–21`'s blanket claim that absence is “exactly false” is stronger than the reviewed sources establish for all payment/account settings. This is an evidence/wording concern, not a reproduced inability to create an unpaid invoice.
- **URL whitespace:** E:114–115 uses X:587–598 and trims boundary whitespace; encoded query tokens inside URLs are not decoded again. The crate's README excludes URLs from its general business-text fidelity policy. No useful vendor link was shown to break. Do not revive this as a normal-wire defect solely because XSD string preserves whitespace.
- **Validation scope:** Date chronology, reference existence, tax/country consistency, prefix registration, erasure enablement, carrier identifier lengths and legal special-code selection remain vendor/caller responsibilities. Lower-level serialization is not full business validation.

### Small documentation opportunities and unverified execution

- I:548–551 calls aggregator/guardian “for contracted integrations”. Fresh PHP calls aggregator `webáruházat futtató motor neve` (webshop engine name); the reviewed sources do not establish a contractual prerequisite or guardian's complete semantics. The correct fields work; avoid treating this unverified gloss as a functional failure.
- T:703 says no tax number “(private individual)”; S says only no tax number, and the VAT PDF also discusses organizations without tax numbers. The variant/token is usable for either; no runtime private-person classifier was found.
- M expressly redirects test notifications to the account email. I:307–315 describes normal recipient rules without this caveat. The older behavior note's inability to provoke 56 does not prove test accounts never send mail.
- Preview's exact server response shape, interaction with `download_pdf=false`, all six-kind preview eligibility, partial carrier-block rendering, attachment failure notification, Unicode filename display, K.AFA margin rendering, full simpleItems tax/account combinations and e-invoice signing were not executed.
- The response XSD only requires `sikeres`; its optional number is shared by errors/previews/successes. There is no fresh evidence of a normal numbered document being created with its identity absent from both channels. The current requirement for an issued number remains justified; schema optionality alone is insufficient to call it a broken success parser.

## 8. Live-supported deviations and closed prior findings

Read `docs/szamlazz-hu-behaviour.md` in full. Its scope is one TEST account on stated dates; raw probe logs are not in the repository. Nothing below is a fresh live observation by this review.

| Recorded behavior | Current treatment |
|---|---|
| P48-P5: requested create issue date can become today (behavior:91) | I:141–148 correctly calls it a request, not guarantee. Do not impose storno's 352 rule on creates. |
| P73: true creates queried appearance 3, false 1 (behavior:97–98) | I:532–536 accurately describes create flag and queried code distinction. |
| C6-2: final does not deduct prepayment; one final only (behavior:117–120) | I:62–70 explicitly requires caller deduction lines. |
| C1/C2/D4/D5: implicit proforma linking, consumption and silently dropped stale reference (behavior:104–108) | References are serialized; creating against a supplied number does not verify existence. Explicit ES/VS proforma links still unverified (234–240), correctly qualified in I:50–54,69–70. |
| External id nonunique/newest holder, attaches only on actual creation (behavior:63–71) | No invoice request uniqueness/idempotency assumption introduced. |
| P60: net-check tolerance, EUR values round independently, numeric VAT accepts trailing zeros (behavior:159–162) | Whole/minor-unit calculation remains sensible; Exact/storage caveat and normalization-as-hygiene retained. |
| Buyer master data can change after later creation (behavior:111) | I:329–339 warns of partner updates; no immutable buyer snapshot claimed. |
| Duplicate replay depends on account toggle/content/live document (behavior:33–57) | Request only transports order number; O's two-day/date rules do not justify blind retry. |

### Closed findings independently reverified at f83e5fd

1. **Prior INV-01: silent intermediate arithmetic precision loss — CLOSED.** A:191–211 now uses exact helpers N:6–59. Fresh scratch results:

   | Quantity / price / VAT | Policy | Result |
   |---|---|---|
   | `0.9999999999999999999999999999 × 0.005`, AAM | Exact | `NetOverflow` |
   | Same | Scale(2) | `NetOverflow`, not the old erroneous 0.01 |
   | `0.1 × 1e-28`, AAM | Exact | `NetOverflow` |
   | `1 × 1e-28`, 27% | Exact | `VatOverflow` |
   | `1 × 1e28`, percentage `1e-28` | Exact | `GrossOverflow` |
   | `3 × 33.335`, 27% | Scale(2) | net 100.01, VAT 27.00, gross 127.01 |
   | `1 × -2000`, 27% | Scale(0) | net -2000, VAT -540, gross -2540 |

   Extreme rates exercise the local arithmetic contract, not claimed vendor-accepted tax codes. Existing A:232–295 exact-boundary controls also passed.

2. **Prior CM1: malformed optional metadata erased body-only numbered-56 identity — CLOSED.** E:298–315 now falls back to a separate unique scalar identity read. Fresh scratch bodies `sikeres=false`, `hibakod=56`, `szamlaszam=I-2`, with either `<szamlabrutto><bad/></szamlabrutto>` or duplicate PDF fields, return **Issued I-2 with warning**. Current `tests/response_headers.rs` also checks duplicate/nested identity is not salvaged and a body refusal survives bad metadata beneath header 56. These are malformed-metadata controls, not normal vendor examples.

3. **Prior shared XML lexical gap CQ-2 — closed for the reviewed regressions.** X:157,170–194 adds token grammar/reference/character checking; current completion tests refuse illegal characters, unknown entities and malformed ignored markup while permitting well-formed extensions. EOF/namespace protections remain passing. This is not a claim of exhaustive parser proof.

4. **Payment-method response omission — absent at this HEAD.** E:49–53,248,321–327 exposes the documented header; current header test and scratch encoded-transfer control pass. No imaginary XML payment-method field was added.

The prior transfer-interruption evidence-loss concern belongs to the HTTP transport review; it was not reproduced or adjudicated here. Receipt-only prior findings are likewise not relabelled as invoice findings.

## 9. Verification actually run

### Revision and working-tree checks

```sh
git rev-parse HEAD && git status --short
git diff f83e5fd7f0ca1a72e64b42b5f97a4e4edec679d9 -- crates/szamlazz-agent fixtures/SOURCES.md docs/szamlazz-hu-behaviour.md Cargo.toml Cargo.lock
```

HEAD matched the pin on the recorded checks; the scoped tracked diff was empty. Initial unrelated Restate test edits and untracked research/review material were preserved. Later concurrent unrelated work is not part of this report's conclusions.

Final post-report check again confirmed the same HEAD and empty scoped diff; `git diff --check` passed. Concurrent changes were present in Restate, Adatkapcsolat, CONTEXT and design/operations material, plus other new review reports; this review did not alter them. This report is untracked, so ordinary `git diff --check` does not itself check its contents; its written content was read back separately.

### Existing tests

```sh
cargo test --locked -p szamlazz-agent --lib --test numeric_fidelity --test simple_items --test upstream --test response_headers --test response_namespaces --test response_completion --test business_text --test literals --test error_classification
```

**233 passed, zero failed/ignored:** 185 unit, 6 numeric fidelity, 2 simple items, 11 upstream, 12 response headers, 6 response namespaces, 4 response completion, 2 business text, 2 literals, 3 error classification. No live target selected. The unit suite incidentally includes other operations; passing those does not expand this review's substantive scope.

Tests inspected include I's entire test module, A's arithmetic controls, `simple_items.rs`, `numeric_fidelity.rs`, `response_headers.rs`, `response_completion.rs`, and the upstream request/response comparison machinery. Fixtures are provenance-separated: golden/synthetic are project-authored; upstream examples are dated; the request XSD is explicitly patched; September header excerpts are not complete independent XSDs. Upstream request outlines trim text and omit empty containers (`tests/upstream.rs:1225–1234`); a pass does not prove empty/omitted semantic equivalence or fresh vendor compliance.

### Independent scratch probe

```sh
python3 /tmp/opencode/invoices-f83e5fd-independent/check.py
sha256sum /tmp/opencode/invoice-review-20260910-vat.pdf
```

The checker fetches the current EN/HU request/response pages and download schema in memory; extracts/hashes full inline schemas and examples; downloads/inspects current PHP; and invokes:

```sh
cargo run --quiet --offline --manifest-path /tmp/opencode/invoices-f83e5fd-independent/Cargo.toml
```

The scratch Cargo project pins `rust_decimal=1.43.0`, uses the current local crate with default features off, and has its own generated lock/build artifacts. It has **no HTTP client or vendor call**. Rust emits six richly populated requests, asserts attachment boundaries/five file names, arithmetic regressions and controls, source-shaped response repair controls, encoded payment header, preview handling and numbered-56 preservation.

Python calls installed **libxml2's real XSD validator via ctypes**, not a home-grown order checker. Results: all six rich requests failed each unmodified source for the known conflicts (`1871`); after source-specific removal from **instances only**, six passed each source (18 independent successful controls). Carrier/ledger/other optional fields remained populated. Python reserializes the generated XML through ElementTree for validation, preserving expanded names, order and values; this establishes schema structure/types rather than byte-for-byte XML serialization identity. Existing exact-body tests cover the latter separately.

The first checker run stopped before Rust/schema verification because `pdftotext` was unavailable. That optional extraction call was removed from the scratch script; the PDF was instead read by the dedicated tool after fresh-download/hash equality verification. The corrected checker passed. It was rerun after adding explicit repaired-example/preview/56 assertions and passed again. No failure from that initial tooling issue is attributed to the crate.

### Limits

No live invoice or preview request, account modification, email delivery, attachment processing, PDF/barcode renderer, signature/certificate test, client-feature matrix, full workspace suite or downstream database/export test ran. Rich requests test structural coverage, not every partial-block combination or tax/business-valid combination. Arbitrary dates, all future tokens and all possible Decimal operands are not exhaustively proven. Fresh sources contain contradictions and malformed examples; passing any one XSD cannot certify deployed server behavior. Historical TEST-account results remain bounded to their recorded account/dates.

**Disposition:** retain the implementation based on the evidence in this scope, keep prior fixed findings closed, and resolve A1/A3 with the vendor before changing order or layout mappings. Treat schema drift, response-example repairs and unexecuted account behavior explicitly in future verification.
