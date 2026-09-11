//! Synthetic NAV 2/3 parser controls, not captured vendor replies or XSD validation.
use szamlazz_agent::ops::taxpayer::{
    QueryTaxpayer, TaxpayerHeader, TaxpayerInfo, TaxpayerNotification, TaxpayerSoftware,
};
use szamlazz_agent::wire::{AgentRequest, RawResponse};
use szamlazz_agent::{ErrorCode, ParseError, ResponseError};

fn response(version: u8, inner: &str) -> RawResponse {
    let common = if version == 2 {
        "http://schemas.nav.gov.hu/OSA/2.0/api"
    } else {
        "http://schemas.nav.gov.hu/NTCA/1.0/common"
    };
    RawResponse::new::<&str, &str>([], format!(
        r#"<a:QueryTaxpayerResponse xmlns:a="http://schemas.nav.gov.hu/OSA/{version}.0/api" xmlns:c="{common}" xmlns:f="urn:foreign">{inner}</a:QueryTaxpayerResponse>"#
    ).into_bytes())
}

fn parse(version: u8, inner: &str) -> Result<TaxpayerInfo, ResponseError> {
    QueryTaxpayer::new("12345678")
        .expect("prefix")
        .parse(&response(version, inner))
}

const OK: &str = "<c:result><c:funcCode>OK</c:funcCode></c:result>";

#[test]
fn validity_preserves_absent_true_and_false_in_both_layouts() {
    for version in [2, 3] {
        for (element, expected) in [
            ("", None),
            ("<a:taxpayerValidity>true</a:taxpayerValidity>", Some(true)),
            (
                "<a:taxpayerValidity>false</a:taxpayerValidity>",
                Some(false),
            ),
            (
                "<a:taxpayerValidity> \t1\r\n </a:taxpayerValidity>",
                Some(true),
            ),
            ("<a:taxpayerValidity>0</a:taxpayerValidity>", Some(false)),
            ("<f:taxpayerValidity>invalid</f:taxpayerValidity>", None),
            (
                "<a:unknown><a:taxpayerValidity>true</a:taxpayerValidity></a:unknown>",
                None,
            ),
        ] {
            let info = parse(version, &format!("{OK}{element}<a:taxpayerData><a:taxpayerName>Known</a:taxpayerName></a:taxpayerData>"))
                .expect("successful lookup even when validity is unreported");
            assert_eq!(info.valid, expected);
            assert_eq!(info.name.as_deref(), Some("Known"));
            let json = serde_json::to_value(&info).expect("serialize");
            assert_eq!(json["valid"], serde_json::json!(expected));
            assert_eq!(
                serde_json::from_value::<TaxpayerInfo>(json).expect("roundtrip"),
                info
            );
        }
        for element in [
            "<a:taxpayerValidity/>",
            "<a:taxpayerValidity></a:taxpayerValidity>",
            "<a:taxpayerValidity> \t\r\n </a:taxpayerValidity>",
            "<a:taxpayerValidity>invalid</a:taxpayerValidity>",
            "<a:taxpayerValidity>TRUE</a:taxpayerValidity>",
            "<a:taxpayerValidity>&#160;true&#160;</a:taxpayerValidity>",
        ] {
            assert!(
                matches!(
                    parse(version, &format!("{OK}{element}")),
                    Err(ResponseError::Parse(ParseError::Invalid {
                        field: "taxpayerValidity",
                        ..
                    }))
                ),
                "NAV {version}: {element}"
            );
        }
    }
}

