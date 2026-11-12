## #256 - G-code syntax colouring in the post viewer

**Decided.** Left alignment was already on main; this adds colouring only.
A tokenizer (`egui_app/gcode_highlight.rs`) splits each line into spans
(comment, N block number, G, M, T, F/S, axis letters, other words,
numbers); the row is one `LayoutJob` whose text equals the posted line
byte for byte, so copy/save and the accessibility label are unchanged.

**Colours.** Two fixed palettes (dark/light, picked from `visuals.dark_mode`),
muted editor-style hues; the line-number gutter and the trace pane's
out-of-block rows use the theme's weak text colour. Not derived from the
colour-vision palette: that palette encodes warn/ok state, not syntax.

**Caching.** Tokenization is cached per program in egui temp memory, keyed
by length plus a hash of the first and last 256 bytes (O(1) per frame). A
collision could only mis-colour, never change text. Considered a field on
`PostView` (touches 6 constructors) and dropped.

**Dropped.** Per-word-letter distinct hues beyond the classes above; a
syntax-highlighter crate (new dependency).
