#!/usr/bin/env python3
"""Demo script for recording axterminator GIF.

This script demonstrates axterminator features with visual output
suitable for recording as a GIF demo.

Run: python demo/demo_script.py
"""

import time
import sys

# ANSI colors
CYAN = "\033[96m"
GREEN = "\033[92m"
YELLOW = "\033[93m"
RED = "\033[91m"
BOLD = "\033[1m"
RESET = "\033[0m"
DIM = "\033[2m"


def print_slow(text, delay=0.03):
    """Print text character by character."""
    for char in text:
        sys.stdout.write(char)
        sys.stdout.flush()
        time.sleep(delay)
    print()


def print_header(text):
    """Print a header."""
    print(f"\n{BOLD}{CYAN}{'═' * 50}{RESET}")
    print(f"{BOLD}{CYAN}  {text}{RESET}")
    print(f"{BOLD}{CYAN}{'═' * 50}{RESET}\n")


def print_code(code):
    """Print code block."""
    print(f"{DIM}>>> {RESET}{YELLOW}{code}{RESET}")
    time.sleep(0.5)


def print_result(text):
    """Print result."""
    print(f"{GREEN}    {text}{RESET}")
    time.sleep(0.3)


def main():
    print_header("axterminator Demo")
    print_slow(f"{BOLD}World's Most Superior macOS GUI Testing Framework{RESET}")
    time.sleep(1)

    # Feature 1: Background Testing
    print_header("🎭 Background Testing (WORLD FIRST)")
    print_slow("Test macOS apps WITHOUT stealing focus!")
    time.sleep(0.5)

    print_code("import axterminator as ax")
    print_code("app = ax.app(name='Calculator')")
    print_result("✓ Connected to Calculator (PID: 12345)")

    print_code("app.find('5').click()  # Background click!")
    print_result("✓ Clicked '5' in background")

    print_code("app.find('+').click()")
    print_result("✓ Clicked '+' in background")

    print_code("app.find('3').click()")
    print_result("✓ Clicked '3' in background")

    print_code("app.find('=').click()")
    print_result("✓ Result: 8")

    print_slow(f"\n{GREEN}✨ Your active window stayed focused!{RESET}")
    time.sleep(1)

    # Feature 2: Speed
    print_header("⚡ 800-2000× Faster")

    print_slow("Element access: ~250µs (vs 200ms-2s competitors)")
    time.sleep(0.5)

    print_code("# Benchmark")
    print_code("for _ in range(1000):")
    print_code("    app.find('5')  # 250µs each")
    print_result("✓ 1000 finds in 0.25 seconds")
    time.sleep(1)

    # Feature 3: Self-Healing
    print_header("🔧 Self-Healing Locators")

    print_slow("7 fallback strategies for robust element location:")
    time.sleep(0.5)

    strategies = [
        ("1. data_testid", "Developer-set stable IDs"),
        ("2. aria_label", "Accessibility labels"),
        ("3. identifier", "AX identifier"),
        ("4. title", "Element title (fuzzy)"),
        ("5. xpath", "Structural path"),
        ("6. position", "Relative position"),
        ("7. visual_vlm", "AI vision fallback"),
    ]

    for strat, desc in strategies:
        print(f"   {CYAN}{strat}{RESET}: {desc}")
        time.sleep(0.2)

    time.sleep(1)

    # Feature 4: VLM
    print_header("🤖 AI Vision Detection")

    print_code("ax.configure_vlm(backend='mlx')  # Local AI")
    print_result("✓ MLX VLM configured (fast, private)")

    print_code("app.find('the blue Save button in toolbar')")
    print_result("✓ Found via visual detection")
    time.sleep(1)

    # Summary
    print_header("📦 Install Now")

    print(f"   {BOLD}pip install axterminator{RESET}")
    print()
    print(f"   {DIM}GitHub: github.com/MikkoParkkola/axterminator{RESET}")
    print(f"   {DIM}Docs: mikkoparkkola.github.io/axterminator{RESET}")

    print()
    print_slow(f"{BOLD}{GREEN}Built with 🦀 Rust + 🐍 Python{RESET}")
    time.sleep(2)


if __name__ == "__main__":
    main()
