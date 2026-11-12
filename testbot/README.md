# irc-colors-testbot

Standalone demo bot using `irc` (TLS on port `6697` via `rustls` by default) to showcase `irc-colors`.

## Run

Quote channel names starting with `#` so your shell doesn't treat `#` as a comment:

```sh
cd testbot
cargo run -- --server irc.libera.chat --channels '##testbot'
```

Optional flags:

- `--nick my-testbot`
- `--port 6697` (or `--no-tls` for plain TCP on `6667`)
- `--channels '##testbot,##other'`

## Channel / PM Commands

- `!demo` — full showcase (palette, styles/in-place toggles, 24-bit RGB gradient, fallback theme, byte-accurate wrapping)
- `!palette` — all 16 IRC palette colors (`Color::ALL`)
- `!styles` — bold, italic, underline, strikethrough, monospace, reverse, bg colors, and same-color in-place toggles
- `!rgb [text]` — grapheme-safe 24-bit `\x04RRGGBB` rainbow gradient
- `!theme [text]` — `const Style` fallback theme (`with_fallback_style`)
- `!wrap [max_bytes] [text]` — byte-accurate `Text::wrap` preserving formatting and ZWJ emoji across chunks
- `!inspect <styled text>` — parses incoming IRC formatting with `Text::parse`, dumps spans, and round-trips
- `!strip <styled text>` — compares `Text::parse(..).reset()` (strips full wire formatting) vs `Text::raw` (strips raw control bytes)
- `!help` — command summary
