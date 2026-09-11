//! The executable's input and reporting contract, over real loopback HTTP.

use std::process::{Output, Stdio};
use std::time::Duration;

use serde_json::{Value, json};
use tokio::io::AsyncWriteExt as _;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

// No trailing newline: the stdout writer must flush before reporting success.
const PDF: &[u8] = b"%PDF-1.4";
const PDF_BASE64: &str = "JVBERi0xLjQ=";

fn invoice() -> Value {
    serde_json::from_str(include_str!("../examples/invoice.json")).expect("invoice example")
}

fn receipt() -> Value {
    serde_json::from_str(include_str!("../examples/receipt.json")).expect("receipt example")
}

fn command(server: &MockServer, args: &[&str]) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_szamlazz"));
    command
        .args(args)
        .env("SZAMLAZZ_AGENT_KEY", "loopback-key")
        .env("SZAMLAZZ_ENDPOINT", server.uri())
        .env("NO_PROXY", "*")
        .env("no_proxy", "*")
        .env("TOKIO_WORKER_THREADS", "2")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    command
}

async fn run(command: &mut tokio::process::Command, input: Option<&str>) -> Output {
    let mut child = command.spawn().expect("spawn CLI");
    if let Some(input) = input {
        child
            .stdin
            .take()
            .expect("stdin")
            .write_all(input.as_bytes())
            .await
            .expect("write input");
    } else {
        drop(child.stdin.take());
    }
    tokio::time::timeout(Duration::from_secs(15), child.wait_with_output())
        .await
        .expect("CLI deadline")
        .expect("CLI output")
}

async fn invoke(server: &MockServer, args: &[&str], input: Option<&Value>) -> Output {
    run(
        &mut command(server, args),
        input.map(Value::to_string).as_deref(),
    )
    .await
}

fn text(bytes: &[u8]) -> &str {
    std::str::from_utf8(bytes).expect("UTF-8 report")
}

fn report(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("{error}: {output:?}"))
}

async fn respond(server: &MockServer, body: String) {
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "application/xml"))
        .mount(server)
        .await;
}

fn invoice_reply(number: &str, gross: Option<&str>) -> String {
    let gross = gross
        .map(|gross| format!("<szamlabrutto>{gross}</szamlabrutto>"))
        .unwrap_or_default();
    format!(
        "<xmlszamlavalasz xmlns=\"http://www.szamlazz.hu/xmlszamlavalasz\">\
         <sikeres>true</sikeres><szamlaszam>{number}</szamlaszam>{gross}\
         <pdf>{PDF_BASE64}</pdf></xmlszamlavalasz>"
    )
}

async fn original_invoice(server: &MockServer, appearance: i64) {
    Mock::given(method("POST")).and(wiremock::matchers::body_string_contains("name=\"action-szamla_agent_xml\""))
        .respond_with(ResponseTemplate::new(200).set_body_raw(format!(
            "<szamla xmlns=\"http://www.szamlazz.hu/szamla\"><szallito><nev>Seller</nev><cim><irsz>1111</irsz><telepules>Budapest</telepules><cim>Street</cim></cim></szallito>\
            <alap><id>1</id><szamlaszam>E-2026-1</szamlaszam><tipus>SZ</tipus><eszamla>{appearance}</eszamla><telj>2026-07-15</telj></alap>\
            <vevo><nev>Buyer</nev></vevo><tetelek/><osszegek><totalossz><netto>100</netto><afa>27</afa><brutto>127</brutto></totalossz></osszegek></szamla>"
        ), "application/xml")).with_priority(1).mount(server).await;
}

