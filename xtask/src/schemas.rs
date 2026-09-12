//! Offline request XSD validation. Only xmllint decides document validity.
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::Duration,
};

use anyhow::{Context, Result, bail, ensure};
use roxmltree::{Document, Node};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::{io::AsyncWriteExt, process::Command};

const OPERATIONS: [&str; 11] = [
    "xmlszamla",
    "xmlszamlast",
    "xmlszamlakifiz",
    "xmlszamlapdf",
    "xmlszamlaxml",
    "xmlszamladbkdel",
    "xmlnyugtacreate",
    "xmlnyugtast",
    "xmlnyugtaget",
    "xmlnyugtasend",
    "xmltaxpayer",
];
const LABELS: [&str; 2] = ["en-inline", "download"];
const XS: &str = "http://www.w3.org/2001/XMLSchema";
const CORPUS: &str = "fixtures/upstream/agent/request-xsd-2026-09-11";

/// The checked-in schema corpus used by both the checker and its regressions.
#[must_use]
pub fn default_corpus() -> PathBuf {
    crate::workspace().join(CORPUS)
}

#[derive(Deserialize)]
struct Source {
    sha256: String,
    source: String,
}

#[derive(Deserialize)]
struct Request {
    root: String,
    name: String,
    auth: String,
    xml: String,
    errors: BTreeMap<String, Vec<String>>,
}

/// Invoke the actual XSD validator with disabled catalogs and a 15-second deadline.
///
/// # Errors
/// Fails on process errors, timeout or input pipe failures.
pub async fn run_validator(executable: &Path, schema: &Path, xml: &str) -> Result<Output> {
    let mut child = Command::new(executable)
        .args(["--nonet", "--noout", "--schema"])
        .arg(schema)
        .arg("-")
        .env("LC_ALL", "C")
        .env("XML_CATALOG_FILES", "")
        .env("SGML_CATALOG_FILES", "")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context("start xmllint")?;
    tokio::time::timeout(Duration::from_secs(15), async {
        let mut stdin = child.stdin.take().context("xmllint stdin")?;
        stdin.write_all(xml.as_bytes()).await?;
        drop(stdin);
        Ok(child.wait_with_output().await?)
    })
    .await
    .context("xmllint timed out")?
}

/// Require either validity or precisely the declared source-conflict diagnostics.
///
/// # Errors
/// Unexpected success, failure, namespace or diagnostic fails the check.
pub async fn validate(
    executable: &Path,
    schema: &Path,
    xml: &str,
    expected: &[String],
) -> Result<String> {
    let output = run_validator(executable, schema, xml).await?;
    let diagnostic = String::from_utf8_lossy(&output.stderr).into_owned();
    if expected.is_empty() {
        ensure!(output.status.success(), "{diagnostic}");
    } else {
        ensure!(
            output.status.code() == Some(3),
            "expected XSD invalidity (3), got {}: {diagnostic}",
            output.status
        );
        let root = schema
            .file_stem()
            .and_then(|s| s.to_str())
            .context("schema filename")?;
        let prefix = format!("Schemas validity error : Element '{{http://www.szamlazz.hu/{root}}}");
        let mut actual = Vec::new();
        for line in diagnostic
            .lines()
            .filter(|line| line.contains("Schemas validity error"))
        {
            let element = line
                .split_once(&prefix)
                // libxml2 may append a list of expected elements to this diagnostic.
                // Match the same error prefix as the original checker.
                .and_then(|(_, rest)| rest.split_once("': This element is not expected."))
                .map(|(element, _)| element);
            let element = element
                .filter(|element| !element.contains('\''))
                .with_context(|| format!("unrecognized validation failure: {line}"))?;
            actual.push(element.to_owned());
        }
        actual.sort();
        let mut expected = expected.to_vec();
        expected.sort();
        ensure!(
            actual == expected,
            "expected only {expected:?}, got {actual:?}: {diagnostic}"
        );
    }
    Ok(diagnostic)
}

