//! `irc-colors` — write IRC-formatted colored text, the fun way.
//!
//! Implements the formatting characters and color table described in
//! [IRC Formatting](https://modern.ircdocs.horse/formatting) (the de facto
//! standard reference for what's understood across IRC clients today,
//! maintained as part of the modern IRC documentation effort). These codes
//! originated in mIRC, but are universal enough now that "mIRC formatting"
//! is a historical name, not an accurate description of scope.
//!
//! ```
//! use irc_colors::Colorize;
//!
//! let msg = "Hello".bold().red() + " world".underline().blue();
//!
//! // background colors, `colored`-crate style
//! let warn = "DANGER".bold().white().on_red();
//!
//! // italics, strikethrough, monospace, and 24-bit color too
//! let fancy = "old price".strikethrough() + " " + "new price".rgb(255, 105, 180);
//!
//! // plain strings mix in for free
//! let line = "Level: " + "critical".bold().red() + "!";
//!
//! // and it splits itself into IRC-safe messages, correctly
//! for chunk in line.wrap(400) {
//!     println!("{}", chunk); // ready to send as a PRIVMSG
//! }
//! # let _ = (warn, fancy);
//! ```

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::borrow::Cow;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;
use core::ops::{Add, AddAssign};
use unicode_segmentation::UnicodeSegmentation;

// ---------------------------------------------------------------------------
// Colors
// ---------------------------------------------------------------------------

/// The 16 standard IRC colors (codes 0-15), per the
/// [IRC Formatting](https://modern.ircdocs.horse/formatting#colors) color table.
///
/// Note codes 6, 10, and 11 are worth calling out: some older documentation
/// (and mIRC itself) names them Purple/Teal/Cyan respectively, but the
/// codified naming is Magenta/Cyan/Light Cyan, which is what's used here.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    White = 0,
    Black = 1,
    Blue = 2,
    Green = 3,
    Red = 4,
    Brown = 5,
    Magenta = 6,
    Orange = 7,
    Yellow = 8,
    LightGreen = 9,
    Cyan = 10,
    LightCyan = 11,
    LightBlue = 12,
    Pink = 13,
    Grey = 14,
    LightGrey = 15,
}

impl Color {
    const ALL: [Color; 16] = [
        Color::White,
        Color::Black,
        Color::Blue,
        Color::Green,
        Color::Red,
        Color::Brown,
        Color::Magenta,
        Color::Orange,
        Color::Yellow,
        Color::LightGreen,
        Color::Cyan,
        Color::LightCyan,
        Color::LightBlue,
        Color::Pink,
        Color::Grey,
        Color::LightGrey,
    ];

    pub const fn code(self) -> u8 {
        self as u8
    }

    pub const fn from_code(code: u8) -> Option<Color> {
        if (code as usize) < Self::ALL.len() {
            Some(Self::ALL[code as usize])
        } else {
            None
        }
    }

    /// The canonical RGB value for this palette color, per mIRC's own
    /// published color table (the same values IRC clients render these
    /// indices as). Used internally so a palette color never has to be
    /// silently dropped when it ends up sharing a span with an RGB color —
    /// see [`ColorValue`].
    pub const fn to_rgb(self) -> (u8, u8, u8) {
        use Color::*;
        match self {
            White => (255, 255, 255),
            Black => (0, 0, 0),
            Blue => (0, 0, 127),
            Green => (0, 147, 0),
            Red => (255, 0, 0),
            Brown => (127, 0, 0),
            Magenta => (156, 0, 156),
            Orange => (252, 127, 0),
            Yellow => (255, 255, 0),
            LightGreen => (0, 252, 0),
            Cyan => (0, 147, 147),
            LightCyan => (0, 255, 255),
            LightBlue => (0, 0, 252),
            Pink => (255, 0, 255),
            Grey => (127, 127, 127),
            LightGrey => (210, 210, 210),
        }
    }
}

/// Either a palette [`Color`] (the classic 16-color set, sent with the
/// `\x03` code) or a 24-bit RGB value (sent with the newer `\x04` hex-color
/// code from [IRC Formatting](https://modern.ircdocs.horse/formatting#hex-color)).
///
/// Support for `\x04` varies by client; standard `\x04RRGGBB` and
/// `\x04RRGGBB,RRGGBB` sequences are specified so clients that don't render
/// them still parse past them safely. However, the `\x04` specification has
/// no default-foreground placeholder (unlike `\x0399,<BG>`), so an RGB
/// background without a foreground emits non-standard `\x04,RRGGBB` — pair
/// [`Colorize::on_rgb`] with an explicit foreground color for broad client
/// compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorValue {
    Palette(Color),
    Rgb(u8, u8, u8),
}

