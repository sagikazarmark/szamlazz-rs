//! NAV expanded names and parent paths, through `QueryTaxpayer::parse`.
use szamlazz_agent::ops::taxpayer::QueryTaxpayer;
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{ErrorCode, ResponseError};

#[test]
fn info_date_is_advisory_source_text_not_a_datetime() {
    for (source, expected) in [
        ("2004-12-26T23:00:00.000Z", Some("2004-12-26T23:00:00.000Z")),
        (
            "2026-09-10T01:02:03+02:00",
            Some("2026-09-10T01:02:03+02:00"),
        ),
        (
            "2026-09-10T01:02:03-05:00",
            Some("2026-09-10T01:02:03-05:00"),
        ),
        ("2026-09-10T01:02:03", Some("2026-09-10T01:02:03")),
        (
            "2026-09-10T01:02:03.12345678901234567890",
            Some("2026-09-10T01:02:03.12345678901234567890"),
        ),
        ("2026-09-10T24:00:00", Some("2026-09-10T24:00:00")),
        (" not-a-date é中 ", Some(" not-a-date é中 ")),
        (" A&amp;B<![CDATA[<C>]]>&#160; ", Some(" A&B<C>\u{a0} ")),
        ("&#160;", Some("\u{a0}")),
        ("", None),
        (" \t\r\n ", None),
    ] {
        let info = parse(&format!("<result><funcCode>OK</funcCode></result><infoDate>{source}</infoDate><taxpayerValidity>true</taxpayerValidity>")).expect("useful lookup");
        assert!(info.valid);
        assert_eq!(info.info_date.as_deref(), expected, "{source}");
    }
}

#[test]
fn new_business_fields_use_only_their_versioned_paths() {
    use szamlazz_agent::ops::taxpayer::TaxpayerInfo;
    fn absent(info: &TaxpayerInfo) {
        assert_eq!(info.short_name, None);
        assert_eq!(info.county_code, None);
        assert_eq!(info.vat_group_membership, None);
        assert_eq!(info.incorporation, None);
        assert_eq!(info.info_date, None);
    }
    for data in [
        "",
        "<taxpayerShortName>wrong root</taxpayerShortName><d:countyCode>02</d:countyCode><vatGroupMembership>12345678</vatGroupMembership><incorporation>ORGANIZATION</incorporation>",
        "<f:infoDate>foreign</f:infoDate><foreign><infoDate>hidden</infoDate><taxpayerData><taxpayerShortName>hidden</taxpayerShortName><incorporation>ORGANIZATION</incorporation><vatGroupMembership>12345678</vatGroupMembership><taxNumberDetail><d:countyCode>02</d:countyCode></taxNumberDetail></taxpayerData></foreign>",
        "<taxpayerData><infoDate>wrong parent</infoDate><f:taxpayerShortName>foreign</f:taxpayerShortName><f:incorporation>ORGANIZATION</f:incorporation><f:vatGroupMembership>12345678</f:vatGroupMembership><taxNumberDetail><countyCode>wrong namespace</countyCode></taxNumberDetail></taxpayerData>",
        "<infoDate/><taxpayerData><taxpayerShortName> \t </taxpayerShortName><incorporation/><vatGroupMembership/><taxNumberDetail><d:countyCode/></taxNumberDetail></taxpayerData>",
    ] {
        absent(&parse(&format!("<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity>{data}")).expect("sparse data"));
    }
    let old: TaxpayerInfo = serde_json::from_str(
        r#"{"valid":true,"name":"Known","tax_number":"12345678","vat_code":"2","addresses":[]}"#,
    )
    .expect("old JSON");
    absent(&old);

    let body = include_str!("synthetic/taxpayer_v3.xml");
    let wrong = body
        .replace("api:taxpayerShortName", "base:taxpayerShortName")
        .replace("base:countyCode", "api:countyCode")
        .replace("api:vatGroupMembership", "common:vatGroupMembership")
        .replace("api:incorporation", "base:incorporation")
        .replace("api:infoDate", "common:infoDate");
    absent(
        &QueryTaxpayer::new("12345678")
            .expect("prefix")
            .parse(&RawResponse::new::<&str, &str>([], wrong.into_bytes()))
            .expect("sparse NAV 3"),
    );
}

#[test]
fn incorporation_tokens_are_open_in_v3_and_tolerated_as_a_v2_extension() {
    use szamlazz_agent::ops::taxpayer::{Incorporation, TaxpayerInfo};
    for (token, expected) in [
        ("ORGANIZATION", Incorporation::Organization),
        ("SELF_EMPLOYED", Incorporation::SelfEmployed),
        ("TAXABLE_PERSON", Incorporation::TaxablePerson),
        ("FUTURE_KIND", Incorporation::Other("FUTURE_KIND".into())),
    ] {
        // Incorporation is declared only in NAV 3.0. This sparse 2.0 sample
        // checks extension tolerance, not schema conformance.
        let v2 = format!(
            "<result><funcCode>OK</funcCode></result><taxpayerValidity>true</taxpayerValidity><taxpayerData><taxpayerShortName> Short </taxpayerShortName><taxNumberDetail><d:countyCode>02</d:countyCode></taxNumberDetail><vatGroupMembership>87654321</vatGroupMembership><incorporation>{token}</incorporation></taxpayerData>"
        );
        let v3 = include_str!("synthetic/taxpayer_v3.xml").replace("ORGANIZATION", token);
        for info in [
            parse(&v2).expect("NAV 2"),
            QueryTaxpayer::new("12345678")
                .expect("prefix")
                .parse(&RawResponse::new::<&str, &str>([], v3.into_bytes()))
                .expect("NAV 3"),
        ] {
            assert_eq!(info.incorporation, Some(expected.clone()));
            assert_eq!(info.county_code.as_deref(), Some("02"));
            assert_eq!(info.vat_group_membership.as_deref(), Some("87654321"));
            let json = serde_json::to_value(&info).expect("JSON");
            assert_eq!(json["incorporation"], token);
            assert_eq!(
                serde_json::from_value::<TaxpayerInfo>(json).expect("roundtrip"),
                info
            );
        }
        assert_eq!(
            token.parse::<Incorporation>().expect("open token"),
            expected
        );
        assert_eq!(expected.to_string(), token);
    }
}

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
    assert_eq!(info.short_name.as_deref(), Some("SYNTHETIC KFT."));
    assert_eq!(info.county_code.as_deref(), Some("02"));
    assert_eq!(info.vat_group_membership.as_deref(), Some("87654321"));
    assert_eq!(
        info.incorporation,
        Some(szamlazz_agent::ops::taxpayer::Incorporation::Organization)
    );
    assert_eq!(
        info.info_date.as_deref(),
        Some("2026-09-10T12:34:56.123456789012+02:00")
    );
    assert_eq!(info.addresses.len(), 2);
    assert_eq!(info.addresses[1].city.as_deref(), Some("MASIKVAROS"));
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
