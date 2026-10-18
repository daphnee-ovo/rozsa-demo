package sessions

import (
	"context"
	"errors"
)

// CancellationOnly distinguishes a normal stop from joined write, response or
// cleanup failures, which still require an observable error terminal.
func CancellationOnly(err error) bool {
	if err == nil {
		return false
	}
	if joined, ok := err.(interface{ Unwrap() []error }); ok {
		children := joined.Unwrap()
		if len(children) == 0 {
			return false
		}
		for _, child := range children {
			if !CancellationOnly(child) {
				return false
			}
		}
		return true
	}
	if wrapped, ok := err.(interface{ Unwrap() error }); ok {
		return CancellationOnly(wrapped.Unwrap())
	}
	return errors.Is(err, context.Canceled)
}