impl From<Color> for ColorValue {
    fn from(c: Color) -> Self {
        ColorValue::Palette(c)
    }
}

impl From<(u8, u8, u8)> for ColorValue {
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        ColorValue::Rgb(r, g, b)
    }
}

impl ColorValue {
    /// This color's RGB `(r, g, b)` value, converting a palette [`Color`] via
    /// its canonical [`Color::to_rgb`] mapping if needed.
    pub const fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            ColorValue::Rgb(r, g, b) => (r, g, b),
            ColorValue::Palette(c) => c.to_rgb(),
        }
    }
}

// ---------------------------------------------------------------------------
// Style & Span — the core data model
// ---------------------------------------------------------------------------

/// A fully-resolved set of attributes for a run of text.
///
/// Unlike the raw IRC byte stream (where codes are stateful toggles),
/// every [`Span`] carries its *complete* resolved style. This means a
/// [`Text`] can always be serialized correctly no matter how it's sliced,
/// concatenated, or split across multiple IRC messages — there's no
/// "current state" to lose track of.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style {
    pub fg: Option<ColorValue>,
    pub bg: Option<ColorValue>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub monospace: bool,
    pub reverse: bool,
}

/// A single run of same-styled text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    pub fn new(text: impl Into<String>, style: Style) -> Self {
        Span {
            text: text.into(),
            style,
        }
    }
}

impl Style {
    pub const fn new() -> Self {
        Style {
            fg: None,
            bg: None,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            monospace: false,
            reverse: false,
        }
    }

    pub const fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub const fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub const fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub const fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
    pub const fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }
    pub const fn reverse(mut self) -> Self {
        self.reverse = true;
        self
    }

    pub fn fg(mut self, c: impl Into<ColorValue>) -> Self {
        self.fg = Some(c.into());
        self
    }
    pub fn bg(mut self, c: impl Into<ColorValue>) -> Self {
        self.bg = Some(c.into());
        self
    }
    pub const fn rgb(mut self, r: u8, g: u8, b: u8) -> Self {
        self.fg = Some(ColorValue::Rgb(r, g, b));
        self
    }
    /// Set an RGB background color; pair with an explicit foreground color
    /// for maximum client compatibility (see [`ColorValue`]).
    pub const fn on_rgb(mut self, r: u8, g: u8, b: u8) -> Self {
        self.bg = Some(ColorValue::Rgb(r, g, b));
        self
    }

    const fn palette_fg(mut self, c: Color) -> Self {
        self.fg = Some(ColorValue::Palette(c));
        self
    }
    const fn palette_bg(mut self, c: Color) -> Self {
        self.bg = Some(ColorValue::Palette(c));
        self
    }

    /// Returns a new style with `fallback` filling in any attributes not
    /// already set on `self`. Formatting toggles (bold, italic, etc.) are unioned.
    pub const fn with_fallback(self, fallback: Style) -> Self {
        Style {
            fg: if self.fg.is_some() {
                self.fg
            } else {
                fallback.fg
            },
            bg: if self.bg.is_some() {
                self.bg
            } else {
                fallback.bg
            },
            bold: self.bold | fallback.bold,
            italic: self.italic | fallback.italic,
            underline: self.underline | fallback.underline,
            strikethrough: self.strikethrough | fallback.strikethrough,
            monospace: self.monospace | fallback.monospace,
            reverse: self.reverse | fallback.reverse,
        }
    }
}