#[test]
fn both_layouts_preserve_header_software_and_success_diagnostics() {
    for version in [2, 3] {
        // Notifications are a declared NAV 3 result field, a tolerated extension in NAV 2.
        let info = parse(version, r"
            <c:header><c:requestId> R&amp;1 </c:requestId>
                <c:timestamp>not-a-date é中</c:timestamp>
                <c:requestVersion>future</c:requestVersion><c:headerVersion>1.0</c:headerVersion>
            </c:header>
            <c:result><c:funcCode> OK </c:funcCode><c:errorCode> 0057 </c:errorCode>
                <c:message> A&amp;B<![CDATA[<C>]]>&#160; </c:message>
                <c:notifications>
                    <c:notification><c:notificationCode>FUTURE_CODE</c:notificationCode><c:notificationText> First </c:notificationText></c:notification>
                    <c:notification/>
                    <c:notification><c:notificationText><![CDATA[<second>]]>&amp;&#13;
<!--comment--><?pi ok?>end</c:notificationText></c:notification>
                    <c:notification><c:notificationCode>LAST</c:notificationCode></c:notification>
                </c:notifications>
            </c:result>
            <a:software><a:softwareId>SW-1</a:softwareId><a:softwareName> Software </a:softwareName>
                <a:softwareOperation>FUTURE_OPERATION</a:softwareOperation><a:softwareMainVersion>v1</a:softwareMainVersion>
                <a:softwareDevName>Developer</a:softwareDevName><a:softwareDevContact>dev@example.test</a:softwareDevContact>
                <a:softwareDevCountryCode>HU</a:softwareDevCountryCode><a:softwareDevTaxNumber>00123456</a:softwareDevTaxNumber>
            </a:software>").expect("success with diagnostics and no reported validity");
        assert_eq!(info.valid, None);
        let header = info.header.as_ref().expect("header");
        assert_eq!(header.request_id.as_deref(), Some(" R&1 "));
        assert_eq!(header.timestamp.as_deref(), Some("not-a-date é中"));
        assert_eq!(header.request_version.as_deref(), Some("future"));
        assert_eq!(header.header_version.as_deref(), Some("1.0"));
        let software = info.software.as_ref().expect("software");
        assert_eq!(software.id.as_deref(), Some("SW-1"));
        assert_eq!(software.name.as_deref(), Some(" Software "));
        assert_eq!(software.operation.as_deref(), Some("FUTURE_OPERATION"));
        assert_eq!(software.main_version.as_deref(), Some("v1"));
        assert_eq!(software.developer_name.as_deref(), Some("Developer"));
        assert_eq!(
            software.developer_contact.as_deref(),
            Some("dev@example.test")
        );
        assert_eq!(software.developer_country_code.as_deref(), Some("HU"));
        assert_eq!(software.developer_tax_number.as_deref(), Some("00123456"));
        let diagnostics = info.diagnostics.as_ref().expect("result");
        assert_eq!(diagnostics.func_code.as_deref(), Some("OK"));
        assert_eq!(diagnostics.error_code.as_deref(), Some("0057"));
        assert_eq!(diagnostics.message.as_deref(), Some(" A&B<C>\u{a0} "));
        let notifications: Vec<_> = diagnostics
            .notifications
            .iter()
            .map(|n| (n.code.as_deref(), n.text.as_deref()))
            .collect();
        assert_eq!(
            notifications,
            [
                (Some("FUTURE_CODE"), Some(" First ")),
                (None, None),
                (None, Some("<second>&\r\nend")),
                (Some("LAST"), None),
            ]
        );
        assert_eq!(
            serde_json::from_value::<TaxpayerInfo>(serde_json::to_value(&info).expect("serialize"))
                .expect("roundtrip"),
            info
        );
    }
}

#[test]
fn metadata_is_optional_and_blank_fields_remain_unreported() {
    for version in [2, 3] {
        let info = parse(version, OK).expect("sparse success");
        assert_eq!(info.header, None);
        assert_eq!(info.software, None);
        let diagnostics = info.diagnostics.expect("parsed result");
        assert_eq!(diagnostics.error_code, None);
        assert_eq!(diagnostics.message, None);
        assert!(diagnostics.notifications.is_empty());
        let info = parse(version, r"
            <c:header><c:requestId/><c:timestamp> &#9;&#13; </c:timestamp></c:header>
            <a:software><a:softwareName> </a:softwareName></a:software>
            <c:result><c:funcCode>OK</c:funcCode><c:errorCode/><c:message> </c:message>
                <c:notifications><c:notification><c:notificationCode> </c:notificationCode><c:notificationText/></c:notification></c:notifications>
            </c:result>").expect("empty metadata");
        assert_eq!(info.header, Some(TaxpayerHeader::default()));
        assert_eq!(info.software, Some(TaxpayerSoftware::default()));
        assert_eq!(
            info.diagnostics.expect("result").notifications,
            [TaxpayerNotification::default()]
        );
    }
    let old: TaxpayerInfo = serde_json::from_str(
        r#"{"valid":false,"name":null,"tax_number":null,"vat_code":null,"addresses":[]}"#,
    )
    .expect("older JSON");
    assert_eq!(old.valid, Some(false));
    assert_eq!(old.header, None);
    assert_eq!(old.software, None);
    assert_eq!(old.diagnostics, None);
    let sparse: TaxpayerInfo = serde_json::from_str(
        r#"{"addresses":[],"header":{},"software":{},"diagnostics":{"notifications":[{}]}}"#,
    )
    .expect("additive defaults");
    assert_eq!(sparse.valid, None);
    assert_eq!(sparse.header, Some(TaxpayerHeader::default()));
    assert_eq!(sparse.software, Some(TaxpayerSoftware::default()));
    assert_eq!(
        sparse.diagnostics.expect("diagnostics").notifications,
        [TaxpayerNotification::default()]
    );
}

#[test]
fn metadata_requires_its_expanded_name_and_direct_parent() {
    for version in [2, 3] {
        let info = parse(version, &format!(r"{OK}
            <f:header><c:requestId>foreign parent</c:requestId></f:header>
            <f:software><a:softwareId>foreign parent</a:softwareId></f:software>
            <a:unknown><c:header><c:requestId>hidden</c:requestId></c:header><a:software/></a:unknown>
            <c:requestId>wrong parent</c:requestId><a:softwareId>wrong parent</a:softwareId>
            <c:notifications><c:notification><c:notificationCode>wrong parent</c:notificationCode></c:notification></c:notifications>
            <c:header><f:requestId>foreign</f:requestId><c:unknown><c:requestId>hidden</c:requestId></c:unknown></c:header>
            <a:software><f:softwareName>foreign</f:softwareName><a:unknown><a:softwareId>hidden</a:softwareId></a:unknown></a:software>")).expect("ignored metadata");
        assert_eq!(info.header, Some(TaxpayerHeader::default()));
        assert_eq!(info.software, Some(TaxpayerSoftware::default()));
        assert!(info.diagnostics.expect("result").notifications.is_empty());
        let info = parse(version, r"<c:result><c:funcCode>OK</c:funcCode>
            <f:notifications><c:notification/></f:notifications>
            <c:notification/><c:notificationCode>wrong parent</c:notificationCode>
            <c:notifications><f:notification><c:notificationCode>hidden</c:notificationCode></f:notification>
                <c:notification><f:notificationCode>foreign</f:notificationCode><c:message>wrong leaf</c:message><c:unknown><c:notificationText>hidden</c:notificationText></c:unknown></c:notification>
            </c:notifications></c:result>").expect("only direct notification entry");
        let diagnostics = info.diagnostics.expect("result");
        assert_eq!(diagnostics.message, None);
        assert_eq!(diagnostics.notifications, [TaxpayerNotification::default()]);
    }
    // NAV 3 header/result use Common, whereas software stays in the API namespace.
    let info = parse(3, &format!("{OK}<a:header><c:requestId>wrong</c:requestId></a:header><c:software><a:softwareId>wrong</a:softwareId></c:software>")).expect("wrong namespace ignored");
    assert_eq!(info.header, None);
    assert_eq!(info.software, None);
    let info = parse(3, "<c:header><a:requestId>wrong</a:requestId></c:header><a:software><c:softwareId>wrong</c:softwareId></a:software><c:result><c:funcCode>OK</c:funcCode><a:notifications><c:notification/></a:notifications></c:result>").expect("wrong leaf namespace ignored");
    assert_eq!(info.header, Some(TaxpayerHeader::default()));
    assert_eq!(info.software, Some(TaxpayerSoftware::default()));
    assert!(info.diagnostics.expect("result").notifications.is_empty());
}

#[test]
fn metadata_singletons_and_scalars_cannot_be_overwritten_or_nested() {
    for version in [2, 3] {
        for metadata in [
            "<c:header/><c:header/>",
            "<a:software/><a:software/>",
            "<c:header><c:requestId/><c:requestId>second</c:requestId></c:header>",
            "<a:software><a:softwareId/><a:softwareId>second</a:softwareId></a:software>",
            "<c:header><c:timestamp><f:child/></c:timestamp></c:header>",
            "<a:software><a:softwareName><f:child/></a:softwareName></a:software>",
        ] {
            assert!(
                matches!(
                    parse(version, &format!("{OK}{metadata}")),
                    Err(ResponseError::Parse(_))
                ),
                "{metadata}"
            );
        }
        for notifications in [
            "<c:notifications/><c:notifications/>",
            "<c:notifications><c:notification><c:notificationCode/><c:notificationCode>A</c:notificationCode></c:notification></c:notifications>",
            "<c:notifications><c:notification><c:notificationText/><c:notificationText>A</c:notificationText></c:notification></c:notifications>",
            "<c:notifications><c:notification><c:notificationText><f:child/></c:notificationText></c:notification></c:notifications>",
        ] {
            assert!(
                matches!(
                    parse(
                        version,
                        &format!("<c:result><c:funcCode>OK</c:funcCode>{notifications}</c:result>")
                    ),
                    Err(ResponseError::Parse(_))
                ),
                "{notifications}"
            );
        }
    }
}

#[test]
fn missing_verdict_and_failure_classification_keep_existing_behavior() {
    for version in [2, 3] {
        for result in [
            "",
            "<c:result/>",
            "<c:result><c:funcCode> </c:funcCode></c:result>",
        ] {
            assert!(matches!(
                parse(version, result),
                Err(ResponseError::Parse(ParseError::Missing("funcCode")))
            ));
        }
        for (fields, code, message) in [
            (
                "<c:errorCode>57</c:errorCode><c:message> padded </c:message>",
                ErrorCode::MalformedXml,
                " padded ",
            ),
            (
                "<c:errorCode>FUTURE_ERROR</c:errorCode>",
                ErrorCode::Unknown("FUTURE_ERROR".into()),
                "FUTURE_ERROR",
            ),
            ("", ErrorCode::Absent, "NAV funcCode FUTURE_VERDICT"),
        ] {
            let error = parse(version, &format!(r"<c:header><c:requestId>R</c:requestId></c:header>
                <c:result><c:funcCode>FUTURE_VERDICT</c:funcCode>{fields}<c:notifications>
                <c:notification><c:notificationText>not the error message</c:notificationText></c:notification>
                </c:notifications></c:result><a:software/>")).expect_err("non-OK remains an API error");
            assert!(
                matches!(error, ResponseError::Api(api) if api.code == code && api.message == message)
            );
        }
        let body = response(version, OK);
        let raw = RawResponse::new(
            [
                ("szlahu_error_code", "57"),
                ("szlahu_error", "header+error"),
            ],
            body.body().to_vec(),
        );
        assert!(
            matches!(QueryTaxpayer::new("12345678").expect("prefix").parse(&raw), Err(ResponseError::Api(api)) if api.code == ErrorCode::MalformedXml)
        );
    }
}
