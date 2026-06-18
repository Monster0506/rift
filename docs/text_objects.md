# Text Objects

A text object is `[count] operator [direction] modifier [nest-count] object-key`, e.g. `di(`, `2daw`, `cin(`, `da F`. Operators (`d`/`c`/`y`) enter `OperatorPending` mode; pressing `i`/`a`/`I`/`A` there starts the text-object grammar.

## Modifiers

| Key | Modifier | Meaning |
| :-- | :--- | :--- |
| `i` | Inner | contents only |
| `a` | Around | contents + delimiter/whitespace |
| `I` | Inner (strict) | contents with leading/trailing whitespace trimmed |
| `A` | Around (loose) | contents + delimiter, eating extra trailing whitespace |

## Direction prefix (optional, before the object key)

| Key | Direction | Meaning |
| :-- | :--- | :--- |
| (none) | Current | object enclosing the cursor |
| `n` | Next | next occurrence of the object after the cursor |
| `p` | Last | previous occurrence of the object before the cursor |

## Nest-count (optional, digits between modifier/direction and object key)

A digit sequence right before the object key selects nesting depth, e.g. `2i(` selects the second-enclosing pair of parens. It composes with any leading operator count (`2di2(` = nesting 4).

## Objects

| Key | Object | Notes |
| :-- | :--- | :--- |
| `w` | word | |
| `W` | WORD | whitespace-delimited |
| `"` | double-quoted string | |
| `'` | single-quoted string | |
| `` ` `` | backtick-quoted string | |
| `(` / `)` | parens | |
| `B` | curly braces | |
| `[` / `]` | square brackets | |
| `<` / `>` | angle brackets | |
| `{` / `}` | paragraph | |
| `s` | sentence | |
| `l` | line | |
| `g` | whole buffer | |
| `b` | any bracket | matches whichever of `()[]{}` encloses the cursor |
| `q` | any quote | matches whichever of `"`/`'`/`` ` `` encloses the cursor |

### Tree-Sitter backed

Require a parse tree for the current buffer's filetype. On a buffer with no grammar available, these no-op.

| Key | Object | Notes |
| :-- | :--- | :--- |
| `f` | function call | `af` = call with parens, `if` = args only |
| `a` | function argument | comma-aware; `aa` eats the adjacent comma |
| `F` | function/method definition | `aF` = whole def, `iF` = body only |
| `c` | class / struct / impl | `ac` = whole item, `ic` = body only |
| `o` | block / compound statement | nearest enclosing `{}`-style block |
| `t` | HTML/XML tag | `at` = with tags, `it` = inner content |
| `d` | number literal | integer or float |

## Surround

Add, change, or delete a pair of delimiters around a motion, text object, or existing pair: `sd<ch>`, `sc<from><to>`, `sg<motion><ch>`, `sgg<ch>`. `s` is its own top-level leader in `Normal` mode (not tied to `d`/`c`/`y`), so it works whether or not a multi-region selection set is active. `<motion>` can be any motion or text object from the grammar above, including the `i`/`a`/`I`/`A` modifiers (e.g. `sgiw"`, `sgi(`).

| Command | Effect |
| :-- | :--- |
| `sd<ch>` | Delete the surrounding `<ch>` pair, keeping its contents |
| `sc<from><to>` | Replace the surrounding `<from>` pair with `<to>` |
| `sg<motion><ch>` | Wrap the resolved motion/text-object range in `<ch>` |
| `sgg<ch>` | Wrap the current line's inner content in `<ch>` |

### Delimiter keys

| Key(s) | Opening | Closing | Padding |
| :-- | :--- | :--- | :--- |
| `)` / `b` | `(` | `)` | none |
| `(` | `( ` | ` )` | spaces |
| `}` / `B` | `{` | `}` | none |
| `{` | `{ ` | ` }` | spaces |
| `]` / `r` | `[` | `]` | none |
| `[` | `[ ` | ` ]` | spaces |
| `>` | `<` | `>` | none |
| `<` | `< ` | ` >` | spaces |
| `"` / `'` / `` ` `` | same | same | none |

Typing an opening bracket char pads with a space on the inside; typing the closing char or a letter alias does not.

### Count

A leading count before `s` repeats the delimiter character(s) on each side, for all three commands:

- `2sd"` removes a doubled `""` pair, leaving the content.
- `2sc"(` replaces a doubled `""` pair with `( ( ... ) )`.
- `2sgiw"` wraps a word in `""`.

For `sd`/`sc`, the existing pair is located the normal way, then the match is extended outward while the same delimiter char repeats. `sgg` accepts a *second*, independent count typed between the two `g`
presses, which extends the wrapped span to that many lines (like `2yy`):

```
2sgg"     ->  ""line""
2s2gg"    ->  ""line
              line""
```

The leading `2` (before `s`) doubles the quote; the inner `2` (between `g` and `g`) spans two lines. The two counts are independent and compose freely.