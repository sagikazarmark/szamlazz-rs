//! Output helpers: human-readable by default, `--json` for scripts.

use std::fmt::{Arguments, Display};
use std::path::Path;

use anyhow::Context as _;

/// Whether a PDF sink refers to stdout (`-`), in which case a summary printed
/// to stdout would corrupt the PDF stream.
pub fn is_stdout(target: &Path) -> bool {
    target == Path::new("-")
}

/// A summary sink. Routes to stderr when stdout is occupied by a PDF stream,
/// so the two never interleave on the same stream; otherwise to stdout.
pub struct Report {
    stderr: bool,
}

/// A report sink; pass `true` when a PDF is being written to stdout.
pub fn report(pdf_on_stdout: bool) -> Report {
    Report {
        stderr: pdf_on_stdout,
    }
}

impl Report {
    fn line(&self, args: Arguments) {
        if self.stderr {
            eprintln!("{args}");
        } else {
            println!("{args}");
        }
    }

    /// Prints a labeled field when it has a value.
    pub fn field<T: Display + ?Sized>(&self, label: &str, value: Option<&T>) {
        if let Some(value) = value {
            self.line(format_args!("{label:<22} {value}"));
        }
    }

    /// Prints a required labeled field.
    pub fn field_required(&self, label: &str, value: &dyn Display) {
        self.line(format_args!("{label:<22} {value}"));
    }

    /// Serializes a value as pretty JSON.
    pub fn json<T: serde::Serialize>(&self, value: &T) -> anyhow::Result<()> {
        self.line(format_args!("{}", serde_json::to_string_pretty(value)?));

        Ok(())
    }
}

/// Prints a labeled field to stdout when it has a value.
pub fn field<T: Display + ?Sized>(label: &str, value: Option<&T>) {
    report(false).field(label, value);
}

/// Prints a required labeled field to stdout.
pub fn field_required(label: &str, value: &dyn Display) {
    report(false).field_required(label, value);
}

/// Serializes a value as pretty JSON to stdout.
pub fn json<T: serde::Serialize>(value: &T) -> anyhow::Result<()> {
    report(false).json(value)
}

/// Warns (on stderr) when a PDF was requested but the response carried none, so
/// the caller does not silently believe a file was written.
pub fn warn_missing_pdf(requested: bool, present: bool) {
    if requested && !present {
        eprintln!("warning: a PDF was requested but the response contained none");
    }
}

/// Writes a PDF where the user asked: a file path, or stdout for `-`.
pub fn write_pdf(pdf: &[u8], target: &Path) -> anyhow::Result<()> {
    use std::io::Write as _;
    if is_stdout(target) {
        let mut stdout = std::io::stdout().lock();
        stdout.write_all(pdf).context("writing PDF to stdout")?;
        stdout.flush().context("flushing PDF to stdout")?;
    } else {
        std::fs::write(target, pdf)
            .with_context(|| format!("writing PDF to {}", target.display()))?;
        eprintln!("PDF written to {}", target.display());
    }

    Ok(())
}

/// Reports the remote result even when saving its PDF fails. Local output is
/// a separate part of the JSON contract, never a replacement for that result.
pub fn document<T: serde::Serialize>(
    json: bool,
    remote: &T,
    pdf: Option<&szamlazz_agent::Pdf>,
    target: Option<&Path>,
    human: impl FnOnce(&Report),
) -> anyhow::Result<()> {
    #[derive(serde::Serialize)]
    #[serde(tag = "status", rename_all = "snake_case")]
    enum PdfOutput {
        NotRequested,
        Missing { target: String },
        Written { target: String },
        Failed { target: String, error: String },
    }

    #[derive(serde::Serialize)]
    struct DocumentReport<'a, T> {
        remote: &'a T,
        pdf_output: PdfOutput,
    }

    // The filesystem gets the original path; JSON gets its lossy display
    // representation so non-UTF-8 paths cannot hide an issued document.
    let (pdf_output, result) = match (target, pdf) {
        (Some(target), Some(pdf)) => match write_pdf(pdf.as_bytes(), target) {
            Ok(()) => (
                PdfOutput::Written {
                    target: target.display().to_string(),
                },
                Ok(()),
            ),
            Err(error) => (
                PdfOutput::Failed {
                    target: target.display().to_string(),
                    error: format!("{error:#}"),
                },
                Err(error.context("local PDF output failed; the remote result above still applies")),
            ),
        },
        (Some(target), None) => {
            warn_missing_pdf(true, false);
            (
                PdfOutput::Missing {
                    target: target.display().to_string(),
                },
                Ok(()),
            )
        }
        (None, _) => (PdfOutput::NotRequested, Ok(())),
    };
    let out = report(target.is_some_and(is_stdout));
    if json {
        out.json(&DocumentReport { remote, pdf_output })?;
    } else {
        human(&out);
        if let PdfOutput::Failed { error, .. } = &pdf_output {
            out.field_required("PDF output failed", error);
        }
    }
    result
}

/// Reads a JSON input document: a file path, or stdin for `-`.
pub fn read_json_input<T: serde::de::DeserializeOwned>(path: &Path) -> anyhow::Result<T> {
    let content = if path == Path::new("-") {
        use std::io::Read as _;
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .context("reading JSON from stdin")?;
        buffer
    } else {
        std::fs::read_to_string(path)
            .with_context(|| format!("reading JSON from {}", path.display()))?
    };

    let parse = || -> anyhow::Result<T> {
        let mut deserializer = serde_json::Deserializer::from_str(&content);
        let mut ignored = Vec::new();
        // Wrap the actual deserialize traversal, including enum payloads and
        // custom serde collections (e.g. InvoiceAttachments), rather than
        // maintaining a second list of the library's request fields.
        let value = serde_ignored::deserialize(&mut deserializer, |path| {
            ignored.push(path.to_string());
        })?;
        deserializer.end()?;
        anyhow::ensure!(
            ignored.is_empty(),
            "unknown JSON field(s): {}",
            ignored.join(", ")
        );
        Ok(value)
    };
    parse().with_context(|| {
        if path == Path::new("-") {
            "parsing JSON from stdin".to_owned()
        } else {
            format!("parsing JSON from {}", path.display())
        }
    })
}
