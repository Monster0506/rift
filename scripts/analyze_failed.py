#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# ///

"""Analyze which key combinations failed to capture."""

import json
from pathlib import Path

# US keyboard layout - all pressable keys (same as generate_key_combos.py)
US_KEYS = [
    # Letters
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm',
    'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z',
    # Numbers
    '0', '1', '2', '3', '4', '5', '6', '7', '8', '9',
    # Symbols
    '`', '-', '=', '[', ']', '\\', ';', "'", ',', '.', '/',
    # Special keys
    'space', 'enter', 'tab', 'backspace', 'escape',
    # Arrow keys
    'up', 'down', 'left', 'right',
    # Function keys
    'f1', 'f2', 'f3', 'f4', 'f5', 'f6', 'f7', 'f8', 'f9', 'f10', 'f11', 'f12',
    # Other keys
    'home', 'end', 'page up', 'page down', 'insert', 'delete',
]

# Modifier combinations
MODIFIERS = [
    (False, False, False),  # No modifiers
    (True, False, False),    # Ctrl
    (False, True, False),    # Shift
    (False, False, True),    # Alt
    (True, True, False),     # Ctrl+Shift
    (True, False, True),     # Ctrl+Alt
    (False, True, True),     # Shift+Alt
    (True, True, True),      # Ctrl+Shift+Alt
]

def format_combo_str(key: str, ctrl: bool, shift: bool, alt: bool) -> str:
    """Format a combination as a string."""
    combo_name = []
    if ctrl:
        combo_name.append("Ctrl")
    if shift:
        combo_name.append("Shift")
    if alt:
        combo_name.append("Alt")
    combo_name.append(key.upper() if key.isalpha() else key)
    return "+".join(combo_name)

def main():
    json_path = Path("key_combinations.json")
    
    if not json_path.exists():
        print(f"Error: {json_path} not found!")
        return
    
    # Load captured combinations
    with open(json_path, 'r', encoding='utf-8') as f:
        data = json.load(f)
    
    captured = {
        (c['key'], c['modifiers']['ctrl'], c['modifiers']['shift'], c['modifiers']['alt'])
        for c in data['combinations']
    }
    
    # Generate all possible combinations
    all_combos = {
        (k, ctrl, shift, alt)
        for k in US_KEYS
        for ctrl, shift, alt in MODIFIERS
    }
    
    # Find failed combinations
    failed = sorted(all_combos - captured)
    
    print(f"Total expected: {len(all_combos)}")
    print(f"Total captured: {len(captured)}")
    print(f"Total failed: {len(failed)}")
    print()
    
    if not failed:
        print("All combinations were successfully captured!")
        return
    
    # Group failures by key
    failures_by_key = {}
    for key, ctrl, shift, alt in failed:
        if key not in failures_by_key:
            failures_by_key[key] = []
        failures_by_key[key].append((ctrl, shift, alt))
    
    print("Failed combinations grouped by key:")
    print("=" * 80)
    
    for key in sorted(failures_by_key.keys()):
        mods = failures_by_key[key]
        print(f"\n{key.upper()}: {len(mods)} failed combinations")
        for ctrl, shift, alt in sorted(mods):
            combo_str = format_combo_str(key, ctrl, shift, alt)
            print(f"  - {combo_str}")
    
    # Summary statistics
    print("\n" + "=" * 80)
    print("Summary:")
    print(f"  Keys with failures: {len(failures_by_key)}")
    print(f"  Keys without failures: {len(US_KEYS) - len(failures_by_key)}")
    
    # Most problematic keys
    print("\nMost problematic keys (most failures):")
    sorted_keys = sorted(failures_by_key.items(), key=lambda x: len(x[1]), reverse=True)
    for key, mods in sorted_keys[:10]:
        print(f"  {key.upper()}: {len(mods)} failures")

if __name__ == "__main__":
    main()