#[rustfmt::skip]
impl Style {
    pub const fn white(self) -> Self { self.palette_fg(Color::White) }
    pub const fn on_white(self) -> Self { self.palette_bg(Color::White) }
    pub const fn black(self) -> Self { self.palette_fg(Color::Black) }
    pub const fn on_black(self) -> Self { self.palette_bg(Color::Black) }
    pub const fn blue(self) -> Self { self.palette_fg(Color::Blue) }
    pub const fn on_blue(self) -> Self { self.palette_bg(Color::Blue) }
    pub const fn green(self) -> Self { self.palette_fg(Color::Green) }
    pub const fn on_green(self) -> Self { self.palette_bg(Color::Green) }
    pub const fn red(self) -> Self { self.palette_fg(Color::Red) }
    pub const fn on_red(self) -> Self { self.palette_bg(Color::Red) }
    pub const fn brown(self) -> Self { self.palette_fg(Color::Brown) }
    pub const fn on_brown(self) -> Self { self.palette_bg(Color::Brown) }
    pub const fn magenta(self) -> Self { self.palette_fg(Color::Magenta) }
    pub const fn on_magenta(self) -> Self { self.palette_bg(Color::Magenta) }
    pub const fn orange(self) -> Self { self.palette_fg(Color::Orange) }
    pub const fn on_orange(self) -> Self { self.palette_bg(Color::Orange) }
    pub const fn yellow(self) -> Self { self.palette_fg(Color::Yellow) }
    pub const fn on_yellow(self) -> Self { self.palette_bg(Color::Yellow) }
    pub const fn light_green(self) -> Self { self.palette_fg(Color::LightGreen) }
    pub const fn on_light_green(self) -> Self { self.palette_bg(Color::LightGreen) }
    pub const fn cyan(self) -> Self { self.palette_fg(Color::Cyan) }
    pub const fn on_cyan(self) -> Self { self.palette_bg(Color::Cyan) }
    pub const fn light_cyan(self) -> Self { self.palette_fg(Color::LightCyan) }
    pub const fn on_light_cyan(self) -> Self { self.palette_bg(Color::LightCyan) }
    pub const fn light_blue(self) -> Self { self.palette_fg(Color::LightBlue) }
    pub const fn on_light_blue(self) -> Self { self.palette_bg(Color::LightBlue) }
    pub const fn pink(self) -> Self { self.palette_fg(Color::Pink) }
    pub const fn on_pink(self) -> Self { self.palette_bg(Color::Pink) }
    pub const fn grey(self) -> Self { self.palette_fg(Color::Grey) }
    pub const fn on_grey(self) -> Self { self.palette_bg(Color::Grey) }
    pub const fn light_grey(self) -> Self { self.palette_fg(Color::LightGrey) }
    pub const fn on_light_grey(self) -> Self { self.palette_bg(Color::LightGrey) }
}

// ---------------------------------------------------------------------------
// Text — a sequence of Spans, the thing you actually build and send
// ---------------------------------------------------------------------------

/// Formatted IRC text: an ordered sequence of styled [`Span`]s.
///
/// Build one with plain strings, style them with [`Colorize`], glue pieces
/// together with `+`, and turn the result into wire bytes with
/// [`Text::to_irc_string`] (or just `.to_string()` / `{}` via [`Display`](fmt::Display)).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Text {
    spans: Vec<Span>,
}

impl Text {
    pub const fn new() -> Self {
        Text { spans: Vec::new() }
    }

    /// Plain, unformatted text. Any raw IRC formatting control bytes embedded
    /// in `s` are stripped so they cannot spoof styles or break out of
    /// surrounding spans on the wire; use [`Text::parse`] to interpret
    /// wire-formatted IRC strings.
    pub fn raw(s: impl Into<String>) -> Self {
        Text::from(s.into())
    }

    /// The canonical sequence of non-empty, style-coalesced [`Span`]s.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Consume this text and return its canonical sequence of [`Span`]s.
    pub fn into_spans(self) -> Vec<Span> {
        self.spans
    }

