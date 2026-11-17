# Nyx Calculator

A fast, lightweight desktop calculator for NyxOS and Omarchy, part of the Nyx application suite.

Nyx Calculator unites the clean elegance of macOS Calculator, the mathematical versatility of Windows Calculator, the natural-language scratchpad workflow of Numi, and the interactive graphical capabilities of Desmos into a single native Linux application.

---

## Install

On NyxOS, choose Calculator in Welcome when you set up, or install it later from the Store or with `nyx install nyx-calculator`. On Omarchy:

```sh
curl -fsSL https://raw.githubusercontent.com/design-nexus/nyx-calculator/main/install.sh | bash
```

## Key Features & Modes

Nyx Calculator provides 6 dedicated calculation modes switchable via the topbar dropdown or keyboard shortcuts (`Alt+1` through `Alt+6`):

### 1. Standard Mode (`Alt+1`)
- **Exact Decimal Arithmetic:** 128-bit decimal arithmetic backed by `rust_decimal` with zero floating-point representation errors (`0.1 + 0.2 = 0.3`).
- **Pratt Expression Parser:** Operator precedence (`+`, `−`, `×`, `÷`, `^`, `%`), parentheses, implicit multiplication (`2(3+4)`), unary negation, and immediate functions (`sqrt`, `sqr`, `recip`).
- **Percentage Semantics:** Real-world additive and multiplicative percentage calculations (`100 + 10% = 110`, `100 − 10% = 90`, `250 × 15% = 37.5`).
- **Live Preview:** Instant result preview as you type with clear syntax and domain error indicators.
- **Memory Operations:** Full standard memory register supporting `MC` (Clear), `MR` (Recall), `M+` (Add), `M−` (Subtract), and `MS` (Store) with a persistent status indicator.
- **Searchable History Sidebar:** SQLite-persisted calculation history with instant LIKE-search, pin/favorite calculation entries, single-click restoration, and copy to clipboard (`Ctrl+H`).

