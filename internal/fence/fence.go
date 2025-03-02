package fence

import "unicode/utf8"

// LongestBacktickRun scans the text and returns the maximum consecutive run of '`'.
func LongestBacktickRun(s string) int {
	max := 0
	current := 0

	for i := 0; i < len(s); {
		r, width := utf8.DecodeRuneInString(s[i:])
		if r == '`' {
			current++
		} else {
			if current > max {
				max = current
			}
			current = 0
		}
		i += width
	}
	if current > max {
		max = current
	}
	return max
}
