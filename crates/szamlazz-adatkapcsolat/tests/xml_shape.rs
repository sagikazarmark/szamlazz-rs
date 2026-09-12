//! XML well-formedness is shape even in ignored extensions. Business content
//! and the root-start-only identification boundary are separate contracts.

use szamlazz_adatkapcsolat::{Document, ParseError, RootKind};

const BANK: &str = r#"<banktranz xmlns="http://www.szamlazz.hu/banktranz"><id>7</id></banktranz>"#;

#[test]
fn dtd_element_and_attribute_names_require_qnames() {
    for name in ["a:b:c", ":a", "a:", "a:1b", "a:\u{300}b"] {
        for declaration in [
            format!("<!ELEMENT {name} EMPTY>"),
            format!("<!ATTLIST {name} future CDATA #IMPLIED>"),
            format!("<!ATTLIST extension {name} CDATA #IMPLIED>"),
            format!("<!ELEMENT extension ({name})>"),
            format!("<!ELEMENT extension ((valid|{name})*,next) >"),
            format!("<!ELEMENT extension (#PCDATA|{name})*>"),
        ] {
            let body = format!("<!DOCTYPE banktranz [{declaration}]>{BANK}");
            assert!(
                matches!(Document::parse(body.as_bytes()), Err(ParseError::Xml(_))),
                "accepted {declaration}"
            );
        }
        let body = format!("<!DOCTYPE {name}>{BANK}");
        assert!(
            Document::parse(body.as_bytes()).is_err(),
            "accepted DOCTYPE {name}"
        );
    }
}

#[test]
fn dtd_entities_notations_and_instruction_targets_require_ncnames() {
    for name in ["a:b", "a:b:c", ":a", "a:"] {
        for declaration in [
            format!("<!ENTITY {name} 'x'>"),
            format!("<!ENTITY % {name} 'x'>"),
            format!("<!NOTATION {name} SYSTEM 'urn:notation'>"),
            format!("<!ENTITY unused SYSTEM 'urn:unused' NDATA {name}>"),
            format!("<!ATTLIST extension future NOTATION ({name}|valid) #IMPLIED>"),
            format!("<!ENTITY unused '&{name};'>"),
            format!("<!ATTLIST extension future CDATA '&{name};'>"),
            format!("%{name};"),
            format!("<?{name} instruction?>"),
        ] {
            let body = format!("<!DOCTYPE banktranz [{declaration}]>{BANK}");
            assert!(
                matches!(Document::parse(body.as_bytes()), Err(ParseError::Xml(_))),
                "accepted {declaration}"
            );
        }
    }
}

#[test]
fn dtd_unused_prefixes_and_colons_in_non_name_content_remain_legal() {
    for name in ["future", "p:future", "é:未来", "p:a\u{300}"] {
        let subset = format!(
            "<!ELEMENT {name} ((p:first|p:second)*,p:last?)><!ELEMENT p:mixed (#PCDATA|{name})*><!ATTLIST {name} p:attribute CDATA #IMPLIED>"
        );
        let body = format!("<!DOCTYPE banktranz [{subset}]>{BANK}");
        let Document::BankTransaction(tx) =
            Document::parse(body.as_bytes()).expect("unused DTD prefixes need no bindings")
        else {
            panic!("bank transaction")
        };
        assert_eq!(tx.id, 7);
        assert_eq!(tx.raw_xml(), Some(body.as_str()));
    }
    for subset in [
        "<!ENTITY é 'x'><!ENTITY unused '&é;'>",
        "<!ENTITY % 未来 SYSTEM 'urn:unused'>%未来;",
        "<!NOTATION é SYSTEM 'urn:notation'><!ENTITY unused SYSTEM 'urn:unused' NDATA é>",
        "<!ATTLIST extension future NOTATION (é|未来) #IMPLIED>",
        // Enumerations are Nmtokens, not QNames; values aren't names either.
        "<!ATTLIST extension future (a:b:c|:|1) 'a:b:c'>",
        "<!ENTITY unused 'a:b:c'><!ATTLIST extension future CDATA 'a:b:c'>",
        "<?é colon:in:data?>",
    ] {
        let body = format!("<!DOCTYPE banktranz [{subset}]>{BANK}");
        assert!(
            Document::parse(body.as_bytes()).is_ok(),
            "rejected {subset}"
        );
    }
}

