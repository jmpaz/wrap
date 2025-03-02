package wrap

import (
	"fmt"
	"strings"

	"github.com/example/wrap-go/internal/fence"
)

// WrapContent wraps the given content according to the given format.
// Supported formats: "xml" or "md".
func WrapContent(content string, format string) string {
	switch format {
	case "xml":
		return wrapXML(content)
	case "md":
		return wrapMD(content)
	default:
		return content // or error, but let's just return content.
	}
}

func wrapXML(content string) string {
	return fmt.Sprintf("<paste>\n%s\n</paste>\n", content)
}

func wrapMD(content string) string {
	longest := fence.LongestBacktickRun(content)
	fenceLen := 3
	if longest >= 3 {
		fenceLen = longest + 2
	}

	f := strings.Repeat("`", fenceLen)
	return fmt.Sprintf("%s\n%s\n%s\n", f, content, f)
}
