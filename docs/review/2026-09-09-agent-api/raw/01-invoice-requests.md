# Számla Agent invoice-creation requests — current official documentation audit

Date: **2026-09-09**. Code baseline: **`a804c740eb8446211c1cdca3eea4fb93d298d25d`**, clean working tree at the start.

## Summary

**One missing documented request feature; no demonstrated serialization bug in the supported fields.** `simpleItems` cannot be requested. Five additional findings concern public field/type documentation, including a wrong VAT-code meaning and omitted consequential semantics. The current official sources disagree with one another; those disagreements are reported separately, not charged to the serializer.

| ID | Severity | Confidence | Classification | Result |
|---|---|---|---|---|
| IR-01 | Medium | High | Missing feature | No `simpleItems` header field; the tour-operator view cannot be enabled through `CreateInvoice`. |
| IR-02 | Medium | High | Incorrect API documentation | `VatRate::Tahk` describes TAM's exemption instead of outside-the-scope VAT. |
| IR-03 | Medium | High | Incomplete API documentation | `eu_vat` does not explain that `eusAfa` suppresses NAV submission, its seller prerequisites, or that it does not replace the item VAT code. |
| IR-04 | Low | High | Incomplete API documentation | `Buyer::id` omits the documented partner-update and shared-document-access consequences of reusing an identifier. |
| IR-05 | Low | High | Stale API documentation | `Currency` says 37 supported codes; the current table has 46 currencies, 47 tokens including `Ft`. |
| IR-06 | Low | High | Incorrect API documentation | The erasure-code field's “errors 537–539 otherwise” misdescribes 538 and omits the test-account prohibition. |

Severity: **High** would mean a demonstrated broadly consequential wrong request; **Medium** means an unavailable documented workflow or field guidance that can materially misdirect invoicing; **Low** means narrower documentation impact. Confidence distinguishes what the code/docs prove from unobserved server outcomes. No issue below asserts that a live document was issued incorrectly during this audit.

## Scope and method

Primary code: [`src/ops/invoice.rs`][invoice], [`src/ops/waybill.rs`][waybill], [`src/item.rs`][item], and invoice-request-related [`src/types.rs`][types] (`InvoiceNumber`, `VatRate`, `Currency`, `Language`, `PaymentMethod`, `TaxpayerStatus`, `ExchangeRate`, `InvoiceTemplate`, `SellerEmail`). Checked the shared XML writer only to establish actual escaping, scalar output and credential ordering. Response parsing, other operations, the Restate worker, general transport hardening and tax-law advice are outside scope. The creation response page was read only to verify request settings (`valaszVerzio`, `szamlaLetoltes`).

Compared **every element** of all 13 named complex types in the current invoice XSD, the root sequence and language enumeration, against the writers and model. Read all ten current *Invoicing settings and rules* pages, request/prose/XSD sources, relevant authentication and error documentation, and first-party linked guidance where needed to resolve meanings. Both current language versions were checked for the schema and the main disputed features. Sources were fetched live with read-only GET requests; no Számla Agent request was sent, including previews.

Read [`fixtures/SOURCES.md`][sources], the cached `xmlszamla.xsd`, [`docs/szamlazz-hu-behaviour.md`][behaviour], the supplied `CONTEXT.md` vocabulary/decisions, and [ADR 0008][adr8]. ADR 0008 deliberately makes request structs plain data: an ability to construct a server-refused value is not automatically a missing-validation bug. Account-side facts (partner identity, OSS status, enabled features, linked-document state) cannot be proven by a request serializer.

Reviewed existing invoice/item/type unit tests, the invoice portions and comparison mechanism of `tests/upstream.rs`, `tests/literals.rs`, `tests/client.rs`, and `tests/live.rs`. Tests were treated as implementation evidence, not specifications. Temporary schema-comparison and offline Rust probes were written under `/tmp/opencode`; the only repository addition is this report.

### URLs visited (all fetched 2026-09-09)

The linked names below are the citation keys used throughout. A successful fetch establishes what was served, not which schema the production service actually enforces.

| Source | URL / subject | Result |
|---|---|---|
| S01 | [Current English XML example + inline XSD][S01] | Read in full; footer `v202608271632`. |
| S02 | [Current Hungarian XML example + inline XSD][S02] | Read in full; same footer. |
| S03 | [Legacy `/xsd` page][S03] | Still serves older content, footer `v202606031507`; no `simpleItems`. |
| S04 | [Linked downloadable `xmlszamla.xsd`][S04] | Actual XML fetched; differs from both inline generations. |
| S05 | [Creation request][S05] | Multipart field and attachment names. |
| S06 | [Invoicing settings and rules index][S06] | Followed all ten child pages. |
| S07 | [Document types][S07] | Kinds, proforma references, e-invoice flag. |
| S08 | [Tour operators / `simpleItems`][S08] | Feature behavior, defaults and per-kind restrictions. |
| S09 | [Hungarian tour-operator page][S09] | Confirms S08. |
| S10 | [VAT rates][S10] | Full token list and `eusAfa`. |
| S11 | [Hungarian VAT rates][S11] | Confirms TAHK and `eusAfa` meanings. |
| S12 | [Rounding][S12] | Net-first, gross-first and server-side HUF rounding. |
| S13 | [Supported currencies][S13] | Full current code table and exchange-rate requirement. |
| S14 | [Invoice templates and languages][S14] | Six templates and 15 languages. |
| S15 | [Order number and duplicate checking][S15] | Per-type restriction and conditional replay. |
| S16 | [Discounts][S16] | Negative-price line, positive quantity, same VAT rate. |
| S17 | [Invoice notification email][S17] | Omitted `sendEmail`, formatting, attachments, test-account routing. |
| S18 | [Data erasure codes][S18] | Optional nonnegative integer, cap 400. |
| S19 | [Basics index][S19] | Discovered relevant basics pages. |
| S20 | [Sending requests][S20] | Operation field, one XML per document, XSD and mistyped-tag behavior. |
| S21 | [Authentication][S21] | Agent key or username/password, legacy key-in-both-fields option. |
| S22 | [Error handling][S22] | Relevant request refusals, especially 202, 259–264, 537–539, 551–556. |
| S23 | [Legacy important-information page][S23] | Footer `v202608081609`; older consolidated prose. |
| S24 | [Creation response][S24] | Request response-version and PDF settings only. |
| S25 | [First-party VAT guidance linked by S10][S25] | Followed to its explanatory PDF. |
| S26 | [First-party VAT table PDF, 2025-11-04][S26] | Downloaded and read, page 1; resolves KBAUK and TAHK. |
| S27 | [First-party erasure-code guidance linked by S18][S27] | Count semantics, `SzlaMost`, placement at item end. |
| S28 | [First-party dynamic email fields linked by S17][S28] | Dynamic tags pass through existing text fields. |
| — | `https://docs.szamlazz.hu/agent/generating_invoice` | GET returned 403; this is not the current index URL. S05/S06 and their children were accessible. |

