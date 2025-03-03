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

// checks whether content is already wrapped as markdown/xml
// returns "md", "xml", or "" if neither
func detectWrappedFormat(content string) string {
	if isAlreadyWrapped(content, "md") {
		return "md"
	}
	if isAlreadyWrapped(content, "xml") {
		return "xml"
	}
	return ""
}

// removes markdown code fences if present
func unwrapMd(content string) string {
	lines := strings.Split(content, "\n")
	lines = trimEmptyLines(lines)
	if len(lines) == 0 {
		return content // nothing to unwrap
	}

	overallLongest := fence.LongestBacktickRun(content)
	firstLineLongest := fence.LongestBacktickRun(lines[0])
	lastLineLongest := fence.LongestBacktickRun(lines[len(lines)-1])

	start := 0
	if firstLineLongest == overallLongest && firstLineLongest >= 3 {
		start = 1
	}

	end := len(lines)
	if lastLineLongest == overallLongest && lastLineLongest >= 3 && end > start {
		end--
	}

	middle := lines[start:end]
	return strings.Join(middle, "\n")
}

// remove <paste> tags if present
func unwrapXml(content string) string {
	lines := strings.Split(content, "\n")
	lines = trimEmptyLines(lines)
	if len(lines) < 2 {
		return content
	}
	if strings.TrimSpace(lines[0]) == "<paste>" &&
		strings.TrimSpace(lines[len(lines)-1]) == "</paste>" {
		middle := lines[1 : len(lines)-1]
		return strings.Join(middle, "\n")
	}
	return content
}

func handlePaste(format string) {
	clipText, err := clip.ReadClipboard()
	if err != nil {
		fmt.Fprintf(os.Stderr, "clipboard read error: %v\n", err)
		os.Exit(1)
	}

	wrappedIn := detectWrappedFormat(clipText)
	var newText string

	switch {
	case wrappedIn == format:
		newText = clipText
	case wrappedIn == "md" && format == "xml":
		newText = wrap.WrapContent(unwrapMd(clipText), "xml")
	case wrappedIn == "xml" && format == "md":
		newText = wrap.WrapContent(unwrapXml(clipText), "md")
	default:
		newText = wrap.WrapContent(clipText, format)
	}

	// trim trailing newlines to avoid extra blank lines when pasting
	newText = strings.TrimRight(newText, "\n")

	if err := clip.WriteClipboard(newText); err != nil {
		fmt.Fprintf(os.Stderr, "failed to write wrapped content to clipboard: %v\n", err)
		os.Exit(1)
	}

	if err := paste.SimulatePaste(); err != nil {
		fmt.Fprintf(os.Stderr, "simulate paste error: %v\n", err)
		os.Exit(1)
	}
}

// check if the content is already fenced/tagged
func isAlreadyWrapped(content, format string) bool {
	lines := strings.Split(content, "\n")
	lines = trimEmptyLines(lines)
	if len(lines) == 0 {
		return false
	}

	switch format {
	case "xml":
		if strings.TrimSpace(lines[0]) == "<paste>" &&
			strings.TrimSpace(lines[len(lines)-1]) == "</paste>" {
			return true
		}
		return false

	case "md":
		overallLongest := fence.LongestBacktickRun(content)
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

// remove leading/trailing lines if they're all whitespace
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
