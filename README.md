# wrap

`wrap` wraps text in a fenced code block or `<paste>` tags.

1. **Wrap from stdin to stdout**
   `wrap [md|xml]`

   - If format is `md`, fences the content with backticks.
     - If the longest run of backticks in the content is >= 3, the fence is `(longest + 2)` backticks.
     - Otherwise, uses 3 backticks.
   - If format is `xml`, wraps content in `<paste> ... </paste>` tags.

2. **Unwrap from stdin to stdout**
   `wrap unwrap`

   - Removes outer markdown fences or `<paste>...</paste>` tags.
   - Leaves content unchanged when no known wrapper is present.

3. **Wrap clipboard content, then paste**
   `wrapctl paste [md|xml]`

   - Sends the request to `wrapd` over `$XDG_RUNTIME_DIR/wrap/wrapd.sock`.
   - Reads the current clipboard text.
   - Checks whether the text is already wrapped.
   - Updates the clipboard with the requested wrapper when needed.
   - Emits `Ctrl+Shift+V`.

4. **Unwrap clipboard content, then paste**
   `wrapctl unwrap-paste`

   - Removes outer markdown fences or `<paste>...</paste>` tags from the clipboard.
   - Emits `Ctrl+Shift+V`.

5. **Check daemon status**
   `wrapctl status`

### Wayland

`wrapd` uses `zwlr_data_control_manager_v1` for clipboard access and `zwp_virtual_keyboard_manager_v1` for paste key events.

## Home Manager

```nix
{
  imports = [ inputs.wrap.homeManagerModules.default ];
  programs.wrap.enable = true;
}
```
