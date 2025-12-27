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


Todo:
- LSP
- Syntax highlighting
- Multi-document support
- :edit filename to open a new document (:e to reload from disk)
- undo + redo
- search + replace
- copy/paste (buffers)
- macros
- setting and command descriptions
- make command pattern more like settings design pattern
- help manual
- dumb gutter to avoid recalcing unless total lines changes significantly.
- command history