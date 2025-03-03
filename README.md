# wrap

`wrap` is a utility for wrapping text in a fenced code block (adaptive to input content) or `<paste>` tags:

1. **Wrap from stdin to stdout**  
   `wrap [md|xml]`

   - If format is `md` (default), fences the content with backticks.
     - If the longest run of backticks in the content is ≥ 3, the fence is `(longest + 2)` backticks.
     - otherwise, uses 3 backticks.
   - If `xml`, wraps content in `<paste> ... </paste>`.

2. **Wrap clipboard content, then paste**  
   `wrap paste [md|xml]`

   - Reads the current clipboard text.
   - Checks if the text is already wrapped in `<paste>...</paste>` or in backticks, depending on the format.
   - If already wrapped, it simply simulates a "paste" keystroke (ctrl+shift+v on linux, cmd+v on macOS).
   - Otherwise:
     1. Wraps clipboard text
     2. Updates clipboard with wrapped text
     3. Simulates a "paste" keystroke after a delay


### Dependencies

- **Linux**  
  - [`ydotool`](https://github.com/ReimuNotMoe/ydotool) is required for simulating Ctrl+Shift+V.


### Usage

wrap from stdin -> stdout:
```bash
echo "some text" | ./wrap md  # defaults to md if no format is specified
echo "some text" | ./wrap xml
```

wrap & paste:
```bash
wrap paste md  # wraps clipboard content in a code block, then emits ctrl+shift+v / cmd+v
wrap paste xml
```

compatible with macOS + Linux (X11/Wayland).