#[tokio::test]
async fn storno_derives_original_appearance_and_refuses_an_unknown_form() {
    for (appearance, electronic) in [(1, Some(false)), (3, Some(true)), (9, None)] {
        let server = MockServer::start().await;
        respond(&server, invoice_reply("SS-1", Some("-127"))).await;
        original_invoice(&server, appearance).await;
        let output = invoke(&server, &["invoice", "storno", "E-2026-1"], None).await;
        assert_eq!(output.status.success(), electronic.is_some(), "{output:?}");
        let requests = server.received_requests().await.expect("requests");
        let sends: Vec<_> = requests
            .iter()
            .filter(|request| text(&request.body).contains("name=\"action-szamla_agent_st\""))
            .collect();
        if let Some(electronic) = electronic {
            assert_eq!(sends.len(), 1);
            let body = text(&sends[0].body);
            assert!(
                body.contains(&format!("<eszamla>{electronic}</eszamla>")),
                "{body}"
            );
            assert!(body.contains("<teljesitesDatum>2026-07-15</teljesitesDatum>"));
        } else {
            assert!(sends.is_empty());
            assert!(text(&output.stderr).contains("appearance"));
        }
    }
}

fn receipt_reply(storno: bool) -> String {
    let (number, kind) = if storno {
        ("SN-2026-1", "SN")
    } else {
        ("NY-2026-1", "NY")
    };
    format!(
        "<xmlnyugtavalasz xmlns=\"http://www.szamlazz.hu/xmlnyugtavalasz\">\
         <sikeres>true</sikeres><nyugtaPdf>{PDF_BASE64}</nyugtaPdf><nyugta><alap>\
         <id>1</id><nyugtaszam>{number}</nyugtaszam><tipus>{kind}</tipus>\
         <stornozott>false</stornozott><kelt>2026-09-09</kelt>\
         <fizmod>cash</fizmod><penznem>HUF</penznem><teszt>true</teszt>\
         </alap><tetelek/><osszegek><totalossz><netto>100</netto><afa>27</afa><brutto>127</brutto>\
         </totalossz></osszegek></nyugta></xmlnyugtavalasz>"
    )
}

fn object_paths(value: &Value, path: &str, paths: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            paths.push(path.to_owned());
            for (key, value) in object {
                object_paths(value, &format!("{path}/{key}"), paths);
            }
        }
        Value::Array(array) => {
            for (index, value) in array.iter().enumerate() {
                object_paths(value, &format!("{path}/{index}"), paths);
            }
        }
        _ => {}
    }
}

fn detailed_invoice() -> Value {
    let mut input = invoice();
    input["header"]["exchange_rate"] = json!({"bank": "MNB", "rate": "400"});
    input["header"]["template"] = json!({"other": "custom-template"});
    input["buyer"]["postal_address"] = json!({"city": "Budapest"});
    input["buyer"]["ledger"] = json!({"buyer_account": "311"});
    input["items"][0]["ledger"] = json!({"revenue_account": "911"});
    input["attachments"] =
        json!([{"filename": "note.txt", "content": [65], "content_type": "text/plain"}]);
    input["waybill"] = json!({
        "trans_o_flex": {"parcel_count": 1},
        "pick_pack_point": {"barcode_prefix": "P"},
        "sprinter": {"parcel_count": 1},
        "mpl": {"customer_code": "C", "barcode": "B", "weight": "1"}
    });
    input
}

fn detailed_receipt() -> Value {
    let mut input = receipt();
    input["exchange_rate"] = json!({"bank": "MNB", "rate": "400"});
    input["template"] = json!({"other": "custom-template"});
    input["items"][0]["ledger"] = json!({"revenue_account": "911"});
    input
}

