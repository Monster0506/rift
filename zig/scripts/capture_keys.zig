//! Helper program to capture Windows Console API key values
//! Reads a single keypress and outputs JSON with vk, ascii, unicode values

const std = @import("std");
const windows = std.os.windows;

const STD_INPUT_HANDLE = @as(windows.DWORD, @bitCast(@as(i32, -10)));

// Console input mode flags
const ENABLE_VIRTUAL_TERMINAL_INPUT: windows.DWORD = 0x0200;

extern "kernel32" fn GetStdHandle(nStdHandle: windows.DWORD) ?windows.HANDLE;
extern "kernel32" fn GetConsoleMode(hConsoleHandle: windows.HANDLE, lpMode: *windows.DWORD) windows.BOOL;
extern "kernel32" fn SetConsoleMode(hConsoleHandle: windows.HANDLE, dwMode: windows.DWORD) windows.BOOL;
extern "kernel32" fn AllocConsole() windows.BOOL;
extern "kernel32" fn FreeConsole() windows.BOOL;
extern "kernel32" fn AttachConsole(dwProcessId: windows.DWORD) windows.BOOL;
extern "kernel32" fn GetCurrentProcessId() windows.DWORD;
extern "kernel32" fn ReadConsoleInputW(
    hConsoleInput: windows.HANDLE,
    lpBuffer: [*]INPUT_RECORD,
    nLength: windows.DWORD,
    lpNumberOfEventsRead: *windows.DWORD,
) windows.BOOL;
extern "kernel32" fn WriteFile(
    hFile: windows.HANDLE,
    lpBuffer: [*]const u8,
    nNumberOfBytesToWrite: windows.DWORD,
    lpNumberOfBytesWritten: *windows.DWORD,
    lpOverlapped: ?*anyopaque,
) windows.BOOL;

const KEY_EVENT = 0x0001;

const INPUT_RECORD = extern struct {
    EventType: windows.WORD,
    Event: extern union {
        KeyEvent: KEY_EVENT_RECORD,
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

pub fn main() !void {
    // If we're in a new console (CREATE_NEW_CONSOLE), we might need to ensure console is attached
    // Try to get stdin handle first
    var input_handle = GetStdHandle(STD_INPUT_HANDLE);
    
    // If stdin handle is invalid, try to allocate/attach to console
    // When CREATE_NEW_CONSOLE is used, a new console is created, so stdin should be valid
    // But if it's not, we'll try to allocate a new console
    if (input_handle == null) {
        // Try to allocate a new console (this should already be done by CREATE_NEW_CONSOLE)
        if (AllocConsole() == 0) {
            std.log.err("Failed to allocate console\n", .{});
            return;
        }
        input_handle = GetStdHandle(STD_INPUT_HANDLE);
        if (input_handle == null) {
            std.log.err("Failed to get stdin handle after console allocation\n", .{});
            return;
        }
    }

    // Get current console mode first to see what's supported
    var current_mode: windows.DWORD = undefined;
    if (GetConsoleMode(input_handle.?, &current_mode) == 0) {
        std.log.err("Failed to get console mode\n", .{});
        return;
    }

    // Set raw input mode - we need ENABLE_PROCESSED_INPUT (0x0001) for key events
    // Clear ENABLE_LINE_INPUT (0x0002) and ENABLE_ECHO_INPUT (0x0004) if they're set
    const input_mode: windows.DWORD = 0x0001; // ENABLE_PROCESSED_INPUT - required for ReadConsoleInputW
    
    if (SetConsoleMode(input_handle.?, input_mode) == 0) {
        std.log.err("Failed to set console mode\n", .{});
        return;
    }

    // Read key events until we get a keydown event
    var input_record: INPUT_RECORD = undefined;
    var num_read: windows.DWORD = undefined;

    while (true) {
        const input_record_ptr: [*]INPUT_RECORD = @ptrCast(&input_record);
        if (ReadConsoleInputW(input_handle.?, input_record_ptr, 1, &num_read) == 0) {
            std.log.err("ReadConsoleInputW failed\n", .{});
            return;
        }

        if (num_read == 0) continue;

        if (input_record.EventType == KEY_EVENT) {
            const key_event = input_record.Event.KeyEvent;
            if (key_event.bKeyDown == 0) continue; // Ignore key releases

            const vk = key_event.wVirtualKeyCode;
            
            // Skip modifier-only key events (Ctrl, Shift, Alt)
            // VK_CONTROL = 0x11, VK_SHIFT = 0x10, VK_MENU (Alt) = 0x12
            // VK_LCONTROL = 0xA2, VK_RCONTROL = 0xA3
            // VK_LSHIFT = 0xA0, VK_RSHIFT = 0xA1
            // VK_LMENU = 0xA4, VK_RMENU = 0xA5
            if (vk == 0x11 or vk == 0x10 or vk == 0x12 or 
                vk == 0xA2 or vk == 0xA3 or vk == 0xA0 or 
                vk == 0xA1 or vk == 0xA4 or vk == 0xA5) {
                continue; // Skip modifier keys, wait for actual character key
            }

            const ascii_char = @as(u8, @intCast(key_event.uChar.AsciiChar));
            const unicode_char = key_event.uChar.UnicodeChar;

            // Output JSON directly using Windows WriteFile
            const STD_OUTPUT_HANDLE = @as(windows.DWORD, @bitCast(@as(i32, -11)));
            const output_handle = GetStdHandle(STD_OUTPUT_HANDLE) orelse {
                std.log.err("Failed to get stdout handle\n", .{});
                return;
            };
            
            var json_buf: [64]u8 = undefined;
            const json = try std.fmt.bufPrint(&json_buf, "{{\"vk\":{d},\"ascii\":{d},\"unicode\":{d}}}\n", .{ vk, ascii_char, unicode_char });
            
            var written: windows.DWORD = undefined;
            _ = WriteFile(output_handle, json.ptr, @as(windows.DWORD, @intCast(json.len)), &written, null);
            return;
        }
    }
}

