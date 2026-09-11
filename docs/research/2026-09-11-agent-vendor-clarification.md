# Számla Agent: response identity, validity, URL encoding and effective schemas

**Status:** send-ready Hungarian draft, not sent; no vendor answer received.
Updated 2026-09-11 after the [whole-crate review](../review/2026-09-11-agent-api-eec57fc.md)
and [later adjudication](../review/2026-09-11-agent-api-28dcec1-adjudication.md),
retaining the credit/PDF and preview/schema questions and extending the draft
with storno identity, taxpayer validity, customer-URL encoding and paid-state semantics.
This is the current message for these topics, superseding the corresponding
questions in the [earlier question list](2026-09-10-agent-vendor-questions.md)
and [credit-success brief](2026-09-10-credit-entry-success-question.md).

## Küldendő üzenet

**Tárgy: Számla Agent – sikeres válaszok, URL-kódolás, fizetettség és irányadó XSD-k**

Tisztelt Számlázz.hu Ügyfélszolgálat!

A Számla Agent integrációjához az alábbi szerződésbeli pontosításokat szeretnénk kérni
a 2026. szeptember 11-én elérhető dokumentáció alapján.

**1. Számlaszám a sikeres befizetés-rögzítés válaszában**

Az `action-szamla_agent_kifiz` művelet `valaszVerzio=2` válaszában garantált-e
**minden sikeres kérésnél** a nem üres, nem pusztán szóközt, tabulátort vagy
sortörést tartalmazó számlaszám
legalább az egyik helyen: az XML `xmlszamlavalasz/szamlaszam` elemében vagy a
`szlahu_szamlaszam` HTTP-fejlécben (URL-dekódolás után)?