### 2. Scientific Mode (`Alt+2`)
- **Trigonometry & Hyperbolics:** `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `sinh`, `cosh`, `tanh`.
- **Degrees & Radians:** Seamless angle mode toggle with exact angle snapping (multiples of 30°, 45°, 90°, 180°, 360°) to eliminate IEEE-754 epsilon artifacts (e.g. `tan(45°) = 1`, `sin(30°) = 0.5`).
- **Advanced Math:** Factorials (`n!`), logarithms (`log` base 10, natural `ln`), exponential functions ($e^x$), arbitrary roots ($\sqrt[y]{x}$), powers ($x^y$), absolute values (`|x|`), modulo, and reciprocal ($1/x$).
- **Mathematical Constants:** Single-tap access to $\pi$ (Pi), $e$ (Euler's number), and $\tau$ (Tau).
- **Scientific Notation:** Supports exponential notation (`1.5e6`, `2.3e-4`).

### 3. Programmer Mode (`Alt+3`)
- **Quad-Radix Display & Synchronization:** Simultaneous synchronized display and input across Hexadecimal (`HEX`), Decimal (`DEC`), Octal (`OCT`), and Binary (`BIN`).
- **Word Widths:** Instant switching between `QWORD` (64-bit), `DWORD` (32-bit), `WORD` (16-bit), and `BYTE` (8-bit) with automatic bit-masking and sign propagation.
- **Signed & Unsigned Arithmetic:** Full two's-complement arithmetic toggle with negative integer representation.
- **Interactive 64-Bit Toggle Grid:** Visual bit board displaying all 64 bits organized into 4-bit nibbles. Clicking any individual bit toggles its value in real time and synchronizes all radices and displays.
- **Bitwise Logic & Shifts:** `AND`, `OR`, `XOR`, `NOT`, logical/arithmetic left shift (`SHL`), right shift (`SHR`), and circular rotation (`ROL`, `ROR`).

### 4. Unit Converter (`Alt+4`)
- **12 Comprehensive Categories:** Length, Mass, Temperature, Area, Volume, Speed, Time, Data storage, Energy, Power, Pressure, and Angles.
- **Instant Conversions:** As soon as either input changes, values recompute instantaneously.
- **Affine Temperature Math:** Exact conversion handling linear offsets (Celsius $\leftrightarrow$ Fahrenheit $\leftrightarrow$ Kelvin).
- **Quick Category Chips:** Horizontal pill selector for swift category switching.
- **Directional Swap:** One-click swap button (`⇄`) to invert source and target units.

### 5. Calculation Sheets (`Alt+5`)
- **Numi-Style Scratchpad:** Multi-line natural-language calculations with expressions, currencies, percentages, and unit conversions (`15% of 249`, `1200 + 8% tax`, `5 miles in km`, `32 fahrenheit to celsius`, `$250 split between 4`).
- **Variables & Line References:** Define arbitrary variables (`radius = 12`, `tax_rate = 0.08`) and reference previous line outputs with `$1`, `$2`, or `prev`.
- **Synchronized Result Gutter:** Dual-pane layout with source document on the left and evaluated results aligned line-by-line in a dedicated right gutter.
- **Document Management:** Multi-sheet notebook saved automatically to local SQLite storage with Create, Delete, and Markdown/Plaintext export.

### 6. Interactive Graphing Calculator (`Alt+6`)
- **Multi-Function Plotting:** Plot multiple simultaneous mathematical expressions with customizable palette colors.
- **Hardware-Accelerated Cairo Canvas:** Smooth pan via drag gestures, cursor-anchored zoom with mouse scroll, and responsive window resizing.
- **Adaptive Sampling & Asymptote Detection:** Discontinuity detection across poles (such as $\tan(x)$ or $1/x$) preventing false vertical line artifacts.
- **Coordinate Crosshair Inspection:** Moving the cursor over curves displays high-precision coordinate tooltips and axis projections.
- **Calculus & Analysis:**
  - Derivative visualization ($f'(x)$ toggle).
  - Numerical integration via Simpson's rule with shaded area under the curve.
  - Root finding ($f(x) = 0$) using Brent's method and bisection.
  - Local extrema detection (minima and maxima).

---

## Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| `Alt+1` | Standard Calculator mode |
| `Alt+2` | Scientific Calculator mode |
| `Alt+3` | Programmer Calculator mode |
| `Alt+4` | Unit Converter mode |
| `Alt+5` | Calculation Sheets mode |
| `Alt+6` | Graphing Calculator mode |
| `0–9`, `.` | Enter digits and decimal separator |
| `+`, `−`, `×`, `÷`, `^`, `%` | Arithmetic operations |
| `Enter`, `=` | Evaluate current expression |
| `Backspace` | Delete previous character |
| `Esc`, `C` | Clear expression |
| `Ctrl+H` | Toggle calculation history sidebar |
| `Ctrl+C` | Copy displayed result |
| `Ctrl+V` | Paste expression from clipboard |
| `Ctrl+M` | Memory Store (`MS`) |
| `Ctrl+R` | Memory Recall (`MR`) |
| `Ctrl+P` | Memory Add (`M+`) |
| `Ctrl+Q` | Memory Subtract (`M−`) |
| `Ctrl+L` | Memory Clear (`MC`) |
| `Ctrl+,` | Open Settings overlay |
| `F1` | Show Shortcuts cheat sheet |

---

## Architecture & Project Structure

The codebase is organized into an independent Cargo workspace adhering strictly to modular separation between UI and mathematical logic:

```text
nyx-calculator/
├── Cargo.toml
├── README.md
├── STYLE.md
├── install.sh
├── uninstall.sh
├── data/
│   ├── icons/
│   │   └── io.github.design_nexus.nyx.Calculator.svg
│   └── io.github.design_nexus.nyx.Calculator.desktop
└── crates/
    ├── calculator-core/         # Independent mathematical engine
    │   ├── src/
    │   │   ├── ast.rs           # Abstract Syntax Tree representation
    │   │   ├── token.rs         # Lexical tokens and operators
    │   │   ├── parser.rs        # Pratt expression parser
    │   │   ├── evaluator.rs     # Exact Decimal and f64 evaluation
    │   │   ├── memory.rs        # Memory register state
    │   │   ├── scientific.rs    # Trig, logs, roots, angle snapping
    │   │   ├── programmer.rs    # Radices, bitwise logic, word sizing
    │   │   ├── units.rs         # 12-category unit conversions
    │   │   └── natural.rs       # Numi-style sheet evaluator
    ├── calculator-storage/      # Local persistence
    │   └── src/
    │       ├── lib.rs           # SQLite schema, queries, migrations
    │       └── models.rs        # HistoryEntry and SheetRecord
    ├── calculator-graph/        # 2D Graphing & Calculus engine
    │   └── src/
    │       ├── viewport.rs      # World/Screen coordinate transforms
    │       ├── function.rs      # Plotted function compilation
    │       ├── sampler.rs       # Adaptive polyline sampling
    │       └── calculus.rs      # Roots, extrema, derivatives, Simpson integration
    └── calculator-app/          # GTK4 User Interface & Application
        └── src/
            ├── main.rs          # GTK Application entrypoint
            ├── window.rs        # Topbar, chrome, mode stack coordinator
            ├── display.rs       # Formula input & result readout
            ├── history_sidebar.rs # Searchable history panel
            ├── keypad/          # Standard, Scientific, Programmer keypads
            ├── views/           # Converter, Sheets, and Graph views
            ├── theme.rs         # Omarchy palette synchronization
            └── style.css        # Scoped CSS matching Nyx STYLE.md
```

---

## Design System Adherence (`STYLE.md`)

- **Single Flat Surface:** Pure `@theme_bg` background with zero window decoration borders or titlebars.
- **Monospace Typography:** JetBrains Mono across all displays, keypads, and tables.
- **1px Hairline Borders:** Subtle `@hairline` borders separating functional regions.
- **Hidden Scrollbars:** `PolicyType::External` applied to all scrollable views for clean, minimalist surfaces.
- **Live Omarchy Synchronization:** Real-time colors update when the Omarchy desktop theme changes, alongside 14 built-in color schemes.

---

## Building & Installation

### Prerequisites

- **Rust:** 1.85+ (Edition 2024)
- **GTK4:** `gtk4` development libraries
- **Cairo & SQLite:** `cairo`, `sqlite3`

### Run Test Suite

```bash
cargo test --workspace
```

### Build & Install

Run the automated installer to build the optimized release binary and install desktop assets:

```bash
./install.sh
```

This installs:
- Executable binary to `~/.local/bin/nyx-calculator`
- Desktop entry to `~/.local/share/applications/io.github.design_nexus.nyx.Calculator.desktop`
- Scalable SVG icon to `~/.local/share/icons/hicolor/scalable/apps/io.github.design_nexus.nyx.Calculator.svg`
- Triggers `update-desktop-database` automatically

### Uninstall

To remove installed files:

```bash
./uninstall.sh
```
