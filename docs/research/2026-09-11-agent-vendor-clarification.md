# Számla Agent: successful response identity and effective schemas

**Status:** send-ready Hungarian draft, not sent; no vendor answer received.
Updated 2026-09-11 after the [whole-crate review](../review/2026-09-11-agent-api-eec57fc.md), including the PDF-query success guarantee.
This is the current message for these two topics, superseding the corresponding
questions in the [earlier question list](2026-09-10-agent-vendor-questions.md)
and [credit-success brief](2026-09-10-credit-entry-success-question.md).

## Küldendő üzenet

**Tárgy: Számla Agent – sikeres befizetés/PDF-válasz számlaszáma és az irányadó XSD-k**

Tisztelt Számlázz.hu Ügyfélszolgálat!

A Számla Agent integrációjához két szerződésbeli pontosítást szeretnénk kérni
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
Lehet-e sikeres válasz érvényes, base64-kódolt PDF-fel, de nem üres számlaszám
nélkül mindkét csatornán? Kérjük, a sikeres PDF-válaszokra vonatkozó feltételt
is rögzítsék, külön a sikeres és sikertelen válaszokat egyaránt leíró XSD-től.

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

Köszönjük a segítséget!

## Internal context and next evidence

### What API calls can establish

An actual call can establish acceptance and returned fields for that account,
request and date, or disprove a universal claim with a counterexample. A series
of numbered successes cannot prove an always-numbered success contract; an
accepted request does not identify an authoritative published XSD. Those two
guarantees still need a vendor answer. A combined-preview experiment must also
verify non-issuance rather than assuming the preview flag was honored.

Receipt lifecycle, omitted-rate MNB and email-resend probes can establish bounded
observations. Email acknowledgements alone do not establish inbox delivery.
The operator confirmed the intended locally configured test account and supplied
an inbox. No existing receipt prefix was available. The new prefix `RSPRB` was
accepted without UI registration after a seven-character candidate was refused
with code 337 (at most five characters). The [dated receipt record](2026-09-11-receipts-live.md)
captures lifecycle/MNB success, the code-153 immediate-resend refusal and delayed
exact-number recovery with verified cleanup. The operator subsequently confirmed
both emails arrived; exact content/attachment equality was not separately checked.

The linked public pages/downloads were retrieved again on 2026-09-11: the
ordering conflict and omitted declarations remain. This is documentation
verification, not new live evidence. Details: [mutations Q1](../review/2026-09-11-agent-api-61c334f-mutations.md#q1--p3-clarification-successful-credit-acknowledgement-requires-an-undocumented-nonblank-echo),
[invoice U-01/U-02](../review/2026-09-11-agent-api-61c334f-invoices.md#source-conflicts-and-unresolved-questions),
[receipt source comparison](../review/2026-09-11-agent-api-61c334f-receipts.md).
The invoice report also records official PHP 2.12.4 using preview-before-simple;
the message relies on the directly linked conflicting schemas.

`RegisterCreditEntry` and `ClearCreditEntries` share a parser requiring a nonblank
reported number. [Two clearing probes passed](2026-09-11-credit-clearing-live.md)
on an operator-confirmed test account on 2026-09-11, both with the expected
reported number. No live numberless success or universal echo guarantee is
established. An uncertain reply is not permission to repeat a
write; the requested number must not be presented as a vendor-reported echo.

To send: choose the sender/contact and submit only the Hungarian section through
the vendor's [support channel](https://www.szamlazz.hu/szamla/kapcsolat?step=4&category=33).
No account identifier, credential or real document sample is needed for this draft.

The operator authorized the `clear_credit_entries_populated` and
`clear_credit_entries_already_empty` probes; both completed with verified cleanup.
Their dated record above retains run labels and document numbers. These probes
do not capture raw response channels and cannot establish a universal success
guarantee. A combined preview/order probe still needs an agreed request matrix
and execution authorization; none was run here.
