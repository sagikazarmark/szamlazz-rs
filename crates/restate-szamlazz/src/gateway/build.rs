//! The pure projection of a [`DocumentInput`] plus the account's defaults and
//! seller block into the Agent's [`CreateInvoice`] (design §6 step 0).

use std::str::FromStr as _;

use rust_decimal::Decimal;
use szamlazz_agent::ops::invoice::{
    Buyer, CreateInvoice, ExchangeRate, InvoiceHeader, InvoiceKind, InvoiceTemplate,
};
use szamlazz_agent::{ArithmeticError, Currency, InvoiceNumber, Language};

use super::Gateway;
use crate::contract::{DocumentInput, IssuedKind};
use crate::identity::{ExternalId, OrderKey, normalize_buyer_name};

/// The documents a create refers to, by number.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DocumentRefs<'a> {
    /// The proforma the document consumes (`dijbekeroSzamlaszam`); carried by
    /// [`IssuedKind::Invoice`], [`IssuedKind::Prepayment`] and
    /// [`IssuedKind::Final`] — the three kinds the Agent lets carry the
    /// reference — and ignored by the others.
    pub proforma: Option<&'a str>,
    /// The prepayment a final invoice settles (`elolegSzamlaszam`); required
    /// for [`IssuedKind::Final`].
    pub prepayment: Option<&'a str>,
    /// The invoice a corrective corrects (`helyesbitettSzamlaszam`); required
    /// for [`IssuedKind::Corrective`].
    pub corrected: Option<&'a str>,
}

/// A [`DocumentInput`] that cannot be projected into a create request.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InputError {
    /// The document has no line items.
    #[error("at least one line item is required")]
    NoItems,
    /// The language token is not one szamlazz.hu accepts.
    #[error("unknown document language {0:?}")]
    UnknownLanguage(String),
    /// A non-HUF currency without an exchange rate and without an automatic
    /// MNB lookup configured.
    #[error(
        "currency {0} requires an exchange rate: set overrides.exchange_rate or configure defaults.exchange_rate_bank = \"MNB\""
    )]
    MissingExchangeRate(String),
    /// An exchange rate without a rate names a bank other than MNB.
    #[error("an exchange rate without a rate is only automatic for MNB, not {0:?}")]
    InvalidExchangeRate(String),
    /// A kind-specific reference is missing.
    #[error("a {kind} requires a {reference} reference")]
    MissingReference {
        /// The document kind.
        kind: IssuedKind,
        /// The missing reference.
        reference: &'static str,
    },
    /// A line item's net, VAT or gross value does not fit a decimal; named by
    /// its position in `items`, as the caller's JSON has it.
    #[error("items[{index}]: {source}")]
    ItemOverflow {
        /// The zero-based position of the item in `items`.
        index: usize,
        /// Which value overflowed.
        #[source]
        source: ArithmeticError,
    },
}