fn element(node: Node<'_, '_>, name: &str) -> bool {
    node.has_tag_name((XS, name))
}

/// Enumerate element paths for coverage, expanding named types at each use.
///
/// # Errors
/// Fails on malformed XML, nameless declarations or recursive schema types.
pub fn schema_paths(xml: &str) -> Result<BTreeSet<String>> {
    let document = Document::parse(xml)?;
    let root = document.root_element();
    let types: BTreeMap<_, _> = root
        .children()
        .filter(|n| element(*n, "complexType"))
        .filter_map(|n| n.attribute("name").map(|name| (name, n)))
        .collect();
    let mut paths = BTreeSet::new();
    walk(root, "", &types, &mut paths, 0)?;
    Ok(paths)
}

fn walk<'a>(
    node: Node<'a, 'a>,
    prefix: &str,
    types: &BTreeMap<&str, Node<'a, 'a>>,
    paths: &mut BTreeSet<String>,
    depth: usize,
) -> Result<()> {
    ensure!(depth < 100, "recursive or excessively nested schema");
    for child in node.children().filter(Node::is_element) {
        if element(child, "element") {
            let name = child.attribute("name").context("element without name")?;
            let path = if prefix.is_empty() {
                name.to_owned()
            } else {
                format!("{prefix}/{name}")
            };
            paths.insert(path.clone());
            let name = child
                .attribute("type")
                .unwrap_or("")
                .trim_start_matches("tns:");
            walk(
                types.get(name).copied().unwrap_or(child),
                &path,
                types,
                paths,
                depth + 1,
            )?;
        } else if ["complexType", "sequence", "all"]
            .iter()
            .any(|name| element(child, name))
            && !(element(node, "schema") && child.attribute("name").is_some())
        {
            walk(child, prefix, types, paths, depth + 1)?;
        }
    }
    Ok(())
}

fn document_paths(xml: &str) -> Result<BTreeSet<String>> {
    let document = Document::parse(xml)?;
    Ok(document
        .descendants()
        .filter(Node::is_element)
        .map(|node| {
            let mut names: Vec<_> = node
                .ancestors()
                .filter(Node::is_element)
                .map(|n| n.tag_name().name())
                .collect();
            names.reverse();
            names.join("/")
        })
        .collect())
}

fn no_entities(xml: &str) -> Result<()> {
    ensure!(
        !xml.contains("<!DOCTYPE") && !xml.contains("<!ENTITY"),
        "external entity declaration"
    );
    Ok(())
}

fn xsd_files(root: &Path, directory: &Path, paths: &mut BTreeSet<String>) -> Result<()> {
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            xsd_files(root, &path, paths)?;
        } else if path.extension().is_some_and(|ext| ext == "xsd") {
            paths.insert(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
            );
        }
    }
    Ok(())
}

async fn exporter(output: &Path) -> Result<()> {
    let status = Command::new("cargo")
        .args([
            "test",
            "-p",
            "szamlazz-agent",
            "--locked",
            "--offline",
            "--test",
            "schema_requests",
            "--",
            "--ignored",
            "--exact",
            "emit_request_matrix",
        ])
        .current_dir(crate::workspace())
        .env("SZAMLAZZ_SCHEMA_OUTPUT", output)
        .status()
        .await?;
    ensure!(status.success(), "request exporter failed: {status}");
    Ok(())
}

