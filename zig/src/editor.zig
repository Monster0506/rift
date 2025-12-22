//! Editor core - main state and event loop

const std = @import("std");
const buffer = @import("buffer.zig");
const mode = @import("mode.zig");
const command = @import("command.zig");
const render = @import("render.zig");
const key = @import("key.zig");
const backend = @import("term/backend.zig");
const windows_term = @import("term/windows.zig");

/// Debug information for last key press
pub const KeyDebugInfo = struct {
    vk: u16,
    ascii: u8,
    unicode: u16,
};

pub const Editor = struct {
    buf: buffer.GapBuffer,
    current_mode: mode.Mode,
    terminal: *backend.Terminal,
    windows_terminal: ?windows_term.WindowsTerminal = null,
    viewport: render.Viewport,
    dispatcher: command.Dispatcher,
    should_quit: bool,
    allocator: std.mem.Allocator,
    debug_mode: bool = false,
    last_key_debug_info: ?KeyDebugInfo = null,

    pub fn init(allocator: std.mem.Allocator, file_path: ?[]const u8) !Editor {
        // Initialize buffer
        var buf = try buffer.GapBuffer.init(allocator);
        // Note: No errdefer needed - buffer is moved into Editor struct and cleaned up by Editor.deinit()

        // Load file if provided
        if (file_path) |path| {
            buf.loadFromFile(path) catch |err| {
                std.log.err("Failed to load file: {s}\n", .{@errorName(err)});
                // Continue with empty buffer
            };
        }

        // Initialize Windows terminal
        const win_term = try windows_term.WindowsTerminal.init(allocator);
        
        // Move win_term into Editor struct first
        var editor = Editor{
            .buf = buf,
            .current_mode = .normal,
            .terminal = undefined, // Will set below after move
            .windows_terminal = win_term,
            .viewport = undefined, // Will set below
            .dispatcher = command.Dispatcher.init(.normal),
            .should_quit = false,
            .allocator = allocator,
        };
        
        // Now set the terminal pointer to point to the stored win_term's terminal
        // and set the context pointer
        // CRITICAL: We must set the ctx pointer on the Terminal struct that editor.terminal will point to
        // Get a reference to the WindowsTerminal stored in the Editor
        const win_term_ref = &editor.windows_terminal.?;
        
        // Set editor.terminal to point to the Terminal struct inside the stored WindowsTerminal
        // This ensures the pointer remains valid as long as the Editor exists
        editor.terminal = &win_term_ref.terminal;
        
        // Set context pointer DIRECTLY on the Terminal struct that editor.terminal points to
        // This ctx will point back to the WindowsTerminal instance
        // Use both @ptrCast and @alignCast to ensure proper pointer conversion
        // CRITICAL: Set ctx on editor.terminal, not win_term_ref.terminal, to ensure we're setting
        // it on the exact Terminal struct that will be used
        editor.terminal.ctx = @ptrCast(@alignCast(win_term_ref));
        
        // Initialize terminal (ctx should now be set on the Terminal that editor.terminal points to)
        try editor.terminal.init();

        // Get terminal size
        const size = try editor.terminal.getSize();

        // Initialize viewport
        editor.viewport = render.Viewport.init(size.rows, size.cols);

        return editor;
    }

    pub fn deinit(self: *Editor) void {
        if (self.windows_terminal) |*win_term| {
            win_term.terminal.deinit();
        }
        self.buf.deinit();
    }

    pub fn run(self: *Editor) !void {
        // Fix terminal pointer and ctx if they were corrupted during struct copy (when Editor is returned from init)
        // This happens because editor.terminal points to a field in windows_terminal, and when
        // the Editor struct is copied, the pointer still points to the old location
        if (self.windows_terminal) |*win_term| {
            // Fix terminal pointer to point to the new WindowsTerminal instance
            if (self.terminal != &win_term.terminal) {
                self.terminal = &win_term.terminal;
            }
            // CRITICAL: Re-set ctx to point to the NEW WindowsTerminal instance
            // The old ctx pointed to the old WindowsTerminal that was copied, so we need to update it
            self.terminal.ctx = @ptrCast(@alignCast(win_term));
        }
        
        // Show cursor
        try self.terminal.showCursor();

        // Initial render
        try render.render(self.terminal, &self.buf, &self.viewport, self.allocator, self.current_mode, self.dispatcher.pending_key, self.debug_mode, self.last_key_debug_info);

        // Main event loop
        while (!self.should_quit) {
            // Read key
            const key_press = try self.terminal.readKey();
            
            // Always retrieve debug info (it's set during readKey)
            // This allows us to see what Windows Console API values were captured
            if (self.windows_terminal) |*win_term| {
                self.last_key_debug_info = win_term.getLastKeyDebugInfo();
            } else {
                self.last_key_debug_info = null;
            }
            
            // Clear debug info if debug mode is disabled
            if (!self.debug_mode) {
                self.last_key_debug_info = null;
            }
            
            // Handle debug mode toggle (?)
            switch (key_press) {
                .char => |ch| {
                    if (ch == '?') {
                        self.debug_mode = !self.debug_mode;
                        if (!self.debug_mode) {
                            self.last_key_debug_info = null;
                        }
                        // Redraw to show/hide debug info
                        try render.render(self.terminal, &self.buf, &self.viewport, self.allocator, self.current_mode, self.dispatcher.pending_key, self.debug_mode, self.last_key_debug_info);
                        continue;
                    }
                },
                else => {},
            }

            // Handle insert mode exit first (before command translation)
            // Support both Escape key and Ctrl+] (^]) as escape
            if (self.current_mode == .insert) {
                var should_exit_insert = false;
                switch (key_press) {
                    .escape => should_exit_insert = true,
                    .ctrl => |ch| {
                        if (ch == ']') {
                            should_exit_insert = true;
                        }
                    },
                    else => {},
                }
                
                if (should_exit_insert) {
                    self.current_mode = .normal;
                    self.dispatcher.mode = .normal;
                    // Clear any pending key when switching modes
                    self.dispatcher.pending_key = null;
                    // Redraw and continue
                    try render.render(self.terminal, &self.buf, &self.viewport, self.allocator, self.current_mode, self.dispatcher.pending_key, self.debug_mode, self.last_key_debug_info);
                    continue;
                }
            }
            
            // Also handle escape in normal mode to clear pending keys
            if (self.current_mode == .normal) {
                switch (key_press) {
                    .escape => {
                        // Clear pending key if any
                        self.dispatcher.pending_key = null;
                        // Redraw to update status bar
                        try render.render(self.terminal, &self.buf, &self.viewport, self.allocator, self.current_mode, self.dispatcher.pending_key, self.debug_mode, self.last_key_debug_info);
                        continue;
                    },
                    .ctrl => |ch| {
                        if (ch == ']') {
                            // Clear pending key if any
                            self.dispatcher.pending_key = null;
                            // Redraw to update status bar
                            try render.render(self.terminal, &self.buf, &self.viewport, self.allocator, self.current_mode, self.dispatcher.pending_key, self.debug_mode, self.last_key_debug_info);
                            continue;
                        }
                    },
                    else => {},
                }
            }

            // Translate key to command
            const cmd = self.dispatcher.translateKey(key_press);

            // Handle mode transitions
            switch (cmd) {
                .enter_insert_mode => {
                    self.current_mode = .insert;
                    self.dispatcher.mode = .insert;
                },
                .enter_insert_mode_after => {
                    self.current_mode = .insert;
                    self.dispatcher.mode = .insert;
                    command.executeCommand(cmd, &self.buf, null);
                },
                .quit => {
                    self.should_quit = true;
                    continue;
                },
                else => {},
            }

            // Execute command
            if (cmd != .enter_insert_mode and cmd != .enter_insert_mode_after) {
                command.executeCommand(cmd, &self.buf, key_press);
            }

            // Redraw screen
            try render.render(self.terminal, &self.buf, &self.viewport, self.allocator, self.current_mode, self.dispatcher.pending_key, self.debug_mode, self.last_key_debug_info);
        }

        // Hide cursor before exit
        try self.terminal.hideCursor();
    }
};

