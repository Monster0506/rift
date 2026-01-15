# Rift

This is a vim-like text editor. 

I use this daily and primarily. 

Implemented:
- Quite a lot more than the below items, I just haven't updated this in a while due to actually working on it.
- Undo and redo with hybrid delta+checkpointing approach
- Fancy undotree (with colors)
- Syntax highlighting
- Treesitter (supporting only Python and Rust currently)
- Open File
- Save File
- Save As
- basic cursor movement
- insert mode
- command mode
- layer system for rendering
- various settings
- double buffer rendering
- buffer composition
- notification system
- parse bang commands and pass the number of bangs
- hook notifications up to actual errors and warnings
- status bar to indicate filename, dirty
- colorizing and themes
- line numbers
- line indexing improvements to gap buffer
- component level dirty flags to avoid clearing and repopulating full layers
- add redraw command
- crlf and lf support
- setlocal for document-level settings
- dumb gutter to avoid recalcing unless total lines changes significantly.
- Multi-document support
- :edit filename to open a new document (:e to reload from disk)
- dirty rectangles
- Not a gap buffer anymore! We now use a rope with a piece table
- setting and command descriptions
- make command pattern more like settings design pattern (this took way too long)
- Buffer next/previous/ls
- Resizing
- Notify clear and clear all
- Granular movement
- Polling instead of blocking input
- search + replace
- Custom regex engine (monster-regex)

Todo:
- LSP
- copy/paste (buffers)
- macros
- help manual
- command history
- animations
- file exploration
- visual selection
- undotree preview

Known issues:
- 4 byte unicode characters are not handled able to be inserted on windows (this is a crossterm issue on windows, idk man)


Fixed stuff (lightly tracked):
- TOCTOU Race Condition between file check and file open
- Gap Buffer uses a lot of `unsafe`. Write more debug asserts
- inefficient string construction wrap_text in render loop
- Ascii only operation
    - Renderer does not handle multibyte characters
    - cursor calculation does not account for multibyte characters
    - fix by using String instead of u8
    - Use unicode-width instead of assuming everything is a single char
- Allow multiline notifications
- Ensure all components use the theme system
