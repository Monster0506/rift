//! Windows console terminal backend
//! Implements raw terminal mode using Windows Console API

const std = @import("std");
const windows = std.os.windows;
const backend = @import("backend.zig");
const ansi = @import("ansi.zig");
const Key = @import("../key.zig").Key;
const editor = @import("../editor.zig");
const keymap = @import("../keymap.zig");

const STD_INPUT_HANDLE = @as(windows.DWORD, @bitCast(@as(i32, -10)));
const STD_OUTPUT_HANDLE = @as(windows.DWORD, @bitCast(@as(i32, -11)));

// Console input mode flags
const ENABLE_LINE_INPUT: windows.DWORD = 0x0002;
const ENABLE_ECHO_INPUT: windows.DWORD = 0x0004;
const ENABLE_PROCESSED_INPUT: windows.DWORD = 0x0001;
const ENABLE_VIRTUAL_TERMINAL_INPUT: windows.DWORD = 0x0200;

// Console output mode flags
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: windows.DWORD = 0x0004;

// Virtual key codes
const VK_BACK: windows.WORD = 0x08;
const VK_RETURN: windows.WORD = 0x0D;
const VK_ESCAPE: windows.WORD = 0x1B;
const VK_UP: windows.WORD = 0x26;
const VK_DOWN: windows.WORD = 0x28;
const VK_LEFT: windows.WORD = 0x25;
const VK_RIGHT: windows.WORD = 0x27;
const VK_HOME: windows.WORD = 0x24;
const VK_END: windows.WORD = 0x23;
const VK_PRIOR: windows.WORD = 0x21; // Page Up
const VK_NEXT: windows.WORD = 0x22; // Page Down
const VK_DELETE: windows.WORD = 0x2E;

const KEY_EVENT = 0x0001;
const MOUSE_EVENT = 0x0002;
const WINDOW_BUFFER_SIZE_EVENT = 0x0004;

extern "kernel32" fn GetStdHandle(nStdHandle: windows.DWORD) ?windows.HANDLE;
extern "kernel32" fn GetConsoleMode(hConsoleHandle: windows.HANDLE, lpMode: *windows.DWORD) windows.BOOL;
extern "kernel32" fn SetConsoleMode(hConsoleHandle: windows.HANDLE, dwMode: windows.DWORD) windows.BOOL;
extern "kernel32" fn ReadConsoleInputW(
    hConsoleInput: windows.HANDLE,
    lpBuffer: [*]INPUT_RECORD,
    nLength: windows.DWORD,
    lpNumberOfEventsRead: *windows.DWORD,
) windows.BOOL;
extern "kernel32" fn WriteConsoleW(
    hConsoleOutput: windows.HANDLE,
    lpBuffer: [*]const windows.WCHAR,
    nNumberOfCharsToWrite: windows.DWORD,
    lpNumberOfCharsWritten: ?*windows.DWORD,
    lpReserved: ?*anyopaque,
) windows.BOOL;
extern "kernel32" fn WriteConsoleA(
    hConsoleOutput: windows.HANDLE,
    lpBuffer: [*]const u8,
    nNumberOfCharsToWrite: windows.DWORD,
    lpNumberOfCharsWritten: ?*windows.DWORD,
    lpReserved: ?*anyopaque,
) windows.BOOL;
extern "kernel32" fn GetConsoleScreenBufferInfo(
    hConsoleOutput: windows.HANDLE,
    lpConsoleScreenBufferInfo: *CONSOLE_SCREEN_BUFFER_INFO,
) windows.BOOL;
extern "kernel32" fn GetLastError() windows.DWORD;
extern "kernel32" fn WriteFile(
    hFile: windows.HANDLE,
    lpBuffer: [*]const u8,
    nNumberOfBytesToWrite: windows.DWORD,
    lpNumberOfBytesWritten: *windows.DWORD,
    lpOverlapped: ?*anyopaque,
) windows.BOOL;
extern "kernel32" fn SetConsoleCursorPosition(
    hConsoleOutput: windows.HANDLE,
    dwCursorPosition: COORD,
) windows.BOOL;
extern "kernel32" fn FillConsoleOutputCharacterA(
    hConsoleOutput: windows.HANDLE,
    cCharacter: windows.CHAR,
    nLength: windows.DWORD,
    dwWriteCoord: COORD,
    lpNumberOfCharsWritten: *windows.DWORD,
) windows.BOOL;
extern "kernel32" fn FillConsoleOutputAttribute(
    hConsoleOutput: windows.HANDLE,
    wAttribute: windows.WORD,
    nLength: windows.DWORD,
    dwWriteCoord: COORD,
    lpNumberOfCharsWritten: *windows.DWORD,
) windows.BOOL;
extern "kernel32" fn GetConsoleCursorInfo(
    hConsoleOutput: windows.HANDLE,
    lpConsoleCursorInfo: *CONSOLE_CURSOR_INFO,
) windows.BOOL;
extern "kernel32" fn SetConsoleCursorInfo(
    hConsoleOutput: windows.HANDLE,
    lpConsoleCursorInfo: *const CONSOLE_CURSOR_INFO,
) windows.BOOL;
extern "kernel32" fn ScrollConsoleScreenBufferA(
    hConsoleOutput: windows.HANDLE,
    lpScrollRectangle: *const SMALL_RECT,
    lpClipRectangle: ?*const SMALL_RECT,
    dwDestinationOrigin: COORD,
    lpFill: *const CHAR_INFO,
) windows.BOOL;

