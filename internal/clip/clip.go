package clip

import (
	"errors"
	"os/exec"
	"runtime"
	"strings"
)

// ReadClipboard tries a sequence of known clipboard commands
// (depending on OS) to retrieve the clipboard content.
func ReadClipboard() (string, error) {
	if runtime.GOOS == "darwin" {
		// Use pbpaste
		cmd := exec.Command("pbpaste")
		out, err := cmd.Output()
		return string(out), err
	}

	// Linux
	// Try wl-paste, xclip, xsel in that order
	checks := [][]string{
		{"wl-paste"},
		{"xclip", "-o", "-selection", "clipboard"},
		{"xsel", "-b"},
	}
	for _, c := range checks {
		if isAvailable(c[0]) {
			cmd := exec.Command(c[0], c[1:]...)
			out, err := cmd.Output()
			if err == nil {
				return string(out), nil
			}
		}
	}
	return "", errors.New("no known clipboard reader utility found or failed to read")
}

// WriteClipboard sets the clipboard to the given text, similarly
// trying a known set of tools for macOS or Linux.
func WriteClipboard(content string) error {
	if runtime.GOOS == "darwin" {
		cmd := exec.Command("pbcopy")
		cmd.Stdin = strings.NewReader(content)
		return cmd.Run()
	}

	// Linux
	checks := [][]string{
		{"wl-copy"},
		{"xclip", "-i", "-selection", "clipboard"},
		{"xsel", "-b"},
	}
	var lastErr error
	for _, c := range checks {
		if isAvailable(c[0]) {
			cmd := exec.Command(c[0], c[1:]...)
			cmd.Stdin = strings.NewReader(content)
			err := cmd.Run()
			if err == nil {
				return nil
			}
			lastErr = err
		}
	}
	if lastErr != nil {
		return lastErr
	}
	return errors.New("no known clipboard writer utility found")
}

// isAvailable returns true if the given program is in the PATH
func isAvailable(program string) bool {
	_, err := exec.LookPath(program)
	return err == nil
}