    /// Whether this text has no visible or whitespace content.
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    fn push<'a>(&mut self, text: impl Into<Cow<'a, str>>, style: Style) {
        match text.into() {
            Cow::Borrowed(s) => {
                for part in s.split(is_format_char) {
                    self.push_clean(part, style);
                }
            }
            Cow::Owned(mut s) => {
                s.retain(|c| !is_format_char(c));
                self.push_clean(s, style);
            }
        }
    }

    fn push_clean<'a>(&mut self, text: impl Into<Cow<'a, str>>, style: Style) {
        let text = text.into();
        if text.is_empty() {
            return;
        }
        match self.spans.last_mut() {
            Some(last) if last.style == style => last.text.push_str(&text),
            _ => self.spans.push(Span {
                text: text.into_owned(),
                style,
            }),
        }
    }

    /// The text content with all formatting stripped.
    pub fn plain(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    /// Length in grapheme clusters, ignoring control codes — the count a
    /// human actually looking at the text would get, which isn't always
    /// the same as the `char` count (an emoji built from multiple code
    /// points, or a letter with a combining accent, is one visible unit).
    pub fn visible_len(&self) -> usize {
        self.spans
            .iter()
            .map(|s| s.text.graphemes(true).count())
            .sum()
    }

    /// Exact byte length this text will occupy on the wire when serialized
    /// with [`Text::to_irc_string`], computed without allocating.
    pub fn wire_len(&self) -> usize {
        let mut w = WireLen(0);
        let _ = self.write_irc(&mut w);
        w.0
    }

    /// Serialize to raw IRC-formatted bytes (as a `String`), resetting with
    /// `\x0f` and emitting the new span's codes whenever the style changes.
    /// This is what you send over the wire.
    ///
    /// If a span's `fg`/`bg` mix a palette [`Color`] with an RGB value (only
    /// possible via [`Colorize::with_fallback_style`] composition, not the ordinary
    /// [`Colorize`] chain), both are still rendered correctly: the whole
    /// pair is sent in the hex form, with the palette side converted via
    /// its canonical RGB value ([`Color::to_rgb`]) rather than dropped.
    pub fn to_irc_string(&self) -> String {
        let mut out = String::with_capacity(self.wire_len());
        let _ = self.write_irc(&mut out);
        out
    }

    fn write_irc(&self, out: &mut impl fmt::Write) -> fmt::Result {
        let mut cur = Style::default();
        for span in &self.spans {
            push_styled(out, &mut cur, &span.text, span.style)?;
        }
        if cur != Style::default() {
            out.write_char(CTRL_RESET)?;
        }
        Ok(())
    }

    /// Split into a list of non-empty `Text`s, each of which will render to at
    /// most `max_bytes` bytes on the wire, breaking on spaces (`' '`, consuming
    /// the break-point spaces) and never splitting a word in half (unless a
    /// single word alone exceeds `max_bytes`, in which case it's hard-split at
    /// a grapheme-cluster boundary — so a multi-codepoint character like an
    /// emoji or an accented letter is never torn apart). Explicit `\n`s,
    /// `\r\n`s, and `\r`s always force a break, and empty lines or chunks are
    /// omitted so every returned `Text` is safe to send directly as an IRC
    /// `PRIVMSG`.
    ///
    /// Because every [`Span`] carries its full resolved style (see above),
    /// each returned chunk re-serializes its own leading codes from
    /// scratch — so styling is preserved correctly across message
    /// boundaries with no extra bookkeeping needed on your end.
    pub fn wrap(&self, max_bytes: usize) -> Vec<Text> {
        let mut lines = Vec::new();
        if self.spans.iter().any(|s| s.text.contains(['\r', '\n'])) {
            for line in self.split_lines() {
                Self::wrap_line(Cow::Owned(line), max_bytes, &mut lines);
            }
        } else {
            Self::wrap_line(Cow::Borrowed(self), max_bytes, &mut lines);
        }
        lines
    }

    fn wrap_line(text: Cow<'_, Text>, max_bytes: usize, lines: &mut Vec<Text>) {
        if text.is_empty() {
            return;
        }
        if text.wire_len() <= max_bytes {
            lines.push(text.into_owned());
            return;
        }

        // Stream `(grapheme cluster, style)` pairs one `(spaces, word)` token
        // at a time so multi-codepoint emoji / combining accents are never torn
        // apart and multi-span words stay glued together without upfront-allocating
        // a buffer for the entire message.
        let mut iter = text
            .spans
            .iter()
            .flat_map(|s| s.text.graphemes(true).map(move |g| (g, s.style)))
            .peekable();

        let mut line = LineBuf::default();
        let mut token = Vec::new();

        while iter.peek().is_some() {
            token.clear();
            while let Some(u) = iter.next_if(|&(g, _)| g == " ") {
                token.push(u);
            }
            let word_start = token.len();
            while let Some(u) = iter.next_if(|&(g, _)| g != " ") {
                token.push(u);
            }

            if line.try_push(&token, max_bytes) {
                continue;
            }

            if !line.is_empty() {
                lines.push(line.take_line());
            }

            let word = &token[word_start..];
            if word.is_empty() || line.try_push(word, max_bytes) {
                continue;
            }

            // Single word alone exceeds `max_bytes`: hard-split by grapheme.
            for &(g, style) in word {
                if !line.try_push(&[(g, style)], max_bytes) {
                    if !line.is_empty() {
                        lines.push(line.take_line());
                    }
                    line.push_unchecked(g, style);
                }
            }
        }

        if !line.is_empty() {
            lines.push(line.take_line());
        }
    }

    /// Split on `\r\n`, `\n`, or `\r` line breaks into separate `Text`s
    /// (formatting carries across the break, matching how most IRC clients
    /// expect multi-line pastes to behave once split into separate messages).
    ///
    /// Unlike [`Text::wrap`], empty lines (including after a trailing newline
    /// or on an empty `Text`) are preserved as empty `Text` entries.
    pub fn split_lines(&self) -> Vec<Text> {
        if !self.spans.iter().any(|s| s.text.contains(['\r', '\n'])) {
            return vec![self.clone()];
        }

        let mut lines = Vec::new();
        let mut cur = Text::new();
        let mut skip_lf = false;

        for span in &self.spans {
            let mut rest = span.text.as_str();
            if skip_lf {
                rest = rest.strip_prefix('\n').unwrap_or(rest);
                skip_lf = false;
            }
            while let Some(pos) = rest.find(['\r', '\n']) {
                cur.push_clean(&rest[..pos], span.style);
                lines.push(core::mem::take(&mut cur));
                let after = &rest[pos + 1..];
                rest = if rest.as_bytes()[pos] == b'\r' {
                    skip_lf = after.is_empty();
                    after.strip_prefix('\n').unwrap_or(after)
                } else {
                    after
                };
            }
            cur.push_clean(rest, span.style);
        }
        lines.push(cur);

        lines
    }

    /// Re-parse raw IRC-formatted text (as received from IRC) back into a
    /// styled [`Text`], for e.g. relaying or re-wrapping someone else's message.
    ///
    /// Palette color code `99` (and any non-`0..=15` palette index) clears that
    /// channel back to the default color.
    pub fn parse(input: &str) -> Text {
        parse_irc(input)
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Text::new() + s
    }
}

