package signals

import (
	"context"
	"os"
	"os/signal"
	"syscall"
)

var onlyOneSignalHandler = make(chan struct{})
var errorExitCodeChannel = make(chan int, 1)

func SetupExitHandler(ctx context.Context) (context.Context, func()) {
	close(onlyOneSignalHandler) // panics when called twice

	ctx, cancel := context.WithCancel(ctx)

	c := make(chan os.Signal, 1)
	signal.Notify(c, shutdownSignals...)
	go func() {
		select {
		case signal := <-c:
			errorExitCodeChannel <- 128 + int(signal.(syscall.Signal))
			cancel()
		case <-ctx.Done():
		}
	}()

	return ctx, func() {
		signal.Stop(c)
		cancel()

		select {
		case exitcode := <-errorExitCodeChannel:
			os.Exit(exitcode)
		default:
			// Do not exit, there are no exit codes in the channel,
			// so just continue and let the main function go out of
			// scope instead.
		}
	}
}