#[tokio::test]
async fn unknown_fields_at_every_object_depth_never_send() {
    let server = MockServer::start().await;
    for (operation, input) in [
        ("invoice", detailed_invoice()),
        ("receipt", detailed_receipt()),
    ] {
        let mut paths = Vec::new();
        object_paths(&input, "", &mut paths);
        for path in paths {
            let mut invalid = input.clone();
            invalid.pointer_mut(&path).expect("object path")["__unknown"] = json!(true);
            let output = invoke(&server, &[operation, "create", "-f", "-"], Some(&invalid)).await;
            assert!(!output.status.success(), "{operation} {path}: {output:?}");
            assert!(
                text(&output.stderr).contains("__unknown"),
                "{operation} {path}: {output:?}"
            );
            assert!(output.stdout.is_empty(), "{output:?}");
        }
    }
    // Each struct variant must be traversed, not merely its enum tag.
    for kind in ["invoice", "prepayment", "final", "corrective"] {
        let mut input = invoice();
        let mut fields = json!({"__unknown": true});
        if kind == "corrective" {
            fields["corrected_number"] = json!("E-2026-1");
        }
        input["kind"] = json!({kind: fields});
        let output = invoke(&server, &["invoice", "create", "-f", "-"], Some(&input)).await;
        assert!(!output.status.success(), "{output:?}");
        assert!(text(&output.stderr).contains("__unknown"), "{output:?}");
    }
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
}

#[tokio::test]
async fn file_input_and_trailing_json_are_rejected_before_http() {
    let server = MockServer::start().await;
    let dir = tempfile::tempdir().expect("temp directory");
    let path = dir.path().join("input.json");
    let mut input = invoice();
    input["header"]["preview_pfd"] = json!(true);
    std::fs::write(&path, input.to_string()).expect("input file");
    let output = invoke(
        &server,
        &["invoice", "create", "-f", path.to_str().expect("path")],
        None,
    )
    .await;
    assert!(!output.status.success());
    assert!(
        text(&output.stderr).contains("header.preview_pfd"),
        "{output:?}"
    );
    let output = run(
        &mut command(&server, &["receipt", "create", "-f", "-"]),
        Some(&format!("{} {{}}", receipt())),
    )
    .await;
    assert!(!output.status.success());
    assert!(text(&output.stderr).contains("trailing"), "{output:?}");
    assert!(
        server
            .received_requests()
            .await
            .expect("requests")
            .is_empty()
    );
}

#[tokio::test]
async fn valid_nested_inputs_and_custom_wire_tokens_still_send() {
    for (operation, mut input, reply) in [
        (
            "invoice",
            detailed_invoice(),
            invoice_reply("E-2026-1", Some("12700")),
        ),
        ("receipt", detailed_receipt(), receipt_reply(false)),
    ] {
        let server = MockServer::start().await;
        respond(&server, reply).await;
        // Custom serde scalars stay open; these aren't unknown object fields.
        input["items"][0]["vat_rate"] = json!("FUTURE-VAT");
        if operation == "invoice" {
            input["header"]["payment_method"] = json!("future-tender");
        } else {
            input["payment_method"] = json!("future-tender");
        }
        let output = invoke(
            &server,
            &[operation, "create", "-f", "-", "--json"],
            Some(&input),
        )
        .await;
        assert!(output.status.success(), "{output:?}");
        assert_eq!(report(&output)["pdf_output"]["status"], "not_requested");
        assert_eq!(server.received_requests().await.expect("requests").len(), 1);
    }
}

