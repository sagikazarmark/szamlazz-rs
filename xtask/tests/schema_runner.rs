//! Real-xmllint regressions run explicitly in the dedicated schema container.
#![allow(clippy::unwrap_used)]
use std::path::{Path, PathBuf};
use xtask::schemas::{schema_paths, validate};

fn corpus() -> PathBuf {
    std::env::var_os("SZAMLAZZ_SCHEMA_CORPUS")
        .map_or_else(xtask::schemas::default_corpus, PathBuf::from)
}

fn receipt(extra: &str, pdf: &str) -> String {
    format!(
        "<xmlnyugtaget xmlns=\"http://www.szamlazz.hu/xmlnyugtaget\"><beallitasok><pdfLetoltes>{pdf}</pdfLetoltes></beallitasok><fejlec><nyugtaszam>NY-1</nyugtaszam>{extra}</fejlec></xmlnyugtaget>"
    )
}

async fn check(source: &str, xml: &str, expected: &[&str]) -> anyhow::Result<String> {
    validate(
        Path::new("xmllint"),
        &corpus().join(format!("{source}/xmlnyugtaget.xsd")),
        xml,
        &expected.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
    )
    .await
}

#[tokio::test]
#[ignore = "requires xmllint; run in ci.schemas or cargo test -p xtask --test schema_runner -- --include-ignored"]
async fn source_conflict_and_valid_counterpart() {
    let xml = receipt("<rendelesSzam>O-1</rendelesSzam>", "false");
    check("en-inline", &xml, &[]).await.unwrap();
    check("download", &xml, &["rendelesSzam"]).await.unwrap();
}

#[tokio::test]
#[ignore = "requires xmllint"]
async fn expected_conflict_cannot_hide_another_error() {
    let error = check(
        "download",
        &receipt("<rendelesSzam>O-1</rendelesSzam>", "not-a-boolean"),
        &["rendelesSzam"],
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unrecognized validation failure"),
        "{error:#}"
    );
}

#[tokio::test]
#[ignore = "requires xmllint"]
async fn wrong_namespace_is_not_the_expected_conflict() {
    let error = check(
        "download",
        &receipt(
            "<rendelesSzam xmlns=\"urn:wrong\">O-1</rendelesSzam>",
            "false",
        ),
        &["rendelesSzam"],
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("unrecognized validation failure"),
        "{error:#}"
    );
}

#[tokio::test]
#[ignore = "requires xmllint"]
async fn unexpected_success_fails() {
    let error = check("download", &receipt("", "false"), &["rendelesSzam"])
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("expected XSD invalidity"),
        "{error:#}"
    );
}

#[tokio::test]
#[ignore = "requires xmllint"]
async fn schema_failures_cannot_count_as_conflicts() {
    let directory = tempfile::tempdir().unwrap();
    let schema = directory.path().join("xmlnyugtaget.xsd");
    for body in [
        None,
        Some("<schema"),
        Some(
            "<schema xmlns=\"http://www.w3.org/2001/XMLSchema\"><element name=\"x\" type=\"nonexistent\"/></schema>",
        ),
    ] {
        if let Some(body) = body {
            std::fs::write(&schema, body).unwrap();
        }
        let error = validate(
            Path::new("xmllint"),
            &schema,
            &receipt("", "false"),
            &["rendelesSzam".to_owned()],
        )
        .await
        .unwrap_err();
        assert!(
            error.to_string().contains("expected XSD invalidity"),
            "{error:#}"
        );
    }
}

#[tokio::test]
#[ignore = "requires xmllint"]
async fn malformed_instance_cannot_count_as_conflict() {
    let error = check("download", "<xmlnyugtaget", &["rendelesSzam"])
        .await
        .unwrap_err();
    assert!(
        error.to_string().contains("expected XSD invalidity"),
        "{error:#}"
    );
}

#[test]
fn coverage_expands_types_at_each_use_not_at_schema_root() {
    let schema = "<schema xmlns=\"http://www.w3.org/2001/XMLSchema\"><complexType name=\"row\"><sequence><element name=\"value\" type=\"string\"/></sequence></complexType><element name=\"root\"><complexType><all><element name=\"left\" type=\"tns:row\"/><element name=\"right\" type=\"tns:row\"/></all></complexType></element></schema>";
    assert_eq!(
        schema_paths(schema).unwrap(),
        [
            "root",
            "root/left",
            "root/left/value",
            "root/right",
            "root/right/value"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
}

#[tokio::test]
async fn missing_validator_fails_before_cargo() {
    let empty = tempfile::tempdir().unwrap();
    let executable =
        std::env::var_os("XTASK_BIN").unwrap_or_else(|| env!("CARGO_BIN_EXE_xtask").into());
    let output = tokio::process::Command::new(executable)
        .arg("check-agent-schemas")
        .env("PATH", empty.path())
        .output()
        .await
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("xmllint is required"));
}
