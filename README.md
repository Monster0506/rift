# Rift

This is a vim-like text editor. 

I use this daily and primarily. 

## Implemented:
- Quite a lot more than the below items, I just haven't updated this in a while due to actually working on it.
- Soft line wrapping (setlocal wrap, with highly optimized rendering)
- Messages buffer (:messages to view editor messages)
- Everything-is-a-buffer architecture (file explorer, undotree, terminal, etc. are all buffers)
- BOM stripping and correct UTF-8 decoding on file load
- Multi-width unicode character rendering
- Count indicator in command-line completion
- Command-line completion engine (commands, subcommands, settings, setting values, file paths)
- vertical and horizontal splits (:split, :vsplit)
- split navigation (Ctrl+W h/j/k/l), resize (<, >), freeze
- terminal emulator (:terminal, via alacritty_terminal)
- change command (c, C)
- dot repeat (.)
- smart dirty indicator (tracks saved state in undotree)
- capital insert commands (I, A)
- Undotree preview with change-focused scrolling
- Fancy undotree (with colors and syntax highlighting)
- persistent buffer state across file switches (dirty, undotree, etc.)
- command and search history (with up/down navigation)
- File explorer (ranger-style: open, new, delete, rename, copy, toggle hidden, metadata, bulk ops)
- ^W to delete back a word in insert/command/search
- benchmarking infrastructure (buffer, render, search, movement, syntax, history, screen, job, input)
- Treesitter (bash, c, c++, c#, css, go, html, java, js, json, lua, markdown, php, python, ruby, rust, typescript, yaml, zig)
- Async treesitter highlighting on background thread
- threaded job manager for async operations
- smartcase search
- search index warming (background cache)
- streaming search with multiple engines
- granular movement (word, sentence, paragraph, big word)
- Character abstraction for proper unicode handling (not just u8/char)
- O(log N) byte/char length synchronization
- interval trees for syntax metadata
- input box component
- select view with scrolling
- operator pending mode (d, c, y with motions)
- search + replace
- Custom regex engine (monster-regex)
- Polling instead of blocking input
- Resizing
- Buffer next/previous/ls
- Notify clear and clear all
- setting and command descriptions
- make command pattern more like settings design pattern (this took way too long)
- Not a gap buffer anymore! We now use a rope with a piece table
- dirty rectangles
- :edit filename to open a new document (:e to reload from disk)
- Multi-document support
- setlocal for document-level settings
- dumb gutter to avoid recalcing unless total lines changes significantly
- crlf and lf support
- add redraw command
- component level dirty flags to avoid clearing and repopulating full layers
- line indexing improvements
- line numbers
- colorizing and themes
- status bar to indicate filename, dirty
- notification system
- parse bang commands and pass the number of bangs
- hook notifications up to actual errors and warnings
- buffer composition
- double buffer rendering
- layer system for rendering
- various settings
- command mode
- insert mode
- basic cursor movement
- Save File (async)
- Save As
- Open File
- Syntax highlighting
- Undo and redo with hybrid delta+checkpointing approach
- debug mode toggle

Todo:
- LSP
- registers + unified yank/paste/delete
- visual selection
- macros
- help manual
- marks and jumps
- animations
- operator pending improvements (indent, format, case, etc.)
- plugin system

Known issues:
- 4 byte unicode characters are not able to be inserted on windows (this is a crossterm issue on windows, idk man)

## Fixed stuff (lightly tracked):
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
- search doesn't close on successful search
- :e to reload a file clears highlights
- floating window components not matching theme bg/fg
- searching for non existent text is really slow
- undo preview does not autoload
- syntax highlighting not updated as typing occurs
- scroll fails on select_view beyond window
- commandline commands do not properly close commandline
- insert operations not grouped as undo transactions
- delete operations not grouped as undo transactions
- undotree initial position wrong
- terminal crashes when running interactive programs
- terminal open in split causes flicker
- notification padding wrong
- cursor does not move after typing a space in the terminal
- input boxes for file explorer not accepting input