#[tokio::test]
async fn remote_documents_survive_local_pdf_failure_in_human_and_json_output() {
    let dir = tempfile::tempdir().expect("temp directory");
    // Writing to a directory fails even when the test runs as root.
    let target = dir.path().to_str().expect("path");
    for json_output in [false, true] {
        for (args, input, reply, number) in [
            (
                vec!["invoice", "create", "-f", "-"],
                Some(invoice()),
                invoice_reply("E-2026-1", Some("12700")),
                "E-2026-1",
            ),
            (
                vec!["invoice", "storno", "E-2026-1"],
                None,
                invoice_reply("E-2026-2", Some("-12700")),
                "E-2026-2",
            ),
            (
                vec!["receipt", "create", "-f", "-"],
                Some(receipt()),
                receipt_reply(false),
                "NY-2026-1",
            ),
            (
                vec!["receipt", "storno", "NY-2026-1"],
                None,
                receipt_reply(true),
                "SN-2026-1",
            ),
            (
                vec!["receipt", "get", "NY-2026-1"],
                None,
                receipt_reply(false),
                "NY-2026-1",
            ),
        ] {
            let server = MockServer::start().await;
            respond(&server, reply).await;
            let mut args = args;
            let invoice_storno = args[0] == "invoice" && args[1] == "storno";
            if invoice_storno {
                original_invoice(&server, 3).await;
            }
            args.extend(["--pdf", target]);
            if json_output {
                args.push("--json");
            }
            let output = invoke(&server, &args, input.as_ref()).await;
            assert!(!output.status.success(), "{output:?}");
            assert!(text(&output.stdout).contains(number), "{output:?}");
            assert!(
                text(&output.stderr).contains("local PDF output failed"),
                "{output:?}"
            );
            if json_output {
                let value = report(&output);
                assert_eq!(value["pdf_output"]["status"], "failed");
                assert_eq!(value["pdf_output"]["target"], target);
                assert!(
                    value["pdf_output"]["error"]
                        .as_str()
                        .expect("local error")
                        .contains("writing PDF")
                );
                if args[0] == "invoice" && args[1] == "create" {
                    assert_eq!(value["remote"]["issued"]["invoice_number"], number);
                } else if args[0] == "invoice" {
                    assert_eq!(value["remote"]["outcome"], "reversed");
                    assert_eq!(value["remote"]["document"]["invoice_number"], number);
                } else {
                    assert_eq!(value["remote"]["receipt_number"], number);
                }
            } else {
                assert!(
                    text(&output.stdout).contains("PDF output failed"),
                    "{output:?}"
                );
            }
            assert_eq!(
                server.received_requests().await.expect("requests").len(),
                if invoice_storno { 2 } else { 1 }
            );
        }
    }
}

#[tokio::test]
async fn numberless_storno_preserves_acknowledgement_and_pdf_without_claiming_reversal() {
    for json_output in [false, true] {
        let server = MockServer::start().await;
        respond(&server, invoice_reply("", Some("-127"))).await;
        original_invoice(&server, 3).await;
        let dir = tempfile::tempdir().expect("temp directory");
        let path = dir.path().join("acknowledgement.pdf");
        let mut args = vec![
            "invoice",
            "storno",
            "E-2026-1",
            "--pdf",
            path.to_str().expect("path"),
        ];
        if json_output {
            args.push("--json");
        }
        let output = invoke(&server, &args, None).await;
        assert!(!output.status.success(), "{output:?}");
        assert_eq!(std::fs::read(&path).expect("saved PDF"), PDF);
        if json_output {
            let value = report(&output);
            assert_eq!(value["remote"]["outcome"], "unconfirmed");
            assert!(value["remote"]["document"].is_null());
            assert_eq!(value["remote"]["acknowledgement"]["gross_total"], "-127");
            assert!(value["remote"]["acknowledgement"]["pdf"].is_string());
            assert_eq!(value["pdf_output"]["status"], "written");
        } else {
            assert!(text(&output.stdout).contains("unnumbered acknowledgement"));
            assert!(text(&output.stdout).contains("-127"));
        }
        assert!(text(&output.stderr).contains("reconcile the original before retrying"));
        assert_eq!(server.received_requests().await.expect("requests").len(), 2);
    }
}

#[tokio::test]
async fn taxpayer_missing_validity_is_reported_as_absent_including_metadata() {
    let server = MockServer::start().await;
    respond(
        &server,
        r#"<QueryTaxpayerResponse xmlns="http://schemas.nav.gov.hu/OSA/2.0/api">
        <header><requestId>NAV-REQUEST</requestId></header>
        <result><funcCode>OK</funcCode><message>advisory</message></result>
        </QueryTaxpayerResponse>"#
            .to_owned(),
    )
    .await;
    let output = invoke(&server, &["taxpayer", "12345678", "--json"], None).await;
    assert!(output.status.success(), "{output:?}");
    let value = report(&output);
    assert!(value["valid"].is_null());
    assert_eq!(value["header"]["request_id"], "NAV-REQUEST");
    assert_eq!(value["diagnostics"]["message"], "advisory");
    let output = invoke(&server, &["taxpayer", "12345678"], None).await;
    assert!(output.status.success(), "{output:?}");
    assert!(text(&output.stdout).contains("not reported"));
}

