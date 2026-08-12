//! `szamlazz taxpayer` — NAV taxpayer lookup.

use clap::Args;
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;

use crate::output;

/// Arguments for `taxpayer`.
#[derive(Debug, Args)]
pub struct TaxpayerArgs {
    /// The first 8 digits of the tax number (törzsszám).
    tax_number_prefix: String,
}

/// Runs the taxpayer lookup.
pub async fn run(cli: &crate::Cli, args: &TaxpayerArgs) -> anyhow::Result<()> {
    let client = crate::client(cli)?;
    let info = client
        .send(&QueryTaxpayer::new(&args.tax_number_prefix)?)
        .await?;

    if cli.json {
        return output::json(&info);
    }
    output::field_required("Valid", &info.valid);
    output::field("Name", info.name.as_ref());
    output::field("Tax number", info.tax_number.as_ref());
    output::field("VAT code", info.vat_code.as_ref());
    for address in &info.addresses {
        let parts: Vec<&str> = [
            address.kind.as_deref(),
            address.country_code.as_deref(),
            address.postal_code.as_deref(),
            address.city.as_deref(),
            address.street_name.as_deref(),
            address.public_place_category.as_deref(),
            address.number.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect();
        println!("{:<22} {}", "Address", parts.join(" "));
    }

    Ok(())
}
