//! `irc-color` — write IRC-formatted colored text, the fun way.
//!
//! Implements the formatting characters and color table described in
//! [IRC Formatting](https://modern.ircdocs.horse/formatting) (the de facto
//! standard reference for what's understood across IRC clients today,
//! maintained as part of the modern IRC documentation effort). These codes
//! originated in mIRC, but are universal enough now that "mIRC formatting"
//! is a historical name, not an accurate description of scope.
//!
//! ```
//! use irc_color::Colorize;
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

use std::fmt;
use std::ops::{Add, AddAssign};
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Color {
    White,
    Black,
    Blue,
    Green,
    Red,
    Brown,
    Magenta,
    Orange,
    Yellow,
    LightGreen,
    Cyan,
    LightCyan,
    LightBlue,
    Pink,
    Grey,
    LightGrey,
}

impl Color {
    pub const fn code(self) -> u8 {
        use Color::*;
        match self {
            White => 0,
            Black => 1,
            Blue => 2,
            Green => 3,
            Red => 4,
            Brown => 5,
            Magenta => 6,
            Orange => 7,
            Yellow => 8,
            LightGreen => 9,
            Cyan => 10,
            LightCyan => 11,
            LightBlue => 12,
            Pink => 13,
            Grey => 14,
            LightGrey => 15,
        }
    }

