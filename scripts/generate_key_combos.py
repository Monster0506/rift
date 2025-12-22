#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.12"
# dependencies = [
#     "pynput",
# ]
# ///

"""
Generate a file with all combinations of Ctrl, Shift, Alt + key presses
and their corresponding Windows Console API values (vk, ascii, unicode).

This script uses pynput to simulate keypresses and a Zig helper program
to capture the actual Windows Console API values.

Run with: uv run scripts/generate_key_combos.py
Make sure to build capture_keys.exe first: zig build-exe scripts/capture_keys.zig -target native
"""

import time
import json
import subprocess
import sys
import os
import logging
from typing import Dict, List, Tuple, Optional, Union
from pathlib import Path

from pynput import keyboard
from pynput.keyboard import Key, KeyCode

# Setup logging
# Use UTF-8 encoding for file handler to avoid Windows console encoding issues
file_handler = logging.FileHandler('key_capture.log', encoding='utf-8')
console_handler = logging.StreamHandler(sys.stdout)

# For console, use a formatter that avoids Unicode issues
console_formatter = logging.Formatter('%(asctime)s - %(levelname)s - %(message)s')
file_formatter = logging.Formatter('%(asctime)s - %(levelname)s - %(message)s')

file_handler.setFormatter(file_formatter)
console_handler.setFormatter(console_formatter)

logging.basicConfig(
    level=logging.INFO,
    handlers=[file_handler, console_handler]
)
logger = logging.getLogger(__name__)

# US keyboard layout - all pressable keys
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

def find_capture_keys_exe() -> Optional[Path]:
    """Find the capture_keys executable."""
    # Check in zig-out/bin first (if built with zig build)
    exe_path = Path("zig-out/bin/capture_keys.exe")
    if exe_path.exists():
        return exe_path
    
    # Check in current directory
    exe_path = Path("capture_keys.exe")
    if exe_path.exists():
        return exe_path
    
    # Check in scripts directory
    exe_path = Path("scripts/capture_keys.exe")
    if exe_path.exists():
        return exe_path
    
    return None

def map_key_to_pynput(key: str) -> Optional[Union[Key, KeyCode]]:
    """Map our key name to pynput Key or KeyCode.
    
    Returns Key objects for special keys (function keys, arrows, etc.)
    Returns KeyCode objects for regular character keys.
    """
    # Special keys - these must use Key objects, not KeyCode
    special_map = {
        'space': Key.space,
        'enter': Key.enter,
        'tab': Key.tab,
        'backspace': Key.backspace,
        'escape': Key.esc,
        'up': Key.up,
        'down': Key.down,
        'left': Key.left,
        'right': Key.right,
        'home': Key.home,
        'end': Key.end,
        'page up': Key.page_up,
        'page down': Key.page_down,
        'insert': Key.insert,
        'delete': Key.delete,
    }
    if key in special_map:
        return special_map[key]
    
    # Function keys - must use Key objects
    if key.startswith('f'):
        try:
            num = int(key[1:])
            if 1 <= num <= 12:
                return getattr(Key, f'f{num}')
        except (ValueError, AttributeError):
            pass
    
    # Letters and digits - use KeyCode
    if key.isalnum():
        return KeyCode.from_char(key.lower() if key.isalpha() else key)
    
    # Symbol keys - use KeyCode for single characters
    symbol_map = {
        '`': '`', '-': '-', '=': '=', '[': '[', ']': ']', '\\': '\\',
        ';': ';', "'": "'", ',': ',', '.': '.', '/': '/'
    }
    if key in symbol_map:
        char = symbol_map[key]
        if len(char) == 1:
            return KeyCode.from_char(char)
    
    return None

