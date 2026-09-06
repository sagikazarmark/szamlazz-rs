//! Request types are plain data: a caller outside this crate can build them
//! as struct literals and extend a constructor's result with functional
//! update, instead of assigning field by field on a `new()` value.

use jiff::civil::date;
use rust_decimal::dec;
use szamlazz_agent::ops::credit_entry::{CreditEntries, CreditEntry, RegisterCreditEntry};
use szamlazz_agent::ops::invoice::{
    Buyer, CreateInvoice, InvoiceHeader, InvoiceKind, PostalAddress, Seller, SellerEmail,
};
use szamlazz_agent::ops::receipt::{ReceiptEmail, SendReceipt};
use szamlazz_agent::ops::storno::StornoInvoice;
use szamlazz_agent::{Currency, Language, LineItem, LineItemLedger, PaymentMethod, VatRate};

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "the assign-after-construct form is what the literal is compared against"
)]
fn a_create_invoice_extends_its_constructor_with_functional_update() {
    let header = InvoiceHeader::new(
        date(2026, 7, 4),
        date(2026, 7, 12),
        PaymentMethod::Transfer,
        Currency::HUF,
        Language::Hungarian,
    );
    let buyer = Buyer::new("Kovács Bt.", "2030", "Érd", "Tárnoki út 23.");
    let item = LineItem::calculated_for_currency(
        "Fejlesztés",
        dec!(1),
        "db",
        dec!(10000),
        VatRate::percent(27),
        &Currency::HUF,
    );

    let literal = CreateInvoice {
        external_id: Some("shop:ORD-1:invoice".to_owned()),
        e_invoice: true,
        header: InvoiceHeader {
            order_number: Some("ORD-1".to_owned()),
            paid: true,
            ..header.clone()
        },
        seller: Seller {
            bank: Some("Bank".to_owned()),
            email: Some(SellerEmail {
                subject: Some("Your invoice".to_owned()),
                ..SellerEmail::default()
            }),
            ..Seller::default()
        },
        buyer: Buyer {
            postal_address: Some(PostalAddress {
                city: Some("Budapest".to_owned()),
                ..PostalAddress::default()
            }),
            ..buyer.clone()
        },
        items: vec![LineItem {
            comment: Some("first row".to_owned()),
            ledger: Some(LineItemLedger {
                revenue_account: Some("911".to_owned()),
                ..LineItemLedger::default()
            }),
            ..item.clone()
        }],
        ..CreateInvoice::new(
            InvoiceKind::invoice(),
            header.clone(),
            buyer.clone(),
            Vec::new(),
        )
    };

    let mut assigned = CreateInvoice::new(InvoiceKind::invoice(), header, buyer, vec![item]);
    assigned.external_id = Some("shop:ORD-1:invoice".to_owned());
    assigned.e_invoice = true;
    assigned.header.order_number = Some("ORD-1".to_owned());
    assigned.header.paid = true;
    assigned.seller.bank = Some("Bank".to_owned());
    let mut email = SellerEmail::default();
    email.subject = Some("Your invoice".to_owned());
    assigned.seller.email = Some(email);
    let mut postal_address = PostalAddress::default();
    postal_address.city = Some("Budapest".to_owned());
    assigned.buyer.postal_address = Some(postal_address);
    assigned.items[0].comment = Some("first row".to_owned());
    let mut ledger = LineItemLedger::default();
    ledger.revenue_account = Some("911".to_owned());
    assigned.items[0].ledger = Some(ledger);

    assert_eq!(literal, assigned);
}

#[test]
#[expect(
    clippy::field_reassign_with_default,
    reason = "the assign-after-construct form is what the literal is compared against"
)]
fn by_number_operations_are_literals_over_their_constructors() {
    let storno = StornoInvoice {
        comment: Some("wrong buyer".to_owned()),
        download_pdf: true,
        ..StornoInvoice::new("E-TST-2026-1")
    };
    let mut assigned_storno = StornoInvoice::new("E-TST-2026-1");
    assigned_storno.comment = Some("wrong buyer".to_owned());
    assigned_storno.download_pdf = true;
    assert_eq!(storno, assigned_storno);

    let entry = CreditEntry {
        description: Some("wire".to_owned()),
        ..CreditEntry::new(date(2026, 7, 5), PaymentMethod::Transfer, dec!(12700))
    };
    let register = RegisterCreditEntry {
        additive: true,
        entries: CreditEntries::try_from(vec![entry.clone()]).expect("one entry"),
        ..RegisterCreditEntry::new("E-TST-2026-1")
    };
    let mut assigned_register = RegisterCreditEntry::new("E-TST-2026-1");
    assigned_register.additive = true;
    assigned_register.entries = CreditEntries::try_from(vec![entry]).expect("one entry");
    assert_eq!(register, assigned_register);

    let send = SendReceipt {
        receipt_number: "NYGTA-2026-1".into(),
        email: Some(ReceiptEmail {
            to: Some("buyer@example.com".to_owned()),
            ..ReceiptEmail::default()
        }),
    };
    let mut assigned_send = SendReceipt::new("NYGTA-2026-1");
    let mut email = ReceiptEmail::default();
    email.to = Some("buyer@example.com".to_owned());
    assigned_send.email = Some(email);
    assert_eq!(send, assigned_send);
}