impl Gateway {
    /// Builds the create request for `document`: the account's defaults,
    /// per-call overrides (override wins), line totals computed for the
    /// currency, the buyer name normalised, the order number and external id
    /// set, no PDF.
    ///
    /// Pure: no I/O and no clock. `issue_date` is passed through as the caller
    /// pinned it (or left unset so the server dates the document).
    ///
    /// # Errors
    ///
    /// See [`InputError`].
    pub fn build_create(
        &self,
        kind: IssuedKind,
        document: &DocumentInput,
        order: &OrderKey,
        external_id: &ExternalId,
        refs: DocumentRefs<'_>,
    ) -> Result<CreateInvoice, InputError> {
        let defaults = &self.account.defaults;
        let overrides = &document.overrides;

        if document.items.is_empty() {
            return Err(InputError::NoItems);
        }
        let language_token = overrides.language.as_deref().unwrap_or(&defaults.language);
        let language = Language::from_str(language_token)
            .map_err(|_| InputError::UnknownLanguage(language_token.to_owned()))?;
        let currency = Currency::new(overrides.currency.as_deref().unwrap_or(&defaults.currency));
        let exchange_rate = if currency.is_huf() {
            None
        } else {
            Some(match overrides.exchange_rate.clone() {
                Some(input) => {
                    if input.rate.is_none() && input.bank != "MNB" {
                        return Err(InputError::InvalidExchangeRate(input.bank));
                    }
                    ExchangeRate::from(input)
                }
                None if defaults.exchange_rate_bank == "MNB" => ExchangeRate::automatic_mnb(),
                None => return Err(InputError::MissingExchangeRate(currency.to_string())),
            })
        };
        let proforma_number = refs.proforma.map(InvoiceNumber::new);
        let invoice_kind = match kind {
            IssuedKind::Proforma => InvoiceKind::Proforma,
            IssuedKind::Invoice => InvoiceKind::Invoice { proforma_number },
            IssuedKind::Prepayment => InvoiceKind::Prepayment { proforma_number },
            IssuedKind::Final => InvoiceKind::Final {
                prepayment_number: Some(InvoiceNumber::new(refs.prepayment.ok_or(
                    InputError::MissingReference {
                        kind,
                        reference: "prepayment",
                    },
                )?)),
                proforma_number,
            },
            IssuedKind::Corrective => InvoiceKind::Corrective {
                corrected_number: InvoiceNumber::new(refs.corrected.ok_or(
                    InputError::MissingReference {
                        kind,
                        reference: "corrected invoice",
                    },
                )?),
            },
        };

        let header = InvoiceHeader {
            issue_date: document.issue_date,
            comment: document.comment.clone(),
            exchange_rate,
            order_number: Some(order.as_str().to_owned()),
            extra_logo: defaults.extra_logo.clone(),
            number_prefix: overrides
                .number_prefix
                .clone()
                .or_else(|| defaults.number_prefix.clone()),
            paid: document.paid,
            template: overrides
                .template
                .as_deref()
                .or(defaults.template.as_deref())
                .map(template),
            ..InvoiceHeader::new(
                document.fulfillment_date,
                document.due_date,
                document.payment_method.clone().into(),
                currency.clone(),
                language,
            )
        };

        let buyer = Buyer {
            name: normalize_buyer_name(&document.buyer.name),
            send_email: overrides.send_email.or(defaults.send_email),
            ..Buyer::from(document.buyer.clone())
        };

        let items = document
            .items
            .iter()
            .enumerate()
            .map(|(index, item)| {
                item.to_line_item(&currency)
                    .map_err(|source| InputError::ItemOverflow { index, source })
            })
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CreateInvoice {
            e_invoice: overrides.e_invoice.unwrap_or(defaults.e_invoice),
            download_pdf: false,
            aggregator: defaults.aggregator.clone(),
            guardian: defaults.guardian,
            external_id: Some(external_id.as_str().to_owned()),
            seller: self.account.seller.to_seller(),
            ..CreateInvoice::new(invoice_kind, header, buyer, items)
        })
    }
}

/// The gross total of a built create request: the sum of its line items'
/// gross values, or `None` when the sum does not fit a decimal. Checked, like
/// every arithmetic on caller input — a panic here would run on the SDK's
/// connection task and take every in-flight invocation on it down with the
/// request (#64).
#[must_use]
pub fn gross_total(create: &CreateInvoice) -> Option<Decimal> {
    create
        .items
        .iter()
        .try_fold(Decimal::ZERO, |sum, item| sum.checked_add(item.gross_value))
}