def capture_key_combo(key: str, ctrl: bool, shift: bool, alt: bool, exe_path: Path) -> Optional[Dict]:
    """Capture a single key combination using the Zig helper and pynput."""
    logger.debug(f"Starting capture for key={key}, ctrl={ctrl}, shift={shift}, alt={alt}")
    
    # Start the capture program
    # Note: We use CREATE_NEW_CONSOLE to get a separate console window that can receive keyboard input
    # When CREATE_NEW_CONSOLE is used, the process gets its own console, so stdin will be connected to that console
    # We still need to capture stdout/stderr for the JSON output
    logger.debug(f"Launching {exe_path}")
    proc = subprocess.Popen(
        [str(exe_path)],
        stdin=None,  # Don't redirect stdin - let it use the console directly (None means inherit, but with CREATE_NEW_CONSOLE it uses the new console)
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding='utf-8',
        errors='replace',
        creationflags=subprocess.CREATE_NEW_CONSOLE if sys.platform == "win32" else 0
    )
    
    # Give it a moment to initialize and create the console window
    logger.debug("Waiting for capture program to initialize...")
    time.sleep(0.8)  # Give more time for console window to be fully created and ready
    
    # Map key to pynput
    pynput_key = map_key_to_pynput(key)
    if not pynput_key:
        logger.warning(f"Could not map key '{key}' to pynput KeyCode")
        proc.kill()
        proc.communicate()
        return None
    
    logger.debug(f"Mapped key '{key}' to pynput key: {pynput_key}")
    
    # Create keyboard controller
    kb = keyboard.Controller()
    
    try:
        # Focus the capture program's console window (bring it to front)
        # We need to find the window belonging to the capture program process
        if sys.platform == "win32":
            logger.debug("Focusing capture program console window...")
            import ctypes
            from ctypes import wintypes
            
            user32 = ctypes.windll.user32
            kernel32 = ctypes.windll.kernel32
            
            # GetWindowThreadProcessId is in user32.dll, not kernel32.dll
            user32.GetWindowThreadProcessId.argtypes = [wintypes.HWND, ctypes.POINTER(ctypes.c_ulong)]
            user32.GetWindowThreadProcessId.restype = ctypes.c_ulong
            
            # Declare additional functions for better window focusing
            user32.AllowSetForegroundWindow.argtypes = [ctypes.c_int]
            user32.AllowSetForegroundWindow.restype = ctypes.c_bool
            user32.SetActiveWindow.argtypes = [wintypes.HWND]
            user32.SetActiveWindow.restype = wintypes.HWND
            user32.SetFocus.argtypes = [wintypes.HWND]
            user32.SetFocus.restype = wintypes.HWND
            
            # Allow the capture process to set foreground window
            user32.AllowSetForegroundWindow(proc.pid)
            
            # Try to find the window by process ID
            found_window = False
            target_hwnd = None
            
            def enum_windows_callback(hwnd, lParam):
                nonlocal found_window, target_hwnd
                try:
                    process_id = ctypes.c_ulong()
                    user32.GetWindowThreadProcessId(hwnd, ctypes.byref(process_id))
                    if process_id.value == proc.pid:
                        # Found a window belonging to our process
                        target_hwnd = hwnd
                        found_window = True
                        logger.debug(f"Found window (hwnd={hwnd}) for process {proc.pid}")
                        return False  # Stop enumeration
                except Exception as e:
                    logger.debug(f"Error in enum callback: {e}")
                return True  # Continue enumeration
            
            EnumWindowsProc = ctypes.WINFUNCTYPE(ctypes.c_bool, wintypes.HWND, ctypes.POINTER(ctypes.c_int))
            try:
                user32.EnumWindows(EnumWindowsProc(enum_windows_callback), None)
            except Exception as e:
                logger.warning(f"Error enumerating windows: {e}")
            
            if found_window and target_hwnd:
                # Aggressively focus the window
                logger.debug(f"Focusing window {target_hwnd}...")
                user32.ShowWindow(target_hwnd, 9)  # SW_RESTORE
                user32.BringWindowToTop(target_hwnd)
                user32.SetForegroundWindow(target_hwnd)
                user32.SetActiveWindow(target_hwnd)
                user32.SetFocus(target_hwnd)
                logger.debug("Window focus operations completed")
            else:
                logger.warning(f"Could not find console window for process {proc.pid}")
            
            # Give time for window to come to front and be ready to receive input
            time.sleep(0.6)  # Increased wait time to ensure window is ready
            logger.debug("Window focus wait completed")
        
        # Press modifiers and key together, then release
        # This ensures the capture program sees the combination, not just modifiers
        logger.debug("Pressing key combination...")
        
        # Press all modifiers first
        if ctrl:
            kb.press(Key.ctrl)
            logger.debug("  Ctrl pressed")
        if shift:
            kb.press(Key.shift)
            logger.debug("  Shift pressed")
        if alt:
            kb.press(Key.alt)
            logger.debug("  Alt pressed")
        
        # Small delay to ensure modifiers are registered
        time.sleep(0.1)
        
        # Press the actual key (this should be the one captured, not the modifiers)
        logger.debug(f"Pressing key: {pynput_key}")
        kb.press(pynput_key)
        time.sleep(0.1)  # Hold the key briefly
        kb.release(pynput_key)
        logger.debug("Key released")
        
        # Small delay before releasing modifiers
        time.sleep(0.05)
        
        # Release modifiers
        logger.debug("Releasing modifiers...")
        if alt:
            kb.release(Key.alt)
        if shift:
            kb.release(Key.shift)
        if ctrl:
            kb.release(Key.ctrl)
        
        # Give time for the key to be captured
        logger.debug("Waiting for capture...")
        time.sleep(0.3)  # Increased wait time
        
        # Read output with timeout
        # The capture program will output JSON and exit after receiving a keypress
        stdout_text = ""
        try:
            logger.debug("Reading output from capture program (timeout: 3 seconds)...")
            stdout, stderr = proc.communicate(timeout=3)
            
            if stderr:
                stderr_text = stderr.strip()
                if stderr_text:
                    logger.warning(f"Capture program stderr: {stderr_text}")
            
            stdout_text = stdout.strip() if stdout else ""
            logger.debug(f"Capture program stdout length: {len(stdout_text)}, content: {repr(stdout_text[:200])}")
            
            # Parse JSON output
            if stdout_text:
                try:
                    data = json.loads(stdout_text)
                    logger.debug(f"Successfully parsed JSON: {data}")
                    return data
                except json.JSONDecodeError as e:
                    logger.error(f"Failed to parse JSON: {e}")
                    logger.error(f"Raw output: {repr(stdout_text)}")
            else:
                logger.warning(f"No output from capture program (stdout empty)")
                if stderr:
                    logger.warning(f"Stderr: {stderr.strip()}")
        except subprocess.TimeoutExpired:
            logger.error(f"Capture program timed out after 3 seconds (pid: {proc.pid})")
            logger.error("The program may still be waiting for input. Killing process...")
            proc.kill()
            try:
                stdout, stderr = proc.communicate(timeout=1)
                logger.debug(f"After kill - stdout: {repr(stdout.strip() if stdout else '')}, stderr: {repr(stderr.strip() if stderr else '')}")
            except Exception as e:
                logger.debug(f"Error reading after kill: {e}")
        except json.JSONDecodeError as e:
            logger.error(f"Failed to parse JSON: {e}")
            logger.error(f"Output was: {repr(stdout_text)}")
            proc.kill()
            try:
                proc.communicate(timeout=1)
            except:
                pass
    
    except Exception as e:
        logger.exception(f"Error capturing {key}: {e}")
        proc.kill()
        proc.communicate()
    
    return None

