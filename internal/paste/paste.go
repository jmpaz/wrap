package paste

import (
	"errors"
	"os/exec"
	"runtime"
	"time"
)

// SimulatePaste tries to simulate a paste keystroke.
func SimulatePaste() error {
	// Wait a bit so the new clipboard data has time to settle in.
	time.Sleep(200 * time.Millisecond)

	if runtime.GOOS == "darwin" {
		// AppleScript: tell application "System Events" to keystroke "v" using command down
		cmd := exec.Command("osascript", "-e", `tell application "System Events" to keystroke "v" using command down`)

		return cmd.Run()
	}

	// Linux: Use ydotool, if available
	if isAvailable("ydotool") {
		cmd := exec.Command("ydotool", "key", "29:1", "42:1", "47:1", "47:0", "42:0", "29:0")
		return cmd.Run()
	}

	return errors.New("cannot emit keystroke (requires ydotool on Linux, osascript on macOS)")
}

func isAvailable(program string) bool {
	_, err := exec.LookPath(program)
	return err == nil
}