/// Generate or consume a request matrix and check it against both schema sources.
///
/// # Errors
/// Missing tools/corpus, changed checksums, coverage gaps and unexpected verdicts fail.
pub async fn check(requests: Option<&Path>, corpus: Option<&Path>) -> Result<()> {
    let executable = Path::new("xmllint");
    let version = Command::new(executable)
        .arg("--version")
        .output()
        .await
        .context(
            "xmllint is required (devenv shell; or install libxml2-utils). No validation ran",
        )?;
    ensure!(version.status.success(), "xmllint --version failed");
    println!(
        "{}{}",
        String::from_utf8_lossy(&version.stdout),
        String::from_utf8_lossy(&version.stderr)
    );
    let default_corpus = default_corpus();
    let corpus = corpus.unwrap_or(&default_corpus);
    let sources: BTreeMap<String, Source> =
        serde_json::from_slice(&std::fs::read(corpus.join("sources.json"))?)?;
    let expected_sources: BTreeSet<_> = OPERATIONS
        .iter()
        .flat_map(|root| LABELS.map(|label| format!("{label}/{root}.xsd")))
        .collect();
    ensure!(
        sources.keys().cloned().collect::<BTreeSet<_>>() == expected_sources,
        "source inventory must cover both sources for all 11 actions"
    );
    let mut files = BTreeSet::new();
    xsd_files(corpus, corpus, &mut files)?;
    ensure!(
        files == expected_sources,
        "schema files and provenance inventory differ"
    );
    let mut paths = BTreeMap::new();
    for (name, source) in sources {
        let body = std::fs::read(corpus.join(&name))?;
        ensure!(
            format!("{:x}", Sha256::digest(&body)) == source.sha256,
            "checksum changed: {name}"
        );
        let xml = std::str::from_utf8(&body)?;
        no_entities(xml)?;
        let document = Document::parse(xml)?;
        ensure!(
            !document
                .descendants()
                .any(|node| ["include", "import", "redefine"]
                    .iter()
                    .any(|name| element(node, name))),
            "schema is not self-contained: {name}"
        );
        paths.insert(name.clone(), schema_paths(xml)?);
        println!("SOURCE {name} sha256={} {}", source.sha256, source.source);
    }
    let temp = tempfile::tempdir()?;
    let generated = temp.path().join("requests.json");
    let requests = if let Some(path) = requests {
        path
    } else {
        exporter(&generated).await?;
        &generated
    };
    let requests: Vec<Request> = serde_json::from_slice(&std::fs::read(requests)?)?;
    check_matrix(executable, corpus, &requests, &paths).await
}

