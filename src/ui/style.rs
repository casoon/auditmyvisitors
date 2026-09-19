//! Terminal presentation policy.
//!
//! One `Console` for the whole process, so the decision "is this a terminal, does
//! `NO_COLOR` apply" is made once. Every styled string in the CLI goes through the
//! [`Paint`] trait; no module emits ANSI codes on its own.
//!
//! The trait records *intent* at the call site — a heading, a warning, a muted
//! aside — and this module maps that intent onto runemark's semantic tones. Two
//! intents may share a tone today (`heading` and `strong` both render as
//! [`Tone::Title`]) and can be told apart later without touching call sites.

use std::fmt::Display;
use std::sync::OnceLock;

use runemark::{ColorMode, Console, Tone};

/// The process-wide console for standard output.
///
/// `ColorMode::Auto` styles an interactive terminal and stays plain when output is
/// redirected or `NO_COLOR` is set.
pub fn console() -> Console {
    static CONSOLE: OnceLock<Console> = OnceLock::new();
    *CONSOLE.get_or_init(|| Console::stdout(ColorMode::Auto))
}

/// Semantic styling for anything printable.
pub trait Paint {
    /// Section heading, e.g. `OVERVIEW` or `TOP PAGES`.
    fn heading(&self) -> String;
    /// Inline emphasis inside a line — a page URL, an insight headline.
    fn strong(&self) -> String;
    /// A value or command the reader is meant to notice or type.
    fn accent(&self) -> String;
    /// Something went well, or a positive finding.
    fn ok(&self) -> String;
    /// Something needs attention but is not broken.
    fn warn(&self) -> String;
    /// Something failed or is broken.
    fn err(&self) -> String;
    /// Secondary information: hints, units, explanations.
    fn muted(&self) -> String;
}

impl<T: Display + ?Sized> Paint for T {
    fn heading(&self) -> String {
        console().paint(Tone::Title, self)
    }

    fn strong(&self) -> String {
        console().paint(Tone::Title, self)
    }

    fn accent(&self) -> String {
        console().paint(Tone::Info, self)
    }

    fn ok(&self) -> String {
        console().paint(Tone::Success, self)
    }

    fn warn(&self) -> String {
        console().paint(Tone::Warning, self)
    }

    fn err(&self) -> String {
        console().paint(Tone::Error, self)
    }

    fn muted(&self) -> String {
        console().paint(Tone::Muted, self)
    }
}
