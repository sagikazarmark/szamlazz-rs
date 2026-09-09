//! The carrier waybill block (`fuvarlevel`) of an invoice, the one part of
//! the invoice request only the delivery-note use needs: the block itself and
//! the four carriers' own sub-blocks. Re-exported from
//! [`ops::invoice`](crate::ops::invoice), where it is a field of
//! [`CreateInvoice`](crate::ops::invoice::CreateInvoice).

use rust_decimal::Decimal;

/// Trans-O-Flex carrier data (`fuvarlevel` / `tof`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct TransOFlex {
    /// Carrier-provided five-digit identifier (`azonosito`).
    pub id: Option<String>,
    /// Shipment identifier (`shipmentID`).
    pub shipment_id: Option<String>,
    /// Number of parcels (`csomagszam`).
    pub parcel_count: Option<u32>,
    /// Destination country code (`countryCode`).
    pub country_code: Option<String>,
    /// Destination ZIP code (`zip`).
    pub zip: Option<String>,
    /// Service code (`service`).
    pub service: Option<String>,
}

/// Pick Pack Pont carrier data (`fuvarlevel` / `ppp`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct PickPackPoint {
    /// Barcode prefix (`vonalkodPrefix`).
    pub barcode_prefix: Option<String>,
    /// Per-invoice barcode suffix (`vonalkodPostfix`).
    pub barcode_suffix: Option<String>,
}

/// Sprinter carrier data (`fuvarlevel` / `sprinter`).
#[derive(Debug, Clone, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Sprinter {
    /// Agreed carrier identifier (`azonosito`).
    pub id: Option<String>,
    /// Sender code (`feladokod`).
    pub sender_code: Option<String>,
    /// Routing code (`iranykod`).
    pub routing_code: Option<String>,
    /// Number of parcels (`csomagszam`).
    pub parcel_count: Option<u32>,
    /// Per-invoice barcode suffix (`vonalkodPostfix`).
    pub barcode_suffix: Option<String>,
    /// Delivery-time text (`szallitasiIdo`).
    pub delivery_time: Option<String>,
}

/// MPL carrier data (`fuvarlevel` / `mpl`).
///
/// Three of its fields are required by the XSD, so there is no `Default`:
/// build it with [`Mpl::new`] and extend the result with functional update.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Mpl {
    /// MPL customer code (`vevokod`).
    pub customer_code: String,
    /// Barcode source (`vonalkod`).
    pub barcode: String,
    /// Parcel weight (`tomeg`).
    pub weight: String,
    /// Extra-service icon configuration (`kulonszolgaltatasok`).
    pub extra_services: Option<String>,
    /// Declared value (`erteknyilvanitas`).
    pub declared_value: Option<Decimal>,
}

impl Mpl {
    /// Creates MPL data with the three XSD-required fields.
    pub fn new(
        customer_code: impl Into<String>,
        barcode: impl Into<String>,
        weight: impl Into<String>,
    ) -> Self {
        Self {
            customer_code: customer_code.into(),
            barcode: barcode.into(),
            weight: weight.into(),
            extra_services: None,
            declared_value: None,
        }
    }
}

/// Optional carrier waybill block (`fuvarlevel`).
#[doc(alias = "fuvarlevel")]
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
pub struct Waybill {
    /// Legacy destination (`uticel`).
    pub destination: Option<String>,
    /// Carrier service token (`futarSzolgalat`).
    pub carrier: Option<String>,
    /// General barcode (`vonalkod`).
    pub barcode: Option<String>,
    /// Waybill comment (`megjegyzes`).
    pub comment: Option<String>,
    /// Trans-O-Flex details (`tof`).
    pub trans_o_flex: Option<TransOFlex>,
    /// Pick Pack Pont details (`ppp`).
    pub pick_pack_point: Option<PickPackPoint>,
    /// Sprinter details (`sprinter`).
    pub sprinter: Option<Sprinter>,
    /// MPL details (`mpl`).
    pub mpl: Option<Mpl>,
}

impl Waybill {
    /// The parcel counts the block carries (Trans-O-Flex's, Sprinter's), for
    /// the XSD `int` range check.
    pub(crate) fn parcel_counts(&self) -> impl Iterator<Item = u32> {
        [
            self.trans_o_flex
                .as_ref()
                .and_then(|carrier| carrier.parcel_count),
            self.sprinter
                .as_ref()
                .and_then(|carrier| carrier.parcel_count),
        ]
        .into_iter()
        .flatten()
    }

    /// Writes the block in XSD order.
    pub(crate) fn write(&self, w: &mut crate::xml::Element<'_>) {
        w.text_opt("uticel", self.destination.as_deref());
        w.text_opt("futarSzolgalat", self.carrier.as_deref());
        w.text_opt("vonalkod", self.barcode.as_deref());
        w.text_opt("megjegyzes", self.comment.as_deref());
        if let Some(tof) = &self.trans_o_flex {
            w.node("tof", |c| {
                c.text_opt("azonosito", tof.id.as_deref());
                c.text_opt("shipmentID", tof.shipment_id.as_deref());
                if let Some(count) = tof.parcel_count {
                    c.text("csomagszam", &count.to_string());
                }
                c.text_opt("countryCode", tof.country_code.as_deref());
                c.text_opt("zip", tof.zip.as_deref());
                c.text_opt("service", tof.service.as_deref());
            });
        }
        if let Some(ppp) = &self.pick_pack_point {
            w.node("ppp", |c| {
                c.text_opt("vonalkodPrefix", ppp.barcode_prefix.as_deref());
                c.text_opt("vonalkodPostfix", ppp.barcode_suffix.as_deref());
            });
        }
        if let Some(sprinter) = &self.sprinter {
            w.node("sprinter", |c| {
                c.text_opt("azonosito", sprinter.id.as_deref());
                c.text_opt("feladokod", sprinter.sender_code.as_deref());
                c.text_opt("iranykod", sprinter.routing_code.as_deref());
                if let Some(count) = sprinter.parcel_count {
                    c.text("csomagszam", &count.to_string());
                }
                c.text_opt("vonalkodPostfix", sprinter.barcode_suffix.as_deref());
                c.text_opt("szallitasiIdo", sprinter.delivery_time.as_deref());
            });
        }
        if let Some(mpl) = &self.mpl {
            w.node("mpl", |c| {
                c.text("vevokod", &mpl.customer_code);
                c.text("vonalkod", &mpl.barcode);
                c.text("tomeg", &mpl.weight);
                c.text_opt("kulonszolgaltatasok", mpl.extra_services.as_deref());
                if let Some(value) = mpl.declared_value {
                    c.decimal("erteknyilvanitas", value);
                }
            });
        }
    }
}