async fn check_matrix(
    executable: &Path,
    corpus: &Path,
    requests: &[Request],
    paths: &BTreeMap<String, BTreeSet<String>>,
) -> Result<()> {
    ensure!(
        requests
            .iter()
            .map(|r| r.root.as_str())
            .collect::<BTreeSet<_>>()
            == OPERATIONS.into_iter().collect(),
        "generated matrix misses an operation"
    );
    let identities: BTreeSet<_> = requests
        .iter()
        .map(|r| (&r.root, &r.name, r.auth.as_str()))
        .collect();
    ensure!(identities.len() == requests.len(), "duplicate matrix case");
    for (root, name, _) in &identities {
        ensure!(
            identities
                .iter()
                .filter(|(r, n, _)| r == root && n == name)
                .map(|(_, _, auth)| *auth)
                .collect::<BTreeSet<_>>()
                == ["key", "password"].into_iter().collect(),
            "missing credential form: {root}/{name}"
        );
    }
    let mut failures = Vec::new();
    let mut counts = BTreeMap::<&str, usize>::new();
    let mut coverage = BTreeMap::<String, BTreeSet<String>>::new();
    for request in requests {
        no_entities(&request.xml)?;
        for label in LABELS {
            let name = format!("{label}/{}.xsd", request.root);
            let expected = request
                .errors
                .get(label)
                .context("missing source expectations")?;
            let case = format!("{name} {} [{}]", request.name, request.auth);
            match validate(executable, &corpus.join(&name), &request.xml, expected).await {
                Ok(diagnostic) => {
                    let verdict = if expected.is_empty() {
                        "VALID"
                    } else {
                        "EXPECTED-SOURCE-CONFLICT"
                    };
                    *counts.entry(verdict).or_default() += 1;
                    println!("{verdict} {case}");
                    if expected.is_empty() {
                        coverage
                            .entry(name)
                            .or_default()
                            .extend(document_paths(&request.xml)?);
                    } else {
                        println!("{}", diagnostic.trim_end());
                    }
                }
                Err(error) => failures.push(format!("{case}: {error:#}")),
            }
        }
    }
    for (name, declared) in paths {
        let covered = coverage.entry(name.clone()).or_default();
        let missing: Vec<_> = declared.difference(covered).collect();
        if missing.is_empty() {
            println!(
                "COVERAGE {name}: {}/{} declared element paths in valid requests",
                declared.len(),
                declared.len()
            );
        } else {
            failures.push(format!(
                "{name}: paths never validated successfully: {missing:?}"
            ));
        }
    }
    let controls = negative_controls(executable, corpus, requests).await?;
    println!(
        "\n{} generated requests; {counts:?}; {controls} negative controls",
        requests.len()
    );
    println!(
        "GAP HU PDF inline: malformed XML and conflicting selector optionality/order; see docs/testing.md."
    );
    ensure!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

async fn negative_controls(
    executable: &Path,
    corpus: &Path,
    requests: &[Request],
) -> Result<usize> {
    let controls = [
        (
            "xmlszamla",
            "minimal",
            "<eszamla>false</eszamla>",
            "<eszamla>not-bool</eszamla>",
        ),
        ("xmlszamla", "minimal", "<elado></elado>", ""),
        ("xmltaxpayer", "01234567", "01234567", "0123456x"),
        (
            "xmlszamlaxml",
            "number-pdf-false",
            "<pdf>false</pdf>",
            "<pdf>false</pdf><pdf>true</pdf>",
        ),
        (
            "xmlszamla",
            "minimal",
            "<nettoErtek>100</nettoErtek>",
            "<nettoErtek>not-money</nettoErtek>",
        ),
        (
            "xmlszamla",
            "erasure-0",
            "<torloKod>0</torloKod>",
            "<torloKod>-1</torloKod>",
        ),
        (
            "xmlszamlapdf",
            "order",
            "<rendelesSzam>O-1&lt;&amp;</rendelesSzam><valaszVerzio>2</valaszVerzio>",
            "<valaszVerzio>2</valaszVerzio><rendelesSzam>O-1&lt;&amp;</rendelesSzam>",
        ),
    ];
    for (root, name, old, new) in controls {
        let request = requests
            .iter()
            .find(|r| r.root == root && r.name == name && r.auth == "key")
            .context("missing negative control input")?;
        ensure!(
            request.xml.contains(old),
            "negative control no longer mutates its input: {root}/{name}"
        );
        let result = run_validator(
            executable,
            &corpus.join(format!("en-inline/{root}.xsd")),
            &request.xml.replacen(old, new, 1),
        )
        .await?;
        ensure!(
            result.status.code() == Some(3),
            "negative XSD control did not fail validation: {root}/{name}"
        );
    }
    Ok(controls.len())
}

/// Export the matrix, checker and validator regression binary for Dagger.
///
/// # Errors
/// Fails if Cargo cannot build the test binary or any artifact cannot be exported.
pub async fn prepare(output: &Path) -> Result<()> {
    std::fs::create_dir_all(output)?;
    let output = output.canonicalize()?;
    exporter(&output.join("requests.json")).await?;
    std::fs::copy(std::env::current_exe()?, output.join("xtask"))?;
    let build = Command::new("cargo")
        .args([
            "test",
            "-p",
            "xtask",
            "--locked",
            "--offline",
            "--test",
            "schema_runner",
            "--no-run",
            "--message-format=json",
        ])
        .current_dir(crate::workspace())
        .stderr(Stdio::inherit())
        .output()
        .await?;
    ensure!(build.status.success(), "schema regression build failed");
    for line in String::from_utf8(build.stdout)?.lines() {
        let message: serde_json::Value = serde_json::from_str(line)?;
        if message["reason"] == "compiler-artifact"
            && message["target"]["name"] == "schema_runner"
            && let Some(executable) = message["executable"].as_str()
        {
            std::fs::copy(executable, output.join("schema-runner-tests"))?;
            return Ok(());
        }
    }
    bail!("Cargo did not report the schema regression executable")
}