def generate_combinations(exe_path: Path) -> List[Dict]:
    """Generate all key combinations by capturing live values."""
    logger.info("Starting key combination capture")
    results = []
    total = len(US_KEYS) * len(MODIFIERS)
    current = 0
    successful = 0
    failed = 0
    
    logger.info(f"Total combinations to capture: {total}")
    
    for key in US_KEYS:
        for ctrl, shift, alt in MODIFIERS:
            current += 1
            
            combo_name = []
            if ctrl:
                combo_name.append("Ctrl")
            if shift:
                combo_name.append("Shift")
            if alt:
                combo_name.append("Alt")
            combo_name.append(key.upper() if key.isalpha() else key)
            combo_str = "+".join(combo_name)
            
            logger.info(f"[{current}/{total}] Capturing {combo_str}...")
            print(f"[{current}/{total}] Capturing {combo_str}...", end=" ", flush=True)
            
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
    
    logger.info(f"Capture complete: {successful} successful, {failed} failed out of {total} total")
    return results

def main():
    logger.info("=" * 80)
    logger.info("Key Combination Capture Tool")
    logger.info("=" * 80)
    
    print("Generating key combination mappings...")
    print("This will simulate keypresses and capture actual Windows Console API values.")
    print("Make sure to build capture_keys.exe first: zig build-exe scripts/capture_keys.zig -target native")
    print()
    logger.info("Starting key combination capture process")
    
    # Check if executable exists
    logger.debug("Looking for capture_keys.exe...")
    exe_path = find_capture_keys_exe()
    if not exe_path:
        logger.error("capture_keys.exe not found!")
        print("ERROR: capture_keys.exe not found!")
        print("Build it first with: zig build-exe scripts/capture_keys.zig -target native")
        return 1
    
    logger.info(f"Found capture_keys.exe at: {exe_path}")
    print(f"Using capture_keys.exe from: {exe_path}")
    print()
    print("WARNING: This will simulate many keypresses!")
    print("Make sure no other applications are active.")
    print("Press Enter to continue or Ctrl+C to cancel...")
    try:
        input()
        logger.info("User confirmed, starting capture...")
    except KeyboardInterrupt:
        logger.info("User cancelled")
        print("\nCancelled.")
        return 1
    
    results = generate_combinations(exe_path)
    
    # Sort results
    results.sort(key=lambda x: (
        x["modifiers"]["ctrl"],
        x["modifiers"]["shift"],
        x["modifiers"]["alt"],
        x["key"]
    ))
    
    output_file = "key_combinations.json"
    
    logger.info(f"Writing results to {output_file}...")
    with open(output_file, 'w') as f:
        json.dump({
            "generated_at": time.strftime("%Y-%m-%d %H:%M:%S"),
            "total_combinations": len(results),
            "combinations": results
        }, f, indent=2)
    
    logger.info(f"Successfully wrote {len(results)} combinations to {output_file}")
    print()
    print(f"Generated {len(results)} key combinations")
    print(f"Output written to: {output_file}")
    
    # Also generate a human-readable text file
    txt_file = "key_combinations.txt"
    logger.info(f"Writing human-readable output to {txt_file}...")
    with open(txt_file, 'w') as f:
        f.write("Key Combination Mappings (Captured Live)\n")
        f.write("=" * 80 + "\n\n")
        f.write(f"Generated: {time.strftime('%Y-%m-%d %H:%M:%S')}\n")
        f.write(f"Total combinations: {len(results)}\n\n")
        
        current_mod = None
        for combo in results:
            mod_key = (
                combo["modifiers"]["ctrl"],
                combo["modifiers"]["shift"],
                combo["modifiers"]["alt"]
            )
            if mod_key != current_mod:
                current_mod = mod_key
                mod_str = []
                if mod_key[0]:
                    mod_str.append("Ctrl")
                if mod_key[1]:
                    mod_str.append("Shift")
                if mod_key[2]:
                    mod_str.append("Alt")
                f.write(f"\n{' + '.join(mod_str) if mod_str else 'No modifiers'}:\n")
                f.write("-" * 80 + "\n")
            
            f.write(f"  {combo['combo']:25} ")
            f.write(f"vk=0x{combo['vk_decimal']:02X} ")
            if combo['ascii'] is not None:
                f.write(f"ascii={combo['ascii']:3} (0x{combo['ascii']:02X}) ")
            else:
                f.write(f"ascii=None ")
            if combo['unicode'] is not None:
                f.write(f"unicode={combo['unicode']:4} (0x{combo['unicode']:04X})")
            else:
                f.write(f"unicode=None")
            f.write("\n")
    
    logger.info(f"Successfully wrote human-readable output to {txt_file}")
    logger.info("=" * 80)
    logger.info("Capture process completed successfully")
    logger.info("=" * 80)
    print(f"Human-readable output written to: {txt_file}")
    print(f"Log file: key_capture.log")
    return 0

if __name__ == "__main__":
    sys.exit(main())

