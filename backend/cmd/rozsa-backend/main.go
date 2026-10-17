package main

import (
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"

	"rozsa/backend/internal/app/bootstrap"
	"rozsa/backend/internal/app/runtime"
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
	cwd, err := os.Getwd()
	if err != nil {
		fail(err.Error())
	}
	writeDirectory := ""
	catalogDirectories := []string(dirs)
	if len(catalogDirectories) == 0 {
		writeDirectory, catalogDirectories, err = bootstrap.SessionDirectories(cwd)
		if err != nil {
			fail(err.Error())
		}
	} else {
		for index, directory := range catalogDirectories {
			catalogDirectories[index], err = filepath.Abs(directory)
			if err != nil {
				fail(err.Error())
			}
		}
		writeDirectory = catalogDirectories[0]
	}
	model, apiKey, authPath, err := bootstrap.ConfiguredModel("", "")
	if err != nil {
		fail(err.Error())
	}
	app, err := runtime.New(runtime.Config{
		SessionDir: writeDirectory, SessionDirs: catalogDirectories,
		Model: model, APIKey: apiKey, AuthPath: authPath, Output: io.Discard,
	})
	if err != nil {
		fail(err.Error())
	}
	if err := server.ServeProductApplication(os.Stdin, os.Stdout, app, productMessageApplication{app: app}); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func fail(message string) {
	fmt.Fprintln(os.Stderr, message)
	os.Exit(1)
}
