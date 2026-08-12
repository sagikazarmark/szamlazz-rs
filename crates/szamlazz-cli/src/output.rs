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
        std::io::stdout()
            .write_all(pdf)
            .context("writing PDF to stdout")?;
    } else {
        std::fs::write(target, pdf)
            .with_context(|| format!("writing PDF to {}", target.display()))?;
        eprintln!("PDF written to {}", target.display());
    }

    Ok(())
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

    serde_json::from_str(&content).with_context(|| {
        if path == Path::new("-") {
            "parsing JSON from stdin".to_owned()
        } else {
            format!("parsing JSON from {}", path.display())
        }
    })
}