/// Maps a template token — the wire value (`SzlaMost`) or the
/// [`InvoiceTemplate`] variant in snake case (`most`) — to the template;
/// anything else is passed through verbatim.
fn template(token: &str) -> InvoiceTemplate {
    match token {
        "SzlaMost" | "most" => InvoiceTemplate::Most,
        "SzlaAlap" | "default" => InvoiceTemplate::Default,
        "SzlaNoEnv" | "no_envelope" => InvoiceTemplate::NoEnvelope,
        "Szla8cm" | "eight_centimeter" => InvoiceTemplate::EightCentimeter,
        "SzlaTomb" | "continuous" => InvoiceTemplate::Continuous,
        "SzlaFuvarlevelesAlap" | "delivery_note" => InvoiceTemplate::DeliveryNote,
        other => InvoiceTemplate::Other(other.to_owned()),
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use rust_decimal::dec;
    use serde_json::json;
    use szamlazz_agent::wire::AgentRequest as _;
    use szamlazz_agent::{Credentials, PaymentMethod};

    use super::*;
    use crate::account::{Account, Endpoint};
    use crate::config::{Defaults, SellerConfig};
    use crate::contract::document::tests::sample_document;
    use crate::contract::{DocumentKind, ExchangeRateInput, LineItemInput};

    /// A gateway for the test account with `defaults` and a fixed seller
    /// block, as the prologue would open it.
    fn gateway(defaults: &serde_json::Value) -> Gateway {
        let mut account = Account::new("acct", "acct");
        account.endpoint = Endpoint::parse("http://127.0.0.1:1/").expect("endpoint");
        account.defaults = serde_json::from_value::<Defaults>(defaults.clone()).expect("defaults");
        account.seller = serde_json::from_value::<SellerConfig>(
            json!({"bank": "Bank", "bank_account": "1234", "email": {"subject": "Hi"}}),
        )
        .expect("seller");
        Gateway::open(account, Credentials::agent_key("key")).expect("gateway")
    }

    fn order() -> OrderKey {
        OrderKey::parse("ORD-1").expect("order")
    }

    fn external_id() -> ExternalId {
        ExternalId::new("acct:ORD-1:invoice")
    }

    #[test]
    fn projects_defaults_and_identity() {
        let gateway = gateway(&json!({
            "e_invoice": true,
            "send_email": false,
            "number_prefix": "WEB",
            "extra_logo": "logo",
            "aggregator": "agg",
            "guardian": true,
            "template": "SzlaMost",
        }));
        let mut document = sample_document();
        document.buyer.name = "  Kova\u{301}cs Bt.  ".to_owned();
        document.issue_date = Some(date(2026, 9, 3));
        document.paid = true;
        let create = gateway
            .build_create(
                IssuedKind::Invoice,
                &document,
                &order(),
                &external_id(),
                DocumentRefs {
                    proforma: Some("D-1"),
                    ..DocumentRefs::default()
                },
            )
            .expect("build");

        assert_eq!(
            create.kind,
            InvoiceKind::Invoice {
                proforma_number: Some(InvoiceNumber::new("D-1"))
            }
        );
        assert!(create.e_invoice);
        assert!(!create.download_pdf);
        assert_eq!(create.aggregator.as_deref(), Some("agg"));
        assert_eq!(create.guardian, Some(true));
        assert_eq!(create.external_id.as_deref(), Some("acct:ORD-1:invoice"));
        assert_eq!(create.header.order_number.as_deref(), Some("ORD-1"));
        assert_eq!(create.header.issue_date, Some(date(2026, 9, 3)));
        assert_eq!(create.header.fulfillment_date, date(2026, 7, 4));
        assert_eq!(create.header.due_date, date(2026, 7, 12));
        assert_eq!(create.header.payment_method, PaymentMethod::Transfer);
        assert_eq!(create.header.currency, Currency::HUF);
        assert_eq!(create.header.language, Language::Hungarian);
        assert_eq!(create.header.comment.as_deref(), Some("thanks"));
        assert_eq!(create.header.exchange_rate, None);
        assert_eq!(create.header.extra_logo.as_deref(), Some("logo"));
        assert_eq!(create.header.number_prefix.as_deref(), Some("WEB"));
        assert!(create.header.paid);
        assert_eq!(create.header.template, Some(InvoiceTemplate::Most));
        assert_eq!(create.buyer.name, "Kovács Bt.", "trimmed and NFC");
        assert_eq!(create.buyer.send_email, Some(false));
        assert_eq!(create.buyer.email.as_deref(), Some("buyer@example.com"));
        assert_eq!(create.seller.bank.as_deref(), Some("Bank"));
        assert_eq!(
            create
                .seller
                .email
                .as_ref()
                .and_then(|e| e.subject.as_deref()),
            Some("Hi")
        );
        assert_eq!(create.items.len(), 1);
        assert_eq!(create.items[0].net_value, dec!(20000));
        assert_eq!(create.items[0].gross_value, dec!(25400));
        assert_eq!(gross_total(&create), Some(dec!(25400)));
        create
            .to_wire(&Credentials::agent_key("key"))
            .expect("valid request");
    }

    #[test]
    fn overrides_win_over_defaults() {
        let gateway = gateway(&json!({
            "e_invoice": true,
            "send_email": true,
            "number_prefix": "WEB",
            "template": "SzlaMost",
            "language": "hu",
        }));
        let mut document = sample_document();
        document.overrides.e_invoice = Some(false);
        document.overrides.send_email = Some(false);
        document.overrides.number_prefix = Some("SHOP".to_owned());
        document.overrides.template = Some("no_envelope".to_owned());
        document.overrides.language = Some("en".to_owned());
        document.overrides.currency = Some("EUR".to_owned());
        document.overrides.exchange_rate = Some(ExchangeRateInput {
            bank: "OTP".to_owned(),
            rate: Some(dec!(395.5)),
        });
        let create = gateway
            .build_create(
                IssuedKind::Proforma,
                &document,
                &order(),
                &external_id(),
                DocumentRefs::default(),
            )
            .expect("build");
        assert_eq!(create.kind, InvoiceKind::Proforma);
        assert!(!create.e_invoice);
        assert_eq!(create.buyer.send_email, Some(false));
        assert_eq!(create.header.number_prefix.as_deref(), Some("SHOP"));
        assert_eq!(create.header.template, Some(InvoiceTemplate::NoEnvelope));
        assert_eq!(create.header.language, Language::English);
        assert_eq!(create.header.currency, Currency::EUR);
        assert_eq!(
            create.header.exchange_rate,
            Some(ExchangeRate::new("OTP", dec!(395.5)))
        );
        assert_eq!(create.items[0].net_value, dec!(20000));
    }

    #[test]
    fn non_huf_needs_a_rate_unless_mnb_is_automatic() {
        let mnb = gateway(&json!({"currency": "EUR"}));
        let document = sample_document();
        let create = mnb
            .build_create(
                IssuedKind::Invoice,
                &document,
                &order(),
                &external_id(),
                DocumentRefs::default(),
            )
            .expect("automatic MNB");
        assert_eq!(
            create.header.exchange_rate,
            Some(ExchangeRate::automatic_mnb())
        );

        let otp = gateway(&json!({"currency": "EUR", "exchange_rate_bank": "OTP"}));
        assert_eq!(
            otp.build_create(
                IssuedKind::Invoice,
                &document,
                &order(),
                &external_id(),
                DocumentRefs::default(),
            ),
            Err(InputError::MissingExchangeRate("EUR".to_owned()))
        );
        let mut without_rate = document.clone();
        without_rate.overrides.exchange_rate = Some(ExchangeRateInput {
            bank: "OTP".to_owned(),
            rate: None,
        });
        assert_eq!(
            otp.build_create(
                IssuedKind::Invoice,
                &without_rate,
                &order(),
                &external_id(),
                DocumentRefs::default(),
            ),
            Err(InputError::InvalidExchangeRate("OTP".to_owned()))
        );
    }

    #[test]
    fn kind_references_and_input_errors() {
        let gateway = gateway(&json!({}));
        let document = sample_document();
        let build =
            |kind, refs| gateway.build_create(kind, &document, &order(), &external_id(), refs);
        assert_eq!(
            build(IssuedKind::Prepayment, DocumentRefs::default())
                .expect("prepayment")
                .kind,
            InvoiceKind::prepayment()
        );
        assert_eq!(
            build(IssuedKind::Final, DocumentRefs::default()),
            Err(InputError::MissingReference {
                kind: IssuedKind::Final,
                reference: "prepayment",
            })
        );
        assert_eq!(
            build(
                IssuedKind::Final,
                DocumentRefs {
                    prepayment: Some("ES-1"),
                    ..DocumentRefs::default()
                }
            )
            .expect("final")
            .kind,
            InvoiceKind::Final {
                prepayment_number: Some(InvoiceNumber::new("ES-1")),
                proforma_number: None,
            }
        );
        assert_eq!(
            build(IssuedKind::Corrective, DocumentRefs::default()),
            Err(InputError::MissingReference {
                kind: IssuedKind::Corrective,
                reference: "corrected invoice",
            })
        );
        assert_eq!(
            build(
                IssuedKind::Corrective,
                DocumentRefs {
                    corrected: Some("SZ-1"),
                    ..DocumentRefs::default()
                }
            )
            .expect("corrective")
            .kind,
            InvoiceKind::Corrective {
                corrected_number: InvoiceNumber::new("SZ-1")
            }
        );

        let mut empty = document.clone();
        empty.items.clear();
        assert_eq!(
            gateway.build_create(
                IssuedKind::Invoice,
                &empty,
                &order(),
                &external_id(),
                DocumentRefs::default(),
            ),
            Err(InputError::NoItems)
        );
        let mut klingon = document;
        klingon.overrides.language = Some("tlh".to_owned());
        assert_eq!(
            gateway.build_create(
                IssuedKind::Invoice,
                &klingon,
                &order(),
                &external_id(),
                DocumentRefs::default(),
            ),
            Err(InputError::UnknownLanguage("tlh".to_owned()))
        );
        assert_eq!(IssuedKind::from(DocumentKind::Final), IssuedKind::Final);
    }

    /// The proforma reference rides on every kind the Agent lets carry it —
    /// the invoice, the prepayment invoice (#69) and the final invoice — as
    /// `dijbekeroSzamlaszam`: what step 2 linked is what the create carries.
    /// The other kinds ignore it.
    #[test]
    fn proforma_reference_rides_on_the_kinds_the_agent_lets_carry_it() {
        let gateway = gateway(&json!({}));
        let document = sample_document();
        let refs = DocumentRefs {
            proforma: Some("D-1"),
            prepayment: Some("ES-1"),
            corrected: Some("SZ-1"),
        };
        let kind_of = |kind| {
            gateway
                .build_create(kind, &document, &order(), &external_id(), refs)
                .expect("build")
                .kind
        };
        let proforma_number = Some(InvoiceNumber::new("D-1"));

        assert_eq!(
            kind_of(IssuedKind::Invoice),
            InvoiceKind::Invoice {
                proforma_number: proforma_number.clone(),
            }
        );
        assert_eq!(
            kind_of(IssuedKind::Prepayment),
            InvoiceKind::Prepayment {
                proforma_number: proforma_number.clone(),
            }
        );
        assert_eq!(
            kind_of(IssuedKind::Final),
            InvoiceKind::Final {
                prepayment_number: Some(InvoiceNumber::new("ES-1")),
                proforma_number,
            }
        );
        for kind in [IssuedKind::Proforma, IssuedKind::Corrective] {
            assert_eq!(kind_of(kind).proforma_number(), None, "{kind}");
        }
    }

    #[test]
    fn overflowing_line_item_is_an_input_error_naming_the_item() {
        let gateway = gateway(&json!({}));
        let mut document = sample_document();
        document.items.push(LineItemInput::new(
            "adversarial",
            dec!(10),
            "db",
            Decimal::MAX,
            "27",
        ));
        let error = gateway
            .build_create(
                IssuedKind::Invoice,
                &document,
                &order(),
                &external_id(),
                DocumentRefs::default(),
            )
            .expect_err("overflow");
        assert_eq!(
            error,
            InputError::ItemOverflow {
                index: 1,
                source: ArithmeticError::NetOverflow,
            }
        );
        assert_eq!(
            error.to_string(),
            "items[1]: line item net value (unit price × quantity) overflows a decimal"
        );
    }

    /// A built request's total is summed with checked arithmetic: two items
    /// that fit on their own but not together are `None`, never a panic
    /// (#64, J8 — a panic on the connection task would tear down every
    /// in-flight invocation on it).
    #[test]
    fn gross_total_of_items_that_overflow_together_is_none_not_a_panic() {
        let gateway = gateway(&json!({}));
        let mut document = sample_document();
        // Each item's own arithmetic fits: 1 × (MAX / 2) at 0 % VAT.
        let half = Decimal::MAX / dec!(2);
        document.items = vec![
            LineItemInput::new("a", dec!(1), "db", half, "0"),
            LineItemInput::new("b", dec!(1), "db", half, "0"),
            LineItemInput::new("c", dec!(1), "db", half, "0"),
        ];
        let create = gateway
            .build_create(
                IssuedKind::Invoice,
                &document,
                &order(),
                &external_id(),
                DocumentRefs::default(),
            )
            .expect("each item fits on its own");
        assert_eq!(gross_total(&create), None);
    }

    #[test]
    fn template_tokens() {
        assert_eq!(template("SzlaAlap"), InvoiceTemplate::Default);
        assert_eq!(template("default"), InvoiceTemplate::Default);
        assert_eq!(template("Szla8cm"), InvoiceTemplate::EightCentimeter);
        assert_eq!(template("continuous"), InvoiceTemplate::Continuous);
        assert_eq!(template("delivery_note"), InvoiceTemplate::DeliveryNote);
        assert_eq!(
            template("SzlaCustom"),
            InvoiceTemplate::Other("SzlaCustom".to_owned())
        );
    }
}
