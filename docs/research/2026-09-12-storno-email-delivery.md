# Storno email recipient and delivery — primary-source research (#223)

Researched **2026-09-12** for [#223](https://github.com/sagikazarmark/szamlazz-rs/issues/223).
Primary-source research is followed by a controlled live execution record below.
No mock, XSD check or PHP example establishes delivery.

## Conclusion

**Support an explicit optional storno recipient, with a request-level guarantee
only.** Official storno XML/XSD and the first-party PHP example support
`vevo/email`, including on a paper storno. They do **not** specify that omission
inherits the original invoice's recipient. General Agent guidance instead says
that no email in the request means no notification, but does not expressly settle
the storno-specific inheritance question. There is consequently no evidence-backed
configuration-only promise that the worker's current omission delivers storno
documents to the intended buyer. [S1–S5]

Keep issuance and notification outcomes separate. First-party PHP explicitly
handles **code 56 with a reported invoice number** as successful issuance with
notification failure, including in its storno example. Neither a successful
acknowledgement nor the absence of 56 proves an attempted notification, mailbox
receipt or receipt by the requested buyer. [S5–S7, S11–S12]

## Evidence and its limits

### 1. Omitted versus explicit `vevo/email`

| Case | Established by primary sources | Remaining uncertainty / supported reading |
|---|---|---|
| Entire `vevo` omitted | Both current inline and downloadable storno XSD make `vevo` optional (`minOccurs="0"`). [S1–S2] | Schema acceptance does not define notification behavior or fallback to the original invoice/partner record. |
| `vevo` present, `email` omitted | `email` is optional `xs:string`; no XSD default. [S1–S2] | No storno-specific inheritance rule found. Do not promise either delivery or an explicit suppression switch. |
| Nonblank `vevo/email` | Storno XML example supplies it. PHP's paper-storno example calls `Buyer::setEmail`; the storno branch of `Buyer::buildXmlData` emits it when nonblank. [S1, S5] | This is the documented recipient input. It does not establish exclusivity, bypass of account settings, or actual receipt. |
| Empty/blank `email` | Unrestricted `xs:string` permits empty content structurally; PHP omits blank email. [S1–S2, S5] | Empty, whitespace-only and omitted are not proven equivalent at the server. Avoid assigning empty a special “disable” meaning. |
| `sendEmail` on storno | Neither storno XSD nor PHP's storno buyer writer contains it. PHP emits it only for ordinary invoice creation. [S1–S2, S5] | Do not copy the ordinary-invoice `sendEmail=false` control into a storno request. |

The **general invoice-creation** documentation says a filled buyer email plus
`sendEmail=true` **or omitted** requests notification. Its knowledge-base companion
says explicitly: “ha az XML-ben nem szerepel vevő e-mail címe, akkor a Számla Agent
nem fog számlaértesítő emailt küldeni” — without the buyer email in the XML, Agent
will not send an invoice notification. That companion describes sending `true`,
but does not explain the omission default given by the current Agent page.
Both discuss invoice creation; neither names an original-recipient fallback for
`xmlszamlast`. [S3–S4]

**Inference:** explicit email is the best-supported way to express recipient
intent for storno; relying on omission to reuse an earlier address is unsupported
by the sources inspected. This is not a finding that every omitted-email storno
fails to notify on every account. [S1–S5]

General invoice guidance supports comma-separated multiple addresses. That is
not a storno-specific validation specification. A worker may deliberately support
one validated address initially; document that as its own narrower contract,
rather than asserting that szamlazz.hu accepts only one. The XSD supplies no
email grammar or length bound. [S1–S3]

### 2. Paper/electronic appearance and account settings

- **Paper is compatible with the recipient input.** The official storno XML uses
  `eszamla=false` together with `vevo/email`; PHP constructs
  `ReverseInvoice::INVOICE_TYPE_P_INVOICE` with email. Thus the field is not
  documented as electronic-only. These examples establish intended use, not a
  tested paper-account send matrix. [S1, S5]
- **Electronic does not mean automatically emailed.** The knowledge base warns
  that selecting e-invoice in account settings alone does not send notification.
  That page describes the browser workflow; Agent has its own request-driven
  behavior. Do not turn the browser statement into “Agent never sends
  automatically,” or the electronic flag into a delivery guarantee. [S3–S4, S9]
- **Paper notification setting exists.** The account UI offers
  “Számlaértesítőt küldök (papír alapú számla).” The knowledge base says enabling it
  permits notification immediately after paper invoice creation; otherwise it can
  be sent later from the invoice's notification action. Its Agent subsection
  delegates to the request-driven article. **Whether this UI setting gates,
  defaults or is overridden by an explicit Agent storno email is not specified.**
  Record it in the separate probe; do not claim it is either required or irrelevant
  for storno. [S8]
- **Sender setup, reply-to, BCC and templates are distinct.** The notification
  guide requires selection of an `@szamlazz.hu` sender before the first send,
  describes reply forwarding and an optional configured BCC address. The Agent
  template guide gives subject/body precedence: request XML, customized account
  template, default template. Those are not original-recipient fallback rules;
  BCC also prevents “only this address receives a copy” from being a safe promise.
  Storno's `elado` permits reply-to, subject and body, but the sources do not give
  a complete storno-specific first-send/settings precedence table. [S1–S2, S8, S10]
- **Test accounts redirect notifications.** Both current EN/HU Agent pages and
  the knowledge base say test-account notification goes to the email configured
  in account settings, **not** the XML buyer email. A test-account mailbox result
  must name this routing limitation; it cannot prove production recipient
  selection. These pages state the general Agent rule, without a separate storno
  exception or matrix. [S3–S4]
- **First-party source conflict:** PHP 2.12.4 `Buyer.php:54–58` says test accounts
  send no email for safety. Current Agent prose and the knowledge base expressly
  say they send to the account email. Record the contradiction rather than using
  that PHP comment as a no-mail guarantee. Source code here is client-side,
  not the deployed provider's mail-routing implementation. [S3–S5]

The vendor also distinguishes emailing a paper invoice's image from delivery of
the paper original, and recommends electronic invoices for electronic-only
handling. This records the vendor's product guidance; it does not independently
verify its legal claim against NAV. Appearance and notification remain distinct
decisions. [S8, S13]

### 3. Code 56: issuance may succeed while notification fails

The PHP response documentation explicitly describes successful invoice issuance
with an unsuccessful notification and directs consumers to inspect both
`isSuccess()` and `hasInvoiceNotificationSendError()`. [S6]

Freshly downloaded official PHP **2.12.4** supplies the numeric connection: [S5]

- `src/szamlaagent/Response/InvoiceResponse.php:14–17` defines
  `INVOICE_NOTIFICATION_SEND_FAILED = 56` (“Számlaértesítő kézbesítése sikertelen”).
- `:314–323` makes `isError()` false when both `hasInvoiceNumber()` and
  `hasInvoiceNotificationSendError()` hold. Its comment explicitly says issuance
  succeeded in that case. `:199–200` requires a nonblank reported number;
  `:427–431` tests error code 56.
- `:128–158` reads the number and error from `szlahu_szamlaszam`,
  `szlahu_error` and `szlahu_error_code`; `:165–167` then records success when
  `isNotError()` holds. This demonstrates a client path for number plus warning.
- `examples/document/invoice/create_reverse_invoice.php:50–60` separately checks
  successful **storno** creation and notification failure. This is direct evidence
  that the first-party SDK intends the distinction to apply to storno.

**Documentation gap:** the current EN/HU central error tables skip from 55 to 57;
they do not list 56. The current storno response page says that error-code headers
omit the invoice number/totals, while PHP explicitly accommodates code 56 plus a
number. The response XSD permits `hibakod`, `hibauzenet` and `szamlaszam` together
but defines no code-56-specific success/identity guarantee. No real code-56
response was obtained in this investigation. Do not fabricate its exact
`sikeres` value, body/header combination, or promise that every 56 supplies a
number. [S5–S7]

**Recommended interpretation:** when the existing operation-specific reversal
evidence rules establish issuance, retain that result and expose the provider's
notification-failure warning. A bare 56 without usable identity does not identify
the reversal and grants no new send permission. Contradictory identity remains
uncertainty; adding a notification warning must not weaken those checks.
`notification_delivery_failed=false` can mean only **no such failure reported**,
not “delivered,” “recipient verified,” or even “notification requested.”
This recommendation follows PHP's separation and the response's limited evidence;
the exact worker mapping remains the parent implementation decision. [S5–S7]

### 4. Recipient, send record and actual delivery are different facts

1. **Recipient intent:** email included in the outgoing storno XML. This is what
   input validation and serialization tests can establish. [S1–S2, S5]
2. **Document acknowledgement:** provider acknowledges/identifies a storno. The
   documented response contains no effective-recipient field or affirmative
   mailbox-delivery receipt. A PDF or buyer-account URL is not one. [S7]
3. **Notification activity:** the provider UI exposes earlier sends, counts,
   details and a later resend action. It can also show opening the invoice from
   the last notification, but cannot attribute that opening to a particular
   recipient when there were multiple recipients. These UI facts are additional
   evidence, not fields guaranteed in the storno response. [S11]
4. **Mailbox receipt:** independently observe the correlated message in the
   controlled destination mailbox. The vendor itself documents recipient-side
   spam filtering. A sender/BCC copy is not evidence of receipt by the requested
   buyer, and a missing message after a finite wait does not prove no later
   delivery. No mailbox evidence is claimed here. [S8, S12]

No guarantee of exactly-once notification, delivery deadline, all-recipient
delivery, or complete asynchronous bounce reporting by code 56 was found in the
sources inspected. That is an explicit research limit, not a claim that the
provider has no internal mail tracking.

## Recommended supported caller contract

This is a recommendation for #223, **not an implemented contract change**:

> `buyer_email`, when supplied, is the caller's requested recipient for the
> notification associated with this storno request. The worker validates and
> forwards it as `vevo/email`. When omitted, the worker supplies no recipient;
> it promises neither original-recipient inheritance nor notification suppression.
> Provider settings and test-account routing can affect notification behavior.
> A successful storno result confirms the document outcome under the existing
> evidence policy, not email delivery. A reported notification failure is exposed
> separately and does not undo known issuance.

Recommended consequences:

- Add the same optional, validated input to `Szamlazz.Order.storno_invoice` and
  the applicable unmanaged `Szamlazz.Agent.storno` path. Preserve managed-order
  routing and their distinct execution guarantees. Keep the fulfillment date and
  appearance derived from the verified original per [ADR 0007](../adr/0007-storno-repeats-the-originals-fulfillment-date.md).
- Prefer a nonblank address contract; reject invalid/empty explicit values rather
  than silently converting them to omission. Choose and document any worker
  grammar/length bound independently of XSD permissiveness. [S1–S2]
- Retain the chosen recipient with the original mutation intent across execution,
  replay and recovery. Do not re-resolve it from mutable caller/partner settings
  midway through an unresolved write. This preserves caller intent; it cannot
  freeze provider-side routing or templates.
- Never repeat the storno operation to retry notification, including after a
  warning, a lost answer, reconciliation, or an already-reversed result. Once the
  document is established, use the provider's separately documented notification
  UI/operator process, or a separately designed delivery mechanism for the existing
  document. The UI documents notification resend; it does not instruct users to
  create another storno. [S11]
- Until separate evidence fills the settings matrix, describe the supported
  feature as **explicit recipient forwarding**, not guaranteed automatic storno
  delivery. The present sources justify this capability without proving the
  current omitted-input path is broken. [S1–S5]

## Controlled live execution, 2026-09-12

The operator authorized test-document issuance and email to their controlled Gmail
mailbox, using the existing `.env` credential. Ran exactly once, serially:

```sh
set -a
source .env
set +a
export SZAMLAZZ_STORNO_EMAIL="operator-controlled-address"
SZAMLAZZ_LIVE_RUN_ID=issue223-20260912 cargo test -p szamlazz-agent \
  --all-features --locked --test probes storno_email:: \
  -- --ignored --test-threads=1 --nocapture
```

The recipient value is intentionally absent from this public record. Each case
created a fresh HUF invoice with buyer email present and `sendEmail=false`, queried
`teszt=true` and matching appearance, then sent one storno with the original's
fulfillment date and appearance, a unique external id, and a unique email subject
`Storno email probe {case} {order}`. The omitted cases sent an empty `<vevo>` block,
matching worker omission, not an entirely absent block. Every returned reversal
was queried by exact number and verified as `SS` referencing the exact original;
the freshly queried original was reversed. Stored date and appearance matched.

| Case | Order / subject suffix | Original | Storno | Acknowledgement / warning | Mailbox observation reported by operator |
|---|---|---|---|---|---|
| electronic-explicit | `fed8935c-14e8-4d77-8b35-a1b1459b270d` | E-CTEST-2026-59 | E-CTEST-2026-60 | Numbered; no notification failure reported | Received |
| electronic-omitted | `7c05873f-8157-4421-a5fc-a8e0827c58ae` | E-CTEST-2026-61 | E-CTEST-2026-62 | Numbered; no notification failure reported | Not reported received at observation time |
| paper-explicit | `0498dbea-ef2b-46bc-95bb-33a967609542` | CTEST-2026-19 | CTEST-2026-20 | Numbered; no notification failure reported | Received |
| paper-omitted | `4dda32d9-5e43-4773-811f-2d80e8a5c075` | CTEST-2026-21 | CTEST-2026-22 | Numbered; no notification failure reported | Not reported received at observation time |

All four protocol probes passed in 20 seconds. Every original was verified reversed;
there was no unresolved write or additional cleanup mutation. No repeat storno or
notification retry was performed. These were direct Számla Agent operations;
separate real-Restate/mocked-provider tests establish worker forwarding and replay.

**Evidence limits:** receipt is the operator's explicit in-session report for the
two matching cases, not direct mailbox access or retained message headers. Arrival
timestamps, effective To/BCC and provider send-history records were not collected.
Account sender initialization, paper-notification checkbox, BCC and customized
templates were not inspected or changed. The original and explicit storno used
the same controlled recipient; test-account redirection also prevents testing
production override routing. Missing messages at one observation time do not prove
suppression or exclude delayed delivery. No real code 56 was induced. Thus request
acceptance and reversal are independently verified, mailbox receipt is reported for
explicit cases, and provider delivery-attempt details remain unobserved.

The implemented request-forwarding contract and these limitations are recorded in
[ADR 0017](../adr/0017-explicit-storno-notification-recipient.md).

## Remaining questions for further investigation / vendor

Useful discriminators for a separately authorized investigation:

1. For paper and electronic originals, does a storno with omitted `vevo` inherit
   the original recipient, the current partner email, or neither? Does a present
   empty buyer block differ? Use distinct fresh originals per case so repeat
   storno behavior does not contaminate the comparison.
2. With explicit email different from the original, which effective recipient is
   recorded? Does the paper-notification setting alter the outcome? Record sender
   initialization, test mode, BCC and relevant template/settings state.
3. Test-account redirection means the account mailbox may receive both cases.
   Distinguish evidence of a notification being generated from proof of the
   production recipient-selection rule. If that cannot be observed, request a
   vendor answer and leave it explicitly unresolved.
4. Correlate request acceptance, reported number, verified reversal, provider send
   record and actual mailbox message separately, using identifiers/timestamps.
   Record an unobserved mailbox or absent provider-log access as a blocker.
5. Ask for a real redacted storno code-56 response (headers and body), including
   whether number/`sikeres` are guaranteed and which failures it detects. Does it
   report partial multi-recipient failure or only a synchronous send failure?
6. Ask whether repeating an already-completed storno can send another notification.
   No exactly-once email claim follows from repeat storno returning the same
   document. Recovery must remain read-only regardless of that answer.

## Sources and acquisition

Started from [`fixtures/SOURCES.md`](../../fixtures/SOURCES.md), preserving its
distinction between historical official corpus and synthetic fixtures. The URLs
below were fetched by unauthenticated GET on **2026-09-12**. The docs display
`v202608271632`; that is the site build, not a verified revision of every claim.
The PHP ZIP was read in memory; no PHP example was executed and no Számla Agent
operation or authenticated account-setting request was made. This note retains
source links and selected observations, not full snapshots of the web pages.

| ID | Primary source |
|---|---|
| S1 | [Storno XML and inline XSD (HU)](https://docs.szamlazz.hu/hu/agent/reversing_invoice/xml); also [request](https://docs.szamlazz.hu/hu/agent/reversing_invoice/request). |
| S2 | [Current downloadable storno XSD](https://www.szamlazz.hu/szamla/docs/xsds/agentst/xmlszamlast.xsd). |
| S3 | Invoice notification rules: [HU](https://docs.szamlazz.hu/hu/agent/generating_invoice/settings_and_rules/email-notification), [EN](https://docs.szamlazz.hu/agent/generating_invoice/settings_and_rules/email-notification); [invoice XML](https://docs.szamlazz.hu/hu/agent/generating_invoice/xml). |
| S4 | [Automatic invoicing and automatic notification](https://tudastar.szamlazz.hu/gyik/automatikus-szamlazas-automatikus-ertesito-kuldes). |
| S5 | [Official PHP documentation/download page](https://docs.szamlazz.hu/hu/php/), [storno example page](https://docs.szamlazz.hu/hu/php/sztorno-szamla-generalas), [official PHP 2.12.4 ZIP](https://docs.szamlazz.hu/hu/assets/files/PHPApiAgent-2.12.4-33e323cd64c5ba9601bec272b218f2d1.zip). Paths/line numbers above are relative to `PHPApiAgent-2.12.4/szamlaagent/` in that ZIP. Download SHA-256: `30bdac74e54a653a96d6a71912f56a4f419f2f2946872d388a1150aeb776a741`. |
| S6 | [PHP response processing — special errors](https://docs.szamlazz.hu/hu/php/valasz-feldolgozas#egyedi-hibák-kezelése). |
| S7 | [Storno response and response XSD](https://docs.szamlazz.hu/hu/agent/reversing_invoice/response); central error tables [HU](https://docs.szamlazz.hu/hu/agent/basics/error-handling), [EN](https://docs.szamlazz.hu/agent/basics/error-handling). |
| S8 | [Notification settings: sender, reply-to, paper option, BCC, Agent](https://tudastar.szamlazz.hu/gyik/szamlaertesito-mukodese). |
| S9 | [Selecting electronic invoices alone does not send notification (UI)](https://tudastar.szamlazz.hu/gyik/elektronikus-szamla-szamlaertesito). |
| S10 | [Agent notification subject/body precedence](https://tudastar.szamlazz.hu/gyik/szamlaertesito-targya-es-szovege). |
| S11 | [Notification history, opening information and resend](https://tudastar.szamlazz.hu/gyik/szamlaertesitok-fizetesi-felszolitasok-lekerdezese). |
| S12 | [Recipient-side spam filtering](https://tudastar.szamlazz.hu/gyik/elektronikus-szamla-szamlaertesito-spam). |
| S13 | [Emailing a paper invoice — vendor guidance](https://tudastar.szamlazz.hu/gyik/elkuldhetem-e-mailben-a-papiralapu-szamlat). |

Discovery also used the official [docs sitemap](https://docs.szamlazz.hu/sitemap.xml)
and [knowledge-base sitemap](https://tudastar.szamlazz.hu/sitemap.xml). Guessed
`/hu/agent/reversing_invoice/` and
`/hu/agent/generating_invoice/settings_and_rules/email` returned HTTP 403;
the actual linked pages above were retrieved successfully. These failed guesses
are not evidence that the subject is undocumented.