#[test]
fn malformed_dtd_declarations_are_xml_shape() {
    for dtd in [
        "<!DOCTYPE banktranz [<!ELEMENT !!!>]>",
        "<!DOCTYPE banktranz [<!ENTITY unused '&#0;'>]>",
        "<!DOCTYPE banktranz [<?XML invalid?>]>",
    ] {
        let body = format!("{dtd}{BANK}");
        assert!(
            matches!(Document::parse(body.as_bytes()), Err(ParseError::Xml(_))),
            "accepted {body}"
        );
    }
}

#[test]
fn quoted_delimiters_in_dtd_declarations_are_legal() {
    let body = format!("<!DOCTYPE banktranz [<!ATTLIST extension future CDATA 'a>b'>]>{BANK}");
    let Document::BankTransaction(tx) = Document::parse(body.as_bytes()).expect("legal DTD") else {
        panic!("bank transaction")
    };
    assert_eq!(tx.id, 7);
    assert_eq!(tx.raw_xml(), Some(body.as_str()));
}

#[test]
fn dtd_sibling_declarations_validate_grammar_without_validating_document_content() {
    for subset in [
        "<!ELEMENT banktranz EMPTY>", // deliberately does not describe the record
        "<!ELEMENT banktranz ANY>",
        "<!ELEMENT banktranz (id)>",
        "<!ELEMENT banktranz ((id|extension)*,next?,last+)>",
        "<!ELEMENT extension (#PCDATA)>",
        "<!ELEMENT extension (#PCDATA)*>",
        "<!ELEMENT extension (#PCDATA|other)*>",
        "<!ATTLIST extension>",
        "<!ATTLIST extension future CDATA 'a>b'>",
        "<!ATTLIST extension future CDATA \"a>b\">",
        "<!ATTLIST extension future CDATA 'a]b' more CDATA 'a[b'>",
        "<!ATTLIST extension future CDATA '&amp;#0;literal'>",
        "<!ATTLIST extension a ID #REQUIRED b IDREF #IMPLIED c IDREFS #IMPLIED d ENTITY #IMPLIED e ENTITIES #IMPLIED f NMTOKEN #IMPLIED g NMTOKENS #IMPLIED>",
        "<!ATTLIST extension future (0|a-b|é) #FIXED '0'>",
        "<!ATTLIST extension future NOTATION (gif|png) #IMPLIED>",
        "<!NOTATION gif PUBLIC 'image/gif'>",
        "<!NOTATION gif PUBLIC 'image/gif' 'urn:gif'>",
        "<!NOTATION gif SYSTEM 'urn:gif'>",
        "<!ENTITY unused 'a>b'>",
        "<!ENTITY unused 'a<b'>",
        "<!ENTITY unused '&later;&#9;&#x10FFFF;'>",
        "<!ENTITY unused SYSTEM 'urn:a>b'>",
        "<!ENTITY unused PUBLIC 'public-id' 'urn:a>b'>",
        "<!ENTITY unused SYSTEM 'urn:unused' NDATA gif>",
        "<!ENTITY % unused 'replacement'>",
        "<!ENTITY % unused SYSTEM 'urn:unused'>",
        "<!ENTITY % unused SYSTEM 'urn:unused'>%unused;",
        "<!-- > [ ] < &#0; --><?valid a>b [ ] &#0;?>",
        "<?valid?>",
        "<!ATTLIST banktranz xmlns CDATA 'urn:wrong'>", // defaults are not applied
        "<!ATTLIST extension future CDATA '%literal;'>",
    ] {
        let body = format!("<!DOCTYPE banktranz [{subset}]>{BANK}");
        let Document::BankTransaction(tx) =
            Document::parse(body.as_bytes()).unwrap_or_else(|error| panic!("{subset}: {error}"))
        else {
            panic!("bank transaction")
        };
        assert_eq!(tx.id, 7);
        assert_eq!(tx.raw_xml(), Some(body.as_str()));
    }
}

