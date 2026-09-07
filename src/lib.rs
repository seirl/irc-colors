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
    pub fn code(self) -> u8 {
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

    pub fn from_code(code: u8) -> Option<Color> {
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
    pub fn to_rgb(self) -> (u8, u8, u8) {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

macro_rules! style_color_shortcut_methods {
    ($( $variant:ident => $fg_name:ident, $bg_name:ident );* $(;)?) => {
        $(
            pub fn $fg_name(self) -> Self { self.fg(Color::$variant) }
            pub fn $bg_name(self) -> Self { self.bg(Color::$variant) }
        )*
    };
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

    style_color_shortcut_methods! {
        White => white, on_white;
        Black => black, on_black;
        Blue => blue, on_blue;
        Green => green, on_green;
        Red => red, on_red;
        Brown => brown, on_brown;
        Magenta => magenta, on_magenta;
        Orange => orange, on_orange;
        Yellow => yellow, on_yellow;
        LightGreen => light_green, on_light_green;
        Cyan => cyan, on_cyan;
        LightCyan => light_cyan, on_light_cyan;
        LightBlue => light_blue, on_light_blue;
        Pink => pink, on_pink;
        Grey => grey, on_grey;
        LightGrey => light_grey, on_light_grey;
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
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

    /// Combine several already-styled pieces under one *fallback* style: a
    /// child span keeps its own fg/bg if it set one, otherwise inherits the
    /// wrapper's; bold/italic/underline/strikethrough/monospace/reverse are
    /// unioned in. This is the "nested tag" composition some IRC formatting
    /// libraries offer — most people won't need it since the [`Colorize`]
    /// builder methods below are usually more direct, but it's handy for
    /// composing pre-built chunks.
    ///
    /// ```
    /// use irc_color::{Text, Style, Colorize};
    /// let inner = "SPARTA".bold().blue();
    /// let msg = Text::wrap_style(Style::new().red().underline(), "This is " + inner);
    /// // "This is " -> red + underline
    /// // "SPARTA"   -> stays blue (child wins), gains underline (union), keeps bold
    /// # let _ = msg;
    /// ```
    pub fn wrap_style(style: Style, inner: impl Into<Text>) -> Text {
        let mut inner = inner.into();
        for span in inner.spans.iter_mut() {
            span.style.fg = span.style.fg.or(style.fg);
            span.style.bg = span.style.bg.or(style.bg);
            span.style.bold |= style.bold;
            span.style.italic |= style.italic;
            span.style.underline |= style.underline;
            span.style.strikethrough |= style.strikethrough;
            span.style.monospace |= style.monospace;
            span.style.reverse |= style.reverse;
        }
        inner
    }

    /// Serialize to raw IRC-formatted bytes (as a `String`), emitting only
    /// the control codes needed when the style actually changes between
    /// spans. This is what you send over the wire.
    ///
    /// If a span's `fg`/`bg` mix a palette [`Color`] with an RGB value (only
    /// possible via [`Text::wrap_style`] composition, not the ordinary
    /// [`Colorize`] chain), both are still rendered correctly: the whole
    /// pair is sent in the hex form, with the palette side converted via
    /// its canonical RGB value ([`Color::to_rgb`]) rather than dropped.
    pub fn to_irc_string(&self) -> String {
        let mut out = String::new();
        let mut cur = Style::default();

        for span in &self.spans {
            if span.style.bold != cur.bold {
                out.push('\u{02}');
            }
            if span.style.italic != cur.italic {
                out.push('\u{1d}');
            }
            if span.style.underline != cur.underline {
                out.push('\u{1f}');
            }
            if span.style.strikethrough != cur.strikethrough {
                out.push('\u{1e}');
            }
            if span.style.monospace != cur.monospace {
                out.push('\u{11}');
            }
            if span.style.reverse != cur.reverse {
                out.push('\u{16}');
            }

            if (span.style.fg, span.style.bg) != (cur.fg, cur.bg) {
                let use_hex = span.style.fg.is_some_and(ColorValue::is_rgb)
                    || span.style.bg.is_some_and(ColorValue::is_rgb);

                out.push(if use_hex { '\u{04}' } else { '\u{03}' });
                let bare_reset = span.style.fg.is_none() && span.style.bg.is_none();

                use std::fmt::Write;
                if use_hex {
                    if let Some(fg) = span.style.fg {
                        let (r, g, b) = fg.as_rgb();
                        let _ = write!(out, "{:02X}{:02X}{:02X}", r, g, b);
                    }
                    if let Some(bg) = span.style.bg {
                        let (r, g, b) = bg.as_rgb();
                        out.push(',');
                        let _ = write!(out, "{:02X}{:02X}{:02X}", r, g, b);
                    }
                } else {
                    if let Some(ColorValue::Palette(fg)) = span.style.fg {
                        let _ = write!(out, "{:02}", fg.code());
                    }
                    if let Some(ColorValue::Palette(bg)) = span.style.bg {
                        out.push(',');
                        let _ = write!(out, "{:02}", bg.code());
                    }
                }

                // A bare "reset color" code followed by a digit or comma
                // would be mis-parsed as part of the code. Nudge it apart
                // with a harmless bold-toggle-toggle.
                if bare_reset {
                    if let Some(first) = span.text.chars().next() {
                        if first.is_ascii_digit() || first == ',' {
                            out.push('\u{02}');
                            out.push('\u{02}');
                        }
                    }
                }
            }

            out.push_str(&span.text);
            cur = span.style;
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
    /// accented letter is never torn apart). Explicit `\n`s always force a
    /// break.
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

    /// Split on literal `\n` characters into separate `Text`s (formatting
    /// carries across the break, matching how most IRC clients expect
    /// multi-line pastes to behave once split into separate messages).
    pub fn split_lines(&self) -> Vec<Text> {
        let mut lines = vec![Text::new()];

        for span in &self.spans {
            let mut parts = span.text.split('\n').peekable();
            while let Some(part) = parts.next() {
                if !part.is_empty() {
                    lines.last_mut().unwrap().spans.push(Span {
                        text: part.to_string(),
                        style: span.style,
                    });
                }
                if parts.peek().is_some() {
                    lines.push(Text::new());
                }
            }
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
                s.text
                    .graphemes(true)
                    .map(move |g| (g, style))
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

        // Group into whitespace-delimited words, each a run of (grapheme, style).
        let mut words: Vec<Vec<(&str, Style)>> = Vec::new();
        let mut current: Vec<(&str, Style)> = Vec::new();
        for (g, style) in units {
            let is_space = g == " ";
            current.push((g, style));
            if is_space {
                words.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            words.push(current);
        }

        let mut lines: Vec<Vec<(&str, Style)>> = Vec::new();
        let mut line: Vec<(&str, Style)> = Vec::new();
        let mut line_len = 0usize;
        let mut line_style = Style::default();

        for word in words {
            let (mut word_len, mut word_end_style) = word_cost(line_style, &word);
            let mut trailing = usize::from(word_end_style != Style::default());

            if !line.is_empty() && line_len + word_len + trailing > max_bytes {
                lines.push(std::mem::take(&mut line));
                line_len = 0;
                line_style = Style::default();
                (word_len, word_end_style) = word_cost(line_style, &word);
                trailing = usize::from(word_end_style != Style::default());
            }

            if word_len + trailing > max_bytes {
                // Doesn't fit even as its own line: hard-split it. Each
                // resulting piece is already sized to fit on its own, so
                // it becomes a complete line and the next word starts fresh.
                for piece in hard_split(&word, max_bytes) {
                    if !line.is_empty() {
                        lines.push(std::mem::take(&mut line));
                    }
                    lines.push(piece);
                    line_len = 0;
                    line_style = Style::default();
                }
            } else {
                line.extend(word);
                line_len += word_len;
                line_style = word_end_style;
            }
        }

        if !line.is_empty() || lines.is_empty() {
            lines.push(line);
        }

        lines.iter().map(|l| Text::from_units(l)).collect()
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
        self.spans.extend(rhs.spans);
        self
    }
}

impl Add<&str> for Text {
    type Output = Text;
    fn add(self, rhs: &str) -> Text {
        self + Text::raw(rhs)
    }
}

impl Add<String> for Text {
    type Output = Text;
    fn add(self, rhs: String) -> Text {
        self + Text::raw(rhs)
    }
}

impl Add<Text> for &str {
    type Output = Text;
    fn add(self, rhs: Text) -> Text {
        Text::raw(self) + rhs
    }
}

// -- `+=`, for building messages incrementally in a loop --------------------

impl AddAssign<Text> for Text {
    fn add_assign(&mut self, rhs: Text) {
        self.spans.extend(rhs.spans);
    }
}

impl AddAssign<&str> for Text {
    fn add_assign(&mut self, rhs: &str) {
        *self += Text::raw(rhs);
    }
}

impl AddAssign<String> for Text {
    fn add_assign(&mut self, rhs: String) {
        *self += Text::raw(rhs);
    }
}

// ---------------------------------------------------------------------------
// Colorize — the fun, chainable builder, `colored`/`owo-colors`-style
// ---------------------------------------------------------------------------

macro_rules! color_shortcut_methods {
    ($( $variant:ident => $fg_name:ident, $bg_name:ident );* $(;)?) => {
        $(
            fn $fg_name(self) -> Text { self.fg(Color::$variant) }
            fn $bg_name(self) -> Text { self.bg(Color::$variant) }
        )*
    };
}

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
        let mut t = self.into();
        for span in t.spans.iter_mut() {
            span.style = f(span.style);
        }
        t
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
    fn on_rgb(self, r: u8, g: u8, b: u8) -> Text {
        self.bg(ColorValue::Rgb(r, g, b))
    }

    color_shortcut_methods! {
        White => white, on_white;
        Black => black, on_black;
        Blue => blue, on_blue;
        Green => green, on_green;
        Red => red, on_red;
        Brown => brown, on_brown;
        Magenta => magenta, on_magenta;
        Orange => orange, on_orange;
        Yellow => yellow, on_yellow;
        LightGreen => light_green, on_light_green;
        Cyan => cyan, on_cyan;
        LightCyan => light_cyan, on_light_cyan;
        LightBlue => light_blue, on_light_blue;
        Pink => pink, on_pink;
        Grey => grey, on_grey;
        LightGrey => light_grey, on_light_grey;
    }
}

impl<T: Into<Text>> Colorize for T {}

// ---------------------------------------------------------------------------
// Byte-cost accounting for wrapping, without building any strings
// ---------------------------------------------------------------------------

/// How many bytes `to_irc_string` would spend on control codes transitioning
/// from `from` to `to`, right before a run of text starting with
/// `next_char`. Mirrors `Text::to_irc_string`'s logic exactly (same toggles,
/// same color-code accounting, same digit/comma disambiguation workaround)
/// but does no allocation — used so wrapping can track a running byte
/// budget with plain arithmetic instead of re-serializing on every word.
fn transition_bytes(from: Style, to: Style, next_char: Option<char>) -> usize {
    let mut n = 0;
    if to.bold != from.bold {
        n += 1;
    }
    if to.italic != from.italic {
        n += 1;
    }
    if to.underline != from.underline {
        n += 1;
    }
    if to.strikethrough != from.strikethrough {
        n += 1;
    }
    if to.monospace != from.monospace {
        n += 1;
    }
    if to.reverse != from.reverse {
        n += 1;
    }

    if (to.fg, to.bg) != (from.fg, from.bg) {
        n += 1; // the \x03 or \x04 byte itself
        let use_hex =
            to.fg.is_some_and(ColorValue::is_rgb) || to.bg.is_some_and(ColorValue::is_rgb);
        let bare_reset = to.fg.is_none() && to.bg.is_none();

        if use_hex {
            if to.fg.is_some() {
                n += 6;
            }
            if to.bg.is_some() {
                n += 1 + 6;
            }
        } else {
            if matches!(to.fg, Some(ColorValue::Palette(_))) {
                n += 2;
            }
            if matches!(to.bg, Some(ColorValue::Palette(_))) {
                n += 1 + 2;
            }
        }

        if bare_reset {
            if let Some(c) = next_char {
                if c.is_ascii_digit() || c == ',' {
                    n += 2;
                }
            }
        }
    }

    n
}

/// Byte cost of appending `word` to a line whose running style is
/// `start_style`, plus the style it leaves the line in.
fn word_cost(start_style: Style, word: &[(&str, Style)]) -> (usize, Style) {
    let mut len = 0;
    let mut cur = start_style;
    for (g, style) in word {
        len += transition_bytes(cur, *style, g.chars().next());
        len += g.len();
        cur = *style;
    }
    (len, cur)
}

// ---------------------------------------------------------------------------
// Hard-splitting an overlong "word" at a grapheme-cluster boundary
// ---------------------------------------------------------------------------

fn hard_split<'a>(word: &[(&'a str, Style)], max_bytes: usize) -> Vec<Vec<(&'a str, Style)>> {
    let mut pieces = Vec::new();
    let mut current: Vec<(&'a str, Style)> = Vec::new();
    let mut cur_style = Style::default();
    let mut cur_len = 0usize;

    for (g, style) in word {
        let cost = transition_bytes(cur_style, *style, g.chars().next()) + g.len();
        let trailing = usize::from(*style != Style::default());

        if !current.is_empty() && cur_len + cost + trailing > max_bytes {
            pieces.push(std::mem::take(&mut current));
            cur_style = Style::default();
            cur_len = transition_bytes(cur_style, *style, g.chars().next()) + g.len();
        } else {
            cur_len += cost;
        }
        current.push((*g, *style));
        cur_style = *style;
    }
    if !current.is_empty() {
        pieces.push(current);
    }

    pieces
}

// ---------------------------------------------------------------------------
// Parsing raw IRC-formatted text back into a Text
// ---------------------------------------------------------------------------

fn parse_irc(input: &str) -> Text {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut spans = Vec::new();
    let mut style = Style::default();
    let mut buf = String::new();

    macro_rules! flush {
        () => {
            if !buf.is_empty() {
                spans.push(Span {
                    text: std::mem::take(&mut buf),
                    style,
                });
            }
        };
    }

    // Reads up to `max_digits` ASCII digits starting at `i`, WITHOUT
    // consuming anything if none are found. Returns the parsed number and
    // advances `i` past it, or leaves `i` untouched.
    fn read_digits(chars: &[char], i: &mut usize, max_digits: usize) -> Option<u8> {
        let start = *i;
        while *i < chars.len() && *i - start < max_digits && chars[*i].is_ascii_digit() {
            *i += 1;
        }
        if *i == start {
            None
        } else {
            chars[start..*i].iter().collect::<String>().parse().ok()
        }
    }

    // Reads exactly 6 hex digits starting at `i`, without consuming
    // anything if the full run isn't there.
    fn read_hex6(chars: &[char], i: &mut usize) -> Option<(u8, u8, u8)> {
        let start = *i;
        while *i < chars.len() && *i - start < 6 && chars[*i].is_ascii_hexdigit() {
            *i += 1;
        }
        if *i - start == 6 {
            let hex: String = chars[start..*i].iter().collect();
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some((r, g, b))
        } else {
            *i = start; // partial run: not a valid code, don't consume it
            None
        }
    }

    while i < chars.len() {
        let c = chars[i];
        match c {
            '\u{02}' => {
                flush!();
                style.bold = !style.bold;
                i += 1;
            }
            '\u{1d}' => {
                flush!();
                style.italic = !style.italic;
                i += 1;
            }
            '\u{1f}' => {
                flush!();
                style.underline = !style.underline;
                i += 1;
            }
            '\u{1e}' => {
                flush!();
                style.strikethrough = !style.strikethrough;
                i += 1;
            }
            '\u{11}' => {
                flush!();
                style.monospace = !style.monospace;
                i += 1;
            }
            '\u{16}' => {
                flush!();
                style.reverse = !style.reverse;
                i += 1;
            }
            '\u{0f}' => {
                flush!();
                style = Style::default();
                i += 1;
            }
            '\u{03}' => {
                flush!();
                i += 1;
                match read_digits(&chars, &mut i, 2) {
                    Some(fg_code) => {
                        style.fg = Color::from_code(fg_code).map(ColorValue::Palette);
                        // Only consume the comma if real background digits
                        // follow it — otherwise it's just a comma in the
                        // text, and must be left alone.
                        if i < chars.len() && chars[i] == ',' {
                            let mut j = i + 1;
                            if let Some(bg_code) = read_digits(&chars, &mut j, 2) {
                                style.bg = Color::from_code(bg_code).map(ColorValue::Palette);
                                i = j;
                            }
                        }
                    }
                    None => {
                        // Bare \x03: either a full reset, or (less common)
                        // a background-only spec like \x03,4.
                        if i < chars.len() && chars[i] == ',' {
                            let mut j = i + 1;
                            if let Some(bg_code) = read_digits(&chars, &mut j, 2) {
                                style.fg = None;
                                style.bg = Color::from_code(bg_code).map(ColorValue::Palette);
                                i = j;
                                continue;
                            }
                        }
                        style.fg = None;
                        style.bg = None;
                    }
                }
            }
            '\u{04}' => {
                flush!();
                i += 1;
                match read_hex6(&chars, &mut i) {
                    Some((r, g, b)) => {
                        style.fg = Some(ColorValue::Rgb(r, g, b));
                        if i < chars.len() && chars[i] == ',' {
                            let mut j = i + 1;
                            if let Some((r, g, b)) = read_hex6(&chars, &mut j) {
                                style.bg = Some(ColorValue::Rgb(r, g, b));
                                i = j;
                            }
                        }
                    }
                    None => {
                        style.fg = None;
                        style.bg = None;
                    }
                }
            }
            _ => {
                buf.push(c);
                i += 1;
            }
        }
    }
    flush!();

    Text { spans }
}
