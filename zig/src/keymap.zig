//! Windows-specific key mapping using captured Windows Console API values
//! Maps (vk, ascii, unicode, modifiers) to logical keys
//! This module is Windows-only and uses actual captured Windows Console API values

const std = @import("std");
const windows = std.os.windows;
const Key = @import("key.zig").Key;

/// Key combination identifier
pub const KeyCombo = struct {
    vk: u16,
    ascii: ?u8,
    unicode: ?u16,
    ctrl: bool,
    shift: bool,
    alt: bool,
};

/// Lookup table entry
const LookupEntry = struct {
    combo: KeyCombo,
    key: Key,
};

/// Build a lookup table using discovered patterns and captured data
/// Uses patterns: VK codes are constant, Ctrl+Letter = ASCII 1-26, etc.
pub fn buildLookupTable(allocator: std.mem.Allocator) !std.HashMap(KeyCombo, Key, KeyComboContext, std.hash_map.default_max_load_percentage) {
    var map = std.HashMap(KeyCombo, Key, KeyComboContext, std.hash_map.default_max_load_percentage).init(allocator);
    
    // PATTERN: Special keys have fixed VK codes (VK is constant regardless of modifiers)
    // Arrow keys - VK codes are constant, use VK-only lookup
    try map.put(KeyCombo{ .vk = 0x26, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.arrow_up); // VK_UP
    try map.put(KeyCombo{ .vk = 0x28, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.arrow_down); // VK_DOWN
    try map.put(KeyCombo{ .vk = 0x25, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.arrow_left); // VK_LEFT
    try map.put(KeyCombo{ .vk = 0x27, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.arrow_right); // VK_RIGHT
    
    // Navigation keys
    try map.put(KeyCombo{ .vk = 0x24, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.home); // VK_HOME
    try map.put(KeyCombo{ .vk = 0x23, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.end); // VK_END
    try map.put(KeyCombo{ .vk = 0x21, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.page_up); // VK_PRIOR
    try map.put(KeyCombo{ .vk = 0x22, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.page_down); // VK_NEXT
    
    // Control keys
    // From captured data, these keys come with specific ascii/unicode values
    // Backspace: vk=0x08, ascii=8, unicode=8
    try map.put(KeyCombo{ .vk = 0x08, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.backspace); // VK_BACK (no ascii/unicode)
    try map.put(KeyCombo{ .vk = 0x08, .ascii = 8, .unicode = 8, .ctrl = false, .shift = false, .alt = false }, Key.backspace); // VK_BACK (with ascii/unicode)
    
    // Enter: vk=0x0D, ascii=13, unicode=13 (normal), ascii=10/unicode=10 (Ctrl+Enter)
    try map.put(KeyCombo{ .vk = 0x0D, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.enter); // VK_RETURN (no ascii/unicode)
    try map.put(KeyCombo{ .vk = 0x0D, .ascii = 13, .unicode = 13, .ctrl = false, .shift = false, .alt = false }, Key.enter); // VK_RETURN (normal Enter)
    try map.put(KeyCombo{ .vk = 0x0D, .ascii = 13, .unicode = 13, .ctrl = false, .shift = true, .alt = false }, Key.enter); // VK_RETURN (Shift+Enter)
    try map.put(KeyCombo{ .vk = 0x0D, .ascii = 10, .unicode = 10, .ctrl = true, .shift = false, .alt = false }, Key.enter); // VK_RETURN (Ctrl+Enter)
    
    // Escape: vk=0x1B, ascii=27, unicode=27
    try map.put(KeyCombo{ .vk = 0x1B, .ascii = 27, .unicode = 27, .ctrl = false, .shift = false, .alt = false }, Key.escape); // VK_ESCAPE
    
    // Delete: vk=0x2E
    try map.put(KeyCombo{ .vk = 0x2E, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key.delete); // VK_DELETE
    
    // Tab: vk=0x09, ascii=9, unicode=9
    try map.put(KeyCombo{ .vk = 0x09, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false }, Key{ .char = 9 }); // VK_TAB (no ascii/unicode)
    try map.put(KeyCombo{ .vk = 0x09, .ascii = 9, .unicode = 9, .ctrl = false, .shift = false, .alt = false }, Key{ .char = 9 }); // VK_TAB (with ascii/unicode)
    
    // Space: vk=0x20, ascii=32, unicode=32
    try map.put(KeyCombo{ .vk = 0x20, .ascii = 32, .unicode = 32, .ctrl = false, .shift = false, .alt = false }, Key{ .char = ' ' }); // VK_SPACE
    
    // PATTERN: Letters A-Z have sequential VK codes (0x41-0x5A)
    // Generate all letter combinations programmatically
    var ch: u8 = 'a';
    while (ch <= 'z') : (ch += 1) {
        const vk = 0x41 + (ch - 'a'); // VK code for letter (A=0x41, B=0x42, etc.)
        const lower_ascii = ch; // lowercase ASCII
        const upper_ascii = ch - 32; // uppercase ASCII
        
        // PATTERN: Ctrl+Letter maps to ASCII 1-26
        const ctrl_ascii = ch - 'a' + 1;
        try map.put(KeyCombo{ .vk = vk, .ascii = ctrl_ascii, .unicode = ctrl_ascii, .ctrl = true, .shift = false, .alt = false }, Key{ .ctrl = ch });
        try map.put(KeyCombo{ .vk = vk, .ascii = ctrl_ascii, .unicode = ctrl_ascii, .ctrl = true, .shift = true, .alt = false }, Key{ .ctrl = ch });
        
        // PATTERN: Shift changes ASCII/Unicode but not VK
        // No modifiers: lowercase
        try map.put(KeyCombo{ .vk = vk, .ascii = lower_ascii, .unicode = lower_ascii, .ctrl = false, .shift = false, .alt = false }, Key{ .char = ch });
        // Shift: uppercase
        try map.put(KeyCombo{ .vk = vk, .ascii = upper_ascii, .unicode = upper_ascii, .ctrl = false, .shift = true, .alt = false }, Key{ .char = ch - 32 });
        
        // PATTERN: Alt often preserves ASCII/Unicode
        try map.put(KeyCombo{ .vk = vk, .ascii = lower_ascii, .unicode = lower_ascii, .ctrl = false, .shift = false, .alt = true }, Key{ .char = ch });
        try map.put(KeyCombo{ .vk = vk, .ascii = upper_ascii, .unicode = upper_ascii, .ctrl = false, .shift = true, .alt = true }, Key{ .char = ch - 32 });
    }
    
    // PATTERN: Numbers 0-9 have sequential VK codes (0x30-0x39)
    var num: u8 = '0';
    while (num <= '9') : (num += 1) {
        const vk = num; // VK code equals ASCII for numbers
        const ascii_val = num;
        
        // No modifiers
        try map.put(KeyCombo{ .vk = vk, .ascii = ascii_val, .unicode = ascii_val, .ctrl = false, .shift = false, .alt = false }, Key{ .char = num });
        
        // Shift (produces symbols like !@#$%^&*())
        // Note: Shift+number produces different characters, but we'll handle via ASCII lookup
        
        // PATTERN: Alt often preserves ASCII/Unicode
        try map.put(KeyCombo{ .vk = vk, .ascii = ascii_val, .unicode = ascii_val, .ctrl = false, .shift = false, .alt = true }, Key{ .char = num });
    }
    
    // PATTERN: Function keys F1-F12 have sequential VK codes (0x70-0x7B)
    // Function keys don't produce characters, handled via VK lookup in fallback logic
    // F1=0x70, F2=0x71, F3=0x72, F4=0x73, F5=0x74, F6=0x75, F7=0x76, F8=0x77, F9=0x78, F10=0x79, F11=0x7A, F12=0x7B
    
    // Symbol keys - use captured VK codes
    // These vary, so we use specific mappings from captured data
    const symbol_vks = [_]struct { vk: u16, base_ascii: u8, shift_ascii: u8 }{
        .{ .vk = 0xDE, .base_ascii = 39, .shift_ascii = 34 }, // ' and "
        .{ .vk = 0xBC, .base_ascii = 44, .shift_ascii = 60 }, // , and <
        .{ .vk = 0xBE, .base_ascii = 46, .shift_ascii = 62 }, // . and >
        .{ .vk = 0xBF, .base_ascii = 47, .shift_ascii = 63 }, // / and ?
        .{ .vk = 0xBA, .base_ascii = 59, .shift_ascii = 58 }, // ; and :
        .{ .vk = 0xDB, .base_ascii = 91, .shift_ascii = 123 }, // [ and {
        .{ .vk = 0xDC, .base_ascii = 92, .shift_ascii = 124 }, // \ and |
        .{ .vk = 0xDD, .base_ascii = 93, .shift_ascii = 125 }, // ] and }
        .{ .vk = 0xC0, .base_ascii = 96, .shift_ascii = 126 }, // ` and ~
        .{ .vk = 0xBD, .base_ascii = 45, .shift_ascii = 95 }, // - and _
        .{ .vk = 0xBB, .base_ascii = 61, .shift_ascii = 43 }, // = and +
    };
    
    for (symbol_vks) |sym| {
        // PATTERN: Shift changes ASCII/Unicode but not VK
        try map.put(KeyCombo{ .vk = sym.vk, .ascii = sym.base_ascii, .unicode = sym.base_ascii, .ctrl = false, .shift = false, .alt = false }, Key{ .char = sym.base_ascii });
        try map.put(KeyCombo{ .vk = sym.vk, .ascii = sym.shift_ascii, .unicode = sym.shift_ascii, .ctrl = false, .shift = true, .alt = false }, Key{ .char = sym.shift_ascii });
        
        // PATTERN: Alt often preserves ASCII/Unicode
        try map.put(KeyCombo{ .vk = sym.vk, .ascii = sym.base_ascii, .unicode = sym.base_ascii, .ctrl = false, .shift = false, .alt = true }, Key{ .char = sym.base_ascii });
        try map.put(KeyCombo{ .vk = sym.vk, .ascii = sym.shift_ascii, .unicode = sym.shift_ascii, .ctrl = false, .shift = true, .alt = true }, Key{ .char = sym.shift_ascii });
        
        // Ctrl+] special case (ASCII 29)
        if (sym.vk == 0xDD) {
            try map.put(KeyCombo{ .vk = 0xDD, .ascii = 29, .unicode = 29, .ctrl = true, .shift = false, .alt = false }, Key{ .ctrl = ']' });
        }
    }
    
    // PATTERN: Ctrl+symbol combinations often have ascii=0, unicode=0
    // These are handled via VK-only lookup in the fallback
    
    return map;
}

/// Context for hashing KeyCombo
pub const KeyComboContext = struct {
    pub fn hash(self: KeyComboContext, combo: KeyCombo) u64 {
        _ = self;
        var hasher = std.hash.Wyhash.init(0);
        std.hash.autoHash(&hasher, combo.vk);
        std.hash.autoHash(&hasher, combo.ascii);
        std.hash.autoHash(&hasher, combo.unicode);
        std.hash.autoHash(&hasher, combo.ctrl);
        std.hash.autoHash(&hasher, combo.shift);
        std.hash.autoHash(&hasher, combo.alt);
        return hasher.final();
    }
    
    pub fn eql(self: KeyComboContext, a: KeyCombo, b: KeyCombo) bool {
        _ = self;
        return a.vk == b.vk and
               a.ascii == b.ascii and
               a.unicode == b.unicode and
               a.ctrl == b.ctrl and
               a.shift == b.shift and
               a.alt == b.alt;
    }
};

/// Look up a key from Windows Console API values
/// Uses discovered patterns for efficient lookup
pub fn lookupKey(
    map: *const std.HashMap(KeyCombo, Key, KeyComboContext, std.hash_map.default_max_load_percentage),
    vk: u16,
    ascii: u8,
    unicode: u16,
    ctrl: bool,
    shift: bool,
    alt: bool,
) ?Key {
    // PATTERN: VK codes are constant - use them as primary lookup key
    
    // Try exact match first (vk + ascii + unicode + modifiers)
    const combo = KeyCombo{
        .vk = vk,
        .ascii = ascii,
        .unicode = unicode,
        .ctrl = ctrl,
        .shift = shift,
        .alt = alt,
    };
    
    if (map.get(combo)) |key| {
        return key;
    }
    
    // Special cases for control keys - handle early as fallback
    // These should be in the map, but handle them explicitly if lookup fails
    
    // Backspace (VK 0x08): ascii=8, unicode=8
    if (vk == 0x08) {
        return Key.backspace;
    }
    
    // Enter (VK 0x0D): ascii=13/unicode=13 (normal), ascii=10/unicode=10 (Ctrl+Enter), or ascii=0/unicode=0
    if (vk == 0x0D) {
        return Key.enter;
    }
    
    // Tab (VK 0x09): ascii=9, unicode=9
    if (vk == 0x09) {
        return Key{ .char = 9 };
    }
    
    // PATTERN: Ctrl+Letter = ASCII 1-26, handle even if exact match fails
    if (ctrl and ascii >= 1 and ascii <= 26) {
        const letter = 'a' + (ascii - 1);
        return Key{ .ctrl = letter };
    }
    
    // Try with VK only, ignoring modifiers (for special keys where modifiers don't matter)
    // This handles Escape, Arrow keys, etc. - they should work regardless of modifiers
    // Do this BEFORE trying ASCII/Unicode matches to catch special keys early
    if (map.get(KeyCombo{ .vk = vk, .ascii = null, .unicode = null, .ctrl = false, .shift = false, .alt = false })) |key| {
        return key;
    }
    
    // PATTERN: Shift changes ASCII/Unicode but not VK
    // Try with VK + ASCII/Unicode (ignoring shift state for some keys)
    if (!ctrl and !alt and unicode >= 32 and unicode < 127) {
        // Try with current shift state
        if (map.get(KeyCombo{ .vk = vk, .ascii = ascii, .unicode = unicode, .ctrl = false, .shift = shift, .alt = false })) |key| {
            return key;
        }
        // Try with opposite shift state (in case shift wasn't detected correctly)
        if (map.get(KeyCombo{ .vk = vk, .ascii = ascii, .unicode = unicode, .ctrl = false, .shift = !shift, .alt = false })) |key| {
            return key;
        }
    }
    
    // PATTERN: Alt often preserves ASCII/Unicode
    // Try without Alt modifier (Alt might not affect the character)
    if (alt and !ctrl) {
        if (map.get(KeyCombo{ .vk = vk, .ascii = ascii, .unicode = unicode, .ctrl = false, .shift = shift, .alt = false })) |key| {
            return key;
        }
    }
    
    // Try with VK only, with current modifiers (for keys that might care about modifiers)
    if (map.get(KeyCombo{ .vk = vk, .ascii = null, .unicode = null, .ctrl = ctrl, .shift = shift, .alt = alt })) |key| {
        return key;
    }
    
    // PATTERN: Printable characters can be identified by Unicode alone
    if (!ctrl and unicode >= 32 and unicode < 127) {
        return Key{ .char = @as(u8, @intCast(unicode)) };
    }
    
    return null;
}