A [válasz dokumentációjában](https://docs.szamlazz.hu/hu/agent/credit_entry/response)
a `szamlaszam` opcionális, és további fejlécek „érkezhetnek”; a séma ugyanakkor
sikeres és sikertelen válaszokat is leír. Megengedett-e HTTP 200 mellett,
számlaszámot és hibát jelző fejléc nélkül például ez a teljes válasz?

```xml
<xmlszamlavalasz xmlns="http://www.szamlazz.hu/xmlszamlavalasz">
  <sikeres>true</sikeres>
</xmlszamlavalasz>
```

Kérjük, térjenek ki az additív és a felülíró rögzítésre, valamint a szándékos
üres felülírásra is (`additiv=false`, egyetlen `kifizetes` elem nélkül), meglévő
befizetésekkel rendelkező és már üres számlán. Ezt az alakot a
[kérés sémája](https://docs.szamlazz.hu/hu/agent/credit_entry/xml) megengedi;
siker esetén valóban az összes korábbi befizetés törlését igazolja?
Ha számlaszám nélkül is lehet sikeres a válasz, önmagában a `sikeres=true`
véglegesen igazolja-e a kért művelet befejezését, akár a nettó/bruttó összeg és
a kintlévőség hiányában is? Ha a számlaszám garantált, kérjük ezt kifejezetten
a sikeres v2 válaszokra dokumentálni.

2026. szeptember 11-én egy tesztfiókon az üres felülírást mindkét állapoton
ellenőriztük: a meglévő befizetés eltűnt, az üres számla üres maradt, és mindkét
válaszban megkaptuk a várt számlaszámot és a teljes bruttó összegnek megfelelő
kintlévőséget. Ezért elsősorban az általános válaszgarancia pontosítását kérjük.

**1/b. Számlaszám a sikeres PDF-lekérdezés válaszában**

Ugyanez a garancia érvényes-e az `action-szamla_agent_pdf` művelet sikeres
`valaszVerzio=2` válaszaira, akár számlaszám, rendelésszám vagy külső azonosító
alapján kérdezünk? A [PDF-válasz dokumentációja](https://docs.szamlazz.hu/hu/agent/querying_pdf/response)
itt is opcionális `szamlaszam` elemet és esetlegesen érkező fejléceket ír le.
Lehet-e sikeres válasz érvényes, base64-kódolt PDF-fel úgy, hogy egyik csatorna
sem tartalmaz nem üres számlaszámot? Kérjük, a sikeres PDF-válaszokra vonatkozó
feltételt is rögzítsék, külön a sikeres és sikertelen válaszokat egyaránt leíró XSD-től.

**1/c. Számlaszám a sikeres sztornóválaszban**

Az `action-szamla_agent_st` művelet sikeres `valaszVerzio=2` válaszában is
garantált-e a nem üres, nem pusztán szóközt, tabulátort vagy sortörést tartalmazó
számlaszám legalább az XML `szamlaszam` elemében vagy a dekódolt
`szlahu_szamlaszam` fejlécben? A [sztornóválasz dokumentációja](https://docs.szamlazz.hu/hu/agent/reversing_invoice/response)
itt is opcionális elemet és esetlegesen érkező fejléceket ír le.

Lehet-e teljes, sikeres válasz a fenti, csak `sikeres=true` elemet tartalmazó
XML, számlaszám nélkül mindkét csatornán? Ha igen, pontosan mit igazol a sikerjel,
és hogyan azonosítható az eredeti számlához tartozó sztornóbizonylat? Kérjük,
különítsék el az új sztornó létrehozását, a már sztornózott számlára ismételt
kérést és a sztornózható számlának nem minősülő célbizonylat esetét. Korábbi
tesztfiókos megfigyelésünkben díjbekérő és szállítólevél sztornókérése az eredeti
szám változatlan visszaadásával, `sikeres=true` mellett nem hozott létre sztornót;
ezért a sikerjelet önmagában nem tekintjük azonosított sztornóbizonylatnak.

**2. Az éles feldolgozó által elfogadott elemsorrend és az irányadó sémák**

A számlakérés [magyar](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml)
és [angol](https://docs.szamlazz.hu/agent/generating_invoice/xml) oldali XSD-jében
a fejléc vége `szamlaSablon → simpleItems → elonezetpdf`, míg a
[letölthető XSD-ben](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd)
`szamlaSablon → elonezetpdf → simpleItems`.

Ha mindkét opcionális elem jelen van, melyik sorrendet fogadja el a feldolgozó?
Ez az explicit `false` értékekre is érvényes? A `simpleItems=true` és
`elonezetpdf=true` együtt biztosan csak előnézetet készít, számla kiállítása nélkül?

Emellett az alábbi, az oldali sémákban szereplő mezők hiányoznak a letöltésekből:

| Művelet | Oldali séma szerinti mező | Eltérő letölthető séma |
|---|---|---|
| [Számla létrehozása](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml) | `vevo/csoportazonosito`, `tetelek/tetel/torloKod` | [xmlszamla.xsd](https://www.szamlazz.hu/szamla/docs/xsds/agent/xmlszamla.xsd) |
| [Nyugta létrehozása](https://docs.szamlazz.hu/hu/agent/generating_receipt/xml) | `tetelek/tetel/torloKod` | [xmlnyugtacreate.xsd](https://www.szamlazz.hu/szamla/docs/xsds/nyugtacreate/xmlnyugtacreate.xsd) |
| [Nyugta lekérdezése](https://docs.szamlazz.hu/hu/agent/querying_receipt/xml) | `rendelesSzam` (a `nyugtaszam` helyett választható azonosító) | [xmlnyugtaget.xsd](https://www.szamlazz.hu/szamla/docs/xsds/nyugtaget/xmlnyugtaget.xsd) |

Kérjük, erősítsék meg e mezők támogatását és pontos helyét, valamint adják meg
a feldolgozóhoz tartozó aktuális, teljes XSD-k irányadó URL-jét/verzióját, és
egyeztessék az oldali és letölthető változatokat. Jelenleg egyik számlaséma sem
fedi le egyszerre a másikban szereplő mezőket és sorrendet.

**3. Adózólekérdezés: `OK` válasz hiányzó `taxpayerValidity` mellett**

Az [adózólekérdezés válaszleírása](https://docs.szamlazz.hu/agent/querying_taxpayer/response)
a NAV válaszformátumára hivatkozik. A NAV 2.0 és 3.0 sémáiban a
`taxpayerValidity` opcionális; a hivatkozott
[NAV 3.0 specifikáció](https://onlineszamla.nav.gov.hu/files/container/download/Online_Szamla_interfesz%20specifikacio_HU_v3.0.pdf)
nyomtatott 67. oldalának táblázata sem kötelezőként jelöli, miközben a következő
oldal szabálya létező adószámra `true`, érvénytelen vagy nem létező adószámra
`false` értéket ír le.

Adhat-e a Számla Agent `QueryTaxpayerResponse` választ `result/funcCode=OK`
értékkel, de `taxpayerValidity` nélkül? Ha igen, mi a hiány jelentése: a
feldolgozás sikeres, de az érvényességről nincs közölt adat, vagy más állapot?
Lehetnek-e ilyenkor adózóadatok vagy diagnosztikai üzenetek a válaszban?
Kérjük, különítsék el ezt az explicit `false` és a hibás lekérdezés esetét,
és adjanak teljes, anonimizált példát a támogatott névterekkel. Ha `OK` mellett
mindig kötelező az érvényesség, kérjük ezt sikerfeltételként dokumentálni.
A hiányt nem szeretnénk sem `false`, sem igazolt érvényességként értelmezni.

**4. A `szlahu_vevoifiokurl` fejléc kódolása**

A [számlaválasz fejlécleírása](https://docs.szamlazz.hu/agent/generating_invoice/response)
vevői fiók URL-ként nevezi meg ezt az értéket, de nem határozza meg a külső
kódolását. A fejléc már közvetlenül használható URL, egyszer százalékkódolt URL,
vagy űrlapkódolású érték, ahol a `+` szóközt jelent? Hány dekódolási lépés
szükséges, és hogyan különül el a fejléc külső kódolása az URL útvonalának és
lekérdezési paramétereinek saját százalékkódolásától?

Kérjük, adjanak pontos nyers fejléc → használható URL példákat a `+`, `%2B`,
`%20` és `%252B` alakokra, útvonalban és lekérdezési paraméterben egyaránt.
Például az alábbi szintetikus fejlécérték megengedett-e, és ha igen, mi a belőle
előállítandó pontos URL?

```text
https://example.test/a+b/%2B/%20/%252B?q=a+b&plus=%2B&space=%20&escaped=%252B
```

Az XML `vevoifiokurl` elemében ugyanezt az URL-t csak XML-escape-eléssel kell-e
értelmezni, további URL-dekódolás nélkül? Ugyanaz a fejlécszabály érvényes-e
számlakiállításra, sztornóra, befizetés-rögzítésre és PDF-lekérdezésre?

A hivatalos PHP 2.12.4 kliens `InvoiceResponse` osztálya a fejléc beolvasásakor
`rawurldecode`-ot használ, majd a nyilvános `getUserAccountUrl()` getter újabb
`urldecode`-ot végez. Emiatt önmagában az első lépésből nem következtetünk a
helyes dekódolásra; a szerver által előállított formátum meghatározását kérjük.

**5. A `fizetve` elhagyása és az explicit `false` érték**

A [számlakérés sémája](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml)
opcionális boolean elemként, alapértelmezés nélkül írja le a `fizetve` mezőt.
Minden támogatott bizonylatfajtánál, fizetési módnál és fiókbeállításnál
azonos-e a mező elhagyása az explicit `<fizetve>false</fizetve>` küldésével?
Különösen készpénzes fizetési módnál: képes-e az explicit `false` megakadályozni
az automatikus fizetettként kezelést, és ha igen, milyen beállítás mellett?

Kérjük, dokumentálják az elhagyás alapértelmezett jelentését és az esetleges
eltéréseket az elhagyott, `false` és `true` alak között. Eltérés esetén kérünk
összehasonlítható kéréseket és válaszokat, valamint az ezekből lekérdezhető
kintlévőséget és befizetési adatokat. Nem feltételezzük, hogy a klienskönyvtárak
mezőelhagyási gyakorlata önmagában szerveroldali egyenértékűséget jelent.

Köszönjük a segítséget!

## Internal context and next evidence

### What API calls can establish

An actual call can establish acceptance and returned fields for that account,
request and date, or disprove a universal claim with a counterexample. A series
of numbered successes cannot prove an always-numbered success contract; an
accepted request does not identify an authoritative published XSD. Those
guarantees and the omission/encoding semantics above still need a vendor answer.
A combined-preview experiment must also verify non-issuance rather than assuming
the preview flag was honored.

Receipt lifecycle, omitted-rate MNB and email-resend probes can establish bounded
observations. Email acknowledgements alone do not establish inbox delivery.
The operator confirmed the intended locally configured test account and supplied
an inbox. No existing receipt prefix was available. The new prefix `RSPRB` was
accepted without UI registration after a seven-character candidate was refused
with code 337 (at most five characters). The [dated receipt record](2026-09-11-receipts-live.md)
captures lifecycle/MNB success, the code-153 immediate-resend refusal and delayed
exact-number recovery with verified cleanup. The operator subsequently confirmed
both emails arrived; exact content/attachment equality was not separately checked.
That record also reports 338 for repetition of a completed, verified create,
with the same original found afterwards. Only 337 is among the thirteen #195
additions; 338 predates them. The other twelve additions and 55/56 remain
unobserved in the recorded probes. These are transcribed observations without
archived raw response channels; no continuity with the historical invoice-probe
account, concurrent deduplication or call-id retention guarantee is inferred.

The linked public pages/downloads were retrieved again on 2026-09-11: the
ordering conflict and omitted declarations remain. This is documentation
verification, not new live evidence. Details: [mutations Q1](../review/2026-09-11-agent-api-61c334f-mutations.md#q1--p3-clarification-successful-credit-acknowledgement-requires-an-undocumented-nonblank-echo),
[invoice U-01/U-02](../review/2026-09-11-agent-api-61c334f-invoices.md#source-conflicts-and-unresolved-questions),
[receipt source comparison](../review/2026-09-11-agent-api-61c334f-receipts.md).
The invoice report also records official PHP 2.12.4 using preview-before-simple;
the message relies on the directly linked conflicting schemas.

At the reviewed `28dcec1` baseline, `RegisterCreditEntry` and `ClearCreditEntries`
shared a parser requiring a nonblank reported number.
[Two clearing probes passed](2026-09-11-credit-clearing-live.md)
on an operator-confirmed test account on 2026-09-11, both with the expected
reported number. No live numberless success or universal echo guarantee is
established. An uncertain reply is not permission to repeat a
write; the requested number must not be presented as a vendor-reported echo.

### Approved response work and unresolved guarantees

The approved upcoming optional-fact changes are local contract-support decisions,
not vendor answers or claims that the missing-fact replies were observed:

- Credit registration/clearing will retain a successful acknowledgement with an
  optional **reported** invoice number and balance metadata. PDF retrieval will
  retain a fetched PDF with optional reported number; request provenance is
  separate and must never be substituted as an echo.
- Storno response work will distinguish an unnumbered acknowledgement from a
  numbered result. The acknowledgement is neither preview nor a verified
  reversal; reconciliation must establish the exact original and matching
  reversal. The numbered `CreatedInvoice` invariant is not vendor proof of an
  always-numbered reply.
- Taxpayer `OK` with omitted validity will retain unreported validity explicitly,
  alongside the verdict and available data/diagnostics. Omission is neither
  false nor verified validity; malformed booleans and absent/invalid `funcCode`
  remain distinct from an omitted optional fact.
- Explicit `fizetve=false` will be representable separately from omission, with
  omission retained as the default. This does not establish different paid-state
  effects; no omission-versus-false comparison was executed.

Implementation belongs to separate work; this document records the approved
direction, not its completion. The code-56 decision remains operation-specific:
accepted numbered issuance-envelope
cases carry the warning on create/storno, while credit/clear retain 56 as an
error. PDF retains its current accepted numbered-56 handling, requires the PDF
and exposes no notification flag while its reported number becomes optional for
ordinary success. Optional identity does not make numberless 56 a success.
No 55/56 execution or success-only identity/validity guarantee is established.

### Customer-URL evidence correction

The [latest adjudication, A7](../review/2026-09-11-agent-api-28dcec1-adjudication.md#a7-customer-url--the-php-comparison-was-incomplete)
traces the complete official PHP 2.12.4 path in `Response/InvoiceResponse.php`:
header assignment at lines 136–137 uses `rawurldecode`, the setter at 354–355
assigns it, and `getUserAccountUrl()` at 347–348 applies `urldecode` again.
This supersedes the incomplete single-`rawurldecode` premise in earlier reviews;
it does not establish a vendor encoding grammar or justify copying double decoding.
Source tracing, not executed PHP, gives this comparison for the header fragments:

| Raw fragment | Rust's current single form decode | PHP header-to-public-getter path |
|---|---|---|
| `+` | space | space |
| `%2B` | `+` | space |
| `%20` | space | space |
| `%252B` | `%2B` | `+` |

No affected vendor URL capture establishes which interpretation is intended.
The support question requests the raw grammar, decoding layers and path/query
examples. A URL-specific decoding change awaits that evidence; number/error
header decoding cannot be inferred from the URL's rule.

### Sending status

To send: choose the sender/contact and submit only the Hungarian section through
the vendor's [support channel](https://www.szamlazz.hu/szamla/kapcsolat?step=4&category=33).
No account identifier, credential or real document sample is needed for this draft.
No available tool can submit that support form. This update made no outbound
contact: the draft is **not sent**, and **no vendor answer has been received**.

The operator authorized the `clear_credit_entries_populated` and
`clear_credit_entries_already_empty` probes; both completed with verified cleanup.
Their dated record above retains run labels and document numbers. These probes
do not capture raw response channels and cannot establish a universal success
guarantee. A combined preview/order probe still needs an agreed request matrix
and execution authorization; none was run here.
