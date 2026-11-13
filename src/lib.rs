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
//! # let _ = (msg, warn, fancy);
//! ```
//!
//! Enable the optional `format` Cargo feature (`features = ["format"]`) for
//! `format!`-style interpolation via `irc_format!`:
//!
//! ```
//! # #[cfg(feature = "format")]
//! # {
//! use irc_colors::{irc_format, Colorize};
//!
//! let score = 42;
//! let line = irc_format!("Level: {} (score: {score:04})!", "critical".bold().red());
//! assert_eq!(line.plain(), "Level: critical (score: 0042)!");
//! # }
//! ```

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

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
    /// All 16 palette colors in code order (`0..=15`).
    pub const ALL: [Color; 16] = [
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

/// An extended IRC palette color (codes `16..=98`), per the
/// [IRC Formatting extended color table](https://modern.ircdocs.horse/formatting#colors-16-98):
/// a 72-color hue/lightness grid (`16..=87`) followed by an 11-step grayscale
/// ramp (`88..=98`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExtendedColor(u8);

#[rustfmt::skip]
const EXTENDED_RGB: [(u8, u8, u8); 83] = [
    // 16..=27
    (0x47, 0x00, 0x00), (0x47, 0x21, 0x00), (0x47, 0x47, 0x00), (0x32, 0x47, 0x00),
    (0x00, 0x47, 0x00), (0x00, 0x47, 0x2c), (0x00, 0x47, 0x47), (0x00, 0x27, 0x47),
    (0x00, 0x00, 0x47), (0x2e, 0x00, 0x47), (0x47, 0x00, 0x47), (0x47, 0x00, 0x2a),
    // 28..=39
    (0x74, 0x00, 0x00), (0x74, 0x3a, 0x00), (0x74, 0x74, 0x00), (0x51, 0x74, 0x00),
    (0x00, 0x74, 0x00), (0x00, 0x74, 0x49), (0x00, 0x74, 0x74), (0x00, 0x40, 0x74),
    (0x00, 0x00, 0x74), (0x4b, 0x00, 0x74), (0x74, 0x00, 0x74), (0x74, 0x00, 0x45),
    // 40..=51
    (0xb5, 0x00, 0x00), (0xb5, 0x63, 0x00), (0xb5, 0xb5, 0x00), (0x7d, 0xb5, 0x00),
    (0x00, 0xb5, 0x00), (0x00, 0xb5, 0x71), (0x00, 0xb5, 0xb5), (0x00, 0x63, 0xb5),
    (0x00, 0x00, 0xb5), (0x75, 0x00, 0xb5), (0xb5, 0x00, 0xb5), (0xb5, 0x00, 0x6b),
    // 52..=63
    (0xff, 0x00, 0x00), (0xff, 0x8c, 0x00), (0xff, 0xff, 0x00), (0xb2, 0xff, 0x00),
    (0x00, 0xff, 0x00), (0x00, 0xff, 0xa0), (0x00, 0xff, 0xff), (0x00, 0x8c, 0xff),
    (0x00, 0x00, 0xff), (0xa5, 0x00, 0xff), (0xff, 0x00, 0xff), (0xff, 0x00, 0x98),
    // 64..=75
    (0xff, 0x59, 0x59), (0xff, 0xb4, 0x59), (0xff, 0xff, 0x71), (0xcf, 0xff, 0x60),
    (0x6f, 0xff, 0x6f), (0x65, 0xff, 0xc9), (0x6d, 0xff, 0xff), (0x59, 0xb4, 0xff),
    (0x59, 0x59, 0xff), (0xc4, 0x59, 0xff), (0xff, 0x66, 0xff), (0xff, 0x59, 0xbc),
    // 76..=87
    (0xff, 0x9c, 0x9c), (0xff, 0xd3, 0x9c), (0xff, 0xff, 0x9c), (0xe2, 0xff, 0x9c),
    (0x9c, 0xff, 0x9c), (0x9c, 0xff, 0xdb), (0x9c, 0xff, 0xff), (0x9c, 0xd3, 0xff),
    (0x9c, 0x9c, 0xff), (0xdc, 0x9c, 0xff), (0xff, 0x9c, 0xff), (0xff, 0x94, 0xd3),
    // 88..=98 (grayscale)
    (0x00, 0x00, 0x00), (0x13, 0x13, 0x13), (0x28, 0x28, 0x28), (0x36, 0x36, 0x36),
    (0x4d, 0x4d, 0x4d), (0x65, 0x65, 0x65), (0x81, 0x81, 0x81), (0x9f, 0x9f, 0x9f),
    (0xbc, 0xbc, 0xbc), (0xe2, 0xe2, 0xe2), (0xff, 0xff, 0xff),
];

impl ExtendedColor {
    pub const MIN: u8 = 16;
    pub const MAX: u8 = 98;

    /// All 83 extended palette colors in code order (`16..=98`).
    pub const ALL: [ExtendedColor; 83] = {
        let mut arr = [ExtendedColor(Self::MIN); 83];
        let mut i = 0;
        while i < 83 {
            arr[i] = ExtendedColor(Self::MIN + i as u8);
            i += 1;
        }
        arr
    };

    /// Construct an extended color if `code` is in `16..=98`.
    pub const fn new(code: u8) -> Option<Self> {
        if code >= Self::MIN && code <= Self::MAX {
            Some(Self(code))
        } else {
            None
        }
    }

    pub const fn code(self) -> u8 {
        self.0
    }

    /// Reference `(r, g, b)` value from the
    /// [IRC Formatting 16–98 color table](https://modern.ircdocs.horse/formatting#colors-16-98).
    pub const fn to_rgb(self) -> (u8, u8, u8) {
        EXTENDED_RGB[(self.0 - Self::MIN) as usize]
    }
}

/// A standard 16-color palette [`Color`] (`0..=15`, sent with `\x03`), an
/// [`ExtendedColor`] (`16..=98`, also sent with `\x03`), or a 24-bit RGB value
/// (sent with the newer `\x04` hex-color code from
/// [IRC Formatting](https://modern.ircdocs.horse/formatting#hex-color)).
///
/// Support for `\x04` varies by client: clients that recognize `\x04` without
/// rendering 24-bit color can strip standard `\x04RRGGBB` and
/// `\x04RRGGBB,RRGGBB` sequences, whereas clients unaware of `\x04` altogether
/// typically display the hex digits as literal text. In addition, the `\x04`
/// specification has no default-foreground placeholder (unlike `\x0399,<BG>`),
/// so an RGB background without a foreground emits non-standard `\x04,RRGGBB`
/// — pair [`Colorize::on_rgb`] with an explicit foreground color for broader
/// `\x04` client compatibility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorValue {
    Palette(Color),
    Extended(ExtendedColor),
    Rgb(u8, u8, u8),
}

