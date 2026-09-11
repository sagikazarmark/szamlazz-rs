#!/usr/bin/env python3
"""Required, offline XSD 1.0 check. Python stdlib orchestrates; xmllint validates.

No fixture acquisition happens here. Missing tools/corpus, schema compilation
errors, unexpected validity, and unrecognized validation diagnostics all fail.
"""

import collections
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import xml.etree.ElementTree as ET

ROOT = Path(__file__).resolve().parents[1]
CORPUS = ROOT / "fixtures/upstream/agent/request-xsd-2026-09-11"
OPERATIONS = {
    "xmlszamla", "xmlszamlast", "xmlszamlakifiz", "xmlszamlapdf", "xmlszamlaxml",
    "xmlszamladbkdel", "xmlnyugtacreate", "xmlnyugtast", "xmlnyugtaget",
    "xmlnyugtasend", "xmltaxpayer",
}
XS = "{http://www.w3.org/2001/XMLSchema}"


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def schema_paths(schema):
    """Coverage accounting only. Actual validity is always xmllint's verdict."""
    types = {node.attrib["name"]: node for node in schema.findall(XS + "complexType")}

    def walk(node, prefix):
        for child in node:
            if child.tag == XS + "element":
                path = (*prefix, child.attrib["name"])
                yield path
                type_name = child.get("type", "").removeprefix("tns:")
                yield from walk(types.get(type_name, child), path)
            elif child.tag in {XS + "complexType", XS + "sequence", XS + "all"} and not (node is schema and child.get("name")):
                yield from walk(child, prefix)

    return set(walk(schema, ()))


def document_paths(node, prefix=()):
    path = (*prefix, node.tag.split("}")[-1])
    yield path
    for child in node:
        yield from document_paths(child, path)


def run_validator(executable, schema, xml):
    return subprocess.run(
        [executable, "--nonet", "--noout", "--schema", str(schema), "-"],
        input=xml.encode(), capture_output=True, timeout=15,
        env={**os.environ, "LC_ALL": "C", "XML_CATALOG_FILES": "", "SGML_CATALOG_FILES": ""},
    )


def validate(executable, schema, xml, expected):
    result = run_validator(executable, schema, xml)
    diagnostic = result.stderr.decode(errors="replace")
    if not expected:
        require(result.returncode == 0, diagnostic)
    else:
        # Exit 3 is document invalidity. Missing files, malformed XML/schema,
        # failed compilation, crashes, etc. cannot satisfy an expected conflict.
        require(result.returncode == 3, f"expected XSD invalidity (3), got {result.returncode}: {diagnostic}")
        errors = [line for line in diagnostic.splitlines() if "Schemas validity error" in line]
        actual = []
        for line in errors:
            namespace = re.escape(f"http://www.szamlazz.hu/{schema.stem}")
            match = re.search(rf"Schemas validity error : Element '\{{{namespace}\}}([^']+)': This element is not expected\.", line)
            require(match is not None, f"unrecognized validation failure: {line}")
            actual.append(match[1])
        require(collections.Counter(actual) == collections.Counter(expected),
                f"expected only {expected}, got {actual}: {diagnostic}")
    return diagnostic


