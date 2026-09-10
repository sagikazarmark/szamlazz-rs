//! NAV expanded names and parent paths, through `QueryTaxpayer::parse`.
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{ErrorCode, ResponseError};

fn parse(inner: &str) -> Result<szamlazz_agent::ops::taxpayer::TaxpayerInfo, ResponseError> {
    QueryTaxpayer::new("12345678").expect("prefix").parse(&RawResponse::new::<&str, &str>([], format!(r#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api" xmlns:d="http://schemas.nav.gov.hu/OSA/2.0/data" xmlns:f="urn:foreign">{inner}</QueryTaxpayerResponse>"#).into_bytes()))
}

#[test]
fn recognized_singletons_cannot_be_overwritten_or_nested() {
    for inner in [
        "<result><funcCode>ERROR</funcCode><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity>",
        "<result><funcCode>ERROR</funcCode></result><result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity>",
        "<result><funcCode>OK</funcCode></result><taxpayerValidity>false</taxpayerValidity><taxpayerValidity>true</taxpayerValidity>",
        "<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><taxpayerData/><taxpayerData/>",
        "<result><funcCode>OK<foreign/></funcCode></result><taxpayerValidity>true</taxpayerValidity>",
        "<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerName>A&bogus;B</taxpayerName></taxpayerData>",
        "<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><foreign>&bogus;</foreign>",
        "<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><taxpayerData><taxNumberDetail/><taxNumberDetail/></taxpayerData>",
        "<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerName/><taxpayerName>A</taxpayerName></taxpayerData>",
    ] {
        assert!(
            matches!(parse(inner), Err(ResponseError::Parse(_))),
            "{inner}"
        );
    }
}

#[test]
fn unknown_or_foreign_paths_cannot_supply_a_verdict_or_business_data() {
    for result in [
        "<foreign><result><funcCode>OK</funcCode></result></foreign>",
        "<f:result><funcCode>OK</funcCode></f:result>",
        "<result><f:funcCode>OK</f:funcCode></result>",
        "<funcCode>OK</funcCode>",
    ] {
        assert!(
            parse(&format!(
                "{result}<taxpayerValidity>true</taxpayerValidity>"
            ))
            .is_err()
        );
    }
    let info = parse("<result><funcCode> OK </funcCode></result><taxpayerValidity> 1 </taxpayerValidity><taxpayerName>root</taxpayerName><foreign><taxpayerData><taxpayerName>hidden</taxpayerName></taxpayerData></foreign><taxpayerData><f:taxpayerName>foreign</f:taxpayerName><taxNumberDetail><taxpayerId>wrong namespace</taxpayerId></taxNumberDetail></taxpayerData><taxpayerAddressItem/>").expect("sparse success");
    assert!(info.valid);
    assert_eq!(info.name, None);
    assert_eq!(info.tax_number, None);
    assert!(info.addresses.is_empty());
}

#[test]
fn text_decodes_without_losing_characters_and_addresses_are_independent() {
    let info = parse("<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerName> A&amp;B<![CDATA[ <C>]]>&#xA0;&#13;\r\n<!--comment--><?pi ok?>Z </taxpayerName><taxNumberDetail><d:taxpayerId> 12345678 </d:taxpayerId><d:vatCode>&#160;</d:vatCode></taxNumberDetail><taxpayerAddressList><taxpayerAddressItem><taxpayerAddressType> HQ </taxpayerAddressType><taxpayerAddress><d:city> First </d:city><d:region> \t\r\n </d:region></taxpayerAddress></taxpayerAddressItem><taxpayerAddressItem><taxpayerAddressType/><taxpayerAddress><d:city>Second</d:city><d:postalCode>42</d:postalCode></taxpayerAddress></taxpayerAddressItem></taxpayerAddressList></taxpayerData>").expect("decoded");
    assert_eq!(info.name.as_deref(), Some(" A&B <C>\u{a0}\r\nZ "));
    assert_eq!(info.tax_number.as_deref(), Some(" 12345678 "));
    assert_eq!(info.vat_code.as_deref(), Some("\u{a0}"));
    assert_eq!(info.addresses.len(), 2);
    assert_eq!(info.addresses[0].kind.as_deref(), Some(" HQ "));
    assert_eq!(info.addresses[0].city.as_deref(), Some(" First "));
    assert_eq!(info.addresses[0].region, None);
    assert_eq!(info.addresses[0].postal_code, None);
    assert_eq!(info.addresses[1].kind, None);
    assert_eq!(info.addresses[1].city.as_deref(), Some("Second"));
    assert_eq!(info.addresses[1].postal_code.as_deref(), Some("42"));
}

#[test]
fn sparse_errors_and_empty_content_keep_their_meaning() {
    let error = parse("<result><funcCode> ERROR </funcCode><errorCode> 57 </errorCode><message> padded </message></result>").expect_err("error");
    assert!(
        matches!(error, ResponseError::Api(api) if api.code == ErrorCode::MalformedXml && api.message == " padded ")
    );
    assert!(
        matches!(parse("<result><funcCode>ERROR</funcCode></result>"), Err(ResponseError::Api(api)) if api.code == ErrorCode::Absent)
    );
    assert!(!parse("<result><funcCode>OK</funcCode></result><taxpayerValidity>false</taxpayerValidity><taxpayerData/>").expect("not valid").valid);
    assert!(parse("<result><funcCode>OK</funcCode></result><taxpayerValidity/>").is_err());
}

#[test]
fn address_singletons_are_checked_per_item() {
    for inner in [
        "<taxpayerAddress/><taxpayerAddress/>",
        "<taxpayerAddressType/><taxpayerAddressType>HQ</taxpayerAddressType>",
        "<taxpayerAddress><d:city/><d:city>B</d:city></taxpayerAddress>",
    ] {
        let body = format!(
            "<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerAddressList><taxpayerAddressItem>{inner}</taxpayerAddressItem></taxpayerAddressList></taxpayerData>"
        );
        assert!(matches!(parse(&body), Err(ResponseError::Parse(_))));
    }
}

#[test]
fn error_followed_by_a_second_ok_document_is_malformed() {
    let body = concat!(
        "<QueryTaxpayerResponse xmlns=\"http://schemas.nav.gov.hu/OSA/2.0/api\"><result><funcCode>ERROR</funcCode></result></QueryTaxpayerResponse>",
        "<QueryTaxpayerResponse xmlns=\"http://schemas.nav.gov.hu/OSA/2.0/api\"><result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity></QueryTaxpayerResponse>"
    );
    assert!(matches!(
        QueryTaxpayer::new("12345678")
            .expect("prefix")
            .parse(&RawResponse::new::<&str, &str>(
                [],
                body.as_bytes().to_vec()
            )),
        Err(ResponseError::Parse(_))
    ));
}

#[test]
fn nav_3_uses_common_result_and_base_components_with_arbitrary_prefixes() {
    let body = include_str!("synthetic/taxpayer_v3.xml");
    let request = QueryTaxpayer::new("12345678").expect("prefix");
    let info = request
        .parse(&RawResponse::new::<&str, &str>(
            [],
            body.as_bytes().to_vec(),
        ))
        .expect("NAV 3");
    assert!(info.valid);
    assert_eq!(info.name.as_deref(), Some("SYNTHETIC SOFTWARE KFT."));
    assert_eq!(info.tax_number.as_deref(), Some("12345678"));
    assert_eq!(info.addresses[0].city.as_deref(), Some("TESTVAROS"));
    for wrong in [
        body.replace("<common:result>", "<api:result>")
            .replace("</common:result>", "</api:result>"),
        body.replace("common:funcCode", "api:funcCode"),
    ] {
        assert!(
            request
                .parse(&RawResponse::new::<&str, &str>([], wrong.into_bytes()))
                .is_err()
        );
    }
}
