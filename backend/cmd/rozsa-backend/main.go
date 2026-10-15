package main

import (
	"flag"
	"fmt"
	"os"

	"rozsa/backend/internal/app/session"
	"rozsa/backend/internal/transport/server"
)

type directories []string

func (directories *directories) String() string { return fmt.Sprint([]string(*directories)) }
func (directories *directories) Set(value string) error {
	if value == "" {
		return fmt.Errorf("session directory cannot be empty")
	}
	*directories = append(*directories, value)
	return nil
}
func main() {
	var dirs directories
	flag.Var(&dirs, "session-dir", "session directory; repeat in layer order")
	flag.Parse()
	if flag.NArg() != 0 {
		fmt.Fprintln(os.Stderr, "unexpected positional arguments")
		os.Exit(2)
	}
	if err := server.Serve(os.Stdin, os.Stdout, session.Catalog{Directories: dirs}); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