Current inline schema excerpts were extracted from the HTML code blocks, HTML entities decoded, without reformatting. SHA-256 for reproducibility:

```text
S01 English inline: 06d96231248068d195ee669e6752a6341215ddc82892f886da16c68578776de4
S02 Hungarian inline: 09141775e3c25532ee9e2ef5616ea2446d753bd80f7b5a9271be524d0879fe6a
S03 legacy inline: 508162a8a38ec80648db3d013b7ab1b258532cae914997887192a17dcbada801
S04 downloaded bytes: 90af7504bab00e92bcf84971ed3088d9b7c67dd70219148dabe454e32a3b5498
```

## Detailed coverage mapping

Notation: **R** = `minOccurs=1`, **O** = `minOccurs=0`; all fields have `maxOccurs=1` except repeated `tetel`. `String?`, `Date?`, `bool?`, `Decimal?` mean `Option<…>`, omitted for `None`. Sequences in the following tables are **wire order**, not Rust declaration order. “Matches” concerns representability, lexical type, presence and ordering; it does not certify that arbitrary content is a valid invoice. Unless stated otherwise, the authority is the current inline XSD [S01][S01], independently checked against [S02][S02]. No XSD `default=` value is declared for these elements: Rust defaults and prose-described server behavior are distinguished below.

### Envelope and settings

Root: `xmlszamla`, default namespace **`http://www.szamlazz.hu/xmlszamla`**; UTF-8 XML 1.0. Root sequence: `beallitasok` R → `fejlec` R → `elado` R → `vevo` R → `fuvarlevel` O → `tetelek` R. `tetelek/tetel` is **1..unbounded**. Writer: `invoice.rs:684–862`; empty seller still emits its required container. `validate` rejects no items at `629–632`. No invented root attribute or extra totals block. Omitting the example's `xsi:schemaLocation` is valid. Operation field **`action-xmlagentxmlfile`**, `invoice.rs:625–627`, matches [S05][S05]/[S20][S20].

| Ordered settings field | XSD | Model / output | Assessment |
|---|---|---|---|
| `felhasznalo`, `jelszo`, `szamlaagentkulcs` | each O string | `Credentials`; writer delegates at 687; `xml.rs:251–260` emits username then password, or agent key | Both documented auth alternatives supported; credentials precede flags. Schema makes each optional but prose requires authentication [S21][S21]. |
| `eszamla` | R boolean | `e_invoice: bool`, default false, always emitted at 688 | Matches paper/e-invoice semantics [S07][S07]; P73 confirms false→paper, true→e-invoice. |
| `szamlaLetoltes` | R boolean | `download_pdf: bool`, default false, always emitted at 689 | Matches [S24][S24]. |
| `szamlaLetoltesPld` | O int | `download_copies: Option<u8>`, default absent; 690–692 | Narrower than `xs:int`, but deprecated/ignored according to current inline annotation; correctly documented at 492–496. |
| `valaszVerzio` | O int | Always `ops::RESPONSE_VERSION` = 2, line 693 | Intentional supported choice; does not expose version 1/default text mode [S24][S24]. |
| `aggregator` | O string | `aggregator: String?`; 694 | Representable, default omitted as example requests. Contracted-integration semantics not established by public prose. |
| `guardian` | O boolean | `guardian: bool?`; 695–697 | Both booleans/absence represented; public schema supplies type, not operational semantics. |
| `cikkazoninvoice` | O boolean | `item_identifiers_on_invoice: bool?`; 698–700 | Matches; account-rendering behavior not tested. |
| `szamlaKulsoAzon` | O string | `external_id: String?`; 701 | Correct spelling, location and late-query purpose. No server uniqueness guarantee added [S01][S01]; see XPRB evidence. |

Settings model/defaults: `invoice.rs:478–505,549–557`. Constructor documentation at `523–525` calls `e_invoice`/`download_pdf` “absent”, although their actual default is explicit **false**; this is a minor wording correction, not a wrong wire default.

### Header (27 schema elements)