#[tokio::test]
async fn storno_reports_genuine_repeat_zero_noop_and_unconfirmed_honestly() {
    for (number, gross, outcome, success) in [
        ("E-2026-2", Some("-12700"), "reversed", true),
        ("E-2026-2", Some("0"), "reversed", true),
        ("E-2026-1", Some("12700"), "noop", false),
        ("E-2026-1", None, "noop", false),
        ("E-2026-2", None, "unconfirmed", false),
        ("E-2026-2", Some("12700"), "unconfirmed", false),
    ] {
        let server = MockServer::start().await;
        respond(&server, invoice_reply(number, gross)).await;
        original_invoice(&server, 3).await;
        // Repeat exactly the same call: the server echoes the existing storno.
        for json_output in [true, false, true] {
            let mut args = vec!["invoice", "storno", "E-2026-1"];
            if json_output {
                args.push("--json");
            }
            let output = invoke(&server, &args, None).await;
            assert_eq!(output.status.success(), success, "{output:?}");
            if json_output {
                let value = report(&output);
                assert_eq!(value["remote"]["outcome"], outcome);
                assert_eq!(value["remote"]["original_number"], "E-2026-1");
                assert_eq!(value["remote"]["document"]["invoice_number"], number);
            } else {
                assert!(text(&output.stdout).contains(outcome), "{output:?}");
                if outcome == "unconfirmed" {
                    assert!(!text(&output.stdout).contains("nothing was reversed"));
                }
            }
        }
        assert_eq!(server.received_requests().await.expect("requests").len(), 6);
    }
}

#[tokio::test]
async fn stdout_pdf_is_separate_from_human_and_json_reports() {
    for json_output in [false, true] {
        for (operation, input, reply) in [
            (
                "invoice",
                invoice(),
                invoice_reply("E-2026-1", Some("12700")),
            ),
            ("receipt", receipt(), receipt_reply(false)),
        ] {
            let server = MockServer::start().await;
            respond(&server, reply).await;
            let mut args = vec![operation, "create", "-f", "-", "--pdf", "-"];
            if json_output {
                args.push("--json");
            }
            let output = invoke(&server, &args, Some(&input)).await;
            assert!(output.status.success(), "{output:?}");
            assert_eq!(output.stdout, PDF);
            if json_output {
                let value: Value = serde_json::from_slice(&output.stderr).expect("stderr JSON");
                assert_eq!(value["pdf_output"]["status"], "written");
                assert_eq!(value["pdf_output"]["target"], "-");
            } else {
                assert!(text(&output.stderr).contains("2026-1"), "{output:?}");
            }
        }
    }
}

#[tokio::test]
async fn preview_and_missing_pdf_keep_their_remote_meaning() {
    let dir = tempfile::tempdir().expect("temp directory");
    let target = dir.path().to_str().expect("path");
    let server = MockServer::start().await;
    respond(
        &server,
        format!(
            "<xmlszamlavalasz xmlns=\"http://www.szamlazz.hu/xmlszamlavalasz\">\
             <sikeres>true</sikeres><pdf>{PDF_BASE64}</pdf></xmlszamlavalasz>"
        ),
    )
    .await;
    let mut input = invoice();
    input["header"]["preview_pdf"] = json!(true);
    let output = invoke(
        &server,
        &["invoice", "create", "-f", "-", "--pdf", target, "--json"],
        Some(&input),
    )
    .await;
    assert!(!output.status.success());
    assert!(report(&output)["remote"]["preview"].is_object());
    assert!(report(&output)["remote"].get("issued").is_none());
    assert_eq!(report(&output)["pdf_output"]["status"], "failed");

    server.reset().await;
    respond(
        &server,
        invoice_reply("E-2026-1", Some("12700")).replace(&format!("<pdf>{PDF_BASE64}</pdf>"), ""),
    )
    .await;
    let path = dir.path().join("missing.pdf");
    let output = invoke(
        &server,
        &[
            "invoice",
            "create",
            "-f",
            "-",
            "--pdf",
            path.to_str().expect("path"),
            "--json",
        ],
        Some(&invoice()),
    )
    .await;
    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        report(&output)["remote"]["issued"]["invoice_number"],
        "E-2026-1"
    );
    assert_eq!(report(&output)["pdf_output"]["status"], "missing");
    assert!(!path.exists());
    assert!(text(&output.stderr).contains("contained none"));
}