#[test]
fn malformed_dtd_sibling_contexts_are_refused() {
    for subset in [
        "<!ELEMENT !!!>",
        "<!ELEMENT extension>",
        "<!ELEMENT extension EMPTY junk>",
        "<!ELEMENT extension ()>",
        "<!ELEMENT extension (a|b,c)>",
        "<!ELEMENT extension (a,)>",
        "<!ELEMENT extension ((a) b)>",
        "<!ELEMENT extension (#PCDATA|a)>",
        "<!ELEMENT extension (a,#PCDATA)>",
        "<!ELEMENT extension (a)**>",
        "<!ELEMENT extension (%content;)>",
        "<!ATTLIST extension future CDATA>",
        "<!ATTLIST extension future INVALID 'x'>",
        "<!ATTLIST extension future () 'x'>",
        "<!ATTLIST extension future (a|) 'x'>",
        "<!ATTLIST extension future NOTATION (1) 'x'>",
        "<!ATTLIST extension future CDATA '<'>",
        "<!ATTLIST extension future CDATA '&#0;'>",
        "<!ATTLIST extension future CDATA '&#xD800;'>",
        "<!ATTLIST extension future CDATA '&#1114112;'>",
        "<!ATTLIST extension future CDATA '&broken'>",
        "<!ATTLIST extension future CDATA #FIXED'x'>",
        "<!ATTLIST extension future CDATA 'x'next CDATA 'y'>",
        "<!ENTITY unused '&#0;'>",
        "<!ENTITY unused '&#xD800;'>",
        "<!ENTITY unused '&#x110000;'>",
        "<!ENTITY unused '&#xFFFE;'>",
        "<!ENTITY unused '&#+32;'>",
        "<!ENTITY unused '&#x;'>",
        "<!ENTITY unused '&;'>",
        "<!ENTITY unused '&broken'>",
        "<!ENTITY unused '%parameter;'>",
        "<!ENTITY % unused '%parameter;'>",
        "<!ENTITY % unused SYSTEM 'urn:x' NDATA gif>",
        "<!ENTITY unused PUBLIC 'public-id'>",
        "<!ENTITY unused SYSTEM 'urn:x'NDATA gif>",
        "<!NOTATION !!!>",
        "<!NOTATION gif PUBLIC 'not\ta\tpubid'>",
        "<!NOTATION gif PUBLIC 'id''urn:x'>",
        "<!NOTATION gif SYSTEM>",
        "<!-- bad -- comment -->",
        "<!-- bad --->",
        "<?XML invalid?>",
        "<?xml version='1.0'?>",
        "<?p:target invalid?>",
        "<?1target invalid?>",
        "<?target/data?>",
        "%broken",
        "<![IGNORE[anything]]>", // only legal in an external subset
        "<!BOGUS extension>",
    ] {
        let body = format!("<!DOCTYPE banktranz [{subset}]>{BANK}");
        assert!(
            Document::parse(body.as_bytes()).is_err(),
            "accepted {subset}"
        );
    }
}

