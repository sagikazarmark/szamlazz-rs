#!/usr/bin/env python3
"""Targeted runner regressions with real xmllint; no Cargo or network calls."""

from pathlib import Path
import runpy
import shutil
import tempfile
import unittest
from unittest.mock import patch
import xml.etree.ElementTree as ET

RUNNER = runpy.run_path(str(Path(__file__).with_name("check-agent-schemas.py")))
VALIDATE = RUNNER["validate"]
CORPUS = RUNNER["CORPUS"]
XMLLINT = shutil.which("xmllint")
if XMLLINT is None:
    raise SystemExit("xmllint is required for runner regressions; no validation ran")


class SchemaRunnerTests(unittest.TestCase):
    def receipt(self, extra="", pdf="false"):
        return (
            '<xmlnyugtaget xmlns="http://www.szamlazz.hu/xmlnyugtaget">'
            f'<beallitasok><pdfLetoltes>{pdf}</pdfLetoltes></beallitasok>'
            f'<fejlec><nyugtaszam>NY-1</nyugtaszam>{extra}</fejlec></xmlnyugtaget>'
        )

    def test_source_conflict_and_valid_counterpart(self):
        xml = self.receipt("<rendelesSzam>O-1</rendelesSzam>")
        VALIDATE(XMLLINT, CORPUS / "en-inline/xmlnyugtaget.xsd", xml, [])
        VALIDATE(XMLLINT, CORPUS / "download/xmlnyugtaget.xsd", xml, ["rendelesSzam"])

    def test_expected_conflict_cannot_hide_another_error(self):
        xml = self.receipt("<rendelesSzam>O-1</rendelesSzam>", pdf="not-a-boolean")
        with self.assertRaisesRegex(RuntimeError, "unrecognized validation failure"):
            VALIDATE(XMLLINT, CORPUS / "download/xmlnyugtaget.xsd", xml, ["rendelesSzam"])

    def test_wrong_namespace_is_not_the_expected_conflict(self):
        xml = self.receipt('<rendelesSzam xmlns="urn:wrong">O-1</rendelesSzam>')
        with self.assertRaisesRegex(RuntimeError, "unrecognized validation failure"):
            VALIDATE(XMLLINT, CORPUS / "download/xmlnyugtaget.xsd", xml, ["rendelesSzam"])

    def test_unexpected_success_fails(self):
        with self.assertRaisesRegex(RuntimeError, "expected XSD invalidity"):
            VALIDATE(XMLLINT, CORPUS / "download/xmlnyugtaget.xsd", self.receipt(), ["rendelesSzam"])

    def test_schema_failures_cannot_count_as_conflicts(self):
        with tempfile.TemporaryDirectory(prefix="schema-runner-") as directory:
            schema = Path(directory) / "xmlnyugtaget.xsd"
            for body in [None, "<schema", '<schema xmlns="http://www.w3.org/2001/XMLSchema"><element name="x" type="nonexistent"/></schema>']:
                if body is not None:
                    schema.write_text(body, encoding="utf-8")
                with self.subTest(body=body), self.assertRaisesRegex(RuntimeError, "expected XSD invalidity"):
                    VALIDATE(XMLLINT, schema, self.receipt(), ["rendelesSzam"])

    def test_malformed_instance_cannot_count_as_conflict(self):
        with self.assertRaisesRegex(RuntimeError, "expected XSD invalidity"):
            VALIDATE(XMLLINT, CORPUS / "download/xmlnyugtaget.xsd", "<xmlnyugtaget", ["rendelesSzam"])

    def test_coverage_expands_types_at_each_use_not_at_schema_root(self):
        schema = ET.fromstring('''<schema xmlns="http://www.w3.org/2001/XMLSchema">
          <complexType name="row"><sequence><element name="value" type="string"/></sequence></complexType>
          <element name="root"><complexType><all>
            <element name="left" type="tns:row"/><element name="right" type="tns:row"/>
          </all></complexType></element>
        </schema>''')
        self.assertEqual(RUNNER["schema_paths"](schema), {
            ("root",), ("root", "left"), ("root", "left", "value"),
            ("root", "right"), ("root", "right", "value"),
        })

    def test_missing_validator_fails_before_cargo(self):
        with patch("shutil.which", return_value=None), patch("subprocess.run") as run:
            with self.assertRaisesRegex(RuntimeError, "xmllint is required"):
                RUNNER["main"]()
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
