#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# ///

"""Generate a Zig lookup table from captured key combinations JSON."""

import json
from pathlib import Path

def main():
    json_path = Path("scripts/key_combinations.json")
    
    if not json_path.exists():
        print(f"Error: {json_path} not found!")
        return 1
    
    with open(json_path, 'r', encoding='utf-8') as f:
        data = json.load(f)
    
    combinations = data.get('combinations', [])
    
    # Generate Zig code
    zig_code = []
    zig_code.append("//! Auto-generated key mapping from captured Windows Console API values")
    zig_code.append("//! Generated from key_combinations.json")
    zig_code.append("")
    zig_code.append("const std = @import(\"std\");")
    zig_code.append("const Key = @import(\"key.zig\").Key;")
    zig_code.append("const KeyCombo = @import(\"keymap.zig\").KeyCombo;")
    zig_code.append("")
    zig_code.append("pub fn populateLookupTable(")
    zig_code.append("    map: *std.HashMap(KeyCombo, Key, @import(\"keymap.zig\").KeyComboContext, std.hash_map.default_max_load_percentage),")
    zig_code.append(") !void {")
    zig_code.append("    // Populate with captured key combinations")
    zig_code.append("")
    
    # Map key names to Key enum values
    key_map = {
        'backspace': 'Key.backspace',
        'enter': 'Key.enter',
        'escape': 'Key.escape',
        'tab': 'Key{ .char = 9 }',  # Tab as character
        'space': 'Key{ .char = 32 }',  # Space as character
        'up': 'Key.arrow_up',
        'down': 'Key.arrow_down',
        'left': 'Key.arrow_left',
        'right': 'Key.arrow_right',
        'home': 'Key.home',
        'end': 'Key.end',
        'page up': 'Key.page_up',
        'page down': 'Key.page_down',
        'delete': 'Key.delete',
        'insert': 'Key{ .char = 0 }',  # Not in our Key enum yet
    }
    
    # Function keys
    for i in range(1, 13):
        key_map[f'f{i}'] = f'Key{{ .char = 0 }}'  # Not in our Key enum yet
    
    entries_added = 0
    
    for combo in combinations:
        key_name = combo['key']
        mods = combo['modifiers']
        vk_hex = combo['vk']
        vk_decimal = combo['vk_decimal']
        ascii_val = combo.get('ascii')
        unicode_val = combo.get('unicode')
        
        # Determine the Key value
        if key_name in key_map:
            key_value = key_map[key_name]
        elif key_name.isalnum() and len(key_name) == 1:
            if mods['ctrl']:
                # Ctrl+key combination
                key_value = f'Key{{ .ctrl = \'{key_name.lower()}\' }}'
            else:
                # Regular character
                char_val = ord(key_name.lower() if key_name.isalpha() else key_name)
                key_value = f'Key{{ .char = {char_val} }}'
        else:
            # Symbol or other
            if ascii_val is not None:
                key_value = f'Key{{ .char = {ascii_val} }}'
            else:
                continue  # Skip if we can't determine the key
        
        # Build the KeyCombo
        ascii_str = f'{ascii_val}' if ascii_val is not None else 'null'
        unicode_str = f'{unicode_val}' if unicode_val is not None else 'null'
        
        zig_code.append(f"    try map.put(KeyCombo{{")
        zig_code.append(f"        .vk = {vk_decimal},")
        zig_code.append(f"        .ascii = {ascii_str},")
        zig_code.append(f"        .unicode = {unicode_str},")
        zig_code.append(f"        .ctrl = {str(mods['ctrl']).lower()},")
        zig_code.append(f"        .shift = {str(mods['shift']).lower()},")
        zig_code.append(f"        .alt = {str(mods['alt']).lower()},")
        zig_code.append(f"    }}, {key_value});")
        entries_added += 1
    
    zig_code.append("}")
    zig_code.append("")
    zig_code.append(f"// Total entries: {entries_added}")
    
    # Write to file
    output_path = Path("src/keymap_generated.zig")
    with open(output_path, 'w', encoding='utf-8') as f:
        f.write('\n'.join(zig_code))
    
    print(f"Generated {entries_added} key mappings to {output_path}")
    return 0

if __name__ == "__main__":
    import sys
    sys.exit(main())

