# wrap-go

A single `wrap` binary (written in Go) that implements two main functions:

1. **Wrap from stdin to stdout**  
   `wrap [md|xml]`

   If you pipe content into `wrap` without specifying a format, it defaults to Markdown (`md`).  
   - If `xml`, wraps content in `<paste> ... </paste>`.
   - If `md`, fences the content with backticks.  
     - If the longest run of backticks in the content is ≥ 3, 
       the fence is `(longest + 2)` backticks.
     - Otherwise, uses 3 backticks.

2. **Wrap, then paste**  
   `wrap paste [md|xml]`

   - Reads the current clipboard text.
   - Checks if it’s **already** wrapped in `<paste>...</paste>` (for XML) or 
     in the correct Markdown fence (for MD).
   - If already wrapped, it simply simulates a "paste" keystroke.
   - Otherwise:
     1. Wraps the clipboard text (XML or MD).
     2. Updates the clipboard with the new wrapped text.
     3. Simulates a "paste" keystroke (~0.2s later).

### Dependencies

- **macOS**  
  - You should have the standard `pbpaste` and `pbcopy` commands for clipboard I/O.  
  - Uses `osascript` to simulate `Cmd+V`.
- **Linux**  
  - At least one of `wl-copy`/`wl-paste`, `xclip`, or `xsel` for clipboard I/O.
  - [`ydotool`](https://github.com/ReimuNotMoe/ydotool) is required for simulating Ctrl+Shift+V.

### Installation

```bash
git clone ...
cd wrap-go
go build -o wrap ./cmd/wrap
```

You now have a `wrap` binary.

### Usage

```bash
# 1) Wrap from stdin -> stdout
echo "some text" | ./wrap md  # defaults to md if no format is specified
echo "some text" | ./wrap xml

# 2) Wrap & paste from clipboard
./wrap paste
./wrap paste xml
```

### Examples

```bash
# Example: wrap MD from stdin (default)
echo "\`\`\`some code\`\`\`" | ./wrap
```

This might output something like:

`````````````````````
\`\`\`\`
\`\`\`some code\`\`\`
\`\`\`\`
`````````````````````

where the fence length is determined based on the longest run of backticks in the content.