impl From<Color> for ColorValue {
    fn from(c: Color) -> Self {
        ColorValue::Palette(c)
    }
}

impl From<ExtendedColor> for ColorValue {
    fn from(c: ExtendedColor) -> Self {
        ColorValue::Extended(c)
    }
}

impl From<(u8, u8, u8)> for ColorValue {
    fn from((r, g, b): (u8, u8, u8)) -> Self {
        ColorValue::Rgb(r, g, b)
    }
}

impl ColorValue {
    /// Construct a palette or extended color from any IRC `\x03` code in `0..=98`
    /// (`0..=15` -> [`ColorValue::Palette`], `16..=98` -> [`ColorValue::Extended`],
    /// `99..=u8::MAX` -> `None`).
    pub const fn from_code(code: u8) -> Option<Self> {
        if let Some(c) = Color::from_code(code) {
            Some(ColorValue::Palette(c))
        } else if let Some(e) = ExtendedColor::new(code) {
            Some(ColorValue::Extended(e))
        } else {
            None
        }
    }

    /// The `\x03` palette index (`0..=98`), or `None` for 24-bit [`ColorValue::Rgb`].
    pub const fn code(self) -> Option<u8> {
        match self {
            ColorValue::Palette(c) => Some(c.code()),
            ColorValue::Extended(e) => Some(e.code()),
            ColorValue::Rgb(..) => None,
        }
    }