const INPUT_RECORD = extern struct {
    EventType: windows.WORD,
    Event: extern union {
        KeyEvent: KEY_EVENT_RECORD,
        MouseEvent: MOUSE_EVENT_RECORD,
        WindowBufferSizeEvent: WINDOW_BUFFER_SIZE_RECORD,
    },
};

const KEY_EVENT_RECORD = extern struct {
    bKeyDown: windows.BOOL,
    wRepeatCount: windows.WORD,
    wVirtualKeyCode: windows.WORD,
    wVirtualScanCode: windows.WORD,
    uChar: extern union {
        UnicodeChar: windows.WCHAR,
        AsciiChar: windows.UCHAR,
    },
    dwControlKeyState: windows.DWORD,
};

const MOUSE_EVENT_RECORD = extern struct {
    dwMousePosition: COORD,
    dwButtonState: windows.DWORD,
    dwControlKeyState: windows.DWORD,
    dwEventFlags: windows.DWORD,
};

const WINDOW_BUFFER_SIZE_RECORD = extern struct {
    dwSize: COORD,
};

const COORD = extern struct {
    X: windows.SHORT,
    Y: windows.SHORT,
};

const CONSOLE_SCREEN_BUFFER_INFO = extern struct {
    dwSize: COORD,
    dwCursorPosition: COORD,
    wAttributes: windows.WORD,
    srWindow: SMALL_RECT,
    dwMaximumWindowSize: COORD,
};

const SMALL_RECT = extern struct {
    Left: windows.SHORT,
    Top: windows.SHORT,
    Right: windows.SHORT,
    Bottom: windows.SHORT,
};

const CONSOLE_CURSOR_INFO = extern struct {
    dwSize: windows.DWORD,
    bVisible: windows.BOOL,
};

const CHAR_INFO = extern struct {
    Char: extern union {
        UnicodeChar: windows.WCHAR,
        AsciiChar: windows.CHAR,
    },
    Attributes: windows.WORD,
};

const CONTROL_KEY_STATE = struct {
    const RIGHT_ALT_PRESSED: windows.DWORD = 0x0001;
    const LEFT_ALT_PRESSED: windows.DWORD = 0x0002;
    const RIGHT_CTRL_PRESSED: windows.DWORD = 0x0004;
    const LEFT_CTRL_PRESSED: windows.DWORD = 0x0008;
    const SHIFT_PRESSED: windows.DWORD = 0x0010;
    const NUMLOCK_ON: windows.DWORD = 0x0020;
    const SCROLLLOCK_ON: windows.DWORD = 0x0040;
    const CAPSLOCK_ON: windows.DWORD = 0x0080;
    const ENHANCED_KEY: windows.DWORD = 0x0100;
};