#[test]
fn dtd_values_and_external_identifiers_are_never_applied_to_the_record() {
    for dtd in [
        "<!DOCTYPE banktranz SYSTEM 'file:///nonexistent/dtd'>",
        "<!DOCTYPE banktranz PUBLIC 'public-id' 'http://127.0.0.1:9/never-fetch'>",
        "<!DOCTYPE banktranz [<!ENTITY % remote SYSTEM 'http://127.0.0.1:9/never-fetch'>%remote;]>",
        "<!DOCTYPE banktranz [<!ENTITY a '&b;&b;'><!ENTITY b '&a;&a;'>]>",
        "<!DOCTYPE banktranz [<!ATTLIST banktranz xmlns CDATA 'urn:wrong'>]>",
        "<!DOCTYPE banktranz [<!ENTITY unused 'a>b [ ] &later; é'>]>",
    ] {
        let body = format!("{dtd}{BANK}");
        let Document::BankTransaction(tx) =
            Document::parse(body.as_bytes()).expect("unapplied DTD")
        else {
            panic!("bank transaction")
        };
        assert_eq!(tx.id, 7);
        assert_eq!(tx.raw_xml(), Some(body.as_str()));
    }
    let body = format!(
        "<!DOCTYPE banktranz [<!ENTITY id '7'>]>{}",
        BANK.replace(">7<", ">&id;<")
    );
    assert!(
        Document::parse(body.as_bytes()).is_err(),
        "declared entities are not expanded"
    );
    for dtd in [
        "<!DOCTYPE banktranz><!DOCTYPE banktranz>",
        "<!DOCTYPE banktranz [<!ATTLIST extension a CDATA 'unclosed>]>",
        "<!DOCTYPE banktranz [<!--unclosed]>",
        "<!DOCTYPE banktranz [<?unclosed]>",
        "<!DOCTYPE banktranz [<!ENTITY unused '\0'>]>",
        "<!DOCTYPE banktranz [<!--\0-->]>",
        "<!DOCTYPE banktranz [<?valid \0?>]>",
    ] {
        assert!(
            Document::parse(format!("{dtd}{BANK}").as_bytes()).is_err(),
            "accepted {dtd:?}"
        );
    }
}

#[test]
fn deeply_nested_dtd_content_models_do_not_use_the_call_stack() {
    let model = format!("{}id{}", "(".repeat(4096), ")".repeat(4096));
    let body = format!("<!DOCTYPE banktranz [<!ELEMENT banktranz {model}>]>{BANK}");
    assert!(Document::parse(body.as_bytes()).is_ok());
}

#[test]
fn one_complete_xml_document_is_required() {
    for body in [
        format!("{BANK}{}", BANK.replace(">7<", ">8<")),
        format!("not XML{BANK}"),
        format!("{BANK}not XML"),
        format!("{BANK}\u{a0}"),
        format!("{BANK}&#32;"),
        format!("{BANK}<![CDATA[ ]]>"),
        format!("{BANK}<?xml version=\"1.0\"?>"),
        BANK.replace("</banktranz>", ""),
        BANK.replace("</id>", "</other>"),
    ] {
        assert!(
            Document::parse(body.as_bytes()).is_err(),
            "accepted {body:?}"
        );
    }
}

