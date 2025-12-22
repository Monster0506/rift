//! By convention, root.zig is the root source file when making a library.
const std = @import("std");

// This is a library module - main functionality is in main.zig
// Export any public APIs here if needed

pub fn add(a: i32, b: i32) i32 {
    return a + b;
}

test "basic add functionality" {
    try std.testing.expect(add(3, 7) == 10);
}