pub const WindowsTerminal = struct {
    terminal: backend.Terminal,
    input_handle: windows.HANDLE,
    output_handle: windows.HANDLE,
    original_input_mode: windows.DWORD,
    original_output_mode: windows.DWORD,
    allocator: std.mem.Allocator,
    cursor_pos_buf: [32]u8 = undefined,
    last_key_debug_info: ?editor.KeyDebugInfo = null,
    keymap_table: ?std.HashMap(keymap.KeyCombo, Key, keymap.KeyComboContext, std.hash_map.default_max_load_percentage) = null,

    pub fn init(allocator: std.mem.Allocator) !WindowsTerminal {
        const input_handle = GetStdHandle(STD_INPUT_HANDLE) orelse return error.GetStdHandleFailed;
        const output_handle = GetStdHandle(STD_OUTPUT_HANDLE) orelse return error.GetStdHandleFailed;

        // Save original modes
        // GetConsoleMode will fail if stdin/stdout are not console handles
        var original_input_mode: windows.DWORD = undefined;
        var original_output_mode: windows.DWORD = undefined;
        if (GetConsoleMode(input_handle, &original_input_mode) == 0) {
            std.log.err("Error: Rift requires a console window. Please run from cmd.exe or PowerShell (not PowerShell ISE).\n", .{});
            return error.GetConsoleModeFailed;
        }
        if (GetConsoleMode(output_handle, &original_output_mode) == 0) {
            std.log.err("Error: Rift requires a console window. Please run from cmd.exe or PowerShell (not PowerShell ISE).\n", .{});
            return error.GetConsoleModeFailed;
        }

        // Set raw input mode
        // For ReadConsoleInputW to work, we need to keep the console in a valid state
        // We'll disable line input, echo, and processed input, but keep the console functional
        var input_mode: windows.DWORD = 0;
        // Try to enable virtual terminal input if available
        input_mode |= ENABLE_VIRTUAL_TERMINAL_INPUT;
        // Disable line input, echo, and processed input
        // (These are disabled by not setting their flags)

        if (SetConsoleMode(input_handle, input_mode) == 0) {
            // If virtual terminal input is not supported, try with minimal mode
            // ReadConsoleInputW should work with mode 0, but let's try keeping some flags
            // Actually, mode 0 should work - the issue might be elsewhere
            input_mode = 0;
            if (SetConsoleMode(input_handle, input_mode) == 0) {
                return error.SetConsoleModeFailed;
            }
        }
        
        // Verify the mode was set correctly
        var verify_mode: windows.DWORD = undefined;
        if (GetConsoleMode(input_handle, &verify_mode) == 0) {
            return error.GetConsoleModeFailed;
        }

        // Enable virtual terminal processing for output (ANSI support)
        var output_mode = original_output_mode;
        output_mode |= ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        if (SetConsoleMode(output_handle, output_mode) == 0) {
            // If virtual terminal processing is not supported, continue anyway
            // We'll still try to use ANSI sequences
        }

        // Build the keymap lookup table using captured Windows Console API values
        const keymap_table = try keymap.buildLookupTable(allocator);

        const self = WindowsTerminal{
            .terminal = backend.Terminal{
                .ctx = undefined, // Will be set by caller after storage
                .initFn = initImpl,
                .deinitFn = deinitImpl,
                .readKeyFn = readKeyImpl,
                .writeFn = writeImpl,
                .getSizeFn = getSizeImpl,
                .clearScreenFn = clearScreenImpl,
                .moveCursorFn = moveCursorImpl,
                .hideCursorFn = hideCursorImpl,
                .showCursorFn = showCursorImpl,
                .clearToEndOfLineFn = clearToEndOfLineImpl,
            },
            .input_handle = input_handle,
            .output_handle = output_handle,
            .original_input_mode = original_input_mode,
            .original_output_mode = original_output_mode,
            .allocator = allocator,
            .keymap_table = keymap_table,
        };

        return self;
    }

    fn initImpl(ctx: *anyopaque) !void {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));
        // Already initialized in init()
        _ = self;
    }

    fn deinitImpl(ctx: *anyopaque) void {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));
        _ = SetConsoleMode(self.input_handle, self.original_input_mode);
        _ = SetConsoleMode(self.output_handle, self.original_output_mode);
        // Clean up keymap table
        if (self.keymap_table) |*table| {
            table.deinit();
        }
    }

    fn readKeyImpl(ctx: *anyopaque) !Key {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));

        var input_record: INPUT_RECORD = undefined;
        var num_read: windows.DWORD = undefined;

        while (true) {
            const input_record_ptr: [*]INPUT_RECORD = @ptrCast(&input_record);
            if (ReadConsoleInputW(self.input_handle, input_record_ptr, 1, &num_read) == 0) {
                const err = GetLastError();
                std.log.err("ReadConsoleInputW failed with error code: {}\n", .{err});
                // ERROR_INVALID_HANDLE = 6
                if (err == 6) {
                    return error.InvalidConsoleHandle;
                }
                // ERROR_BROKEN_PIPE = 109 (input redirected)
                if (err == 109) {
                    return error.InputRedirected;
                }
                return error.ReadConsoleInputFailed;
            }

            if (num_read == 0) continue;

            if (input_record.EventType == KEY_EVENT) {
                const key_event = input_record.Event.KeyEvent;
                if (key_event.bKeyDown == 0) continue; // Ignore key releases

                const vk = key_event.wVirtualKeyCode;
                const ctrl_pressed = (key_event.dwControlKeyState & (CONTROL_KEY_STATE.LEFT_CTRL_PRESSED | CONTROL_KEY_STATE.RIGHT_CTRL_PRESSED)) != 0;
                const shift_pressed = (key_event.dwControlKeyState & CONTROL_KEY_STATE.SHIFT_PRESSED) != 0;
                const alt_pressed = (key_event.dwControlKeyState & (CONTROL_KEY_STATE.LEFT_ALT_PRESSED | CONTROL_KEY_STATE.RIGHT_ALT_PRESSED)) != 0;
                const ascii_char = @as(u8, @intCast(key_event.uChar.AsciiChar));
                const unicode_char = key_event.uChar.UnicodeChar;
                
                // Store debug info for the last key event
                self.last_key_debug_info = editor.KeyDebugInfo{
                    .vk = vk,
                    .ascii = ascii_char,
                    .unicode = unicode_char,
                };

                // First, try to use the Windows-specific keymap lookup table (uses captured Windows Console API values)
                // For critical keys (Enter, Backspace, Tab), always check fallback first to ensure they work
                // This is a temporary workaround to ensure these keys always work
                switch (vk) {
                    VK_BACK => return Key.backspace,
                    VK_RETURN => return Key.enter,
                    0x09 => return Key{ .char = 9 }, // Tab
                    else => {},
                }
                
                if (self.keymap_table) |*table| {
                    if (keymap.lookupKey(table, vk, ascii_char, unicode_char, ctrl_pressed, shift_pressed, alt_pressed)) |mapped_key| {
                        return mapped_key;
                    }
                }

                // Fallback to original logic if keymap doesn't have the key
                // Check for escape key first (ASCII 27) - in raw mode, escape comes as a character, not virtual key
                if (ascii_char == 27 or unicode_char == 27) {
                    return Key.escape;
                }
                
                // Check for Ctrl+] (ASCII 29) - can come through even if ctrl_pressed is false in some console modes
                if (ascii_char == 29 or unicode_char == 29) {
                    return Key{ .ctrl = ']' };
                }

                // Handle control key combinations
                if (ctrl_pressed) {
                    if (ascii_char >= 1 and ascii_char <= 26) {
                        // Ctrl+A through Ctrl+Z
                        return Key{ .ctrl = ascii_char + 96 }; // Convert to lowercase letter
                    } else {
                        // Handle other Ctrl combinations like Ctrl+]
                        // Ctrl+] is ASCII 29 (0x1D), but we might also get it as unicode ']' (93) with Ctrl
                        if (unicode_char == ']') {
                            return Key{ .ctrl = ']' };
                        }
                    }
                }

                // Handle special keys
                switch (vk) {
                    VK_BACK => return Key.backspace,
                    VK_RETURN => return Key.enter,
                    VK_ESCAPE => return Key.escape,
                    VK_UP => return Key.arrow_up,
                    VK_DOWN => return Key.arrow_down,
                    VK_LEFT => return Key.arrow_left,
                    VK_RIGHT => return Key.arrow_right,
                    VK_HOME => return Key.home,
                    VK_END => return Key.end,
                    VK_PRIOR => return Key.page_up,
                    VK_NEXT => return Key.page_down,
                    VK_DELETE => return Key.delete,
                    else => {},
                }

                // Handle printable characters
                if (unicode_char >= 32 and unicode_char < 127) {
                    // ASCII printable range
                    return Key{ .char = @as(u8, @intCast(unicode_char)) };
                } else if (unicode_char >= 128) {
                    // Extended character - for now, try to convert to UTF-8
                    // This is a simplification; full UTF-8 handling would be more complex
                    if (unicode_char <= 255) {
                        return Key{ .char = @as(u8, @intCast(unicode_char)) };
                    }
                }
            }
        }
    }

    fn writeImpl(ctx: *anyopaque, bytes: []const u8) !void {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));

        // Write using WriteConsoleA (for ANSI sequences and ASCII)
        var written: windows.DWORD = undefined;
        if (WriteConsoleA(self.output_handle, bytes.ptr, @as(windows.DWORD, @intCast(bytes.len)), &written, null) == 0) {
            // If WriteConsoleA fails, try WriteFile as fallback
            // This can happen if the console handle is invalid or redirected
            var bytes_written: windows.DWORD = undefined;
            if (WriteFile(self.output_handle, bytes.ptr, @as(windows.DWORD, @intCast(bytes.len)), &bytes_written, null) == 0) {
                // Both methods failed - this is unusual but we'll just return
                // The error might be recoverable on the next write
                return;
            }
        }
    }

    fn getSizeImpl(ctx: *anyopaque) !backend.Size {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));

        var info: CONSOLE_SCREEN_BUFFER_INFO = undefined;
        if (GetConsoleScreenBufferInfo(self.output_handle, &info) == 0) {
            return error.GetConsoleScreenBufferInfoFailed;
        }

        return backend.Size{
            .rows = @as(u16, @intCast(info.srWindow.Bottom - info.srWindow.Top + 1)),
            .cols = @as(u16, @intCast(info.srWindow.Right - info.srWindow.Left + 1)),
        };
    }

    fn clearScreenImpl(ctx: *anyopaque) !void {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));
        
        // Get screen buffer info to determine size
        var info: CONSOLE_SCREEN_BUFFER_INFO = undefined;
        if (GetConsoleScreenBufferInfo(self.output_handle, &info) == 0) {
            // Fallback to ANSI if Windows API fails
            try writeImpl(ctx, ansi.CLEAR_SCREEN);
            try writeImpl(ctx, ansi.RESET_CURSOR);
            return;
        }
        
        // Only clear the visible window, not the entire buffer
        // The visible window is defined by srWindow
        const window_width = info.srWindow.Right - info.srWindow.Left + 1;
        
        // Clear each row of the visible window
        // FillConsoleOutputCharacterA fills sequentially, so we need to clear row by row
        var written: windows.DWORD = undefined;
        var row: i16 = info.srWindow.Top;
        while (row <= info.srWindow.Bottom) : (row += 1) {
            const row_start = COORD{
                .X = info.srWindow.Left,
                .Y = row,
            };
            
            // Fill this row with spaces
            if (FillConsoleOutputCharacterA(
                self.output_handle,
                ' ',
                @as(windows.DWORD, @intCast(window_width)),
                row_start,
                &written,
            ) == 0) {
                // If FillConsoleOutputCharacterA fails, fallback to ANSI
                try writeImpl(ctx, ansi.CLEAR_SCREEN);
                try writeImpl(ctx, ansi.RESET_CURSOR);
                return;
            }
            
            // Fill this row with default attributes
            _ = FillConsoleOutputAttribute(
                self.output_handle,
                info.wAttributes,
                @as(windows.DWORD, @intCast(window_width)),
                row_start,
                &written,
            );
        }
        
        // Move cursor to top-left of visible window
        const start_coord = COORD{
            .X = info.srWindow.Left,
            .Y = info.srWindow.Top,
        };
        _ = SetConsoleCursorPosition(self.output_handle, start_coord);
    }

    fn moveCursorImpl(ctx: *anyopaque, row: u16, col: u16) !void {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));
        
        // Get screen buffer info to convert relative coordinates to absolute buffer coordinates
        // The renderer passes coordinates relative to the visible window (0,0 = top-left of window)
        // But SetConsoleCursorPosition expects absolute buffer coordinates
        var info: CONSOLE_SCREEN_BUFFER_INFO = undefined;
        if (GetConsoleScreenBufferInfo(self.output_handle, &info) == 0) {
            // Fallback to ANSI if we can't get buffer info
            const seq = try ansi.formatCursorPosition(&self.cursor_pos_buf, row, col);
            try writeImpl(ctx, seq);
            return;
        }
        
        // Convert relative window coordinates to absolute buffer coordinates
        const abs_coord = COORD{
            .X = @as(windows.SHORT, @intCast(info.srWindow.Left + @as(i16, @intCast(col)))),
            .Y = @as(windows.SHORT, @intCast(info.srWindow.Top + @as(i16, @intCast(row)))),
        };
        
        // Use Windows Console API for reliable cursor positioning
        if (SetConsoleCursorPosition(self.output_handle, abs_coord) == 0) {
            // Fallback to ANSI if Windows API fails
            const seq = try ansi.formatCursorPosition(&self.cursor_pos_buf, row, col);
            try writeImpl(ctx, seq);
        }
    }

    fn clearToEndOfLineImpl(ctx: *anyopaque) !void {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));
        
        // Get current cursor position and buffer info
        var info: CONSOLE_SCREEN_BUFFER_INFO = undefined;
        if (GetConsoleScreenBufferInfo(self.output_handle, &info) == 0) {
            // Fallback to ANSI if Windows API fails
            try writeImpl(ctx, ansi.CLEAR_TO_EOL);
            return;
        }
        
        // Calculate number of characters to clear (from cursor to end of buffer line)
        // dwCursorPosition is in absolute buffer coordinates
        // We clear to the end of the buffer line (dwSize.X), not just the visible window
        const current_col = info.dwCursorPosition.X;
        const buffer_width = info.dwSize.X;
        const chars_to_clear = @as(windows.DWORD, @intCast(buffer_width - current_col));
        
        if (chars_to_clear > 0) {
            var written: windows.DWORD = undefined;
            
            // Fill from cursor position to end of buffer line with spaces
            if (FillConsoleOutputCharacterA(
                self.output_handle,
                ' ',
                chars_to_clear,
                info.dwCursorPosition,
                &written,
            ) == 0) {
                // If FillConsoleOutputCharacterA fails, fallback to ANSI
                try writeImpl(ctx, ansi.CLEAR_TO_EOL);
                return;
            }
            
            // Fill with current attributes
            _ = FillConsoleOutputAttribute(
                self.output_handle,
                info.wAttributes,
                chars_to_clear,
                info.dwCursorPosition,
                &written,
            );
        }
    }

    fn hideCursorImpl(ctx: *anyopaque) !void {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));
        
        // Use Windows Console API for cursor visibility
        var cursor_info: CONSOLE_CURSOR_INFO = undefined;
        if (GetConsoleCursorInfo(self.output_handle, &cursor_info) != 0) {
            cursor_info.bVisible = 0; // Hide cursor
            if (SetConsoleCursorInfo(self.output_handle, &cursor_info) != 0) {
                return; // Success
            }
        }
        
        // Fallback to ANSI if Windows API fails
        try writeImpl(ctx, ansi.HIDE_CURSOR);
    }

    fn showCursorImpl(ctx: *anyopaque) !void {
        const self: *WindowsTerminal = @ptrCast(@alignCast(ctx));
        
        // Use Windows Console API for cursor visibility
        var cursor_info: CONSOLE_CURSOR_INFO = undefined;
        if (GetConsoleCursorInfo(self.output_handle, &cursor_info) != 0) {
            cursor_info.bVisible = 1; // Show cursor
            if (SetConsoleCursorInfo(self.output_handle, &cursor_info) != 0) {
                return; // Success
            }
        }
        
        // Fallback to ANSI if Windows API fails
        try writeImpl(ctx, ansi.SHOW_CURSOR);
    }
    
    /// Get debug information for the last key press
    pub fn getLastKeyDebugInfo(self: *WindowsTerminal) ?editor.KeyDebugInfo {
        return self.last_key_debug_info;
    }
};