impl From<&String> for Text {
    fn from(s: &String) -> Self {
        Text::new() + s
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Text::new() + s
    }
}

impl From<Span> for Text {
    fn from(span: Span) -> Self {
        Text::new() + span
    }
}

impl From<&Text> for Text {
    fn from(t: &Text) -> Self {
        t.clone()
    }
}

impl IntoIterator for Text {
    type Item = Span;
    type IntoIter = alloc::vec::IntoIter<Span>;

    fn into_iter(self) -> Self::IntoIter {
        self.spans.into_iter()
    }
}

impl<'a> IntoIterator for &'a Text {
    type Item = &'a Span;
    type IntoIter = core::slice::Iter<'a, Span>;

    fn into_iter(self) -> Self::IntoIter {
        self.spans.iter()
    }
}

impl<T: Into<Text>> Extend<T> for Text {
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            *self += item.into();
        }
    }
}

impl<T: Into<Text>> FromIterator<T> for Text {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut out = Text::new();
        out.extend(iter);
        out
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.write_irc(f)
    }
}

// -- concatenation, so plain strings and Texts mix freely -------------------

impl<T> Add<T> for Text
where
    Text: AddAssign<T>,
{
    type Output = Text;
    fn add(mut self, rhs: T) -> Text {
        self += rhs;
        self
    }
}

impl<T> Add<T> for &Text
where
    Text: AddAssign<T>,
{
    type Output = Text;
    fn add(self, rhs: T) -> Text {
        self.clone() + rhs
    }
}

impl Add<Text> for &str {
    type Output = Text;
    fn add(self, rhs: Text) -> Text {
        Text::from(self) + rhs
    }
}

impl Add<&Text> for &str {
    type Output = Text;
    fn add(self, rhs: &Text) -> Text {
        Text::from(self) + rhs
    }
}

impl Add<Text> for &String {
    type Output = Text;
    fn add(self, rhs: Text) -> Text {
        Text::from(self) + rhs
    }
}

impl Add<&Text> for &String {
    type Output = Text;
    fn add(self, rhs: &Text) -> Text {
        Text::from(self) + rhs
    }
}

impl Add<Text> for String {
    type Output = Text;
    fn add(self, rhs: Text) -> Text {
        Text::from(self) + rhs
    }
}

impl Add<&Text> for String {
    type Output = Text;
    fn add(self, rhs: &Text) -> Text {
        Text::from(self) + rhs
    }
}

// -- `+=`, for building messages incrementally in a loop --------------------

impl AddAssign<Text> for Text {
    fn add_assign(&mut self, rhs: Text) {
        for span in rhs.spans {
            self.push_clean(span.text, span.style);
        }
    }
}

impl AddAssign<&Text> for Text {
    fn add_assign(&mut self, rhs: &Text) {
        for span in &rhs.spans {
            self.push_clean(&span.text, span.style);
        }
    }
}

impl AddAssign<&str> for Text {
    fn add_assign(&mut self, rhs: &str) {
        self.push(rhs, Style::default());
    }
}

impl AddAssign<&String> for Text {
    fn add_assign(&mut self, rhs: &String) {
        self.push(rhs, Style::default());
    }
}

impl AddAssign<String> for Text {
    fn add_assign(&mut self, rhs: String) {
        self.push(rhs, Style::default());
    }
}

impl AddAssign<Span> for Text {
    fn add_assign(&mut self, rhs: Span) {
        self.push(rhs.text, rhs.style);
    }
}