| Ordered field | XSD | Rust field / writer lines in `invoice.rs` | Assessment / semantics |
|---|---|---|---|
| `keltDatum` | O date | `issue_date: Date?`; 705 | Correct absence; docs at 139–146 already explain P48-P5 silent replacement with today. |
| `teljesitesDatum` | R date | `fulfillment_date: Date`; 706 | Correct fulfillment meaning (English example's “payment date” is mistranslated; Hungarian says fulfillment). |
| `fizetesiHataridoDatum` | R date | `due_date: Date`; 707 | Correct due-date element. |
| `fizmod` | R string | `payment_method: PaymentMethod`; 708 | Open text; built-ins and custom text supported. |
| `penznem` | R string | `currency: Currency`; 709 | Every current token representable; IR-05 concerns docs only. |
| `szamlaNyelve` | R `szamlaNyelveTipus` | `language: Language`; 710 | All 15 exact enumeration tokens supported. |
| `megjegyzes` | O string | `comment: String?`; 711 | Preserves free text, including manual margin-scheme wording. |
| `arfolyamBank`, `arfolyam` | O string, O double | `exchange_rate: Option<ExchangeRate>`; 712–717 | Bank plus optional decimal rate; automatic MNB omission supported. Stricter pair rules discussed below. |
| `rendelesSzam` | O string | `order_number: String?`; 718 | Correct optional tag, preserved text; caller/server determine duplicate behavior [S15][S15]. |
| `dijbekeroSzamlaszam` | O string | `InvoiceKind::proforma_number()`; 721–724 | Invoice/prepayment/final support, before all kind flags. |
| `elolegszamla` | O boolean | `InvoiceKind::Prepayment`; 727 | Emits true only for prepayment. |
| `vegszamla`, `elolegSzamlaszam` | O boolean, O string | `InvoiceKind::Final`; 728–736 | Optional explicit prepayment number; order-number alternative validated at 633–648. |
| `helyesbitoszamla`, `helyesbitettSzamlaszam` | O boolean, O string | `InvoiceKind::Corrective`; 737–740 | True plus supplied original number. Number is a string wrapper, not a nonempty proof. |
| `dijbekero` | O boolean | `InvoiceKind::Proforma`; 741 | True only for proforma. |
| `szallitolevel` | O boolean | `InvoiceKind::DeliveryNote`; 742 | True plus required-by-prose layout selection below. |
| `logoExtra` | O string | `extra_logo: String?`; 744 | Fully represented; account token semantics not detailed publicly. |
| `szamlaszamElotag` | O string | `number_prefix: String?`; 745 | Pre-registration documented; server checks account prefixes, error 202 [S22][S22]. |
| `fizetendoKorrekcio` | O double | `payable_adjustment: Decimal?`; 746–748 | Fully represented; no range/rounding formula published in reviewed request docs. Not a percentage discount [S16][S16]. |
| `fizetve` | O boolean | `paid: bool`; 749–751 | True emitted; false/default omitted. Explicit false is not representable; significance uncertain, not a proven bug. |
| `arresAfa` | O boolean | `margin_vat: bool?`; 752–754 | Both values and absence supported; no documented algorithm inferred. |
| `eusAfa` | O boolean | `eu_vat: bool?`; 755–757 | Correct wire; materially incomplete field explanation, IR-03. |
| `szamlaSablon` | O string | `template: Option<InvoiceTemplate>`; 758–765 | Six tokens + custom token. Delivery-note kind forces `SzlaFuvarlevelesAlap`, consistent with [S07][S07]. |
| `simpleItems` | O boolean | **Missing** between template and preview | IR-01; live download has opposite order relative to preview, see D-01. |
| `elonezetpdf` | O boolean | `preview_pdf: bool?`; 766–768 | Request flag represented; schema describes preview with no document created. No preview request was sent. |

Header declarations/defaults: `invoice.rs:138–218`. Constructors require fulfillment/due dates, method, currency and language; optional fields absent, `paid=false`. They do not invent business dates/currency/language, consistent with ADR 0008.

### Kinds and links

`InvoiceKind`, `invoice.rs:21–115`, supports all **six creation kinds** described by [S07][S07]; storno is correctly a separate operation. A plain invoice sends no kind flag; contradictory simultaneous kind booleans are intentionally unavailable. Corrective includes a number by construction but `InvoiceNumber::new("")` is possible (`types.rs:22–38`); XSD string itself allows empty text, with content validity left to the server.

Final: one explicit prepayment number or a nonblank order number is required locally (`633–648`), in agreement with the schema annotation and the observed C6 linking. One prepayment→one final, and no many-prepayment aggregation, is a server rule [S07][S07]; the model's single reference is appropriate. `invoice.rs:60–68` correctly tells callers to supply a negative prepayment line: C6-2 proves the server does not net the advance automatically. The current feature's inherited `simpleItems` state on finals is not a separate request-kind omission; it belongs to IR-01.

Proforma reference: plain invoice, prepayment and final each carry `dijbekeroSzamlaszam`. **The XSD does not actually enumerate “exactly three permitted kinds”**: it declares independent optional fields. The comment at `invoice.rs:27–29` overstates that evidence; the three-kind model is the project's domain decision, not an XSD constraint. It blocks no additional meaningful documented combination found here. Explicit prepayment/final references remain unprobed in the live notes; C1-3 proves only implicit prepayment consumption by order number. Existing tests cannot turn that into a live guarantee.

### Seller, buyer and buyer ledger

Seller `elado` container is R; **all six children O string**, in order:

| XML → Rust | Writer | Coverage |
|---|---|---|
| `bank` → `Seller.bank`; `bankszamlaszam` → `bank_account` | `invoice.rs:771–772` | Strings preserved; omitted by default. |
| `emailReplyto` → `Seller.email.reply_to`; `emailTargy` → `.subject`; `emailSzoveg` → `.body` | `773–777`; `types.rs:1003–1013` | All optional independently; BBCode/newlines/dynamic tags transmitted as text [S17][S17]/[S28][S28]. |
| `alairoNeve` → `Seller.signer_name` | `778` | Represented, optional. |

There is **no request seller name/address/tax-number element** in any of the compared invoice schemas. Their absence from `Seller` is not missing invoice-creation support; those account data are not supplied by this request. No self-billing or third-party-invoicing feature inferred from another operation's fields.

Buyer declarations: `invoice.rs:255–296`; defaults: `306–324`. The **21 ordered children**:

| XML → Rust | XSD | Writer | Coverage |
|---|---|---|---|
| `nev` → `name` | R string | 782 | Always present. |
| `orszag` → `country` | O string | 783 | No invented default country. |
| `irsz` → `zip`; `telepules` → `city`; `cim` → `address` | each R string | 784–786 | Always present; no address restructuring. |
| `email` → `email` | O string | 787 | Multiple comma-separated addresses supported [S17][S17]. |
| `sendEmail` → `send_email` | O boolean | 788–790 | `None` omitted, explicit false preserved. Omission means send when email filled, per current prose; correct field docs. |
| `adoalany` → `taxpayer_status` | O int | 791–793 | All five documented codes represented; no guessed default. |
| `adoszam` → `tax_number` | O string | 794 | Preserved; account/server validates content. |
| `csoportazonosito` → `group_id` | O string | 795 | Present in both inline generations, absent in download; correct current support. |
| `adoszamEU` → `eu_tax_number` | O string | 796 | Separate EU number; VAT-code prerequisites left to caller/server. |
| `postazasiNev` → `postal_address.name`; `postazasiOrszag` → `.country`; `postazasiIrsz` → `.zip`; `postazasiTelepules` → `.city`; `postazasiCim` → `.address` | each O string | 797–803 | Five independently optional flat children; no incorrect wrapper element. |
| `vevoFokonyv` → `ledger` | O complex | 804–815 | Correct nested block, see below. |
| `azonosito` → `id` | O string | 816 | Correct wire; consequential public-doc omission, IR-04. |
| `alairoNeve` → `signer_name`; `telefonszam` → `phone`; `megjegyzes` → `comment` | each O string | 817–819 | All represented in proper tail order. |

Buyer ledger has **six O children**, exactly in order: `konyvelesDatum` date → `accounting_date`; `vevoAzonosito` string → `buyer_id`; `vevoFokonyviSzam` string → `buyer_account`; `folyamatosTelj` boolean → `continuous_fulfillment`; `elszDatumTol` date → `settlement_from`; `elszDatumIg` date → `settlement_to`. Declarations `invoice.rs:118–133`, writer `805–814`; empty ledger schema-valid, absence is the default. All map correctly.

### Items and item ledger

`LineItem`, `item.rs:70–115`; optional fields initialized absent at `134–147`. Item vector order is preserved, allowing a discount directly after its source row [S16][S16].

| Ordered XML → Rust | XSD | Writer in `invoice.rs` | Assessment |
|---|---|---|---|
| `megnevezes` → `name` | R string | 827 | Matches. |
| `azonosito` → `id` | O string | 828 | Matches. |
| `mennyiseg` → `quantity` | R double | 829 | Decimal lexical output; fractional/negative values representable. |
| `mennyisegiEgyseg` → `unit` | R string | 830 | Arbitrary unit text, e.g. `db`. |
| `nettoEgysegar` → `unit_price` | R double | 831 | Decimal, signed; discounts supported. |
| `afakulcs` → `vat_rate` | R string | 832 | All documented invoice tokens and percentages supported. |
| `arresAfaAlap` → `margin_vat_base` | O double | 833–835 | Explicit value supported, no calculation invented. |
| `nettoErtek` → `net_value`; `afaErtek` → `vat_value`; `bruttoErtek` → `gross_value` | each R double | 836–838 | All amounts sent explicitly as required [S01][S01]. |
| `megjegyzes` → `comment` | O string | 839 | Preserved. |
| `tetelFokonyv` → `ledger` | O complex | 840–855 | Full six-child block below. |
| `torloKod` → `erasure_code_count` | O int restricted ≥0 | 856–858 | Count, not code identifier; u32 plus ≤400 check at 650–657 satisfies current prose. IR-06 is docs only. |

Item ledger: **six O children** in order: `gazdasagiEsem` string → `economic_event`; `gazdasagiEsemAfa` string → `vat_economic_event`; `arbevetelFokonyviSzam` string → `revenue_account`; `afaFokonyviSzam` string → `vat_account`; `elszDatumTol` date → `settlement_from`; `elszDatumIg` date → `settlement_to`. `item.rs:48–68`, `invoice.rs:841–854`. All present and ordered correctly; strings retain leading zeros.

Arithmetic: `LineItem::new` sends caller-computed values, intentionally without duplicating server arithmetic rules (`item.rs:70–79,121–149`). `try_calculated` is explicitly **net-first**, rounds net before VAT, then adds the rounded pair (`151–199`); HUF minor-unit mode matches [S12][S12]'s B2B procedure. It checks decimal overflow. Gross-first B2C arithmetic is achievable through `new`, but has no convenience constructor. Negative-price/positive-quantity discount lines work. Special-code derived VAT is documented as zero; neither the XSD nor reviewed prose establishes a generic algorithm for margin VAT, so `margin_vat_base` is an explicit caller amount, not an automatically handled scheme. A custom numeric string deliberately constructed as `VatRate::Other("27")` also takes the special-code branch; use `VatRate::from("27")`/`Percent` for derived arithmetic. This follows the enum's documented distinctions, not a wrong wire-token mapping.

### Waybill and carrier blocks

The top-level waybill is optional and is **not restricted by XSD to a delivery note**; it requires a layout that can display it. Code permits it on any kind. The module introduction (`waybill.rs:1–3`) saying only the delivery-note use needs it is narrower than that documented rule; no runtime restriction implements that sentence.

| Ordered XML → Rust | XSD | `waybill.rs` declaration / writer | Assessment |
|---|---|---|---|
| `uticel` → `destination` | O string | 91–92 / 127 | Legacy unused field; schema says use Sprinter routing instead. Preserved deliberately. |
| `futarSzolgalat` → `carrier` | O string | 93–94 / 128 | All tokens `TOF`, `PPP`, `SPRINTER`, `FOXPOST`, `MPL`, `GLS`, `EMPTY` representable. No closed enum losing carriers. |
| `vonalkod` → `barcode`; `megjegyzes` → `comment` | each O string | 95–98 / 129–130 | General fallback barcode and waybill comment supported. |
| `tof` → `trans_o_flex` | O complex | 99–100 / 131–142 | Full Trans-O-Flex block. |
| `ppp` → `pick_pack_point` | O complex | 101–102 / 143–148 | Full Pick Pack Pont block. |
| `sprinter` → `sprinter` | O complex | 103–104 / 149–160 | Full Sprinter block. |
| `mpl` → `mpl` | O complex | 105–106 / 161–170 | Full MPL block. |
| `tof/azonosito` → `id`; `shipmentID` → `shipment_id`; `csomagszam` → `parcel_count`; `countryCode` → `country_code`; `zip` → `zip`; `service` → `service` | all O; strings except int count | 11–24 / 133–140 | Six fields, correct order/case; five-digit carrier id documented, not locally length-validated. |
| `ppp/vonalkodPrefix` → `barcode_prefix`; `vonalkodPostfix` → `barcode_suffix` | each O string | 28–33 / 145–146 | Correct mapping. Annotation says agreed 3-character prefix, per-invoice suffix max 7; caller supplies them. |
| `sprinter/azonosito` → `id`; `feladokod` → `sender_code`; `iranykod` → `routing_code`; `csomagszam` → `parcel_count`; `vonalkodPostfix` → `barcode_suffix`; `szallitasiIdo` → `delivery_time` | all O; strings except int count | 37–50 / 151–158 | Correct mapping/order. Annotated 3-character id, 10-character sender, 7–13-character suffix remain caller obligations. |
| `mpl/vevokod` → `customer_code`; `vonalkod` → `barcode`; `tomeg` → `weight` | each R string | 57–63 / 163–165 | Correctly required in constructor; weight **is a string in the XSD**, not a missing numeric type. |
| `mpl/kulonszolgaltatasok` → `extra_services` | O string | 64–65 / 166 | Omitted means no icons according to annotation. |
| `mpl/erteknyilvanitas` → `declared_value` | O double | 66–67 / 167–169 | Optional decimal, correct tail order. |

All optional carrier blocks/fields default absent; `Mpl` deliberately has no `Default`. Parcel counts use u32 but `CreateInvoice::validate` rejects values above `i32::MAX` (`invoice.rs:658–665`, iterator `waybill.rs:109–123`). Negative counts allowed by unrestricted XSD int are intentionally unrepresentable as quantities; zero is not expressly prohibited by the sources. No missing FOXPOST/GLS-specific block: the schemas define no such children. Generic carrier/barcode fields already allow those carriers. Selecting inconsistent carrier sub-blocks is possible because the official schema uses a sequence, not a choice; the all-fields probe is a shape test, not a recommended shipment.

### Value types, enums, defaults and attachments

| Surface | Current requirement | Code / result |
|---|---|---|
| Language | `hu en de it ro sk hr fr es cz pl bg nl ru si` [S01][S01]/[S14][S14] | `types.rs:469–575`: all 15 exact tokens, no unsupported extension. `cz`/`si` are intentional vendor tokens, not ISO corrections to `cs`/`sl`. |
| Taxpayer status | `7` non-EU business, `6` EU business, `1` Hungarian tax number, `0` unknown, `-1` none [S01][S01] | `679–745`: exact mapping, optional at buyer. JSON serializes as strings, but XML uses correct int lexical text. No missing currently documented value. |
| Payment method | XSD string; example `Átutalás`, browser-recognized values [S01][S01] | `577–677`: transfer/cash/card/check/COD/PayPal/SZÉP + `Other`. Lowercase built-in transfer is accepted in recorded live use; case-sensitive parsing retains unfamiliar capitalization as `Other`. No exhaustive invoice payment-method list was found. |
| VAT codes | `TAHK TAM AAM EUT EUKT F.AFA K.AFA HO EUE EUFADE EUFAD37 ATK NAM EAM KBAUK KBAET` [S10][S10] | `178–359`: every token mapped; IR-02 is semantic rustdoc. Receipt-only additional variants are not claimed to be universally accepted on invoices. |
| VAT percentages | `0,1,2,2.1,3,4,4.8,5,5.5,6,7,7.7,8,8.1,9,9.5,10,11,12,13,13.5,14,15,16,17,18,19,20,21,22,23,24,25,25.5,26,27` [S10][S10] | Decimal `Percent` represents all exactly. Unlisted values remain sendable as open data; server decides acceptance. P60 supports normalized tokens, not arbitrary rates. |
| Currency | 47 tokens: `HUF Ft EUR CHF USD AED ALL AUD BAM BGN BRL CAD CNY CZK DKK EEK GBP HKD HRK IDR ILS INR ISK JPY KRW KWD KSH KZT LTL LVL MXN MYR NOK NZD PHP PLN RON RSD RUB SEK SGD THB TRY TWD UAH VND ZAR` [S13][S13] | `361–467`: all representable, five constants are conveniences, not a whitelist. Preserve unusual vendor `KSH` and historical currencies. IR-05 is stale count only. |
| Currency precision | HUF line totals whole; non-HUF fractions allowed [S12][S12] | `minor_unit_digits`, 403–425: explicit ISO-based convenience with HUF override; not a promise of server storage scale, see limitations. |
| Exchange rate | Foreign currency requires bank/rate [S13][S13]; MNB with absent rate uses current rate [S01][S01] | `932–963`, invoice validation `666–677`: exact MNB omission supported. `HUF`/`Ft` recognized case-insensitively; original spelling transmitted. |
| Template | `SzlaMost SzlaAlap SzlaNoEnv Szla8cm SzlaTomb SzlaFuvarlevelesAlap` [S14][S14] | `965–1001`: all six + `Other`; no missing layout token. `Default` means the named `SzlaAlap`, whereas `None` leaves server default. |
| Email content | BBCode, direct newlines, dynamic fields [S17][S17]/[S28][S28] | `SellerEmail` accepts text; XML escaping preserves parsed text without interpreting these features. No new XML field needed. |
| Attachments | Up to five `attachfile1`…`attachfile5`, ≤2 MB each; ignored if email disabled [S05][S05]/[S17][S17] | `invoice.rs:328–465,880–890`: bounded collection, exact names/order, bytes/filename/content type retained. Count and size checked at push, conversion and deserialize; conservative 2,000,000-byte interpretation explicitly stated. |

## Findings

### IR-01 — Missing `simpleItems` request support

**Severity: Medium. Confidence: High. Classification: documented feature gap**, not an existing ordinary-invoice serializer error.

**Exact code:** `crates/szamlazz-agent/src/ops/invoice.rs:179–187` (header tail), `213–216` (defaults), `758–768` (template immediately followed by preview); cached header `fixtures/upstream/agent/xsd/xmlszamla.xsd:133–136` likewise lacks the element.

**Official requirement:** [S08, Enabling it in the request][S08]: “Set the optional boolean `<simpleItems>` field in the `<fejlec>` (header) block of the invoice request, after `<szamlaSablon>`”; “`false` or omitted: the system works in the default (non tour operator) mode”; “via Számla Agent it is controlled **per document** with this field.” [S01][S01] and [S02][S02] declare `<element name="simpleItems" type="boolean" maxOccurs="1" minOccurs="0">` between `szamlaSablon` and `elonezetpdf`.

**Trigger/impact:** A tour operator wants to issue an invoice/proforma/prepayment with price and VAT details hidden on the buyer's invoice image, while supplying complete NAV data. No field or `InvoiceTemplate::Other` token enables that boolean. Merely configuring the account's UI setting is not a substitute under the documented Agent behavior. A complete request deserialized from JSON with `header.simpleItems=true` is accepted but that unknown field is silently discarded; an offline probe confirmed that the resulting request equals the original and emitted XML has no `simpleItems`. This is particularly easy to miss when adapting the official XML vocabulary into JSON. It does not prove a live rendered result, and JSON's field naming is the crate's own contract, not the official XML contract.

**Live evidence:** No `simpleItems` probe or deliberate exclusion exists in the reviewed behavior notes/CONTEXT. Existing margin-VAT and rounding probes do not exercise this feature. Current English/Hungarian prose and both current inline schemas establish the feature; S04 also has it, but disagrees about order.

**Suggested fix:** Add an optional plain-data header field, default absent, and serialize exact case `simpleItems`. Document per-document activation, max two items (final up to four), allowed Hungarian VAT codes, prohibited corrective/delivery-note kinds, and final inheritance. Account-side eligibility remains a server fact. Resolve/document the preview-order discrepancy D-01 before choosing the combined-field order; do not blindly refresh from S04 and lose other fields. Adding a public struct field is a breaking change under ADR 0008.

**Meaningful regression:** An external consumer builds/deserializes a request that enables the option; inspect the generated parsed XML, then validate against the selected current schema. Cover absent/false/true, and **both `simpleItems` and `elonezetpdf` present** so the order dispute cannot hide behind optional omission. Validate permitted kinds with full monetary fields still present; if local content checks are chosen, test the final's four-row exception and their actual precedence. A separate, authorized future server check would need to inspect the PDF; no such check ran here.

### IR-02 — `VatRate::Tahk` documents the wrong VAT category

**Severity: Medium. Confidence: High. Classification: incorrect public type documentation.**

**Exact code:** `crates/szamlazz-agent/src/types.rs:194–196`. Correct token emission is `263`, token parsing `293`.

**Official requirement:** [S10][S10] describes `TAHK` as “Outside VAT subject matter scope”; [S11][S11]: “áfa tárgyi hatályán kívül”. The first-party explanatory [S26, page 1, TAHK row][S26] maps **`TAHK → ATK`**, describes transactions outside VAT, and cites §§2–3. That PDF puts the code's current rustdoc wording (“a tevékenység közérdekű … sajátos jellegére”) under **TAM**, §§85–86.

**Trigger/impact:** A caller selecting an enum variant from its Hungarian/English rustdoc for an exempt public-interest service can choose `Tahk` instead of `Tam`. Both derive zero VAT, so arithmetic tests can pass while the VAT classification sent is different. The serializer faithfully sends the caller's selected `TAHK`; the bug is guidance that can cause a wrong selection, not token corruption.

**Live evidence:** No TAHK-versus-TAM live probe in the behavior notes. P60 percentage arithmetic and AAM examples cannot justify the definition. English prose, Hungarian prose and the linked vendor PDF agree.

**Suggested fix:** Describe `Tahk` as outside VAT's subject-matter scope, noting the legacy mapping to ATK where useful; retain the `TAHK` wire token. Do not replace the token with TAM.

**Meaningful regression:** Review the corrected rustdoc against the cited TAHK/TAM PDF rows. A focused token round-trip table should keep TAHK and TAM distinct across parsing/serialization; it cannot prove tax semantics. Do not write an implementation-mirroring test that merely asserts a comment string.

### IR-03 — `eu_vat` omits `eusAfa`'s consequential protocol semantics

**Severity: Medium. Confidence: High. Classification: public documentation gap.**

**Exact code:** `crates/szamlazz-agent/src/ops/invoice.rs:181–182`; writer `755–757` sends the boolean unchanged.

**Official requirement:** [S10, The eusAfa field][S10]: it “does not trigger NAV Online Invoice data submission”; true “may only be set if the **seller … is OSS-registered or has a non-Hungarian tax number**”; it “**does not replace** item-level VAT code”; “**retroactive data submission is not possible**.” The current inline XSD repeats the suppression and item-code distinction. [S11][S11] agrees.

**Trigger/impact:** A Hungarian seller issuing an EU-related transaction can read only “VAT belongs to another EU member state” and set the flag as a descriptive EU-transaction marker. If accepted, the flag changes submission behavior, not just document labeling. This is a conditional impact explicitly described by the vendor; the audit does not claim every ineligible flag is accepted or that local validation can identify an OSS-registered seller.

**Live evidence:** No OSS/`eusAfa` probe was recorded; ordinary test-account invoices cannot establish harmlessness. The omission is not a live-tested departure.

**Suggested fix:** Expand the field rustdoc with the no-Hungarian-VAT/submission meaning, seller prerequisites and separate item-code obligation, linking the official section. Preserve optionality and the raw boolean; account validation cannot be inferred from this struct.

**Meaningful regression:** Review the generated field documentation against the protocol conditions. Keep the existing false serialization test and add a true/absent XML check only when touching serialization. A mocked acceptance is not evidence of real NAV submission behavior; proving that would require an explicitly authorized account-side test outside this audit.

### IR-04 — `Buyer::id` hides the documented partner-identity consequences

**Severity: Low. Confidence: High. Classification: public documentation gap.**

**Exact code:** `crates/szamlazz-agent/src/ops/invoice.rs:288–289`; transmitted at `816`.

**Official requirement:** [S01, warning after example][S01]: “**DO NOT send an identifier (`<azonosito>`) that has already been registered to another customer**”; matching identifiers allow access to “**all documents belonging to that customer account using the customer account link**”, and “**the partner's information will be automatically updated**” with the XML billing data. [S02][S02] confirms both effects.

**Trigger/impact:** An embedder uses an order-local or recycled identifier as `Buyer.id`, seeing only “Partner identifier from the account's partner database”. A collision with an existing partner can update that partner's billing data and associate documents with the shared buyer portal. The docs explicitly establish these consequences; the audit did not access any buyer portal.

**Live evidence:** D6 (`behaviour.md:111`) establishes that queried buyer data can change after a later create, but does not directly test id collision or portal access. It is consistent with, not proof of, the full official warning.

**Suggested fix:** Document stable, unique-per-partner use within the billing account and the two effects. Do not add unverifiable local uniqueness validation.

**Meaningful regression:** Documentation review against S01/S02. Serialization should preserve a string identifier (including leading zeros) and omit `None`; an offline test cannot validate partner uniqueness or portal isolation.

### IR-05 — Supported-currency count is stale

**Severity: Low. Confidence: High. Classification: stale public documentation.**

**Exact code:** `crates/szamlazz-agent/src/types.rs:361–364` (“37 ISO-style codes”).

**Official requirement:** [S13][S13]: “The list contains **all currencies supported and displayed** in the Számla Agent”; the fetched table has **47 code rows**, including `HUF` and alias `Ft`, hence 46 currencies. Its exact tokens are enumerated in the coverage table above.

**Trigger/impact:** A caller using crate documentation to assess support sees an obsolete inventory count. There is no runtime restriction: `Currency::new` accepts every current token, so this does not block a newly listed currency.

**Live evidence:** P60 exercises EUR/HUF and cannot establish a supported-currency inventory. No intentional fixed 37-code contract exists in the open wrapper.

**Suggested fix:** Remove the hardcoded count and link the vendor's current list, preserving `HUF`/`Ft` explanation.

**Meaningful regression:** A documentation review is sufficient for the count change. For future list-driven tests, load current-source provenance and verify all listed tokens survive round-trip; do not freeze a count as proof of future completeness.

### IR-06 — Erasure-code rustdoc misattributes 538 and omits test-account restriction

**Severity: Low. Confidence: High. Classification: incorrect/incomplete public documentation.**

**Exact code:** `crates/szamlazz-agent/src/item.rs:107–111` (“Requires the account feature; on invoices the `SzlaMost` template (errors 537–539 otherwise)”).

**Official requirement:** [S22][S22]: **537** is the per-item count maximum, **538** is “Data deletion code cannot be used in demo and test accounts”, **539** is the disabled account setting. [S27][S27] independently requires the recommended layout (`SzlaMost`) and says `torloKod` is the **number of requested codes**, max 400, at the end of `tetel`. The retrieved sources do not assign a specific error code to the wrong invoice layout.

**Trigger/impact:** A developer enables the account feature and chooses `SzlaMost`, then attempts to verify erasure-code integration on a test account. The request still receives the separately documented 538; the field docs direct attention to incomplete conditions and incorrectly bundle the code with feature/layout failure. The count mapping, placement and ≤400 validation are correct.

**Live evidence:** No erasure-code live probe in the reviewed record; synthetic `torloKod=123` examples do not prove test-account acceptance. This feature cannot be certified by those fixtures.

**Suggested fix:** State that demo/test accounts cannot use it; name 537 and 539 for their actual conditions and avoid guessing a layout-failure code.

**Meaningful regression:** Review field rustdoc with the error table. For the request boundary, test 0 and 400 are transmitted and 401 rejected (401 already covered); do not create a mock that pretends a test account supplies real codes as evidence of upstream behavior.

## Intentional limitations, source contradictions and uncertainties

### D-01 — There is no single mutually consistent current invoice XSD

| Source | `csoportazonosito` | `torloKod` | Header tail |
|---|---|---|---|
| Current English/Hungarian combined page S01/S02 | present | present | `szamlaSablon`, **`simpleItems`**, `elonezetpdf` |
| Legacy standalone inline S03 | present | present | `szamlaSablon`, `elonezetpdf` |
| Current downloadable S04 | **absent** | **absent** | `szamlaSablon`, `elonezetpdf`, **`simpleItems`** |
| Cached workspace XSD | present (manually patched) | present (manually patched) | `szamlaSablon`, `elonezetpdf` |

The structural comparison of all named complex types, child names/types/min/max occurrences and the language enumeration found **only** the added `simpleItems` between cache and current inline schemas. The download's differences are the three rows above; the old inline schema's structure matches the cache. Comments differ independently. S04 is no longer simply unchanged from July: it now contains `simpleItems`. The provenance warning in `fixtures/SOURCES.md:124–135` remains valuable for the two absent elements but should be refreshed when the corpus is next updated.

Offline libxml2 validation reproduced the split: minimal crate output validates against all four fetched schemas; full outputs containing group id and erasure-code count validate against both current inline schemas and legacy inline S03, but S04 rejects **both elements**. Those rejections are **not code findings**. IR-01's correct order with a simultaneous preview cannot be settled by source freshness alone because S20 explicitly links the downloadable schema as the processing schema. Ordinary requests without both options can avoid the order conflict, but that does not settle it. Vendor clarification or an authorized future non-issuing preview test is needed.

### D-02 — Example/prose optionality contradicts the XSD on the same page

[S01][S01] says “All fields shown in the example are mandatory”, while its own warning says “Elements marked `minOccurs="0"` may be omitted”; its example also says omit the waybill/aggregator. The code correctly follows explicit XSD cardinality and current field-specific prose. `keltDatum`, seller children, references and inactive flags need not be emitted merely to copy the sample. Actual test-account requests corroborate the omission strategy. There is no general proof that **every** empty optional element equals absence; the upstream outline test's broad equivalence rule (`tests/upstream.rs:1091–1098`) must not be mistaken for a server specification.

### D-03 — VAT descriptions and template names have documentation traps

* S10's English **KBAUK** description says “supply of goods to UK”, but its linked first-party PDF S26 explicitly means **new means of transport within the Community**. `types.rs:224–226` matches the detailed PDF; **not a code finding**. The Hungarian list's abbreviated “UK” is also clarified by the PDF.
* The English example calls `teljesitesDatum` a “payment date”; S02 and domain vocabulary correctly identify fulfillment. The crate's name is correct.
* `InvoiceTemplate::NoEnvelope` maps correctly to `SzlaNoEnv`; the current UI name is **Borítékbarát számlakép** (envelope-friendly). The English Rust label is not a reason to change the wire token. `InvoiceTemplate::Default` names `SzlaAlap` (traditional), not necessarily the account default selected by omission.
* S27's prose writes `<szamlasablon>` in lowercase, while the XSD says `szamlaSablon`; the crate correctly follows the case-sensitive schema.

### Intentional behavior retained after reconciliation

| Topic / code | Official docs and recorded live evidence | Audit disposition |
|---|---|---|
| Foreign-currency gate, `invoice.rs:666–677` | XSD rate/bank optional, MNB omission explicit; current S13 says non-HUF **documents** require both. Behavior notes 216–223 explicitly retain the stronger gate pending a probe of foreign-currency AAM/proforma/delivery note. | **Intentional**, not a false-positive missing-rate finding. Current prose supports the general requirement more clearly than the older example did. |
| Issue date, `invoice.rs:139–148` | XSD optional; P48-P5 supplied yesterday but stored today. A4b's changed date replay is consistent with that replacement. | Already correctly documented. Do not “fix” by requiring today or treating the sent date as guaranteed. Live-account and e-invoice-date behavior still unverified. |
| E-invoice bool, `invoice.rs:481–488` | S07 true/false; P73 confirms paper code 1 versus electronic code 3 on query. | Correct. Queried integer `eszamla` is a different surface, not a reason to change request bool. |
| Proforma/prepayment/final references | S01 independent optional reference elements; C1-3 implicit prepayment consumption, C2/D4 explicit plain-invoice conversion, C6 final linking/no automatic netting. | References and negative-final-line guidance are intentional. Explicit prepayment/final proforma reference acceptance still unverified; no contrary source found. |
| External id, `invoice.rs:503–505` | S01 identification/query purpose without uniqueness claim; A3/XPRB repeatedly observed nonunique ids and newest-holder lookup. | No client uniqueness validation, length cap or replay guarantee needed in this request layer. Worker bounds are not invoice-XSD requirements. |
| Item arithmetic/rounding | S12 net-first procedure; P60-H tolerated whole-forint rounding; P60-E1/E3 independently rounded EUR monetary values to two decimals. | Explicit `Rounding` and direct-value constructor intentional; no mandate to use unrounded floats or to duplicate exact arithmetic checks. |
| ISO minor units beyond EUR/HUF | `types.rs:403–425` supports KWD=3, JPY/ISK=0; S13 lists those currencies but gives no storage scale. P60 tested EUR, not KWD/JPY/ISK. | **Uncertainty**, not demonstrated wrong rounding. A three-decimal KWD result could be reduced if EUR's observed storage rule generalizes. Avoid saying any ISO minor-unit result is guaranteed to reconcile to stored values. |
| `paid=false` omits `fizetve`, `invoice.rs:174–178,749–751` | XSD permits both absence/false but declares no default; reviewed prose does not settle explicit-false effects, especially for cash/card/account defaults. | **Representational limitation/uncertainty**. Cannot explicitly force false. No evidence here proves a different server outcome, so no bug claimed. |
| Delivery note forces layout, `invoice.rs:758–760` | S07 prescribes `SzlaFuvarlevelesAlap`; schema independently carries `szallitolevel`; recorded C1 included actual SL issuance. | Intentional; header template ignored for this kind should be stated more clearly. A template-only regular invoice is still expressible; layout and document kind are not the same wire concept. |
| Gross-first convenience absent | S12 B2C gross-first algorithm; `LineItem::new` accepts all its explicit values. | **Intentional low-level interface limitation**, not missing wire fields. Document a recipe rather than changing `try_calculated`'s net-price meaning. |
| Validation scope | ADR 0008 plain data; XSD strings do not restrict blank buyer fields, bank formats or carrier lengths; server has content/account rules. | No blanket bug for unvalidated free text, nonpositive exchange rate, unknown rate/payment/currency token, or inconsistent optional flags. No private-state checks invented. |
| Notifications and invalid addresses | S17 test accounts route to configured account email; D6 malformed/undeliverable buyer email did not yield 56. | Current prose supplies a plausible explanation for D6, not a request bug. Do not infer production delivery from synthetic success or that test result. |
| Attachment rejection | S17 server can still send valid attachments when another is invalid; client bounds reject oversized/sixth attachment first. | Explicit conservative client policy, not silent truncation. Disabled-email requests still obey collection bounds. 2 MB ambiguity documented; no unsupported extension restrictions imposed. |
| Contract-only settings and obscure header fields | `aggregator`, `guardian`, `logoExtra`, `fizetendoKorrekcio`, `arresAfa` have schema support but little/no current public operational prose. | Shape is covered; their exact account/default/interaction semantics remain **unverified**. No invented default or assumed business rule used as a finding. |

## Verification performed and existing-test limits

### Executed, with no upstream issuing

| Check | Result |
|---|---|
| `cargo test -p szamlazz-agent --lib ops::invoice::tests` | **31 passed**. |
| `cargo test -p szamlazz-agent --lib item::tests` | **6 passed**. |
| `cargo test -p szamlazz-agent --lib types::tests` | **11 passed** (includes a few shared-type tests beyond invoice requests). |
| `cargo test -p szamlazz-agent --test upstream requests::` | **1 passed**, the table test covering cached request examples. No live source fetch by that test. |
| Temporary Rust consumer, default-feature sans-I/O crate | Built all six creation kinds with every supported non-credential optional XML field populated; `to_wire` accepted; emitted XML saved only under `/tmp/opencode`. Confirmed unknown JSON `header.simpleItems=true` is discarded. |
| libxml2 XSD validation of generated requests | Minimal + all six full kinds pass both current inline language schemas and legacy inline. Minimal passes download; six full requests rejected by download for group id and erasure count. |
| Parsed sequence/coverage comparison | All generated child sequences in schema order. Union of full probes exercises every current schema element path except username/password (writer inspected separately) and missing `simpleItems`. |
| Current schema/currency inventory script | Structural deltas and language list independently extracted; 47 currency tokens counted. |

The scratch all-fields requests were deliberately **schema probes**, not business-valid records: synthetic tax identifiers, all four carrier blocks, and erasure codes on every kind are useful for field/order coverage but not evidence of server acceptance. The Rust probe depended on the crate with no HTTP-client feature and performed no networking. Python fetched only documentation/XSD URLs. The PDF fetch was a static first-party file.

Temporary tools: `/tmp/opencode/invoice-request-audit-20260909.py`, `/tmp/opencode/invoice-request-audit-probe/`, `/tmp/opencode/invoice-request-audit-validate.py`. System libxml2 was used via Python ctypes because `lxml`/`xmllint` were not installed. Temporary material is not part of the repository report or a maintained test suite.

### What existing tests prove — and do not

* `invoice.rs:942–1015` checks the canonical output and corrective/prepayment/final reference ordering; `1094–1178` checks many optional blocks and carrier fields by string fragments. Useful regression coverage, but a handwritten expected fragment cannot discover a new documented field. The present test suite passes despite IR-01.
* `invoice.rs:1180–1188` asserts the delivery-note flag/layout pair. Attachment tests `1190–1269` verify exact multipart output and count/size refusal. Boundary checks include no items, final reference, invalid XML characters, count 401 and parcel-count overflow (`1400–1497`), plus exchange-rate cases (`1515–1570`). They do not verify account-side rules or real rendering.
* `item.rs:208–350` checks an official arithmetic example, AAM, explicit scales, negative midpoint and overflows. It does not test every special code's legal meaning or the current gross-first recipe. An exact mathematical invariant alone is not a server-storage invariant (P60).
* `types.rs:1096–1206` covers representative value parsing/normalization and currency exponents, not exhaustive current language/VAT/currency completeness. The VAT token normalization test's older “string-matches” comment (`1115–1117`) is stronger than the P60 evidence; production rustdoc correctly calls normalization hygiene.
* `tests/upstream.rs:138–214,943–979` reconstructs the cached invoice example with explicit allowed deviations. Its outline comparison ignores empty elements and trims text (`1091–1098,1151–1158`); it is neither XSD validation nor a proof of empty/absent equivalence. The cached example predates `simpleItems`.
* `tests/literals.rs:22–98` proves external struct construction, not protocol compliance. `tests/client.rs:65–98,151–170` proves multipart operation selection and refusing empty items before HTTP using a local mock; inspected, not rerun for this audit.
* `tests/live.rs:46–132,157–274` covers HUF invoice/proforma lifecycles and the P73 appearance cases. **Not run**. Its introductory wording about kind combinations/empty-vs-omitted behavior is broader than its actual scenarios. The broader A–D/P48/P60/XPRB evidence is recorded in the behavior document, with raw historical logs explicitly outside the repository; this audit did not independently replay those probes.

### Negative results and confidence boundary

No missing seller/buyer/item/ledger/waybill element, wrong supported-field order, wrong required-container presence, wrong language token, missing currently documented VAT token, decimal-to-double lexical failure, attachment naming error, or ordinary creation-kind flag error was found. `simpleItems` is the sole missing current schema element. Decimal rather than binary float is a compatible finite-number representation, and civil dates serialize in the documented date form; exotic XSD-only date/timezone and infinity/NaN representations are not documented invoice features that the crate must expose.

This is a **current documentation and offline conformance audit**, not certification of live-account rendering, emails, accounting treatment, NAV delivery, carrier behavior or account features. The marked uncertainties are intentionally unresolved rather than converted into unsupported bugs. In particular, current inline versus downloaded `simpleItems` ordering and explicit `fizetve=false` semantics need new primary evidence before a confident behavioral change.

[invoice]: ../../../../crates/szamlazz-agent/src/ops/invoice.rs
[waybill]: ../../../../crates/szamlazz-agent/src/ops/waybill.rs
[item]: ../../../../crates/szamlazz-agent/src/item.rs
[types]: ../../../../crates/szamlazz-agent/src/types.rs
[sources]: ../../../../fixtures/SOURCES.md
[behaviour]: ../../../szamlazz-hu-behaviour.md
[adr8]: ../../../adr/0008-agent-request-types-are-plain-data.md
[S01]: https://docs.szamlazz.hu/agent/generating_invoice/xml
[S02]: https://docs.szamlazz.hu/hu/agent/generating_invoice/xml
[S03]: https://docs.szamlazz.hu/agent/generating_invoice/xsd
[S04]: https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd
[S05]: https://docs.szamlazz.hu/agent/generating_invoice/request
[S06]: https://docs.szamlazz.hu/agent/generating_invoice/settings-and-rules
[S07]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/document-types
[S08]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/travel-agency
[S09]: https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/travel-agency
[S10]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/vat-rates
[S11]: https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/vat-rates
[S12]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/rounding
[S13]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/currencies
[S14]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/invoice-template
[S15]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/order-number
[S16]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/discount
[S17]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification
[S18]: https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/data-erasure-code
[S19]: https://docs.szamlazz.hu/agent/category/basics
[S20]: https://docs.szamlazz.hu/agent/basics/sending-requests
[S21]: https://docs.szamlazz.hu/agent/basics/authentication
[S22]: https://docs.szamlazz.hu/agent/basics/error-handling
[S23]: https://docs.szamlazz.hu/agent/generating_invoice/important-information
[S24]: https://docs.szamlazz.hu/agent/generating_invoice/response
[S25]: https://tudastar.szamlazz.hu/gyik/milyen-afakulcsokat-fogad-be-a-nav-online-szamla-rendszere
[S26]: https://www.szamlazz.hu/wp-content/uploads/2025/11/AFA-kulcsok_NOSZ-UFI-segedlet_2025-11-04.pdf
[S27]: https://tudastar.szamlazz.hu/gyik/adattorlo-kod-hasznalata-apin-keresztul-es-tomeges-szamlageneralaskor
[S28]: https://tudastar.szamlazz.hu/gyik/szamlaertesito-egyedi-mezok