def main():
    executable = shutil.which("xmllint")
    require(executable, "xmllint is required (devenv shell; or install libxml2-utils). No validation ran.")
    version = subprocess.run([executable, "--version"], capture_output=True, text=True, check=True)
    print(version.stderr.strip() or version.stdout.strip(), flush=True)
    sources = json.loads((CORPUS / "sources.json").read_text(encoding="utf-8"))
    expected_sources = {f"{label}/{root}.xsd" for root in OPERATIONS for label in ["en-inline", "download"]}
    require(set(sources) == expected_sources, "source inventory must cover both sources for all 11 actions")
    require({str(p.relative_to(CORPUS)) for p in CORPUS.rglob("*.xsd")} == expected_sources,
            "schema files and provenance inventory differ")
    paths = {}
    for name, source in sources.items():
        body = (CORPUS / name).read_bytes()
        require(hashlib.sha256(body).hexdigest() == source["sha256"], f"checksum changed: {name}")
        require(b"<!DOCTYPE" not in body and b"<!ENTITY" not in body, f"external entity declaration: {name}")
        schema = ET.fromstring(body)
        require(not any(node.tag in {XS + "include", XS + "import", XS + "redefine"}
                        for node in schema.iter()), f"schema is not self-contained: {name}")
        paths[name] = schema_paths(schema)
        print(f"SOURCE {name} sha256={source['sha256']} {source['source']}", flush=True)

    with tempfile.TemporaryDirectory(prefix="szamlazz-xsd-") as directory:
        output = Path(directory) / "requests.json"
        subprocess.run(
            ["cargo", "test", "-p", "szamlazz-agent", "--locked", "--offline", "--test", "schema_requests",
             "--", "--ignored", "--exact", "emit_request_matrix"],
            cwd=ROOT, env={**os.environ, "SZAMLAZZ_SCHEMA_OUTPUT": str(output)}, check=True,
        )
        requests = json.loads(output.read_text(encoding="utf-8"))
    require({r["root"] for r in requests} == OPERATIONS, "generated matrix misses an operation")
    identities = {(r["root"], r["name"], r["auth"]) for r in requests}
    require(len(identities) == len(requests), "duplicate matrix case")
    for root, name, _ in identities:
        require({auth for r, n, auth in identities if (r, n) == (root, name)} == {"key", "password"},
                f"missing credential form: {root}/{name}")

    failures = []
    counts = collections.Counter()
    coverage = collections.defaultdict(set)
    for request in requests:
        root = request["root"]
        xml = request["xml"]
        require("<!DOCTYPE" not in xml and "<!ENTITY" not in xml, "generated external entity declaration")
        for label in ["en-inline", "download"]:
            name = f"{label}/{root}.xsd"
            expected = request["errors"][label]
            case = f"{name} {request['name']} [{request['auth']}]"
            try:
                diagnostic = validate(executable, CORPUS / name, xml, expected)
                verdict = "EXPECTED-SOURCE-CONFLICT" if expected else "VALID"
                counts[verdict] += 1
                print(f"{verdict} {case}" + (f" ({', '.join(expected)})" if expected else ""))
                if expected:
                    print(diagnostic.rstrip())
                else:
                    coverage[name].update(document_paths(ET.fromstring(xml)))
            except (RuntimeError, subprocess.TimeoutExpired) as error:
                failures.append(f"{case}: {error}")
    # Every declared element path must occur in a fully valid document for
    # that source. Known-invalid all-options rows cannot earn coverage.
    for name, declared in paths.items():
        missing = declared - coverage[name]
        if missing:
            failures.append(f"{name}: paths never validated successfully: {sorted('/'.join(p) for p in missing)}")
        else:
            print(f"COVERAGE {name}: {len(declared)}/{len(declared)} declared element paths in valid requests")

    # Negative controls prove the validator enforces content, sequence and
    # cardinality rather than merely accepting well-formed XML outlines.
    controls = [
        ("xmlszamla", "minimal", "<eszamla>false</eszamla>", "<eszamla>not-bool</eszamla>"),
        ("xmlszamla", "minimal", "<elado></elado>", ""),
        ("xmltaxpayer", "01234567", "01234567", "0123456x"),
        ("xmlszamlaxml", "number-pdf-false", "<pdf>false</pdf>", "<pdf>false</pdf><pdf>true</pdf>"),
        ("xmlszamla", "minimal", "<nettoErtek>100</nettoErtek>", "<nettoErtek>not-money</nettoErtek>"),
        ("xmlszamla", "erasure-0", "<torloKod>0</torloKod>", "<torloKod>-1</torloKod>"),
        ("xmlszamlapdf", "order", "<rendelesSzam>O-1&lt;&amp;</rendelesSzam><valaszVerzio>2</valaszVerzio>",
         "<valaszVerzio>2</valaszVerzio><rendelesSzam>O-1&lt;&amp;</rendelesSzam>"),
    ]
    for root, name, old, new in controls:
        xml = next(r["xml"] for r in requests if (r["root"], r["name"], r["auth"]) == (root, name, "key"))
        require(old in xml, f"negative control no longer mutates its input: {root}/{name}")
        result = run_validator(executable, CORPUS / f"en-inline/{root}.xsd", xml.replace(old, new, 1))
        require(result.returncode == 3, f"negative XSD control did not fail validation: {root}/{name}")
    print(f"\n{len(requests)} generated requests; {dict(counts)}; {len(controls)} negative controls")
    print("GAP HU PDF inline: malformed XML and conflicting selector optionality/order; see docs/testing.md.")
    require(not failures, "\n".join(failures))


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.SubprocessError) as error:
        sys.exit(f"SCHEMA CHECK FAILED: {error}")
