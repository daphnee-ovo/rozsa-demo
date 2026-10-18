package modelcatalog

import (
	"context"
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"time"

	"rozsa/backend/internal/model/types"
)

type fileStamp struct {
	Path     string
	Size     int64
	Modified time.Time
}

func (catalog *catalog) stamps() ([]fileStamp, error) {
	stamps := []fileStamp{}
	for _, dir := range catalog.options.Directories {
		entries, err := os.ReadDir(dir)
		if os.IsNotExist(err) {
			continue
		}
		if err != nil {
			return nil, err
		}
		for _, entry := range entries {
			if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".json") {
				continue
			}
			info, err := entry.Info()
			if os.IsNotExist(err) {
				continue
			}
			if err != nil {
				return nil, err
			}
			stamps = append(stamps, fileStamp{filepath.Join(dir, entry.Name()), info.Size(), info.ModTime()})
		}
	}
	return stamps, nil
}

// Watch checks file metadata cheaply; unchanged state emits no event. Without
// file changes, renewal checks and retry after a failed refresh are minute-bound.
func (catalog *catalog) Watch(ctx context.Context, deliver func(Snapshot)) error {
	ticker := time.NewTicker(time.Second)
	defer ticker.Stop()
	var stamps []fileStamp
	var previous Snapshot
	var nextRenewal time.Time
	for {
		if err := ctx.Err(); err != nil {
			return err
		}
		current, err := catalog.stamps()
		var snapshot Snapshot
		changed := err != nil || nextRenewal.IsZero() || !reflect.DeepEqual(current, stamps) || time.Now().After(nextRenewal)
		if changed && err == nil {
			snapshot, err = catalog.Available(ctx)
			if err == nil {
				stamps, err = catalog.stamps()
				nextRenewal = time.Now().Add(time.Minute)
			}
		}
		if err != nil {
			if ctx.Err() != nil {
				return ctx.Err()
			}
			snapshot = Snapshot{Models: []types.Model{}, Diagnostics: []Diagnostic{{Message: err.Error()}}}
			nextRenewal = time.Time{}
		}
		if changed && !reflect.DeepEqual(snapshot, previous) {
			deliver(snapshot)
			previous = snapshot
		}
		select {
		case <-ctx.Done():
			return ctx.Err()
		case <-ticker.C:
		}
	}
}