// ---------------------------------------------------------------------------
// Colorize — the fun, chainable builder, `colored`/`owo-colors`-style
// ---------------------------------------------------------------------------

/// Chainable styling for anything that can become [`Text`]: `&str`,
/// `String`, or an existing `Text`. Calling a method twice (e.g. `.bold()`
/// then later `.red()`) just fills in more attributes — later calls in a
/// chain always win, so `"x".blue().red()` is red, same as you'd expect
/// from a normal builder.
///
/// ```
/// use irc_colors::Colorize;
/// let a = "hello".bold().red();
/// let b = "warning".on_yellow().black();
/// let c = a + " " + b;
/// # let _ = c;
/// ```
#[rustfmt::skip]
pub trait Colorize: Into<Text> + Sized {
    /// Map every span's [`Style`] through `f`, coalescing adjacent spans whose
    /// resulting styles match.
    fn styled(self, mut f: impl FnMut(Style) -> Style) -> Text {
        let mut text = self.into();
        for span in &mut text.spans {
            span.style = f(span.style);
        }
        text.spans.dedup_by(|next, prev| {
            if prev.style == next.style {
                prev.text.push_str(&next.text);
                true
            } else {
                false
            }
        });
        text
    }

    fn bold(self) -> Text { self.styled(Style::bold) }
    fn italic(self) -> Text { self.styled(Style::italic) }
    fn underline(self) -> Text { self.styled(Style::underline) }
    fn strikethrough(self) -> Text { self.styled(Style::strikethrough) }
    fn monospace(self) -> Text { self.styled(Style::monospace) }
    fn reverse(self) -> Text { self.styled(Style::reverse) }

    /// Replace all span styles with `style`.
    fn with_style(self, style: Style) -> Text {
        self.styled(|_| style)
    }

    /// Clear all styling on this text (equivalent to IRC's `\x0f` reset).
    fn reset(self) -> Text {
        self.with_style(Style::default())
    }

    /// Apply a fallback style across all spans: a child span keeps its own
    /// fg/bg if it set one, otherwise inherits the fallback's; formatting
    /// flags (bold, italic, etc.) are unioned in.
    ///
    /// ```
    /// use irc_colors::{Style, Colorize};
    /// let inner = "SPARTA".bold().blue();
    /// let msg = ("This is " + inner).with_fallback_style(Style::new().red().underline());
    /// // "This is " -> red + underline
    /// // "SPARTA"   -> stays blue (child wins), gains underline (union), keeps bold
    /// # let _ = msg;
    /// ```
    fn with_fallback_style(self, fallback: Style) -> Text {
        self.styled(|s| s.with_fallback(fallback))
    }

    fn fg(self, c: impl Into<ColorValue>) -> Text {
        let c = c.into();
        self.styled(|s| s.fg(c))
    }
    fn bg(self, c: impl Into<ColorValue>) -> Text {
        let c = c.into();
        self.styled(|s| s.bg(c))
    }

    /// Set an RGB foreground color via IRC's newer hex-color code (`\x04`).
    /// Support varies by client — see [`ColorValue`].
    fn rgb(self, r: u8, g: u8, b: u8) -> Text {
        self.fg(ColorValue::Rgb(r, g, b))
    }
    /// Set an RGB background color via IRC's newer hex-color code (`\x04`).
    ///
    /// Note that IRC's `\x04` hex-color specification does not define a
    /// "default foreground" placeholder (unlike `\x0399,<bg>` for palette
    /// colors), so for maximum client compatibility pair `.on_rgb(...)` with
    /// an explicit foreground color (`.rgb(...)` or `.fg(...)`).
    fn on_rgb(self, r: u8, g: u8, b: u8) -> Text {
        self.bg(ColorValue::Rgb(r, g, b))
    }

