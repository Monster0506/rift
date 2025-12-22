#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# ///

"""Analyze patterns and logical relationships in captured key combinations."""

import json
from pathlib import Path
from collections import defaultdict

def main():
    # Try multiple possible paths
    possible_paths = [
        Path("scripts/key_combinations.json"),
        Path("key_combinations.json"),
        Path(__file__).parent / "key_combinations.json",
    ]
    
    json_path = None
    for path in possible_paths:
        if path.exists():
            json_path = path
            break
    
    if json_path is None:
        print(f"Error: key_combinations.json not found!")
        print(f"Tried: {[str(p) for p in possible_paths]}")
        return 1
    
    with open(json_path, 'r', encoding='utf-8') as f:
        data = json.load(f)
    
    combinations = data.get('combinations', [])
    
    print("=" * 80)
    print("KEY COMBINATION PATTERN ANALYSIS")
    print("=" * 80)
    print()
    
    # Pattern 1: VK codes remain constant for the same physical key
    print("PATTERN 1: Virtual Key (VK) codes are constant for physical keys")
    print("-" * 80)
    vk_groups = defaultdict(list)
    for combo in combinations:
        vk_groups[combo['vk_decimal']].append(combo)
    
    print(f"Found {len(vk_groups)} unique VK codes")
    print("\nExample - VK 0x41 (A key):")
    a_combos = vk_groups[65]  # VK for 'A'
    for c in sorted(a_combos, key=lambda x: (x['modifiers']['ctrl'], x['modifiers']['shift'], x['modifiers']['alt'])):
        mods = []
        if c['modifiers']['ctrl']: mods.append("Ctrl")
        if c['modifiers']['shift']: mods.append("Shift")
        if c['modifiers']['alt']: mods.append("Alt")
        mod_str = "+".join(mods) if mods else "None"
        print(f"  {mod_str:20} vk={c['vk']:6} ascii={c.get('ascii', 0):4} unicode={c.get('unicode', 0):6}")
    print()
    
    # Pattern 2: Ctrl+Letter combinations
    print("PATTERN 2: Ctrl+Letter combinations")
    print("-" * 80)
    ctrl_letters = [c for c in combinations if c['modifiers']['ctrl'] and c['key'].isalpha() and len(c['key']) == 1]
    print(f"Found {len(ctrl_letters)} Ctrl+letter combinations")
    print("\nPattern: Ctrl+A through Ctrl+Z map to ASCII 1-26")
    for c in sorted(ctrl_letters, key=lambda x: x['key'])[:10]:
        if not c['modifiers']['shift'] and not c['modifiers']['alt']:
            print(f"  {c['combo']:15} vk={c['vk']:6} ascii={c.get('ascii'):4} unicode={c.get('unicode'):6}")
    print()
    
    # Pattern 3: Shift changes ASCII/Unicode but not VK
    print("PATTERN 3: Shift modifier changes ASCII/Unicode but not VK")
    print("-" * 80)
    print("Example - Comma key (VK 0xBC):")
    comma_combos = [c for c in combinations if c['key'] == ',']
    for c in sorted(comma_combos, key=lambda x: (x['modifiers']['shift'], x['modifiers']['ctrl'], x['modifiers']['alt'])):
        mods = []
        if c['modifiers']['ctrl']: mods.append("Ctrl")
        if c['modifiers']['shift']: mods.append("Shift")
        if c['modifiers']['alt']: mods.append("Alt")
        mod_str = "+".join(mods) if mods else "None"
        print(f"  {mod_str:20} vk={c['vk']:6} ascii={c.get('ascii', 0):4} unicode={c.get('unicode', 0):6} -> '{chr(c.get('unicode', 0)) if c.get('unicode', 0) > 0 else 'N/A'}'")
    print()
    
    # Pattern 4: Ctrl combinations often have ascii=0, unicode=0
    print("PATTERN 4: Ctrl combinations often have ascii=0, unicode=0")
    print("-" * 80)
    ctrl_zero = [c for c in combinations if c['modifiers']['ctrl'] and c.get('ascii', 0) == 0 and c.get('unicode', 0) == 0]
    print(f"Found {len(ctrl_zero)} Ctrl combinations with ascii=0, unicode=0")
    print("Examples:")
    for c in ctrl_zero[:10]:
        print(f"  {c['combo']:25} vk={c['vk']:6}")
    print()
    
    # Pattern 5: Alt combinations often preserve ASCII/Unicode
    print("PATTERN 5: Alt combinations often preserve ASCII/Unicode")
    print("-" * 80)
    alt_preserved = [c for c in combinations if c['modifiers']['alt'] and not c['modifiers']['ctrl'] and c.get('ascii', 0) > 0]
    print(f"Found {len(alt_preserved)} Alt combinations with preserved ASCII/Unicode")
    print("Examples:")
    for c in alt_preserved[:10]:
        print(f"  {c['combo']:25} vk={c['vk']:6} ascii={c.get('ascii'):4} unicode={c.get('unicode'):6}")
    print()
    
    # Pattern 6: Function keys have consistent VK codes
    print("PATTERN 6: Function keys have consistent VK codes")
    print("-" * 80)
    function_keys = [c for c in combinations if c['key'].startswith('f') and c['key'][1:].isdigit()]
    fk_vks = {}
    for c in function_keys:
        if c['key'] not in fk_vks:
            fk_vks[c['key']] = c['vk_decimal']
    
    print("Function key VK codes:")
    for fk in sorted(fk_vks.keys(), key=lambda x: int(x[1:])):
        print(f"  {fk.upper():5} -> VK 0x{fk_vks[fk]:02X} ({fk_vks[fk]:3})")
    print()
    
    # Pattern 7: Special keys have fixed VK codes
    print("PATTERN 7: Special keys have fixed VK codes")
    print("-" * 80)
    special_keys = {
        'backspace': 0x08,
        'enter': 0x0D,
        'escape': 0x1B,
        'tab': 0x09,
        'space': 0x20,
        'up': 0x26,
        'down': 0x28,
        'left': 0x25,
        'right': 0x27,
        'home': 0x24,
        'end': 0x23,
        'page up': 0x21,
        'page down': 0x22,
        'delete': 0x2E,
    }
    
    print("Special key VK codes (from data):")
    for key_name in special_keys.keys():
        key_combos = [c for c in combinations if c['key'].lower() == key_name.lower() and not any([c['modifiers']['ctrl'], c['modifiers']['shift'], c['modifiers']['alt']])]
        if key_combos:
            vk = key_combos[0]['vk_decimal']
            expected = special_keys[key_name]
            match = "[OK]" if vk == expected else "[DIFF]"
            print(f"  {key_name:15} -> VK 0x{vk:02X} ({vk:3}) {match} (expected 0x{expected:02X})")
    print()
    
    # Pattern 8: Letters have sequential VK codes
    print("PATTERN 8: Letters have sequential VK codes")
    print("-" * 80)
    letter_combos = [c for c in combinations if c['key'].isalpha() and len(c['key']) == 1 and not any([c['modifiers']['ctrl'], c['modifiers']['shift'], c['modifiers']['alt']])]
    letter_vks = {}
    for c in letter_combos:
        if c['key'] not in letter_vks:
            letter_vks[c['key']] = c['vk_decimal']
    
    print("Letter VK codes (lowercase):")
    for letter in sorted(letter_vks.keys()):
        vk = letter_vks[letter]
        expected = ord(letter.upper())
        match = "[OK]" if vk == expected else "[DIFF]"
        print(f"  {letter.upper()} -> VK 0x{vk:02X} ({vk:3}) {match} (expected 0x{expected:02X})")
    print()
    
    # Pattern 9: Numbers have sequential VK codes
    print("PATTERN 9: Numbers have sequential VK codes")
    print("-" * 80)
    number_combos = [c for c in combinations if c['key'].isdigit() and len(c['key']) == 1 and not any([c['modifiers']['ctrl'], c['modifiers']['shift'], c['modifiers']['alt']])]
    number_vks = {}
    for c in number_combos:
        if c['key'] not in number_vks:
            number_vks[c['key']] = c['vk_decimal']
    
    print("Number VK codes:")
    for num in sorted(number_vks.keys()):
        vk = number_vks[num]
        expected = ord(num)
        match = "[OK]" if vk == expected else "[DIFF]"
        print(f"  {num} -> VK 0x{vk:02X} ({vk:3}) {match} (expected 0x{expected:02X})")
    print()
    
    # Pattern 10: Summary statistics
    print("PATTERN 10: Summary Statistics")
    print("-" * 80)
    print(f"Total combinations: {len(combinations)}")
    print(f"Unique VK codes: {len(vk_groups)}")
    print(f"Unique keys: {len(set(c['key'] for c in combinations))}")
    
    # Count by modifier combinations
    modifier_counts = defaultdict(int)
    for c in combinations:
        mods = []
        if c['modifiers']['ctrl']: mods.append("Ctrl")
        if c['modifiers']['shift']: mods.append("Shift")
        if c['modifiers']['alt']: mods.append("Alt")
        mod_str = "+".join(mods) if mods else "None"
        modifier_counts[mod_str] += 1
    
    print("\nCombinations by modifier type:")
    for mod_str, count in sorted(modifier_counts.items(), key=lambda x: x[1], reverse=True):
        print(f"  {mod_str:20}: {count:4}")
    print()
    
    # Keys with most combinations
    key_counts = defaultdict(int)
    for c in combinations:
        key_counts[c['key']] += 1
    
    print("Keys with most captured combinations:")
    for key, count in sorted(key_counts.items(), key=lambda x: x[1], reverse=True)[:15]:
        print(f"  {key:15}: {count:4}")
    print()
    
    print("=" * 80)
    print("ANALYSIS COMPLETE")
    print("=" * 80)
    
    return 0

if __name__ == "__main__":
    import sys
    sys.exit(main())

