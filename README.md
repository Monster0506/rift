# Rift

This is a vim-like text editor. 

I use this daily and primarily. 

Implemented:
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

Todo:
- LSP
- Syntax highlighting
- :edit filename to open a new document (:e to reload from disk)
- undo + redo
- search + replace
- copy/paste (buffers)
- macros
- setting and command descriptions
- make command pattern more like settings design pattern
- help manual
- command history
- mainloop non blocking
- resize handling
- buffer next/previous/ls
- dirty rectangles

Known issues:
- Ascii only operation
    - Renderer does not handle multibyte characters
    - cursor calculation does not account for multibyte characters
    - fix by using String instead of u8
    - Use unicode-width instead of assuming everything is a single char
- Gap Buffer uses a lot of `unsafe`. Write more debug asserts
- TOCTOU Race Condition between file check and file open
- buffer move to start implemented poorly
    - use move_gap_to
- inefficient string construction wrap_text in render loop