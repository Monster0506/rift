#!/usr/bin/env python3
# /// script
# requires-python = ">=3.10"
# dependencies = [
#     "pynput",
# ]
# ///

"""
Retry script for failed key combinations.
Reads the existing key_combinations.json and retries only the combinations that failed.
"""

import json
import time
import subprocess
import sys
import os
import logging
from typing import Dict, List, Tuple, Optional, Set
from pathlib import Path

from pynput import keyboard
from pynput.keyboard import Key, KeyCode

# Import from the main script
# Add the scripts directory to the path so we can import generate_key_combos
scripts_dir = Path(__file__).parent
if str(scripts_dir) not in sys.path:
    sys.path.insert(0, str(scripts_dir))

from generate_key_combos import (
    US_KEYS, MODIFIERS, find_capture_keys_exe, 
    map_key_to_pynput, capture_key_combo
)

# Setup logging
file_handler = logging.FileHandler('key_retry.log', encoding='utf-8')
console_handler = logging.StreamHandler(sys.stdout)

console_formatter = logging.Formatter('%(asctime)s - %(levelname)s - %(message)s')
file_formatter = logging.Formatter('%(asctime)s - %(levelname)s - %(message)s')

file_handler.setFormatter(file_formatter)
console_handler.setFormatter(console_formatter)

logging.basicConfig(
    level=logging.INFO,
    handlers=[file_handler, console_handler]
)
logger = logging.getLogger(__name__)

def load_existing_combinations(json_path: Path) -> Set[Tuple[str, bool, bool, bool]]:
    """Load existing successful combinations from JSON file."""
    if not json_path.exists():
        logger.warning(f"JSON file {json_path} does not exist. Will retry all combinations.")
        return set()
    
    try:
        with open(json_path, 'r', encoding='utf-8') as f:
            data = json.load(f)
        
        combinations = set()
        for combo in data.get('combinations', []):
            key = combo['key']
            mods = combo['modifiers']
            combinations.add((key, mods['ctrl'], mods['shift'], mods['alt']))
        
        logger.info(f"Loaded {len(combinations)} existing successful combinations from {json_path}")
        return combinations
    except Exception as e:
        logger.error(f"Error loading existing combinations: {e}")
        return set()

def generate_all_combinations() -> List[Tuple[str, bool, bool, bool]]:
    """Generate all possible key combinations."""
    all_combos = []
    for key in US_KEYS:
        for ctrl, shift, alt in MODIFIERS:
            all_combos.append((key, ctrl, shift, alt))
    return all_combos

def find_failed_combinations(existing: Set[Tuple[str, bool, bool, bool]]) -> List[Tuple[str, bool, bool, bool]]:
    """Find combinations that are missing from the existing set."""
    all_combos = generate_all_combinations()
    failed = [combo for combo in all_combos if combo not in existing]
    return failed

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

def retry_failed_combinations(exe_path: Path, failed_combos: List[Tuple[str, bool, bool, bool]]) -> List[Dict]:
    """Retry capturing failed combinations."""
    logger.info(f"Retrying {len(failed_combos)} failed combinations...")
    results = []
    successful = 0
    failed = 0
    total = len(failed_combos)
    
    for idx, (key, ctrl, shift, alt) in enumerate(failed_combos, 1):
        combo_str = format_combo_str(key, ctrl, shift, alt)
        
        logger.info(f"[{idx}/{total}] Retrying {combo_str}...")
        print(f"[{idx}/{total}] Retrying {combo_str}...", end=" ", flush=True)
        
        captured = capture_key_combo(key, ctrl, shift, alt, exe_path)
        
        if captured:
            result = {
                "key": key,
                "modifiers": {
                    "ctrl": ctrl,
                    "shift": shift,
                    "alt": alt
                },
                "combo": combo_str,
                "vk": f"0x{captured['vk']:02X}",
                "vk_decimal": captured['vk'],
                "ascii": captured.get('ascii'),
                "unicode": captured.get('unicode'),
                "ascii_hex": f"0x{captured.get('ascii', 0):02X}" if captured.get('ascii') else None,
                "unicode_hex": f"0x{captured.get('unicode', 0):04X}" if captured.get('unicode') else None,
            }
            results.append(result)
            successful += 1
            logger.info(f"[OK] Captured: vk=0x{captured['vk']:02X} ascii={captured.get('ascii', 'None')} unicode={captured.get('unicode', 'None')}")
            print(f"[OK] vk=0x{captured['vk']:02X} ascii={captured.get('ascii', 'None')} unicode={captured.get('unicode', 'None')}")
        else:
            failed += 1
            logger.warning(f"[FAIL] Failed to capture {combo_str}")
            print("[FAIL] Failed")
        
        # Small delay between captures
        time.sleep(0.1)
    
    logger.info(f"Retry complete: {successful} successful, {failed} failed out of {total} total")
    return results