#[cfg(unix)]
#[tokio::test]
async fn non_utf8_pdf_paths_preserve_remote_results_on_success_and_failure() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt as _;

    let dir = tempfile::tempdir().expect("temp directory");
    for non_utf8 in [false, true] {
        for fail in [false, true] {
            let server = MockServer::start().await;
            respond(&server, invoice_reply("E-2026-1", Some("12700"))).await;
            let name = if non_utf8 {
                OsStr::from_bytes(b"invoice-\xff.pdf")
            } else {
                OsStr::new("invoice.pdf")
            };
            let target = if fail {
                dir.path().join("absent-parent").join(name)
            } else {
                dir.path().join(name)
            };
            let mut cmd = command(&server, &["invoice", "create", "-f", "-", "--json"]);
            cmd.arg("--pdf").arg(&target);
            let output = run(&mut cmd, Some(&invoice().to_string())).await;
            assert_eq!(output.status.code(), Some(i32::from(fail)), "{output:?}");
            let value = report(&output);
            assert_eq!(value["remote"]["issued"]["invoice_number"], "E-2026-1");
            assert_eq!(value["pdf_output"]["target"], target.display().to_string());
            assert_eq!(
                value["pdf_output"]["status"],
                if fail { "failed" } else { "written" }
            );
            if fail {
                assert!(value["pdf_output"]["error"].is_string());
                assert!(text(&output.stderr).contains("local PDF output failed"));
                assert!(!target.exists());
            } else {
                assert_eq!(std::fs::read(&target).expect("PDF at original path"), PDF);
            }
            assert!(!text(&output.stderr).contains("panicked"), "{output:?}");
            assert_eq!(server.received_requests().await.expect("requests").len(), 1);
        }
    }
}

#[cfg(target_os = "linux")]
#[tokio::test]
async fn failed_stdout_pdf_keeps_the_remote_report_on_stderr() {
    for json_output in [false, true] {
        let server = MockServer::start().await;
        respond(&server, invoice_reply("E-2026-1", Some("12700"))).await;
        let mut args = vec!["invoice", "create", "-f", "-", "--pdf", "-"];
        if json_output {
            args.push("--json");
        }
        let mut cmd = command(&server, &args);
        cmd.stdout(
            std::fs::OpenOptions::new()
                .write(true)
                .open("/dev/full")
                .expect("full device"),
        );
        let output = run(&mut cmd, Some(&invoice().to_string())).await;
        assert!(!output.status.success(), "{output:?}");
        assert!(output.stdout.is_empty());
        assert!(text(&output.stderr).contains("E-2026-1"), "{output:?}");
        assert!(
            text(&output.stderr).contains("local PDF output failed"),
            "{output:?}"
        );
        if json_output {
            // The report is followed by main's exit diagnostic, not PDF bytes.
            let value = serde_json::Deserializer::from_slice(&output.stderr)
                .into_iter::<Value>()
                .next()
                .expect("JSON report")
                .expect("valid JSON");
            assert_eq!(value["remote"]["issued"]["invoice_number"], "E-2026-1");
            assert_eq!(value["pdf_output"]["status"], "failed");
        }
        assert_eq!(server.received_requests().await.expect("requests").len(), 1);
    }
}