    fn white(self) -> Text { self.fg(Color::White) }
    fn on_white(self) -> Text { self.bg(Color::White) }
    fn black(self) -> Text { self.fg(Color::Black) }
    fn on_black(self) -> Text { self.bg(Color::Black) }
    fn blue(self) -> Text { self.fg(Color::Blue) }
    fn on_blue(self) -> Text { self.bg(Color::Blue) }
    fn green(self) -> Text { self.fg(Color::Green) }
    fn on_green(self) -> Text { self.bg(Color::Green) }
    fn red(self) -> Text { self.fg(Color::Red) }
    fn on_red(self) -> Text { self.bg(Color::Red) }
    fn brown(self) -> Text { self.fg(Color::Brown) }
    fn on_brown(self) -> Text { self.bg(Color::Brown) }
    fn magenta(self) -> Text { self.fg(Color::Magenta) }
    fn on_magenta(self) -> Text { self.bg(Color::Magenta) }
    fn orange(self) -> Text { self.fg(Color::Orange) }
    fn on_orange(self) -> Text { self.bg(Color::Orange) }
    fn yellow(self) -> Text { self.fg(Color::Yellow) }
    fn on_yellow(self) -> Text { self.bg(Color::Yellow) }
    fn light_green(self) -> Text { self.fg(Color::LightGreen) }
    fn on_light_green(self) -> Text { self.bg(Color::LightGreen) }
    fn cyan(self) -> Text { self.fg(Color::Cyan) }
    fn on_cyan(self) -> Text { self.bg(Color::Cyan) }
    fn light_cyan(self) -> Text { self.fg(Color::LightCyan) }
    fn on_light_cyan(self) -> Text { self.bg(Color::LightCyan) }
    fn light_blue(self) -> Text { self.fg(Color::LightBlue) }
    fn on_light_blue(self) -> Text { self.bg(Color::LightBlue) }
    fn pink(self) -> Text { self.fg(Color::Pink) }
    fn on_pink(self) -> Text { self.bg(Color::Pink) }
    fn grey(self) -> Text { self.fg(Color::Grey) }
    fn on_grey(self) -> Text { self.bg(Color::Grey) }
    fn light_grey(self) -> Text { self.fg(Color::LightGrey) }
    fn on_light_grey(self) -> Text { self.bg(Color::LightGrey) }
}

impl<T: Into<Text>> Colorize for T {}

// ---------------------------------------------------------------------------
// IRC formatting control codes & wire emission
// ---------------------------------------------------------------------------

const CTRL_BOLD: char = '\x02';
const CTRL_COLOR: char = '\x03';
const CTRL_HEX_COLOR: char = '\x04';
const CTRL_RESET: char = '\x0f';
const CTRL_MONOSPACE: char = '\x11';
const CTRL_REVERSE: char = '\x16';
const CTRL_ITALIC: char = '\x1d';
const CTRL_STRIKETHROUGH: char = '\x1e';
const CTRL_UNDERLINE: char = '\x1f';

const fn is_format_char(c: char) -> bool {
    matches!(
        c,
        CTRL_BOLD
            | CTRL_COLOR
            | CTRL_HEX_COLOR
            | CTRL_RESET
            | CTRL_MONOSPACE
            | CTRL_REVERSE
            | CTRL_ITALIC
            | CTRL_STRIKETHROUGH
            | CTRL_UNDERLINE
    )
}

/// Emit the formatting and color codes that establish `s` starting from
/// [`Style::default()`], right before `first_char`.
fn emit_style(out: &mut impl fmt::Write, s: Style, first_char: char) -> fmt::Result {
    for (enabled, code) in [
        (s.bold, CTRL_BOLD),
        (s.italic, CTRL_ITALIC),
        (s.underline, CTRL_UNDERLINE),
        (s.strikethrough, CTRL_STRIKETHROUGH),
        (s.monospace, CTRL_MONOSPACE),
        (s.reverse, CTRL_REVERSE),
    ] {
        if enabled {
            out.write_char(code)?;
        }
    }

    // `99` is the codified IRC default/transparent color code, used so `<FG>`
    // is never omitted before `,<BG>` on the wire for palette colors.
    match (s.fg, s.bg) {
        (None, None) => {}
        (Some(ColorValue::Palette(fg)), None) => write!(out, "{CTRL_COLOR}{:02}", fg.code())?,
        (None, Some(ColorValue::Palette(bg))) => write!(out, "{CTRL_COLOR}99,{:02}", bg.code())?,
        (Some(ColorValue::Palette(fg)), Some(ColorValue::Palette(bg))) => {
            write!(out, "{CTRL_COLOR}{:02},{:02}", fg.code(), bg.code())?;
        }
        (fg, bg) => {
            out.write_char(CTRL_HEX_COLOR)?;
            if let Some(fg) = fg {
                let (r, g, b) = fg.to_rgb();
                write!(out, "{r:02X}{g:02X}{b:02X}")?;
            }
            if let Some(bg) = bg {
                let (r, g, b) = bg.to_rgb();
                write!(out, ",{r:02X}{g:02X}{b:02X}")?;
            }
        }
    }

    // A foreground-only color followed by `,` could have `,` + digits
    // eaten as a background color; nudge apart with an empty bold pair.
    if s.fg.is_some() && s.bg.is_none() && first_char == ',' {
        out.write_char(CTRL_BOLD)?;
        out.write_char(CTRL_BOLD)?;
    }

    Ok(())
}