    pub const fn from_code(code: u8) -> Option<Color> {
        use Color::*;
        Some(match code {
            0 => White,
            1 => Black,
            2 => Blue,
            3 => Green,
            4 => Red,
            5 => Brown,
            6 => Magenta,
            7 => Orange,
            8 => Yellow,
            9 => LightGreen,
            10 => Cyan,
            11 => LightCyan,
            12 => LightBlue,
            13 => Pink,
            14 => Grey,
            15 => LightGrey,
            _ => return None,
        })
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
/// Support for the hex form varies by client, but it's fine to send —
/// clients that don't understand it are specified to parse past it safely.
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

impl ColorValue {
    fn is_rgb(self) -> bool {
        matches!(self, ColorValue::Rgb(..))
    }

    /// This color's RGB value, converting a palette color via its canonical
    /// mapping if needed.
    fn as_rgb(self) -> (u8, u8, u8) {
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

impl Style {
    pub fn new() -> Self {
        Style::default()
    }

    pub fn bold(mut self) -> Self {
        self.bold = true;
        self
    }
    pub fn italic(mut self) -> Self {
        self.italic = true;
        self
    }
    pub fn underline(mut self) -> Self {
        self.underline = true;
        self
    }
    pub fn strikethrough(mut self) -> Self {
        self.strikethrough = true;
        self
    }
    pub fn monospace(mut self) -> Self {
        self.monospace = true;
        self
    }
    pub fn reverse(mut self) -> Self {
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
    pub fn rgb(self, r: u8, g: u8, b: u8) -> Self {
        self.fg(ColorValue::Rgb(r, g, b))
    }
    pub fn on_rgb(self, r: u8, g: u8, b: u8) -> Self {
        self.bg(ColorValue::Rgb(r, g, b))
    }

    pub fn white(self) -> Self { self.fg(Color::White) }
    pub fn on_white(self) -> Self { self.bg(Color::White) }
    pub fn black(self) -> Self { self.fg(Color::Black) }
    pub fn on_black(self) -> Self { self.bg(Color::Black) }
    pub fn blue(self) -> Self { self.fg(Color::Blue) }
    pub fn on_blue(self) -> Self { self.bg(Color::Blue) }
    pub fn green(self) -> Self { self.fg(Color::Green) }
    pub fn on_green(self) -> Self { self.bg(Color::Green) }
    pub fn red(self) -> Self { self.fg(Color::Red) }
    pub fn on_red(self) -> Self { self.bg(Color::Red) }
    pub fn brown(self) -> Self { self.fg(Color::Brown) }
    pub fn on_brown(self) -> Self { self.bg(Color::Brown) }
    pub fn magenta(self) -> Self { self.fg(Color::Magenta) }
    pub fn on_magenta(self) -> Self { self.bg(Color::Magenta) }
    pub fn orange(self) -> Self { self.fg(Color::Orange) }
    pub fn on_orange(self) -> Self { self.bg(Color::Orange) }
    pub fn yellow(self) -> Self { self.fg(Color::Yellow) }
    pub fn on_yellow(self) -> Self { self.bg(Color::Yellow) }
    pub fn light_green(self) -> Self { self.fg(Color::LightGreen) }
    pub fn on_light_green(self) -> Self { self.bg(Color::LightGreen) }
    pub fn cyan(self) -> Self { self.fg(Color::Cyan) }
    pub fn on_cyan(self) -> Self { self.bg(Color::Cyan) }
    pub fn light_cyan(self) -> Self { self.fg(Color::LightCyan) }
    pub fn on_light_cyan(self) -> Self { self.bg(Color::LightCyan) }
    pub fn light_blue(self) -> Self { self.fg(Color::LightBlue) }
    pub fn on_light_blue(self) -> Self { self.bg(Color::LightBlue) }
    pub fn pink(self) -> Self { self.fg(Color::Pink) }
    pub fn on_pink(self) -> Self { self.bg(Color::Pink) }
    pub fn grey(self) -> Self { self.fg(Color::Grey) }
    pub fn on_grey(self) -> Self { self.bg(Color::Grey) }
    pub fn light_grey(self) -> Self { self.fg(Color::LightGrey) }
    pub fn on_light_grey(self) -> Self { self.bg(Color::LightGrey) }

    /// Returns a new style with `fallback` filling in any attributes not
    /// already set on `self`. Formatting toggles (bold, italic, etc.) are unioned.
    pub fn with_fallback(self, fallback: Style) -> Self {
        Style {
            fg: self.fg.or(fallback.fg),
            bg: self.bg.or(fallback.bg),
            bold: self.bold | fallback.bold,
            italic: self.italic | fallback.italic,
            underline: self.underline | fallback.underline,
            strikethrough: self.strikethrough | fallback.strikethrough,
            monospace: self.monospace | fallback.monospace,
            reverse: self.reverse | fallback.reverse,
        }
    }
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
    pub spans: Vec<Span>,
}

impl Text {
    pub fn new() -> Self {
        Text { spans: Vec::new() }
    }

    pub fn raw(s: impl Into<String>) -> Self {
        let s = s.into();
        if s.is_empty() {
            Text::new()
        } else {
            Text {
                spans: vec![Span {
                    text: s,
                    style: Style::default(),
                }],
            }
        }
    }

    fn push_span(&mut self, span: Span) {
        if span.text.is_empty() {
            return;
        }
        match self.spans.last_mut() {
            Some(last) if last.style == span.style => last.text.push_str(&span.text),
            _ => self.spans.push(span),
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

    /// Apply a fallback style across all spans in this text: a child span
    /// keeps its own fg/bg if it set one, otherwise inherits the fallback's;
    /// formatting flags (bold, italic, etc.) are unioned in. This is the
    /// "nested tag" composition some IRC formatting libraries offer — most
    /// people won't need it since the [`Colorize`] builder methods below are
    /// usually more direct, but it's handy for composing pre-built chunks.
    ///
    /// ```
    /// use irc_color::{Text, Style, Colorize};
    /// let inner = "SPARTA".bold().blue();
    /// let msg = ("This is " + inner).with_fallback_style(Style::new().red().underline());
    /// // "This is " -> red + underline
    /// // "SPARTA"   -> stays blue (child wins), gains underline (union), keeps bold
    /// # let _ = msg;
    /// ```
    pub fn with_fallback_style(self, fallback: Style) -> Text {
        let mut out = Text::new();
        for mut span in self.spans {
            span.style = span.style.with_fallback(fallback);
            out.push_span(span);
        }
        out
    }

    /// Static constructor: combine several already-styled pieces under one fallback style.
    pub fn with_fallback(fallback: Style, inner: impl Into<Text>) -> Text {
        inner.into().with_fallback_style(fallback)
    }

    /// Serialize to raw IRC-formatted bytes (as a `String`), resetting with
    /// `\x0f` and emitting the new span's codes whenever the style changes.
    /// This is what you send over the wire.
    ///
    /// If a span's `fg`/`bg` mix a palette [`Color`] with an RGB value (only
    /// possible via [`Text::with_fallback`] composition, not the ordinary
    /// [`Colorize`] chain), both are still rendered correctly: the whole
    /// pair is sent in the hex form, with the palette side converted via
    /// its canonical RGB value ([`Color::to_rgb`]) rather than dropped.
    pub fn to_irc_string(&self) -> String {
        let approx_cap: usize = self.spans.iter().map(|s| s.text.len() + 4).sum();
        let mut out = String::with_capacity(approx_cap);
        let mut cur = Style::default();

        for span in &self.spans {
            push_styled(&mut out, &mut cur, &span.text, span.style);
        }

        if cur != Style::default() {
            out.push('\u{0f}');
        }

        out
    }

    /// Split into a list of `Text`s, each of which will render to at most
    /// `max_bytes` bytes on the wire, breaking on whitespace and never
    /// splitting a word in half (unless a single word alone exceeds
    /// `max_bytes`, in which case it's hard-split at a grapheme-cluster
    /// boundary — so a multi-codepoint character like an emoji or an
    /// accented letter is never torn apart). Explicit `\n`s, `\r\n`s, and
    /// `\r`s always force a break.
    ///
    /// Because every [`Span`] carries its full resolved style (see above),
    /// each returned chunk re-serializes its own leading codes from
    /// scratch — so styling is preserved correctly across message
    /// boundaries with no extra bookkeeping needed on your end.
    pub fn wrap(&self, max_bytes: usize) -> Vec<Text> {
        let mut result = Vec::new();

        for line in self.split_lines() {
            result.extend(line.wrap_single_line(max_bytes));
        }

        result
    }

    /// Split on `\r\n`, `\n`, or `\r` line breaks into separate `Text`s
    /// (formatting carries across the break, matching how most IRC clients
    /// expect multi-line pastes to behave once split into separate messages).
    pub fn split_lines(&self) -> Vec<Text> {
        let mut lines = vec![Text::new()];
        let mut skip_lf = false;

        for span in &self.spans {
            let mut buf = String::new();
            for ch in span.text.chars() {
                if skip_lf {
                    skip_lf = false;
                    if ch == '\n' {
                        continue;
                    }
                }
                if ch == '\r' || ch == '\n' {
                    lines.last_mut().unwrap().push_span(Span {
                        text: std::mem::take(&mut buf),
                        style: span.style,
                    });
                    lines.push(Text::new());
                    skip_lf = ch == '\r';
                } else {
                    buf.push(ch);
                }
            }
            lines.last_mut().unwrap().push_span(Span {
                text: buf,
                style: span.style,
            });
        }

        lines
    }

    /// Flatten into one `(grapheme cluster, style)` pair per visible unit.
    /// Operating on grapheme clusters rather than `char`s is what keeps
    /// [`Text::wrap`] from ever tearing a multi-codepoint character (a
    /// combining accent, a flag, a ZWJ emoji sequence) in half.
    fn flatten_graphemes(&self) -> Vec<(&str, Style)> {
        self.spans
            .iter()
            .flat_map(|s| {
                let style = s.style;
                s.text.graphemes(true).map(move |g| (g, style))
            })
            .collect()
    }

    fn from_units(units: &[(&str, Style)]) -> Text {
        let mut spans: Vec<Span> = Vec::new();
        for (g, style) in units {
            match spans.last_mut() {
                Some(last) if last.style == *style => last.text.push_str(g),
                _ => spans.push(Span {
                    text: g.to_string(),
                    style: *style,
                }),
            }
        }
        Text { spans }
    }

    fn wrap_single_line(&self, max_bytes: usize) -> Vec<Text> {
        let units = self.flatten_graphemes();
        if units.is_empty() {
            return vec![Text::new()];
        }

        let mut lines = Vec::new();
        let mut line = LineBuf::default();

        for word in units.split_inclusive(|&(g, _)| g == " ") {
            if line.try_push(word, max_bytes) {
                continue;
            }
            if !line.units.is_empty() {
                lines.push(line.take_line());
                if line.try_push(word, max_bytes) {
                    continue;
                }
            }
            // Word alone exceeds `max_bytes`: hard-split grapheme-by-grapheme.
            for &(g, style) in word {
                if !line.try_push(&[(g, style)], max_bytes) {
                    if !line.units.is_empty() {
                        lines.push(line.take_line());
                        if g == " " {
                            continue;
                        }
                    }
                    line.push_unchecked(g, style);
                }
            }
        }

        if !line.units.is_empty() || lines.is_empty() {
            lines.push(line.take_line());
        }

        lines
    }

    /// Re-parse raw IRC-formatted text (as received from IRC) back into a
    /// styled [`Text`], for e.g. relaying or re-wrapping someone else's message.
    pub fn parse(input: &str) -> Text {
        parse_irc(input)
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Text::raw(s)
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Text::raw(s)
    }
}

impl fmt::Display for Text {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_irc_string())
    }
}

// -- concatenation, so plain strings and Texts mix freely -------------------

impl Add<Text> for Text {
    type Output = Text;
    fn add(mut self, rhs: Text) -> Text {
        self += rhs;
        self
    }
}

impl Add<&str> for Text {
    type Output = Text;
    fn add(mut self, rhs: &str) -> Text {
        self += rhs;
        self
    }
}

impl Add<String> for Text {
    type Output = Text;
    fn add(mut self, rhs: String) -> Text {
        self += rhs;
        self
    }
}

impl Add<Text> for &str {
    type Output = Text;
    fn add(self, rhs: Text) -> Text {
        Text::raw(self) + rhs
    }
}

impl Add<Text> for String {
    type Output = Text;
    fn add(self, rhs: Text) -> Text {
        Text::raw(self) + rhs
    }
}

// -- `+=`, for building messages incrementally in a loop --------------------

impl AddAssign<Text> for Text {
    fn add_assign(&mut self, rhs: Text) {
        for span in rhs.spans {
            self.push_span(span);
        }
    }
}

impl AddAssign<&str> for Text {
    fn add_assign(&mut self, rhs: &str) {
        if !rhs.is_empty() {
            self.push_span(Span {
                text: rhs.to_string(),
                style: Style::default(),
            });
        }
    }
}

impl AddAssign<String> for Text {
    fn add_assign(&mut self, rhs: String) {
        if !rhs.is_empty() {
            self.push_span(Span {
                text: rhs,
                style: Style::default(),
            });
        }
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
/// use irc_color::Colorize;
/// let a = "hello".bold().red();
/// let b = "warning".on_yellow().black();
/// let c = a + " " + b;
/// # let _ = c;
/// ```
pub trait Colorize: Into<Text> + Sized {
    fn styled(self, f: impl Fn(Style) -> Style) -> Text {
        let mut out = Text::new();
        for mut span in self.into().spans {
            span.style = f(span.style);
            out.push_span(span);
        }
        out
    }

    fn bold(self) -> Text {
        self.styled(|s| Style { bold: true, ..s })
    }
    fn italic(self) -> Text {
        self.styled(|s| Style { italic: true, ..s })
    }
    fn underline(self) -> Text {
        self.styled(|s| Style {
            underline: true,
            ..s
        })
    }
    fn strikethrough(self) -> Text {
        self.styled(|s| Style {
            strikethrough: true,
            ..s
        })
    }
    fn monospace(self) -> Text {
        self.styled(|s| Style {
            monospace: true,
            ..s
        })
    }
    fn reverse(self) -> Text {
        self.styled(|s| Style { reverse: true, ..s })
    }

    /// Clear all styling on this text (equivalent to IRC's `\x0f` reset).
    fn reset(self) -> Text {
        self.styled(|_| Style::default())
    }

    /// Apply a fallback style across all spans: a child span keeps its own
    /// fg/bg if it set one, otherwise inherits the fallback's; formatting
    /// flags are unioned in.
    fn with_fallback_style(self, fallback: Style) -> Text {
        self.into().with_fallback_style(fallback)
    }

    fn fg(self, c: impl Into<ColorValue>) -> Text {
        let c = c.into();
        self.styled(move |s| Style { fg: Some(c), ..s })
    }
    fn bg(self, c: impl Into<ColorValue>) -> Text {
        let c = c.into();
        self.styled(move |s| Style { bg: Some(c), ..s })
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
// Wire emission
// ---------------------------------------------------------------------------

/// Emit the formatting and color codes that establish `s` starting from
/// [`Style::default()`], right before `first_char`.
fn emit_style(out: &mut String, s: Style, first_char: char) {
    if s.bold {
        out.push('\u{02}');
    }
    if s.italic {
        out.push('\u{1d}');
    }
    if s.underline {
        out.push('\u{1f}');
    }
    if s.strikethrough {
        out.push('\u{1e}');
    }
    if s.monospace {
        out.push('\u{11}');
    }
    if s.reverse {
        out.push('\u{16}');
    }

    if s.fg.is_some() || s.bg.is_some() {
        use std::fmt::Write;
        if s.fg.is_some_and(ColorValue::is_rgb) || s.bg.is_some_and(ColorValue::is_rgb) {
            out.push('\u{04}');
            if let Some(fg) = s.fg {
                let (r, g, b) = fg.as_rgb();
                let _ = write!(out, "{:02X}{:02X}{:02X}", r, g, b);
            }
            if let Some(bg) = s.bg {
                let (r, g, b) = bg.as_rgb();
                let _ = write!(out, ",{:02X}{:02X}{:02X}", r, g, b);
            }
        } else {
            out.push('\u{03}');
            // `99` is the codified IRC default/transparent color code,
            // used so `<FG>` is never omitted before `,<BG>` on the wire.
            let fg = match s.fg {
                Some(ColorValue::Palette(c)) => c.code(),
                None => 99,
                Some(ColorValue::Rgb(..)) => unreachable!(),
            };
            let _ = write!(out, "{:02}", fg);
            if let Some(ColorValue::Palette(bg)) = s.bg {
                let _ = write!(out, ",{:02}", bg.code());
            }
        }

        // A foreground-only color followed by `,` could have `,` + digits
        // eaten as a background color; nudge apart with an empty bold pair.
        if s.bg.is_none() && first_char == ',' {
            out.push_str("\u{02}\u{02}");
        }
    }
}

fn push_styled(out: &mut String, cur: &mut Style, text: &str, style: Style) {
    let Some(first) = text.chars().next() else {
        return;
    };
    if style != *cur {
        if *cur != Style::default() {
            out.push('\u{0f}');
        }
        emit_style(out, style, first);
        *cur = style;
    }
    out.push_str(text);
}

// ---------------------------------------------------------------------------
// Incremental line buffer for wrapping (checkpoint + truncate)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct LineBuf<'a> {
    units: Vec<(&'a str, Style)>,
    wire: String,
    style: Style,
}

impl<'a> LineBuf<'a> {
    /// Tentatively append `slice` onto the current line, rolling back in O(1)
    /// via `truncate` if the resulting wire length (including any trailing
    /// `\x0f` reset) would exceed `max_bytes`.
    fn try_push(&mut self, slice: &[(&'a str, Style)], max_bytes: usize) -> bool {
        let checkpoint = (self.units.len(), self.wire.len(), self.style);
        for &(g, style) in slice {
            push_styled(&mut self.wire, &mut self.style, g, style);
            self.units.push((g, style));
            if self.wire.len() + usize::from(self.style != Style::default()) > max_bytes {
                self.units.truncate(checkpoint.0);
                self.wire.truncate(checkpoint.1);
                self.style = checkpoint.2;
                return false;
            }
        }
        true
    }

    fn push_unchecked(&mut self, g: &'a str, style: Style) {
        push_styled(&mut self.wire, &mut self.style, g, style);
        self.units.push((g, style));
    }

    fn take_line(&mut self) -> Text {
        let text = Text::from_units(&self.units);
        debug_assert_eq!(
            self.wire.len() + usize::from(self.style != Style::default()),
            text.to_irc_string().len()
        );
        self.units.clear();
        self.wire.clear();
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
    let mut buf = String::new();
    let mut s = input;

    fn flush(buf: &mut String, out: &mut Text, style: Style) {
        if !buf.is_empty() {
            out.push_span(Span {
                text: std::mem::take(buf),
                style,
            });
        }
    }

    // Reads up to `max_digits` ASCII digits from the start of `s`.
    fn read_digits(s: &mut &str, max_digits: usize) -> Option<u8> {
        let count = s
            .as_bytes()
            .iter()
            .take(max_digits)
            .take_while(|b| b.is_ascii_digit())
            .count();
        if count == 0 {
            None
        } else {
            let code = s[..count].parse::<u8>().ok()?;
            *s = &s[count..];
            Some(code)
        }
    }

    // Reads exactly 6 hex digits from the start of `s`.
    fn read_hex6(s: &mut &str) -> Option<(u8, u8, u8)> {
        if s.len() >= 6 && s.as_bytes()[..6].iter().all(|b| b.is_ascii_hexdigit()) {
            let r = u8::from_str_radix(&s[0..2], 16).ok()?;
            let g = u8::from_str_radix(&s[2..4], 16).ok()?;
            let b = u8::from_str_radix(&s[4..6], 16).ok()?;
            *s = &s[6..];
            Some((r, g, b))
        } else {
            None
        }
    }

    // Reads an optional foreground and optional `,`-prefixed background pair.
    fn read_color_pair<T>(
        s: &mut &str,
        read: impl Fn(&mut &str) -> Option<T>,
    ) -> (Option<T>, Option<T>) {
        let fg = read(s);
        let bg = s.strip_prefix(',').and_then(|mut rest| {
            let bg = read(&mut rest)?;
            *s = rest;
            Some(bg)
        });
        (fg, bg)
    }

    fn apply_color_pair<T>(
        style: &mut Style,
        (fg, bg): (Option<T>, Option<T>),
        to_color: impl Fn(T) -> Option<ColorValue>,
    ) {
        if let Some(fg) = fg {
            style.fg = to_color(fg);
            if let Some(bg) = bg {
                style.bg = to_color(bg);
            }
        } else {
            style.fg = None;
            style.bg = bg.and_then(to_color);
        }
    }

    while let Some(c) = s.chars().next() {
        s = &s[c.len_utf8()..];
        match c {
            '\u{02}' | '\u{1d}' | '\u{1f}' | '\u{1e}' | '\u{11}' | '\u{16}' | '\u{0f}'
            | '\u{03}' | '\u{04}' => {
                flush(&mut buf, &mut out, style);
                match c {
                    '\u{02}' => style.bold = !style.bold,
                    '\u{1d}' => style.italic = !style.italic,
                    '\u{1f}' => style.underline = !style.underline,
                    '\u{1e}' => style.strikethrough = !style.strikethrough,
                    '\u{11}' => style.monospace = !style.monospace,
                    '\u{16}' => style.reverse = !style.reverse,
                    '\u{0f}' => style = Style::default(),
                    '\u{03}' => {
                        let pair = read_color_pair(&mut s, |s| read_digits(s, 2));
                        apply_color_pair(&mut style, pair, |c| {
                            Color::from_code(c).map(ColorValue::Palette)
                        });
                    }
                    '\u{04}' => {
                        let pair = read_color_pair(&mut s, read_hex6);
                        apply_color_pair(&mut style, pair, |(r, g, b)| {
                            Some(ColorValue::Rgb(r, g, b))
                        });
                    }
                    _ => unreachable!(),
                }
            }
            _ => buf.push(c),
        }
    }
    flush(&mut buf, &mut out, style);

    out
}