#[test]
fn escaped_namespace_declarations_identify_and_parse_every_root_kind() {
    for (root, kind, content) in [
        (
            "szamla",
            RootKind::OutgoingInvoice,
            "<alap><id>7</id><szamlaszam>E-1</szamlaszam></alap>",
        ),
        (
            "szamlabe",
            RootKind::IncomingInvoice,
            "<alap><id>7</id><szamlaszam>E-1</szamlaszam></alap>",
        ),
        ("banktranz", RootKind::BankTransaction, "<id>7</id>"),
        (
            "xmlnyugtaarchiv",
            RootKind::Receipts,
            "<nyugta><alap><id>7</id></alap></nyugta>",
        ),
    ] {
        for scheme in ["http://", "http:&#47;&#47;", "http:&#x2f;&#x2F;"] {
            let namespace = format!("{scheme}www.szamlazz.hu/{root}");
            for body in [
                format!(r#"<{root} xmlns="{namespace}">{content}</{root}>"#),
                format!(
                    r#"<p:{root} xmlns:p="{namespace}" xmlns="{namespace}">{content}</p:{root}>"#
                ),
                // Both empty and nonempty children redeclare the namespace;
                // following identity elements must recover the parent's scope.
                format!(
                    r#"<{root} xmlns="http://www.szamlazz.hu/{root}"><future xmlns="{namespace}"/><future xmlns:p="{namespace}"><p:child/></future>{content}</{root}>"#
                ),
            ] {
                assert_eq!(
                    Document::identify(body.as_bytes()).expect("identify normalized root"),
                    kind,
                    "{body}"
                );
                assert_eq!(
                    Document::parse(body.as_bytes())
                        .expect("parse normalized namespaces")
                        .kind(),
                    kind,
                    "{body}"
                );
            }
        }
    }
}

#[test]
fn namespace_normalization_does_not_hide_a_different_namespace() {
    for namespace in [
        "http:&amp;#47;&amp;#47;www.szamlazz.hu/banktranz",
        "http://www.szamlazz.hu/banktranz&#32;",
        "http://www.szamlazz.hu/banktranz\n",
    ] {
        let body = format!(r#"<banktranz xmlns="{namespace}"><id>7</id></banktranz>"#);
        assert!(
            matches!(
                Document::identify(body.as_bytes()),
                Err(ParseError::WrongNamespace { .. })
            ),
            "{body}"
        );
        let body = BANK.replace("<id>", &format!(r#"<future xmlns="{namespace}"/><id>"#));
        assert_eq!(
            Document::identify(body.as_bytes()).expect("identify root"),
            RootKind::BankTransaction
        );
        assert!(
            matches!(
                Document::parse(body.as_bytes()),
                Err(ParseError::WrongNamespace { .. })
            ),
            "{body}"
        );
    }
}

#[test]
fn forbidden_characters_in_ignored_content_are_xml_shape() {
    for value in ["\0", "\u{b}", "\u{fffe}", "&#0;", "&#xB;", "&#65535;"] {
        for extension in [
            format!("<future>{value}</future>"),
            format!("<future attr='{value}'/>"),
        ] {
            let body = BANK.replace("<id>", &format!("{extension}<id>"));
            assert!(
                matches!(Document::parse(body.as_bytes()), Err(ParseError::Xml(_))),
                "accepted {body:?}"
            );
            assert_eq!(
                Document::identify(body.as_bytes())
                    .expect("identify root before forbidden content"),
                RootKind::BankTransaction
            );
        }
    }
}

#[test]
fn identification_never_validates_the_tail_except_for_utf8() {
    let start = r#"<banktranz xmlns="http://www.szamlazz.hu/banktranz">"#;
    for tail in ["", "\0", "<broken", "</wrong>", "<child xmlns='wrong'/>"] {
        let body = format!("{start}{tail}");
        assert_eq!(
            Document::identify(body.as_bytes()).expect("identify root without reading tail"),
            RootKind::BankTransaction
        );
        assert!(Document::parse(body.as_bytes()).is_err());
    }
    let mut invalid_utf8 = start.as_bytes().to_vec();
    invalid_utf8.push(0xff);
    assert!(matches!(
        Document::identify(&invalid_utf8),
        Err(ParseError::Utf8(_))
    ));
}

#[test]
fn legitimate_xml_extensions_and_business_content_remain_lenient() {
    let extensions = r#"<future xmlns:a="urn:extension?a=1&amp;b=2" xmlns:b="urn:extension?a=1&amp;b=3" a:flag="yes" b:flag="no" plain="&lt;&amp;&#x9;" xml:lang="hu"><nested/>text <![CDATA[<raw>&#0;]]><!-- &#0; is only comment text --><?új content?></future>"#;
    let bank = BANK.replace("<id>", &format!("{extensions}<id>"));
    for body in [
        bank.clone(),
        format!(
            "\u{feff}<?xml version='1.0' encoding='UTF-8'?>\n<!--before--><?before ok?>{bank}\r\n\t <!--after--><?after ok?>"
        ),
        format!("<?xml\tversion='1.0'?>{bank}"),
        format!("<!DOCTYPE banktranz>{bank}"),
        // A DTD declaration need not be fetched to read this record.
        format!("<!DOCTYPE banktranz SYSTEM 'urn:unused'>{bank}"),
        bank.replace(
            "<id>7</id>",
            "<id>7</id><erteknap>é12345</erteknap><irany>FUTURE</irany><osszeg>-1</osszeg>",
        ),
        BANK.replace(
            "<id>",
            r#"<future xmlns:a="urn:x&#9;y" xmlns:b="urn:x y" a:flag="yes" b:flag="no"/><id>"#,
        ),
    ] {
        let Document::BankTransaction(tx) =
            Document::parse(body.as_bytes()).expect("legal XML extension")
        else {
            panic!("bank transaction")
        };
        assert_eq!(tx.id, 7);
        assert_eq!(tx.raw_xml(), Some(body.as_str()));
    }
    let body = BANK.replace("http://", "http:&#47;&#47;").replace(
        "<id>",
        "<kozlemeny>  A &amp; B <![CDATA[<raw>]]> \t </kozlemeny><id>",
    );
    let Document::BankTransaction(tx) =
        Document::parse(body.as_bytes()).expect("preserved business text")
    else {
        panic!("bank transaction")
    };
    assert_eq!(tx.memo.as_deref(), Some("  A & B <raw> \t "));
    assert_eq!(tx.raw_xml(), Some(body.as_str()));
    // XML shape is not a new nonblank business-identity rule.
    for number in ["", " "] {
        let body = format!(
            r#"<szamla xmlns="http://www.szamlazz.hu/szamla"><alap><id>7</id><szamlaszam>{number}</szamlaszam></alap><pdf>not base64!</pdf></szamla>"#
        );
        let Document::OutgoingInvoice(invoice) =
            Document::parse(body.as_bytes()).expect("lenient invoice content")
        else {
            panic!("invoice")
        };
        assert_eq!(invoice.info.invoice_number, number);
        assert!(invoice.pdf.is_none());
    }
}

#[test]
fn ignored_extensions_still_require_well_formed_xml_and_namespace_bindings() {
    for extension in [
        "<future attr='one' attr='two'/>",
        "<future a:flag='one'/>",
        "<future xmlns:a='urn:same' xmlns:b='urn:same' a:flag='one' b:flag='two'/>",
        "<future xmlns:a='urn:x?a=1&amp;b=2' xmlns:b='urn:x?a=1&#38;b=2' a:flag='one' b:flag='two'/>",
        "<future xmlns:xml='urn:wrong'/>",
        "<future xmlns:a='http:&#47;&#47;www.w3.org/XML/1998/namespace'/>",
        "<future xmlns:a=''/>",
        "<future xmlns:xmlns='urn:wrong'/>",
        "<future xmlns='http://www.w3.org/2000/xmlns/'/>",
        "<future>&unknown;</future>",
        "<future attr='&unknown;'/>",
        "<future attr='<'/>",
        "<future attr='one'next='two'/>",
        "<1future/>",
        "<future a:b:c='one'/>",
        "<future>]]></future>",
        "<future><![CDATA[\0]]></future>",
        "<!-- bad -- comment -->",
        "<!--\0-->",
        "<?future \0?>",
        "<?XML invalid?>",
        "<?p:future invalid?>",
        "<!DOCTYPE banktranz>",
    ] {
        let body = BANK.replace("<id>", &format!("{extension}<id>"));
        assert!(
            Document::parse(body.as_bytes()).is_err(),
            "accepted {body:?}"
        );
        assert_eq!(
            Document::identify(body.as_bytes()).expect("identify root before malformed extension"),
            RootKind::BankTransaction
        );
    }
    // Normalizing the explicit reserved binding is legal; the namespace
    // reader must not validate its raw escaped spelling as a different URI.
    let body = BANK.replace(
        "<id>",
        r#"<future xmlns:xml="http:&#47;&#47;www.w3.org/XML/1998/namespace" xml:lang="hu"/><id>"#,
    );
    assert!(Document::parse(body.as_bytes()).is_ok());
}