def merge_results(existing_path: Path, new_results: List[Dict]) -> List[Dict]:
    """Merge new results with existing results."""
    existing_results: List[Dict] = []
    if existing_path.exists():
        try:
            with open(existing_path, 'r', encoding='utf-8') as f:
                data = json.load(f)
                combinations = data.get('combinations', [])
                if isinstance(combinations, list):
                    existing_results = combinations
        except Exception as e:
            logger.warning(f"Error loading existing results: {e}")
    
    # Create a set of existing combo keys for deduplication
    existing_keys: Set[Tuple[str, bool, bool, bool]] = set()
    for c in existing_results:
        if isinstance(c, dict):
            key = c.get('key', '')
            mods = c.get('modifiers', {})
            if isinstance(mods, dict):
                existing_keys.add((str(key), bool(mods.get('ctrl', False)), 
                                  bool(mods.get('shift', False)), bool(mods.get('alt', False))))
    
    # Add new results, avoiding duplicates
    for result in new_results:
        if isinstance(result, dict):
            combo_key = (str(result.get('key', '')), 
                        bool(result.get('modifiers', {}).get('ctrl', False)), 
                        bool(result.get('modifiers', {}).get('shift', False)), 
                        bool(result.get('modifiers', {}).get('alt', False)))
            if combo_key not in existing_keys:
                existing_results.append(result)
                existing_keys.add(combo_key)
    
    return existing_results

def main():
    logger.info("=" * 80)
    logger.info("Key Combination Retry Tool")
    logger.info("=" * 80)
    
    print("Retrying failed key combinations...")
    print("This will identify failed combinations and retry them.")
    print()
    
    # Check if executable exists
    logger.debug("Looking for capture_keys.exe...")
    exe_path = find_capture_keys_exe()
    if not exe_path:
        logger.error("capture_keys.exe not found!")
        print("ERROR: capture_keys.exe not found!")
        print("Please build it first: zig build-exe scripts/capture_keys.zig -target native")
        sys.exit(1)
    
    logger.info(f"Using capture program: {exe_path}")
    print(f"Using capture program: {exe_path}")
    print()
    
    # Load existing combinations
    json_path = Path("key_combinations.json")
    existing_combos = load_existing_combinations(json_path)
    
    # Find failed combinations
    failed_combos = find_failed_combinations(existing_combos)
    
    if not failed_combos:
        logger.info("No failed combinations found! All combinations were successfully captured.")
        print("No failed combinations found! All combinations were successfully captured.")
        return 0
    
    logger.info(f"Found {len(failed_combos)} failed combinations to retry")
    print(f"Found {len(failed_combos)} failed combinations to retry")
    print()
    
    # Ask for confirmation
    response = input("Proceed with retry? (y/n): ").strip().lower()
    if response != 'y':
        logger.info("Retry cancelled by user")
        print("Retry cancelled.")
        return 0
    
    print()
    
    # Retry failed combinations (exe_path is guaranteed to be Path here)
    assert exe_path is not None
    new_results = retry_failed_combinations(exe_path, failed_combos)
    
    if not new_results:
        logger.warning("No new results captured during retry")
        print("No new results captured during retry")
        return 0
    
    # Merge with existing results
    logger.info("Merging results with existing combinations...")
    all_results = merge_results(json_path, new_results)
    
    # Save merged results
    output_data = {
        "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
        "total_combinations": len(all_results),
        "combinations": sorted(all_results, key=lambda x: (x['key'], x['modifiers']['ctrl'], 
                                                           x['modifiers']['shift'], x['modifiers']['alt']))
    }
    
    logger.info(f"Writing {len(all_results)} total combinations to {json_path}...")
    with open(json_path, 'w', encoding='utf-8') as f:
        json.dump(output_data, f, indent=2, ensure_ascii=False)
    
    logger.info(f"Successfully wrote {len(all_results)} combinations to {json_path}")
    print(f"\nSuccessfully wrote {len(all_results)} total combinations to {json_path}")
    
    # Also write human-readable output
    txt_path = Path("key_combinations.txt")
    logger.info(f"Writing human-readable output to {txt_path}...")
    with open(txt_path, 'w', encoding='utf-8') as f:
        f.write("Key Combinations - Windows Console API Values\n")
        f.write("=" * 80 + "\n\n")
        f.write(f"Generated: {output_data['generated_at']}\n")
        f.write(f"Total combinations: {len(all_results)}\n\n")
        
        for combo in output_data['combinations']:
            mods = combo['modifiers']
            mod_str = []
            if mods['ctrl']:
                mod_str.append("Ctrl")
            if mods['shift']:
                mod_str.append("Shift")
            if mods['alt']:
                mod_str.append("Alt")
            mod_str.append(combo['key'].upper() if combo['key'].isalpha() else combo['key'])
            combo_name = "+".join(mod_str)
            
            f.write(f"{combo_name:30} | vk: {combo['vk']:6} | ascii: {combo.get('ascii', 'N/A'):4} | unicode: {combo.get('unicode', 'N/A'):6}\n")
    
    logger.info(f"Successfully wrote human-readable output to {txt_path}")
    print(f"Human-readable output written to: {txt_path}")
    print(f"Log file: key_retry.log")
    
    logger.info("=" * 80)
    logger.info("Retry process completed successfully")
    logger.info("=" * 80)
    
    return 0

if __name__ == "__main__":
    sys.exit(main())

