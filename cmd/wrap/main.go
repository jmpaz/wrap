package main

import (
	"fmt"
	"os"
	"strings"

	"github.com/example/wrap-go/internal/clip"
	"github.com/example/wrap-go/internal/fence"
	"github.com/example/wrap-go/internal/paste"
	"github.com/example/wrap-go/internal/wrap"
)

func usageAndExit() {
	fmt.Fprintf(os.Stderr, `Usage:
  wrap [md|xml]       # wrap stdin -> stdout (default md if piped & no arg)
  wrap paste [md|xml] # wrap clipboard, then emit paste keypress

Examples:
  echo "some text" | wrap md      # => wraps as markdown (code fences, default)
  echo "some text" | wrap xml     # => wraps in <paste>...</paste> tags
  wrap paste md                   # => wraps clipboard content as code block, pastes
`)
	os.Exit(1)
}

func main() {
	// If no arguments, check if we're receiving data from stdin.
	if len(os.Args) == 1 {
		content, err := readAllStdin()
		if err != nil {
			fmt.Fprintf(os.Stderr, "failed to read stdin: %v\n", err)
			os.Exit(1)
		}
		if content == "" {
			usageAndExit()
		}
		// Default to md
		fmt.Print(wrap.WrapContent(content, "md"))
		return
	}

	switch os.Args[1] {
	case "md", "xml":
		// wrap from stdin -> stdout
		format := os.Args[1]
		content, err := readAllStdin()
		if err != nil {
			fmt.Fprintf(os.Stderr, "failed to read stdin: %v\n", err)
			os.Exit(1)
		}
		fmt.Print(wrap.WrapContent(content, format))

	case "paste":
		// wrap the clipboard if needed, then simulate paste
		format := "md" // default
		if len(os.Args) >= 3 {
			format = os.Args[2]
		}
		handlePaste(format)

	default:
		usageAndExit()
	}
}

func handlePaste(format string) {
	clipText, err := clip.ReadClipboard()
	if err != nil {
		fmt.Fprintf(os.Stderr, "clipboard read error: %v\n", err)
		os.Exit(1)
	}

	if isAlreadyWrapped(clipText, format) {
		// Already wrapped; just simulate paste.
		if err := paste.SimulatePaste(); err != nil {
			fmt.Fprintf(os.Stderr, "simulate paste error: %v\n", err)
			os.Exit(1)
		}
		return
	}

	// Not wrapped -> wrap, update clipboard, then paste.
	wrapped := wrap.WrapContent(clipText, format)
	if err := clip.WriteClipboard(wrapped); err != nil {
		fmt.Fprintf(os.Stderr, "failed to write wrapped content to clipboard: %v\n", err)
		os.Exit(1)
	}

	if err := paste.SimulatePaste(); err != nil {
		fmt.Fprintf(os.Stderr, "simulate paste error: %v\n", err)
		os.Exit(1)
	}
}

// isAlreadyWrapped checks if the clipboard content is “already fenced” by your new rule:
// “If the longest run of backticks in the entire text is found in either the
// first or last non-whitespace line (and is >=3), then we consider it already fenced.”
// For XML, if the first/last lines match <paste>...</paste>, we consider it fenced.
func isAlreadyWrapped(s, format string) bool {
	// Trim trailing + leading whitespace lines
	lines := strings.Split(s, "\n")
	lines = trimEmptyLines(lines)
	if len(lines) == 0 {
		return false
	}

	switch format {
	case "xml":
		// Simple check
		if strings.TrimSpace(lines[0]) == "<paste>" &&
			strings.TrimSpace(lines[len(lines)-1]) == "</paste>" {
			return true
		}
		return false

	case "md":
		overallLongest := fence.LongestBacktickRun(s)
		if overallLongest < 3 {
			return false
		}

		firstLine := lines[0]
		lastLine := lines[len(lines)-1]

		longestFirst := fence.LongestBacktickRun(firstLine)
		longestLast := fence.LongestBacktickRun(lastLine)

		return (longestFirst == overallLongest) || (longestLast == overallLongest)

	default:
		return false
	}
}

// trimEmptyLines removes leading/trailing lines if they're all whitespace.
func trimEmptyLines(lines []string) []string {
	start := 0
	for start < len(lines) && strings.TrimSpace(lines[start]) == "" {
		start++
	}
	end := len(lines)
	for end > start && strings.TrimSpace(lines[end-1]) == "" {
		end--
	}
	return lines[start:end]
}

func readAllStdin() (string, error) {
	info, err := os.Stdin.Stat()
	if err != nil {
		return "", err
	}
	// return empty if stdin is a terminal
	if (info.Mode() & os.ModeCharDevice) != 0 {
		return "", nil
	}

	// read piped content
	data, err := os.ReadFile("/dev/stdin")
	if err != nil {
		return "", err
	}
	return string(data), nil
}
