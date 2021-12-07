package signals

import (
	"context"
	"errors"
)

// SetExitCode sets the exit code to 1 if the error is not a context.Canceled error.
func SetExitCode(err error) {
	if (err != nil) && !errors.Is(err, context.Canceled) {
		errorExitCodeChannel <- 1 // Indicate that there was an error
	}
}
