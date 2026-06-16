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

A digit sequence right before the object key selects nesting depth, e.g.
`2i(` selects the second-enclosing pair of parens. It composes with any
leading operator count (`2di2(` = nesting 4).

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