    /// This color's RGB `(r, g, b)` value, converting a palette or extended
    /// color via its canonical RGB mapping if needed.
    pub const fn to_rgb(self) -> (u8, u8, u8) {
        match self {
            ColorValue::Palette(c) => c.to_rgb(),
            ColorValue::Extended(e) => e.to_rgb(),
            ColorValue::Rgb(r, g, b) => (r, g, b),
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
    pub const fn extended(mut self, c: ExtendedColor) -> Self {
        self.fg = Some(ColorValue::Extended(c));
        self
    }
    pub const fn on_extended(mut self, c: ExtendedColor) -> Self {
        self.bg = Some(ColorValue::Extended(c));
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
/// Build one with `irc_format!` (with the `format` feature), or style plain
/// strings with [`Colorize`] and glue pieces together with `+`, then turn the
/// result into wire bytes with [`Text::to_irc_string`] (or just `.to_string()`
/// / `{}` via [`Display`](fmt::Display)).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct Text {
    spans: Vec<Span>,
}

impl Text {
    pub const fn new() -> Self {
        Text { spans: Vec::new() }
    }

    /// Plain, unformatted text. Any raw IRC formatting control bytes, CTCP
    /// delimiters (`\x01`), or NUL bytes embedded in `s` are stripped so they
    /// cannot spoof styles, break out of surrounding spans, inject CTCP
    /// commands, or inject NULs; use [`Text::parse`] to interpret
    /// wire-formatted IRC strings.
    ///
    /// Line breaks (`\r`, `\n`) are preserved; pass untrusted input through
    /// [`Text::wrap`] or [`Text::split_lines`] before sending on the wire so
    /// embedded `\r`/`\n` cannot inject IRC frames.
    pub fn raw<'a>(s: impl Into<Cow<'a, str>>) -> Self {
        let mut out = Text::new();
        out.push(s, Style::default());
        out
    }

    /// The canonical sequence of non-empty, style-coalesced [`Span`]s.
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Consume this text and return its canonical sequence of [`Span`]s.
    pub fn into_spans(self) -> Vec<Span> {
        self.spans
    }

    /// Whether this text contains no characters (all spans are empty).
    pub fn is_empty(&self) -> bool {
        self.spans.is_empty()
    }

    /// Remove all spans, keeping the underlying allocation for reuse.
    pub fn clear(&mut self) {
        self.spans.clear();
    }

    fn push<'a>(&mut self, text: impl Into<Cow<'a, str>>, style: Style) {
        match text.into() {
            Cow::Borrowed(s) => {
                for part in s.split(is_control_char) {
                    self.push_clean(part, style);
                }
            }
            Cow::Owned(mut s) => {
                s.retain(|c| !is_control_char(c));
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
        let len = self.spans.iter().map(|s| s.text.len()).sum();
        let mut out = String::with_capacity(len);
        for s in &self.spans {
            out.push_str(&s.text);
        }
        out
    }

    /// Length in grapheme clusters, ignoring control codes — the count a
    /// human actually looking at the text would get, which isn't always
    /// the same as the `char` count (an emoji built from multiple code
    /// points, or a letter with a combining accent, is one visible unit).
    ///
    /// Note that this measures neither terminal display width (e.g. double-width
    /// CJK characters) nor IRC protocol length limits, which are counted in
    /// wire bytes — use [`Text::wire_len`] or [`Text::wrap`] for wire budgets.
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

    /// Serialize to raw IRC-formatted bytes (as a `String`), toggling
    /// formatting flags in place when colors match or resetting with `\x0f`
    /// and emitting the new span's codes when colors change. This is what you
    /// send over the wire (note that any `\r` or `\n` inside spans is emitted
    /// verbatim; use [`Text::wrap`] or [`Text::split_lines`] to split
    /// multi-line text into frame-safe IRC messages).
    ///
    /// If a span's `fg`/`bg` mix a `\x03` palette/extended color with an RGB
    /// value (for example `"x".red().on_rgb(1, 2, 3)`), both are still rendered
    /// correctly: the whole pair is sent in `\x04` hex form, with the palette
    /// side converted via [`ColorValue::to_rgb`] rather than dropped.
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
    /// most `max_bytes` bytes on the wire (unless a single grapheme cluster
    /// plus its own setup/reset codes exceeds `max_bytes`, in which case that
    /// 1-grapheme chunk is emitted intact rather than tearing UTF-8/grapheme
    /// sequences or dropping text).
    ///
    /// `max_bytes` is the byte budget for the trailing message parameter alone;
    /// callers sending over IRC must subtract the `:nick!user@host PRIVMSG
    /// <target> :` prefix and trailing `\r\n` from the 512-byte IRC frame limit.
    ///
    /// Breaks on spaces (`' '`, consuming the break-point spaces) and never
    /// splits a word in half unless a single word alone exceeds `max_bytes`, in
    /// which case it's hard-split at a grapheme-cluster boundary — so a
    /// multi-codepoint character like an emoji or an accented letter is never
    /// torn apart. Leading spaces on a line are preserved if they fit together
    /// with the first word within `max_bytes`, and dropped otherwise rather
    /// than emitting a space-only line or tearing a fitting word. Explicit
    /// `\n`s, `\r\n`s, and `\r`s always force a break, and empty lines or
    /// chunks are omitted so every returned `Text` is safe to send directly as
    /// an IRC `PRIVMSG`.
    ///
    /// Because every [`Span`] carries its full resolved style (see above),
    /// each returned chunk re-serializes its own leading codes from
    /// scratch — so styling is preserved correctly across message
    /// boundaries with no extra bookkeeping needed on your end.
    pub fn wrap(&self, max_bytes: usize) -> Vec<Text> {
        let mut lines = Vec::new();
        // Pre-split on line breaks so cross-span `\r\n` logic stays in one place
        // and each fitting line hits `wrap_line`'s `wire_len` fast path without
        // running Unicode grapheme segmentation.
        if self.has_line_break() {
            for line in self.split_lines_inner() {
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
        if !self.has_line_break() {
            return vec![self.clone()];
        }
        self.split_lines_inner()
    }

    fn has_line_break(&self) -> bool {
        self.spans.iter().any(|s| s.text.contains(['\r', '\n']))
    }

    fn split_lines_inner(&self) -> Vec<Text> {
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

    /// Concatenate the items of `iter` into a single [`Text`], inserting `sep`
    /// between adjacent items.
    ///
    /// ```
    /// use irc_colors::{Colorize, Text};
    ///
    /// let users = ["alice", "bob", "carol"];
    /// let list = Text::join(users.iter().map(|u| u.bold()), ", ".grey());
    /// assert_eq!(list.plain(), "alice, bob, carol");
    /// ```
    pub fn join<I, S>(iter: I, sep: S) -> Text
    where
        I: IntoIterator,
        Text: AddAssign<I::Item>,
        S: Into<Text>,
    {
        let mut iter = iter.into_iter().peekable();
        let mut out = Text::new();
        if let Some(first) = iter.next() {
            out += first;
            if iter.peek().is_some() {
                let sep = sep.into();
                for item in iter {
                    AddAssign::<&Text>::add_assign(&mut out, &sep);
                    out += item;
                }
            }
        }
        out
    }

    /// Re-parse raw IRC-formatted text (as received from IRC) back into a
    /// styled [`Text`], for e.g. relaying or re-wrapping someone else's message.
    ///
    /// For `\x03` color codes, `0..=15` map to [`ColorValue::Palette`],
    /// `16..=98` map to [`ColorValue::Extended`], and `99` clears that channel
    /// back to the default color. Per the `\x03[<fg>[,<bg>]]` grammar, `,<bg>`
    /// is only parsed when `<fg>` digits are present (use `\x0399,<bg>` for
    /// default foreground), so a bare `\x03` color reset before `,digits`
    /// (e.g. `\x0304$1\x03,500`) never swallows text; for `\x04` hex colors,
    /// `\x04,<hex>` updates `bg` while preserving any active `fg`.
    pub fn parse(input: &str) -> Text {
        parse_irc(input)
    }
}

// `Cow<'_, str>` intentionally does not implement `From`/`Add`/`AddAssign`:
// without `From<Cow<str>>`, `cow.red()` borrows via `Deref<Target = str>`
// instead of moving `cow` (while `Text::raw(cow)` still takes `Cow` by value).
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

impl From<&Span> for Text {
    fn from(span: &Span) -> Self {
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

impl<T> Extend<T> for Text
where
    Text: AddAssign<T>,
{
    fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I) {
        for item in iter {
            *self += item;
        }
    }
}

impl<T> FromIterator<T> for Text
where
    Text: AddAssign<T>,
{
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut out = Text::new();
        out.extend(iter);
        out
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        Text::from(self).write_irc(f)
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
        if self.is_empty() {
            *self = rhs;
            return;
        }
        self.spans.reserve(rhs.spans.len());
        for span in rhs.spans {
            self.push_clean(span.text, span.style);
        }
    }
}

impl AddAssign<&Text> for Text {
    fn add_assign(&mut self, rhs: &Text) {
        self.spans.reserve(rhs.spans.len());
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

impl AddAssign<&Span> for Text {
    fn add_assign(&mut self, rhs: &Span) {
        self.push(&rhs.text, rhs.style);
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

const CTRL_NUL: char = '\x00';
const CTRL_CTCP: char = '\x01';
const CTRL_BOLD: char = '\x02';
const CTRL_COLOR: char = '\x03';
const CTRL_HEX_COLOR: char = '\x04';
const CTRL_RESET: char = '\x0f';
const CTRL_MONOSPACE: char = '\x11';
const CTRL_REVERSE: char = '\x16';
const CTRL_ITALIC: char = '\x1d';
const CTRL_STRIKETHROUGH: char = '\x1e';
const CTRL_UNDERLINE: char = '\x1f';

const fn is_control_char(c: char) -> bool {
    matches!(
        c,
        CTRL_NUL
            | CTRL_CTCP
            | CTRL_BOLD
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

fn emit_toggles(out: &mut impl fmt::Write, from: Style, to: Style) -> fmt::Result {
    for (changed, code) in [
        (from.bold != to.bold, CTRL_BOLD),
        (from.italic != to.italic, CTRL_ITALIC),
        (from.underline != to.underline, CTRL_UNDERLINE),
        (from.strikethrough != to.strikethrough, CTRL_STRIKETHROUGH),
        (from.monospace != to.monospace, CTRL_MONOSPACE),
        (from.reverse != to.reverse, CTRL_REVERSE),
    ] {
        if changed {
            out.write_char(code)?;
        }
    }
    Ok(())
}

/// Emit the formatting and color codes that establish `s` starting from
/// [`Style::default()`].
fn emit_style(out: &mut impl fmt::Write, s: Style, starts_with_comma: bool) -> fmt::Result {
    emit_toggles(out, Style::default(), s)?;

    // `99` is the codified IRC default/transparent color code, used so `<FG>`
    // is never omitted before `,<BG>` on the wire for `\x03` palette/extended colors.
    match (
        s.fg.and_then(ColorValue::code),
        s.bg.and_then(ColorValue::code),
    ) {
        _ if s.fg.is_none() && s.bg.is_none() => {}
        (Some(fg), None) if s.bg.is_none() => write!(out, "{CTRL_COLOR}{fg:02}")?,
        (None, Some(bg)) if s.fg.is_none() => write!(out, "{CTRL_COLOR}99,{bg:02}")?,
        (Some(fg), Some(bg)) => write!(out, "{CTRL_COLOR}{fg:02},{bg:02}")?,
        _ => {
            out.write_char(CTRL_HEX_COLOR)?;
            if let Some(fg) = s.fg {
                let (r, g, b) = fg.to_rgb();
                write!(out, "{r:02X}{g:02X}{b:02X}")?;
            }
            if let Some(bg) = s.bg {
                let (r, g, b) = bg.to_rgb();
                write!(out, ",{r:02X}{g:02X}{b:02X}")?;
            }
        }
    }

    // A foreground-only color followed by `,` could have `,` + digits
    // eaten as a background color; nudge apart with an empty bold pair.
    // Checked on `,` alone (without digit lookahead) so per-grapheme `LineBuf`
    // accounting always matches coalesced `Span` serialization.
    if s.fg.is_some() && s.bg.is_none() && starts_with_comma {
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
    if text.is_empty() {
        return Ok(());
    }
    if style != *cur {
        if style != Style::default() && (cur.fg, cur.bg) == (style.fg, style.bg) {
            emit_toggles(out, *cur, style)?;
        } else {
            if *cur != Style::default() {
                out.write_char(CTRL_RESET)?;
            }
            if style != Style::default() {
                emit_style(out, style, text.starts_with(','))?;
            }
        }
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
        let line = core::mem::take(self);
        debug_assert_eq!(
            line.wire.0 + usize::from(line.style != Style::default()),
            line.text.wire_len()
        );
        line.text
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
        Some(ColorValue::from_code(code))
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
        allow_bare_bg: bool,
        read: fn(&mut &str) -> Option<Option<ColorValue>>,
    ) {
        let fg = read(s);
        let bg = if fg.is_some() || allow_bare_bg {
            s.strip_prefix(',').and_then(|mut rest| {
                let bg = read(&mut rest)?;
                *s = rest;
                Some(bg)
            })
        } else {
            None
        };
        if fg.is_none() && bg.is_none() {
            style.fg = None;
            style.bg = None;
        } else {
            style.fg = fg.unwrap_or(style.fg);
            style.bg = bg.unwrap_or(style.bg);
        }
    }

    while let Some(pos) = s.find(is_control_char) {
        out.push_clean(&s[..pos], style);
        let c = s.as_bytes()[pos] as char;
        s = &s[pos + 1..];
        match c {
            CTRL_NUL | CTRL_CTCP => {}
            CTRL_BOLD => style.bold = !style.bold,
            CTRL_ITALIC => style.italic = !style.italic,
            CTRL_UNDERLINE => style.underline = !style.underline,
            CTRL_STRIKETHROUGH => style.strikethrough = !style.strikethrough,
            CTRL_MONOSPACE => style.monospace = !style.monospace,
            CTRL_REVERSE => style.reverse = !style.reverse,
            CTRL_RESET => style = Style::default(),
            CTRL_COLOR => apply_color(&mut s, &mut style, false, read_palette),
            CTRL_HEX_COLOR => apply_color(&mut s, &mut style, true, read_hex),
            _ => unreachable!(),
        }
    }
    out.push_clean(s, style);

    out
}

// ---------------------------------------------------------------------------
// `irc_format!` — `format!`-style interpolation returning a `Text`
// ---------------------------------------------------------------------------

/// Build a [`Text`] using [`format!`](alloc::format!)-style string interpolation.
///
/// *Requires the **`format`** Cargo feature (`features = ["format"]`).*
///
/// [`Text`] and [`Span`] arguments (owned or borrowed) preserve their styling,
/// while any other [`Display`](fmt::Display) value (`&str`, `String`, numbers,
/// etc.) is formatted as plain text with embedded IRC control bytes, CTCP
/// delimiters (`\x01`), and NUL bytes stripped — matching [`Text::raw`].
///
/// Supports positional (`{}`, `{0}`), named (`name = val`), and implicitly
/// captured (`{name}`) arguments, as well as standard [`core::fmt`] specifiers
/// (`{:04}`, `{:.2}`, `{:?}`, `{:#x}`, `{:>10}`, etc.).
///
/// # Limitations
///
/// - Format specifiers (`:spec`) render through [`core::fmt`] as plain
///   sanitized text; apply [`Colorize`] to the formatted value (or to the
///   returned [`Text`]) when styling padded/numeric values.
/// - Dynamic width and precision parameters (`{:>width$}`, `{:.*}`) are not
///   supported and produce a compile-time error.
///
/// # Examples
///
/// ```
/// use irc_colors::{irc_format, Colorize};
///
/// let nick = "alice";
/// let score = 42;
/// let msg = irc_format!(
///     "User {} ({nick}) scored {score:04}!",
///     "VIP".bold().yellow(),
/// );
/// assert_eq!(msg.plain(), "User VIP (alice) scored 0042!");
/// ```
///
/// Untrusted plain strings cannot inject IRC formatting or CTCP frames:
///
/// ```
/// use irc_colors::{irc_format, Colorize};
///
/// let untrusted = "\x02spoofed\x01";
/// let msg = irc_format!("Alert: {untrusted} ({})", "real".red());
/// assert_eq!(msg.plain(), "Alert: spoofed (real)");
/// assert_eq!(msg.spans().len(), 3);
/// assert_eq!(msg.spans()[0].style.fg, None);
/// ```
///
/// Compile-time checks mirror `format!` (unmatched braces, missing or unused
/// arguments, positional-after-named):
///
/// ```compile_fail
/// use irc_colors::irc_format;
/// let _ = irc_format!("unused arg", 123);
/// ```
///
/// ```compile_fail
/// use irc_colors::irc_format;
/// let _ = irc_format!("missing {}", );
/// ```
///
/// ```compile_fail
/// use irc_colors::irc_format;
/// let _ = irc_format!("{a} {}", a = 1, 2);
/// ```
///
/// ```compile_fail
/// use irc_colors::irc_format;
/// let w = 4usize;
/// let _ = irc_format!("{:>w$}", "hi");
/// ```
#[cfg(feature = "format")]
#[macro_export]
macro_rules! irc_format {
    ($($tt:tt)*) => {
        $crate::__private::irc_format_impl!($crate; $($tt)*)
    };
}

#[cfg(feature = "format")]
#[doc(hidden)]
pub mod __private {
    use super::*;

    pub use irc_colors_macros::__irc_format_impl as irc_format_impl;

    #[inline]
    pub fn push_clean_str(out: &mut Text, s: &str) {
        out.push_clean(s, Style::default());
    }

    pub struct SanitizingWriter<'a>(pub &'a mut Text);

    impl SanitizingWriter<'_> {
        #[inline]
        pub fn write_fmt(&mut self, args: fmt::Arguments<'_>) -> fmt::Result {
            fmt::Write::write_fmt(self, args)
        }
    }

    impl fmt::Write for SanitizingWriter<'_> {
        #[inline]
        fn write_str(&mut self, s: &str) -> fmt::Result {
            self.0.push(s, Style::default());
            Ok(())
        }
    }

    pub trait AsIrcPiece {
        fn write_piece(&self, out: &mut Text);
    }

    impl AsIrcPiece for Text {
        #[inline]
        fn write_piece(&self, out: &mut Text) {
            *out += self;
        }
    }

    impl AsIrcPiece for Span {
        #[inline]
        fn write_piece(&self, out: &mut Text) {
            *out += self;
        }
    }

    impl<T: AsIrcPiece + ?Sized> AsIrcPiece for &T {
        #[inline]
        fn write_piece(&self, out: &mut Text) {
            (**self).write_piece(out);
        }
    }

    impl<T: AsIrcPiece + ?Sized> AsIrcPiece for &mut T {
        #[inline]
        fn write_piece(&self, out: &mut Text) {
            (**self).write_piece(out);
        }
    }

    pub struct Wrap<T>(pub T);

    impl<T: AsIrcPiece> Wrap<T> {
        #[inline]
        pub fn __write_plain(&self, out: &mut Text) {
            self.0.write_piece(out);
        }
    }

    pub trait FallbackDisplay {
        fn __write_plain(&self, out: &mut Text);
    }

    impl<T: fmt::Display> FallbackDisplay for Wrap<T> {
        #[inline]
        fn __write_plain(&self, out: &mut Text) {
            let _ = fmt::Write::write_fmt(&mut SanitizingWriter(out), format_args!("{}", self.0));
        }
    }
}

#[cfg(test)]
mod lib_test;