fn push_styled(
    out: &mut impl fmt::Write,
    cur: &mut Style,
    text: &str,
    style: Style,
) -> fmt::Result {
    let Some(first) = text.chars().next() else {
        return Ok(());
    };
    if style != *cur {
        if *cur != Style::default() {
            out.write_char(CTRL_RESET)?;
        }
        emit_style(out, style, first)?;
        *cur = style;
    }
    out.write_str(text)
}

#[derive(Default, Clone, Copy)]
struct WireLen(usize);

impl fmt::Write for WireLen {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.0 += s.len();
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Incremental line buffer for wrapping
// ---------------------------------------------------------------------------

#[derive(Default)]
struct LineBuf {
    text: Text,
    wire: WireLen,
    style: Style,
}

impl LineBuf {
    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// Append `slice` onto the current line if the resulting wire length
    /// (including any trailing `\x0f` reset) stays within `max_bytes`.
    fn try_push(&mut self, slice: &[(&str, Style)], max_bytes: usize) -> bool {
        let (mut wire, mut style) = (self.wire, self.style);
        for &(g, s) in slice {
            let _ = push_styled(&mut wire, &mut style, g, s);
            if wire.0 + usize::from(style != Style::default()) > max_bytes {
                return false;
            }
        }
        self.wire = wire;
        self.style = style;
        for &(g, s) in slice {
            self.text.push_clean(g, s);
        }
        true
    }

    fn push_unchecked(&mut self, g: &str, style: Style) {
        let _ = push_styled(&mut self.wire, &mut self.style, g, style);
        self.text.push_clean(g, style);
    }

    fn take_line(&mut self) -> Text {
        let text = core::mem::take(&mut self.text);
        debug_assert_eq!(
            self.wire.0 + usize::from(self.style != Style::default()),
            text.to_irc_string().len()
        );
        self.wire = WireLen(0);
        self.style = Style::default();
        text
    }
}

// ---------------------------------------------------------------------------
// Parsing raw IRC-formatted text back into a Text
// ---------------------------------------------------------------------------

fn parse_irc(input: &str) -> Text {
    let mut out = Text::new();
    let mut style = Style::default();
    let mut s = input;

    // Reads 1 or 2 ASCII digits from the start of `s`.
    fn read_palette(s: &mut &str) -> Option<Option<ColorValue>> {
        let n = s.bytes().take(2).take_while(u8::is_ascii_digit).count();
        let code = s[..n].parse::<u8>().ok()?;
        *s = &s[n..];
        Some(Color::from_code(code).map(ColorValue::Palette))
    }

    // Reads exactly 6 hex digits from the start of `s`.
    fn read_hex(s: &mut &str) -> Option<Option<ColorValue>> {
        let hex = s
            .get(..6)
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))?;
        let [_, r, g, b] = u32::from_str_radix(hex, 16).ok()?.to_be_bytes();
        *s = &s[6..];
        Some(Some(ColorValue::Rgb(r, g, b)))
    }

    // Outer `Option`: whether a color token was present; inner `Option`: channel
    // value (`None` for palette reset code `99`).
    fn apply_color(
        s: &mut &str,
        style: &mut Style,
        read: fn(&mut &str) -> Option<Option<ColorValue>>,
    ) {
        let fg = read(s);
        let bg = s.strip_prefix(',').and_then(|mut rest| {
            let bg = read(&mut rest)?;
            *s = rest;
            Some(bg)
        });
        if fg.is_none() && bg.is_none() {
            style.fg = None;
            style.bg = None;
        } else {
            style.fg = fg.unwrap_or(style.fg);
            style.bg = bg.unwrap_or(style.bg);
        }
    }

    while let Some(pos) = s.find(is_format_char) {
        out.push_clean(&s[..pos], style);
        let c = s.as_bytes()[pos] as char;
        s = &s[pos + 1..];
        match c {
            CTRL_BOLD => style.bold = !style.bold,
            CTRL_ITALIC => style.italic = !style.italic,
            CTRL_UNDERLINE => style.underline = !style.underline,
            CTRL_STRIKETHROUGH => style.strikethrough = !style.strikethrough,
            CTRL_MONOSPACE => style.monospace = !style.monospace,
            CTRL_REVERSE => style.reverse = !style.reverse,
            CTRL_RESET => style = Style::default(),
            CTRL_COLOR => apply_color(&mut s, &mut style, read_palette),
            CTRL_HEX_COLOR => apply_color(&mut s, &mut style, read_hex),
            _ => unreachable!(),
        }
    }
    out.push_clean(s, style);

    out
}
